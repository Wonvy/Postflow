//! 朗读：把文字念成声音。用 Kokoro 模型，sherpa-onnx 运行（和语音识别是同一个运行库，不用另带）。
//! 在本机完成，文字不会离开这台电脑。
//!
//! 模型文件按「角色」找（见 voices.json）：`model`、`voices`（每种声音的特征）、`tokens`、
//! 一个或几个 `lexicon`（英文、中文词典），和几个 `rule_fst`（把中文句子里的数字、日期、电话号码念成中文）。
//! 这个版本的运行库里 Kokoro 不用 espeak-ng 和结巴分词的数据，那两个目录不用下载。

use std::path::Path;

use sherpa_onnx::{
    GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsKokoroModelConfig,
    OfflineTtsModelConfig,
};

use crate::speakable::{self, Words};
use crate::{Error, Model, Result};

pub struct Voice {
    tts: OfflineTts,
    words: Words,
    speakers: Vec<String>,
    /// 模型里实际有几种声音。等价副本里的可能比目录里列的少（比如魔搭上那份 v1.0 少了最后一种）。
    available: usize,
    sample_rate: u32,
}

fn path_str(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| Error::Load("the model path is not valid UTF-8".into()))
}

impl Voice {
    /// 加载一个装好的朗读模型。要一秒左右，放在阻塞线程里调用。
    pub fn load(dir: &Path, model: &Model, threads: i32) -> Result<Voice> {
        if model.kind != "kokoro" {
            return Err(Error::Unsupported(model.kind.clone()));
        }
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
        let lexicons = paths("lexicon")?;
        // 判断英文词念不念得出来要用词典：中文词典里的词不是 ASCII，读进来也会被筛掉
        let mut words = String::new();
        for path in &lexicons {
            words.push_str(&std::fs::read_to_string(path)?);
            words.push('\n');
        }
        let words = Words::from_lexicon(&words);
        let fsts = paths("rule_fst")?;
        let config = OfflineTtsConfig {
            model: OfflineTtsModelConfig {
                kokoro: OfflineTtsKokoroModelConfig {
                    model: Some(one("model")?),
                    voices: Some(one("voices")?),
                    tokens: Some(one("tokens")?),
                    lexicon: (!lexicons.is_empty()).then(|| lexicons.join(",")),
                    length_scale: 1.0,
                    ..Default::default()
                },
                num_threads: threads,
                ..Default::default()
            },
            rule_fsts: (!fsts.is_empty()).then(|| fsts.join(",")),
            max_num_sentences: 1,
            ..Default::default()
        };
        let tts = OfflineTts::create(&config)
            .ok_or_else(|| Error::Load("the voice model could not be loaded".into()))?;
        let available = usize::try_from(tts.num_speakers()).unwrap_or(0);
        let sample_rate = u32::try_from(tts.sample_rate()).unwrap_or(24_000);
        Ok(Voice {
            tts,
            words,
            speakers: model.speakers.clone(),
            available,
            sample_rate,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// 念之前整理过的样子（数字写成字、缩写拆成字母……）。排查念错的地方用。
    pub fn prepare(&self, text: &str) -> String {
        speakable::prepare(text, &self.words)
    }

    /// 念一句。`speaker` 是声音的名字（目录里 `speakers` 的一项），`speed` 1.0 是正常语速。
    /// 返回单声道的采样（-1 到 1），没什么可念的（只有标点、网址）时是空的。
    pub fn speak(&self, text: &str, speaker: &str, speed: f32) -> Result<Vec<f32>> {
        let sid = self
            .speakers
            .iter()
            .position(|s| s == speaker)
            .filter(|&i| i < self.available)
            .ok_or_else(|| Error::Speak(format!("no voice named {speaker}")))?;
        let text = self.prepare(text);
        if !text.chars().any(char::is_alphanumeric) {
            return Ok(Vec::new());
        }
        let options = GenerationConfig {
            sid: i32::try_from(sid).unwrap_or(0),
            speed: speed.clamp(0.5, 3.0),
            ..Default::default()
        };
        let audio = self
            .tts
            .generate_with_config::<fn(&[f32], f32) -> bool>(&text, &options, None)
            .ok_or_else(|| Error::Speak("the voice model returned nothing".into()))?;
        Ok(audio.samples().to_vec())
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
