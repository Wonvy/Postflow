//! 把音频文件读成识别要的样子：单声道、16 kHz、f32。
//!
//! 一边解码一边给，不把整期节目读进内存；可以从中间某个时间开始（边播边转写要用）。

use std::fs::File;
use std::path::Path;

use sherpa_onnx::LinearResampler;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Time, TimeBase};

use crate::{Error, Result};

/// 识别模型要的采样率。
pub const SAMPLE_RATE: u32 = 16_000;

/// 连着这么多个包解不出来就当文件坏了，免得在一个坏文件上空转。
const MAX_BAD_PACKETS: usize = 200;

pub struct AudioReader {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    duration: Option<f64>,
    resampler: Option<LinearResampler>,
    /// 现在这个重采样器是按哪个采样率建的。
    rate: u32,
    interleaved: Vec<f32>,
    mono: Vec<f32>,
    /// 下一块样本从第几秒开始。
    position: f64,
    /// 跳转以后还没用第一个包的时间校准过位置。
    resync: bool,
}

/// 一小块音频：`start` 是它在整期节目里的时间（秒）。
pub struct Chunk<'a> {
    pub start: f64,
    pub samples: &'a [f32],
}

fn unsupported(err: SymError) -> Error {
    Error::Audio(err.to_string())
}

impl AudioReader {
    pub fn open(path: &Path) -> Result<AudioReader> {
        let file = File::open(path)?;
        let stream = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let format = symphonia::default::get_probe()
            .probe(
                &hint,
                stream,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(unsupported)?;
        let track = format
            .default_track(TrackType::Audio)
            .ok_or_else(|| Error::Audio("no audio track".into()))?;
        let params = track
            .codec_params
            .as_ref()
            .and_then(|p| p.audio())
            .ok_or_else(|| Error::Audio("unknown audio codec".into()))?;
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(params, &AudioDecoderOptions::default())
            .map_err(unsupported)?;
        let duration = match (track.time_base, track.duration) {
            (Some(base), Some(d)) => base.calc_duration(d).map(|t| t.as_secs_f64()),
            _ => None,
        };
        Ok(AudioReader {
            track_id: track.id,
            time_base: track.time_base,
            duration,
            format,
            decoder,
            resampler: None,
            rate: 0,
            interleaved: Vec::new(),
            mono: Vec::new(),
            position: 0.0,
            resync: false,
        })
    }

    /// 整期多长（秒）。有的文件头里没写，读到结尾才知道。
    pub fn duration(&self) -> Option<f64> {
        self.duration
    }

    /// 下一块样本从第几秒开始。读到结尾以后就是实际的总长度。
    pub fn position(&self) -> f64 {
        self.position
    }

    /// 跳到大约第 `seconds` 秒，返回实际跳到的位置（不晚于要的位置太多，通常稍早一点）。
    pub fn seek(&mut self, seconds: f64) -> Result<f64> {
        let seconds = seconds.max(0.0);
        let time = Time::try_from_secs_f64(seconds)
            .ok_or_else(|| Error::Audio("bad seek position".into()))?;
        let seeked = self
            .format
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time,
                    track_id: Some(self.track_id),
                },
            )
            .map_err(unsupported)?;
        self.decoder.reset();
        if let Some(r) = &self.resampler {
            r.reset();
        }
        self.position = self
            .time_base
            .and_then(|b| b.calc_time(seeked.actual_ts))
            .map_or(seconds, |t| t.as_secs_f64().max(0.0));
        self.resync = true;
        Ok(self.position)
    }

    /// 下一块。读完了返回 `None`。
    pub fn read(&mut self) -> Result<Option<Chunk<'_>>> {
        let mut bad = 0usize;
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => return self.finish(),
                // 文件被截断（比如没下完）时，读到哪儿算哪儿
                Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return self.finish();
                }
                Err(SymError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(e) => return Err(unsupported(e)),
            };
            if packet.track_id != self.track_id {
                continue;
            }
            if self.resync {
                self.resync = false;
                if let Some(t) = self.time_base.and_then(|b| b.calc_time(packet.pts)) {
                    self.position = t.as_secs_f64().max(0.0);
                }
            }
            let (rate, channels) = match self.decoder.decode(&packet) {
                Ok(buf) => {
                    let spec = buf.spec();
                    let (rate, channels) = (spec.rate(), spec.channels().count());
                    buf.copy_to_vec_interleaved(&mut self.interleaved);
                    (rate, channels)
                }
                // 个别坏包跳过去
                Err(SymError::DecodeError(_) | SymError::IoError(_)) => {
                    bad += 1;
                    if bad > MAX_BAD_PACKETS {
                        return Err(Error::Audio("too many undecodable packets".into()));
                    }
                    continue;
                }
                Err(SymError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(e) => return Err(unsupported(e)),
            };
            if channels == 0 || rate == 0 || self.interleaved.is_empty() {
                continue;
            }
            self.mono.clear();
            if channels == 1 {
                self.mono.extend_from_slice(&self.interleaved);
            } else {
                let scale = 1.0 / channels as f32;
                self.mono.extend(
                    self.interleaved
                        .chunks_exact(channels)
                        .map(|frame| frame.iter().sum::<f32>() * scale),
                );
            }
            if rate != SAMPLE_RATE {
                if self.rate != rate {
                    self.resampler = Some(
                        LinearResampler::create(rate as i32, SAMPLE_RATE as i32)
                            .ok_or_else(|| Error::Audio(format!("cannot resample {rate} Hz")))?,
                    );
                    self.rate = rate;
                }
                if let Some(r) = &self.resampler {
                    self.mono = r.resample(&self.mono, false);
                }
            }
            if self.mono.is_empty() {
                continue;
            }
            let start = self.position;
            self.position += self.mono.len() as f64 / f64::from(SAMPLE_RATE);
            return Ok(Some(Chunk {
                start,
                samples: &self.mono,
            }));
        }
    }

    fn finish(&mut self) -> Result<Option<Chunk<'_>>> {
        if self.duration.is_none_or(|d| d < self.position) {
            self.duration = Some(self.position);
        }
        Ok(None)
    }
}
