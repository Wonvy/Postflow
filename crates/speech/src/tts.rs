//! 朗读：把文字念成声音，sherpa-onnx 运行（和语音识别是同一个运行库，不用另带）。在本机完成，文字不会离开这台电脑。
//!
//! 两种模型：
//! - Kokoro：模型自带几十上百种声音，按名字选（`zf_001`、`af_heart`）。
//! - ZipVoice：照着一段 3–10 秒的真人录音念，音色、语气都学那段录音（[`Speaker::Sample`]），最像真人。
//!
//! 模型文件按「角色」找（见 voices.json）：Kokoro 有 `model`、`voices`；ZipVoice 有 `encoder`、`decoder`、`vocoder`；
//! 都有 `tokens`、一个或几个 `lexicon`（中文、英文词典）。Kokoro 还有几个 `rule_fst`（把中文句子里的数字、日期、
//! 电话号码念成中文）；这个版本的运行库里 ZipVoice 读了规则却不用，它的数字由 speakable.rs 写成汉字。
//! 这个版本的运行库不用 espeak-ng 和结巴分词的数据，那两个目录不用下载。
//!
//! ZipVoice 自带的词典只有汉字，英文词全靠词典，没有的就跳过。它认识的是标准的国际音标，Kokoro 的英文词典
//! 用的是几乎一样的记法，只差 8 个符号：所以 ZipVoice 也下载 Kokoro 的英文词典（`lexicon_en`），第一次加载时
//! 换成国际音标写一份 `lexicon-en-ipa.txt` 在模型目录里。

use std::path::Path;

use sherpa_onnx::{
    GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsKokoroModelConfig,
    OfflineTtsModelConfig, OfflineTtsZipvoiceModelConfig,
};

use crate::speakable::{self, Words};
use crate::{AudioReader, Error, Model, Result};

/// ZipVoice 要的声音样本采样率。
pub const SAMPLE_RATE: u32 = 24_000;
/// 声音样本最长用多少秒：再长也不会更像，每句都要多算一遍。
pub const SAMPLE_MAX_SECONDS: f64 = 12.0;
/// 声音样本整体的音量（均方根）低于这个就当没有声音：正常说话在 0.05 以上，安静房间的底噪在 0.002 左右。
const SILENT_RMS: f32 = 0.005;
/// 换成国际音标的英文词典，ZipVoice 第一次加载时写在模型目录里。
const IPA_LEXICON: &str = "lexicon-en-ipa.txt";

/// 念出来的一句。
pub struct Spoken {
    /// 单声道的采样（-1 到 1），没什么可念的（只有标点、网址）时是空的。
    pub samples: Vec<f32>,
    /// 实际按什么语速念的。ZipVoice 念很短的句子时快不了（见 `Voice::speak`），按正常语速念，
    /// 放的时候再加快。
    pub speed: f32,
}

/// 用谁的声音念。
pub enum Speaker<'a> {
    /// 模型自带的声音（Kokoro 的 `zf_001` 这类名字）。
    Preset(&'a str),
    /// 照着一段录音念（ZipVoice）：录音（单声道，-1 到 1，`SAMPLE_RATE`）和录音里说的话。
    Sample { audio: &'a [f32], text: &'a str },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Kokoro,
    ZipVoice,
}

pub struct Voice {
    tts: OfflineTts,
    kind: Kind,
    words: Words,
    speakers: Vec<String>,
    /// 模型里实际有几种声音。等价副本里的可能比目录里列的少（比如魔搭上那份 v1.0 少了最后一种）。
    available: usize,
    sample_rate: u32,
    /// ZipVoice 每句话去噪几步：越多越细，也越慢。
    steps: i32,
}

fn path_str(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| Error::Load("the model path is not valid UTF-8".into()))
}

