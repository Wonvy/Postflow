//! 加载一个识别模型，把音频转成字幕。
//!
//! 流程：解码出来的音频先过一个很小的模型（Silero VAD）找出哪里有人说话，切成一段一段
//! （一口气说完的一段，最长十来秒），每段交给识别模型，结果再按标点切成句子。

use std::path::Path;

use sherpa_onnx::{
    OfflineDolphinModelConfig, OfflineFireRedAsrCtcModelConfig, OfflineMoonshineModelConfig,
    OfflineNemoEncDecCtcModelConfig, OfflineParaformerModelConfig, OfflineRecognizer,
    OfflineRecognizerConfig, OfflineSenseVoiceModelConfig, OfflineTransducerModelConfig,
    OfflineWenetCtcModelConfig, OfflineWhisperModelConfig, OfflineZipformerCtcModelConfig,
    SileroVadModelConfig, SpeakerEmbeddingExtractor, SpeakerEmbeddingExtractorConfig,
    VadModelConfig, VoiceActivityDetector,
};

use crate::audio::{AudioReader, SAMPLE_RATE};
use crate::catalog::Model;
use crate::cues::{self, Cue};
use crate::speakers::Speakers;
use crate::{Error, Result};

/// VAD 一次吃这么多样本（Silero 的固定窗口）。
const WINDOW: usize = 512;
/// 一段话最长这么久，再长就在最近的小停顿处切开：太长识别慢，字幕也来得晚。
const MAX_SPEECH: f32 = 12.0;
/// 没人说话时，每处理这么多秒音频也报一次进度（好让调用方有机会喊停）。
const REPORT_EVERY: f64 = 5.0;

/// 用几个线程识别：留一半给播放和界面。
pub fn default_threads() -> i32 {
    let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
    (cores / 2).clamp(1, 4) as i32
}

pub struct Engine {
    recognizer: OfflineRecognizer,
    vad_model: String,
    /// 装了声纹模型时，每段话算个声纹，分出是谁在说。
    speaker: Option<SpeakerEmbeddingExtractor>,
}

/// 短于这个（秒）的一段不算声纹：太短了不准，也省点时间。
const SPEAKER_MIN_SECONDS: f64 = 0.6;
/// 一段话切成这么长（秒）的小块分别算声纹：对谈节目里换人很快，一段话里常常混着两个人。
const SPEAKER_WINDOW: f64 = 3.0;

/// 把 `seconds` 长的一段切成差不多长的几块（每块 2–4.5 秒，短于 4.5 秒的不切），返回每块的起止（秒）。
fn speaker_windows(seconds: f64) -> Vec<(f64, f64)> {
    let n = ((seconds / SPEAKER_WINDOW).round() as usize).max(1);
    let size = seconds / n as f64;
    (0..n)
        .map(|i| (i as f64 * size, (i + 1) as f64 * size))
        .collect()
}

/// 一句字幕（相对这段话开头的起止）是哪一块里说的：和它重叠最多、认出了人的那块。
fn speaker_for(start: f64, end: f64, windows: &[(f64, f64, Option<u32>)]) -> Option<u32> {
    windows
        .iter()
        .filter(|w| w.2.is_some())
        .map(|&(a, b, who)| ((end.min(b) - start.max(a)).max(0.0), who))
        .filter(|(overlap, _)| *overlap > 0.0)
        .max_by(|x, y| x.0.total_cmp(&y.0))
        .and_then(|(_, who)| who)
}

/// 转写的一步：新出来的几句，和已经处理到第几秒。
pub struct Step {
    pub cues: Vec<Cue>,
    pub reached: f64,
    /// 到这一步为止认出来的说话人，存盘时跟着字幕一起存。
    pub speakers: Speakers,
}

/// 一次 `run` 的结果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outcome {
    /// 处理到了第几秒。
    pub reached: f64,
    /// 读到了文件结尾。
    pub eof: bool,
}

fn path_str(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| Error::Load("the model path is not valid UTF-8".into()))
}

