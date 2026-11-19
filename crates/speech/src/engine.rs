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
    SileroVadModelConfig, VadModelConfig, VoiceActivityDetector,
};

use crate::audio::{AudioReader, SAMPLE_RATE};
use crate::catalog::Model;
use crate::cues::{self, Cue};
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
}

/// 转写的一步：新出来的几句，和已经处理到第几秒。
pub struct Step {
    pub cues: Vec<Cue>,
    pub reached: f64,
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
    pub fn load(dir: &Path, model: &Model, vad: &Path, threads: i32) -> Result<Engine> {
        if !model.supported() {
            return Err(Error::Unsupported(model.kind.clone()));
        }
        if !vad.is_file() {
            return Err(Error::Load("the voice detection model is missing".into()));
        }
        let config = recognizer_config(dir, model, threads.max(1))?;
        let recognizer = OfflineRecognizer::create(&config)
            .ok_or_else(|| Error::Load(format!("cannot load {}", model.id)))?;
        Ok(Engine {
            recognizer,
            vad_model: path_str(vad)?,
        })
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
    fn drain(&self, vad: &VoiceActivityDetector, base: f64, out: &mut Vec<Cue>) {
        let rate = f64::from(SAMPLE_RATE);
        while let Some(segment) = vad.front() {
            let start = base + segment.start() as f64 / rate;
            let samples = segment.samples();
            let duration = samples.len() as f64 / rate;
            let stream = self.recognizer.create_stream();
            stream.accept_waveform(SAMPLE_RATE as i32, samples);
            self.recognizer.decode(&stream);
            if let Some(result) = stream.get_result() {
                out.extend(cues::split(
                    start,
                    duration,
                    &result.text,
                    &result.tokens,
                    result.timestamps.as_deref(),
                ));
            }
            drop(segment);
            vad.pop();
        }
    }

    /// 从 `reader` 现在的位置转写到第 `until` 秒（或者文件结尾）。
    ///
    /// 每识别完一段调用一次 `sink`；它返回 `false` 就尽快停下（已经听到一半的那段话丢掉，
    /// 返回的 `reached` 只算到确实处理完的地方）。
    pub fn run(
        &self,
        reader: &mut AudioReader,
        until: f64,
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
                self.drain(&vad, base, &mut cues);
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
        self.drain(&vad, base, &mut cues);
        let reached = if eof {
            reader.position()
        } else {
            reader.position().min(until.max(base))
        };
        sink(Step { cues, reached });
        Ok(Outcome { reached, eof })
    }
}