/// Kokoro 英文词典里 ZipVoice 不认识的 8 个符号，换成标准国际音标。
fn ipa_of(ch: char) -> Option<&'static str> {
    Some(match ch {
        'A' => "e ɪ",
        'I' => "a ɪ",
        'O' => "o ʊ",
        'W' => "a ʊ",
        'Y' => "ɔ ɪ",
        'ʤ' => "d ʒ",
        'ʧ' => "t ʃ",
        'ᵊ' => "ə",
        _ => return None,
    })
}

/// 把 Kokoro 的英文词典（每行「词 音素…」）换成 ZipVoice 用的国际音标词典，每个符号之间用空格隔开。
pub fn ipa_lexicon(kokoro: &str) -> String {
    let mut out = String::with_capacity(kokoro.len() + kokoro.len() / 4);
    for line in kokoro.lines() {
        let mut parts = line.split_whitespace();
        let Some(word) = parts.next() else { continue };
        let mut phones: Vec<String> = Vec::new();
        for p in parts {
            for ch in p.chars() {
                match ipa_of(ch) {
                    Some(ipa) => phones.extend(ipa.split(' ').map(str::to_string)),
                    None => phones.push(ch.to_string()),
                }
            }
        }
        if phones.is_empty() {
            continue;
        }
        out.push_str(word);
        out.push(' ');
        out.push_str(&phones.join(" "));
        out.push('\n');
    }
    out
}

/// 读一段声音样本：任何常见格式，混成单声道、换成 `SAMPLE_RATE`，最多 `SAMPLE_MAX_SECONDS` 秒。
pub fn read_sample(mut reader: AudioReader) -> Result<Vec<f32>> {
    reader = reader.output_rate(SAMPLE_RATE);
    let max = (SAMPLE_MAX_SECONDS * f64::from(SAMPLE_RATE)) as usize;
    let mut out = Vec::new();
    while out.len() < max {
        match reader.read()? {
            Some(chunk) => out.extend_from_slice(chunk.samples),
            None => break,
        }
    }
    out.truncate(max);
    // 开头结尾的静音去掉：录音里多出来的空白会让念出来的每句话前后也拖着空白
    let loud = |s: &f32| s.abs() > 0.01;
    // 一点声音都没有（以前这时整段照收，之后每句都念不出来）
    let (Some(first), Some(last)) = (out.iter().position(loud), out.iter().rposition(loud)) else {
        return Err(Error::Silent);
    };
    let start = first.saturating_sub(SAMPLE_RATE as usize / 10);
    let end = (last + SAMPLE_RATE as usize / 5).min(out.len());
    let clip = &out[start..end];
    // 只有零星的杂音、整体很小声的，也学不出音色
    let rms = (clip.iter().map(|s| s * s).sum::<f32>() / clip.len().max(1) as f32).sqrt();
    if rms < SILENT_RMS {
        return Err(Error::Silent);
    }
    Ok(clip.to_vec())
}