/// 按模型类型拼出识别器的配置。`dir` 是模型文件所在的目录。
fn recognizer_config(dir: &Path, model: &Model, threads: i32) -> Result<OfflineRecognizerConfig> {
    let file = |role: &str| -> Result<Option<String>> {
        match model.file(role) {
            Some(f) => {
                let path = dir.join(&f.name);
                if !path.is_file() {
                    return Err(Error::Load(format!("{} is missing", f.name)));
                }
                Ok(Some(path_str(&path)?))
            }
            None => Ok(None),
        }
    };
    let need = |role: &str| -> Result<Option<String>> {
        file(role)?
            .map(Some)
            .ok_or_else(|| Error::Load(format!("the model has no {role} file")))
    };
    let option = |key: &str| model.options.get(key).cloned().filter(|v| !v.is_empty());

    let mut config = OfflineRecognizerConfig::default();
    let m = &mut config.model_config;
    m.tokens = need("tokens")?;
    m.num_threads = threads;
    m.model_type = option("model_type");
    match model.kind.as_str() {
        "sense_voice" => {
            m.sense_voice = OfflineSenseVoiceModelConfig {
                model: need("model")?,
                language: option("language"),
                // 带标点、数字写成阿拉伯数字
                use_itn: option("use_itn").is_none_or(|v| v != "false"),
            };
        }
        "whisper" => {
            m.whisper = OfflineWhisperModelConfig {
                encoder: need("encoder")?,
                decoder: need("decoder")?,
                // 不指定语言就每段自己认
                language: option("language"),
                task: Some("transcribe".into()),
                ..Default::default()
            };
            m.model_type.get_or_insert_with(|| "whisper".into());
        }
        "paraformer" => {
            m.paraformer = OfflineParaformerModelConfig {
                model: need("model")?,
            };
            m.model_type.get_or_insert_with(|| "paraformer".into());
        }
        "transducer" => {
            m.transducer = OfflineTransducerModelConfig {
                encoder: need("encoder")?,
                decoder: need("decoder")?,
                joiner: need("joiner")?,
            };
            m.model_type.get_or_insert_with(|| "transducer".into());
        }
        "nemo_ctc" => {
            m.nemo_ctc = OfflineNemoEncDecCtcModelConfig {
                model: need("model")?,
            };
        }
        "moonshine" => {
            m.moonshine = OfflineMoonshineModelConfig {
                preprocessor: file("preprocessor")?,
                encoder: need("encoder")?,
                uncached_decoder: file("uncached_decoder")?,
                cached_decoder: file("cached_decoder")?,
                merged_decoder: file("merged_decoder")?,
            };
        }
        "dolphin" => {
            m.dolphin = OfflineDolphinModelConfig {
                model: need("model")?,
            };
        }
        "zipformer_ctc" => {
            m.zipformer_ctc = OfflineZipformerCtcModelConfig {
                model: need("model")?,
            };
        }
        "wenet_ctc" => {
            m.wenet_ctc = OfflineWenetCtcModelConfig {
                model: need("model")?,
            };
        }
        "fire_red_asr_ctc" => {
            m.fire_red_asr_ctc = OfflineFireRedAsrCtcModelConfig {
                model: need("model")?,
            };
        }
        other => return Err(Error::Unsupported(other.to_string())),
    }
    Ok(config)
}

impl Engine {
    /// 加载模型。几百兆的模型要一两秒，放在阻塞线程里调用。
    /// `speaker` 是声纹模型（所在目录和目录里的那一条），装了才有；加载不了只记一条日志，照样识别。
    pub fn load(
        dir: &Path,
        model: &Model,
        vad: &Path,
        speaker: Option<(&Path, &Model)>,
        threads: i32,
    ) -> Result<Engine> {
        if !model.supported() {
            return Err(Error::Unsupported(model.kind.clone()));
        }
        if !vad.is_file() {
            return Err(Error::Load("the voice detection model is missing".into()));
        }
        let config = recognizer_config(dir, model, threads.max(1))?;
        let recognizer = OfflineRecognizer::create(&config)
            .ok_or_else(|| Error::Load(format!("cannot load {}", model.id)))?;
        let speaker = speaker.and_then(|(dir, m)| {
            let file = m.file("model")?;
            let config = SpeakerEmbeddingExtractorConfig {
                model: Some(path_str(&dir.join(&file.name)).ok()?),
                num_threads: 1,
                ..Default::default()
            };
            let extractor = SpeakerEmbeddingExtractor::create(&config);
            if extractor.is_none() {
                tracing::warn!(model = %m.id, "cannot load the speaker model; transcribing without speakers");
            }
            extractor
        });
        Ok(Engine {
            recognizer,
            vad_model: path_str(vad)?,
            speaker,
        })
    }

    /// 能不能分出说话人（装了声纹模型）。
    pub fn has_speakers(&self) -> bool {
        self.speaker.is_some()
    }

    /// 一段话的声纹。
    fn embedding(&self, samples: &[f32]) -> Option<Vec<f32>> {
        let extractor = self.speaker.as_ref()?;
        let stream = extractor.create_stream()?;
        stream.accept_waveform(SAMPLE_RATE as i32, samples);
        stream.input_finished();
        extractor
            .is_ready(&stream)
            .then(|| extractor.compute(&stream))
            .flatten()
    }

    fn detector(&self) -> Result<VoiceActivityDetector> {
        let config = VadModelConfig {
            silero_vad: SileroVadModelConfig {
                model: Some(self.vad_model.clone()),
                threshold: 0.5,
                min_silence_duration: 0.5,
                min_speech_duration: 0.25,
                window_size: WINDOW as i32,
                max_speech_duration: MAX_SPEECH,
            },
            sample_rate: SAMPLE_RATE as i32,
            num_threads: 1,
            ..Default::default()
        };
        VoiceActivityDetector::create(&config, 60.0)
            .ok_or_else(|| Error::Load("cannot load the voice detection model".into()))
    }