impl Voice {
    /// 加载一个装好的朗读模型。要一秒左右，放在阻塞线程里调用。
    pub fn load(dir: &Path, model: &Model, threads: i32) -> Result<Voice> {
        let kind = match model.kind.as_str() {
            "kokoro" => Kind::Kokoro,
            "zipvoice" => Kind::ZipVoice,
            other => return Err(Error::Unsupported(other.to_string())),
        };
        let paths = |role: &str| -> Result<Vec<String>> {
            model
                .files
                .iter()
                .filter(|f| f.role == role)
                .map(|f| {
                    let path = dir.join(&f.name);
                    if !path.is_file() {
                        return Err(Error::Load(format!("{} is missing", f.name)));
                    }
                    path_str(&path)
                })
                .collect()
        };
        let one = |role: &str| -> Result<String> {
            paths(role)?
                .into_iter()
                .next()
                .ok_or_else(|| Error::Load(format!("the model has no {role} file")))
        };
        let mut lexicons = paths("lexicon")?;
        // 判断英文词念不念得出来要用英文词典：中文词典里的词不是 ASCII，读进来也会被筛掉
        let mut english = String::new();
        for path in &lexicons {
            english.push_str(&std::fs::read_to_string(path)?);
            english.push('\n');
        }
        if kind == Kind::ZipVoice {
            let source = one("lexicon_en")?;
            let kokoro = std::fs::read_to_string(&source)?;
            let ipa = dir.join(IPA_LEXICON);
            if !ipa.is_file() {
                crate::write_atomic(&ipa, ipa_lexicon(&kokoro).as_bytes())?;
            }
            lexicons.push(path_str(&ipa)?);
            english.push_str(&kokoro);
        }
        let words = Words::from_lexicon(&english);
        let fsts = paths("rule_fst")?;
        let lexicon = (!lexicons.is_empty()).then(|| lexicons.join(","));
        let mut config = OfflineTtsModelConfig {
            num_threads: threads,
            ..Default::default()
        };
        match kind {
            Kind::Kokoro => {
                config.kokoro = OfflineTtsKokoroModelConfig {
                    model: Some(one("model")?),
                    voices: Some(one("voices")?),
                    tokens: Some(one("tokens")?),
                    lexicon,
                    length_scale: 1.0,
                    ..Default::default()
                }
            }
            Kind::ZipVoice => {
                config.zipvoice = OfflineTtsZipvoiceModelConfig {
                    tokens: Some(one("tokens")?),
                    encoder: Some(one("encoder")?),
                    decoder: Some(one("decoder")?),
                    vocoder: Some(one("vocoder")?),
                    lexicon,
                    ..Default::default()
                }
            }
        }
        let config = OfflineTtsConfig {
            model: config,
            rule_fsts: (!fsts.is_empty()).then(|| fsts.join(",")),
            max_num_sentences: 1,
            ..Default::default()
        };
        let tts = OfflineTts::create(&config)
            .ok_or_else(|| Error::Load("the voice model could not be loaded".into()))?;
        let available = usize::try_from(tts.num_speakers()).unwrap_or(0);
        let sample_rate = u32::try_from(tts.sample_rate()).unwrap_or(24_000);
        let steps = model
            .options
            .get("num_steps")
            .and_then(|v| v.parse().ok())
            .unwrap_or(4);
        Ok(Voice {
            tts,
            kind,
            words,
            speakers: model.speakers.clone(),
            available,
            sample_rate,
            steps,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// 要不要声音样本（ZipVoice）。
    pub fn needs_sample(&self) -> bool {
        self.kind == Kind::ZipVoice
    }

    /// 念之前整理过的样子（数字写成字、缩写拆成字母……）。排查念错的地方用。
    pub fn prepare(&self, text: &str) -> String {
        // ZipVoice 不用模型自带的中文数字规则：数字自己写成汉字
        speakable::prepare_with(text, &self.words, self.kind == Kind::ZipVoice)
    }

    /// 念一句，`speed` 1.0 是正常语速。
    pub fn speak(&self, text: &str, speaker: Speaker<'_>, speed: f32) -> Result<Spoken> {
        let speed = speed.clamp(0.5, 3.0);
        let mut options = GenerationConfig {
            speed,
            ..Default::default()
        };
        match (self.kind, speaker) {
            (Kind::Kokoro, Speaker::Preset(name)) => {
                let sid = self
                    .speakers
                    .iter()
                    .position(|s| s == name)
                    .filter(|&i| i < self.available)
                    .ok_or_else(|| Error::VoiceSetup(format!("no voice named {name}")))?;
                options.sid = i32::try_from(sid).unwrap_or(0);
            }
            (Kind::ZipVoice, Speaker::Sample { audio, text }) => {
                if audio.is_empty() || text.trim().is_empty() {
                    return Err(Error::VoiceSetup("the voice sample is empty".into()));
                }
                options.reference_audio = Some(audio.to_vec());
                options.reference_sample_rate = SAMPLE_RATE as i32;
                // 录音里的话也要按同样的方法整理，和录音对得上
                options.reference_text = Some(self.prepare(text));
                options.num_steps = self.steps;
            }
            (Kind::Kokoro, Speaker::Sample { .. }) => {
                return Err(Error::VoiceSetup(
                    "this model has built-in voices only".into(),
                ));
            }
            (Kind::ZipVoice, Speaker::Preset(_)) => {
                return Err(Error::VoiceSetup("this model needs a voice sample".into()));
            }
        }
        let text = self.prepare(text);
        if !text.chars().any(char::is_alphanumeric) {
            return Ok(Spoken {
                samples: Vec::new(),
                speed,
            });
        }
        let generate = |options: &GenerationConfig| {
            self.tts
                .generate_with_config::<fn(&[f32], f32) -> bool>(&text, options, None)
                .map(|audio| audio.samples().to_vec())
        };
        if let Some(samples) = generate(&options) {
            return Ok(Spoken { samples, speed });
        }
        // ZipVoice 按语速估计要念多长：句子很短、又要念得快时，估出来比声音样本还短，它就不念了。
        // 按正常语速再念一次，放的时候再加快
        if self.kind == Kind::ZipVoice && speed > 1.0 {
            options.speed = 1.0;
            if let Some(samples) = generate(&options) {
                return Ok(Spoken {
                    samples,
                    speed: 1.0,
                });
            }
        }
        Err(Error::Speak("the voice model returned nothing".into()))
    }
}

/// 单声道采样存成 16 位 PCM 的 WAV 文件内容。
pub fn wav(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let data_len = u32::try_from(samples.len() * 2).unwrap_or(u32::MAX - 36);
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // 单声道
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kokoro_lexicon_becomes_ipa() {
        let ipa = ipa_lexicon(
            "hello h ə l ˈ O\njudge ʤ ˈ ʌ ʤ\nnow n ˈ W\nchocolate ʧ ˈ ɑ k ᵊ l ə t\n\nbad\n",
        );
        assert_eq!(
            ipa,
            "hello h ə l ˈ o ʊ\njudge d ʒ ˈ ʌ d ʒ\nnow n ˈ a ʊ\nchocolate t ʃ ˈ ɑ k ə l ə t\n"
        );
    }

    fn sample_of(samples: &[f32]) -> Result<Vec<f32>> {
        let bytes = wav(samples, SAMPLE_RATE);
        let reader = AudioReader::from_source(Box::new(std::io::Cursor::new(bytes)), Some("wav"))?;
        read_sample(reader)
    }

    #[test]
    fn silent_or_hissing_samples_are_refused() {
        let rate = SAMPLE_RATE as usize;
        // 三秒静音
        assert!(matches!(
            sample_of(&vec![0.0; rate * 3]),
            Err(Error::Silent)
        ));
        // 三秒很轻的底噪，偶尔一下超过门限
        let hiss: Vec<f32> = (0..rate * 3)
            .map(|i| {
                if i % 4000 == 0 {
                    0.02
                } else {
                    0.001 * ((i % 7) as f32 - 3.0)
                }
            })
            .collect();
        assert!(matches!(sample_of(&hiss), Err(Error::Silent)));
        // 前后各一秒静音、中间两秒正常音量的声音：留下中间（再带一点前后）
        let mut tone = vec![0.0; rate];
        tone.extend((0..rate * 2).map(|i| 0.3 * (i as f32 * 0.05).sin()));
        tone.extend(vec![0.0; rate]);
        let clip = sample_of(&tone).unwrap();
        assert!(
            clip.len() > rate * 2 && clip.len() < rate * 3,
            "{}",
            clip.len()
        );
    }

    #[test]
    fn wav_header() {
        let bytes = wav(&[0.0, 1.0, -1.0, 2.0], 24_000);
        assert_eq!(bytes.len(), 44 + 8);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 36 + 8);
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            24_000
        );
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 8);
        assert_eq!(i16::from_le_bytes([bytes[46], bytes[47]]), 32767);
        // 超出范围的截到最大值
        assert_eq!(i16::from_le_bytes([bytes[50], bytes[51]]), 32767);
        assert_eq!(i16::from_le_bytes([bytes[48], bytes[49]]), -32767);
    }
}