    /// 识别 VAD 里已经攒好的各段。
    fn drain(
        &self,
        vad: &VoiceActivityDetector,
        base: f64,
        speakers: &mut Speakers,
        out: &mut Vec<Cue>,
    ) {
        let rate = f64::from(SAMPLE_RATE);
        while let Some(segment) = vad.front() {
            let start = base + segment.start() as f64 / rate;
            let samples = segment.samples();
            let duration = samples.len() as f64 / rate;
            let stream = self.recognizer.create_stream();
            stream.accept_waveform(SAMPLE_RATE as i32, samples);
            self.recognizer.decode(&stream);
            if let Some(result) = stream.get_result() {
                let mut new = cues::split(
                    start,
                    duration,
                    &result.text,
                    &result.tokens,
                    result.timestamps.as_deref(),
                );
                // 有话才算声纹：切成几小块分别认，每句取和它重叠最多的那块
                if !new.is_empty() && self.speaker.is_some() && duration >= SPEAKER_MIN_SECONDS {
                    let windows: Vec<(f64, f64, Option<u32>)> = speaker_windows(duration)
                        .into_iter()
                        .map(|(a, b)| {
                            let from = (a * rate) as usize;
                            let to = ((b * rate) as usize).min(samples.len());
                            let who = self
                                .embedding(&samples[from..to])
                                .and_then(|e| speakers.assign(&e, b - a));
                            (a, b, who)
                        })
                        .collect();
                    for cue in &mut new {
                        cue.speaker = speaker_for(cue.start - start, cue.end - start, &windows);
                    }
                }
                out.extend(new);
            }
            drop(segment);
            vad.pop();
        }
    }

    /// 从 `reader` 现在的位置转写到第 `until` 秒（或者文件结尾）。
    ///
    /// 每识别完一段调用一次 `sink`；它返回 `false` 就尽快停下（已经听到一半的那段话丢掉，
    /// 返回的 `reached` 只算到确实处理完的地方）。
    /// `speakers` 是这一期已经认出来的说话人，分出新的人会加进去。
    pub fn run(
        &self,
        reader: &mut AudioReader,
        until: f64,
        speakers: &mut Speakers,
        sink: &mut dyn FnMut(Step) -> bool,
    ) -> Result<Outcome> {
        let vad = self.detector()?;
        let rate = f64::from(SAMPLE_RATE);
        let base = reader.position();
        let mut pending: Vec<f32> = Vec::with_capacity(WINDOW * 4);
        // 喂给 VAD 多少样本了
        let mut fed = 0usize;
        // 确实处理完的位置：没人说话时跟着读到的位置走，有人说话时停在这段话开始前
        let mut reached = base;
        let mut reported = base;
        let mut eof = false;
        let mut cues: Vec<Cue> = Vec::new();

        loop {
            if reader.position() >= until {
                break;
            }
            let Some(chunk) = reader.read()? else {
                eof = true;
                break;
            };
            pending.extend_from_slice(chunk.samples);
            let mut offset = 0;
            while pending.len() - offset >= WINDOW {
                vad.accept_waveform(&pending[offset..offset + WINDOW]);
                offset += WINDOW;
                fed += WINDOW;
            }
            pending.drain(..offset);

            let had = !vad.is_empty();
            if had {
                self.drain(&vad, base, speakers, &mut cues);
            }
            let now = base + fed as f64 / rate;
            if !vad.detected() {
                reached = now;
            } else if let Some(last) = cues.last() {
                reached = reached.max(last.end.min(now));
            }
            if had || reached - reported >= REPORT_EVERY {
                reported = reached;
                if !sink(Step {
                    cues: std::mem::take(&mut cues),
                    reached,
                    speakers: speakers.clone(),
                }) {
                    return Ok(Outcome {
                        reached,
                        eof: false,
                    });
                }
            }
        }

        // 到头了：把最后不满一个窗口的补齐，让 VAD 把还在听的那段话交出来
        if !pending.is_empty() {
            pending.resize(WINDOW, 0.0);
            vad.accept_waveform(&pending);
        }
        vad.flush();
        self.drain(&vad, base, speakers, &mut cues);
        let reached = if eof {
            reader.position()
        } else {
            reader.position().min(until.max(base))
        };
        sink(Step {
            cues,
            reached,
            speakers: speakers.clone(),
        });
        Ok(Outcome { reached, eof })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_segments_are_split_for_speakers() {
        assert_eq!(speaker_windows(2.0), [(0.0, 2.0)]);
        assert_eq!(speaker_windows(4.4), [(0.0, 4.4)]);
        let w = speaker_windows(12.0);
        assert_eq!(w.len(), 4);
        assert_eq!(w[3], (9.0, 12.0));

        let labeled = [(0.0, 3.0, Some(0)), (3.0, 6.0, Some(1)), (6.0, 9.0, None)];
        assert_eq!(speaker_for(0.5, 2.0, &labeled), Some(0));
        // 跨两块的取重叠多的
        assert_eq!(speaker_for(2.5, 5.0, &labeled), Some(1));
        // 落在没认出人的那块里：退回旁边认出来的
        assert_eq!(speaker_for(5.5, 8.5, &labeled), Some(1));
        assert_eq!(speaker_for(6.5, 8.5, &labeled), None);
    }
}
