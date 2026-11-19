//! 把音频文件读成识别要的样子：单声道、16 kHz、f32。
//!
//! 一边解码一边给，不把整期节目读进内存；可以从中间某个时间开始（边播边转写要用）。
//! 来源可以是还在下载的文件（`MediaSource`），读到还没下到的地方由来源自己等。
//!
//! 解码用 Symphonia（MP3、AAC、Vorbis、FLAC、WAV）；Opus 它不支持，用纯 Rust 的 opus-decoder，
//! 直接解成 16 kHz 单声道，不用再重采样。

use std::fs::File;
use std::path::Path;

use opus_decoder::OpusDecoder;
use sherpa_onnx::LinearResampler;
use symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::packet::Packet;
use symphonia::core::units::{Time, TimeBase};

use crate::{Error, Result};

/// 识别模型要的采样率。
pub const SAMPLE_RATE: u32 = 16_000;

/// 连着这么多个包解不出来就当文件坏了，免得在一个坏文件上空转。
const MAX_BAD_PACKETS: usize = 200;

/// Opus 的时间戳和「预跳过」都按 48 kHz 算。
const OPUS_RATE: u32 = 48_000;

enum Decoder {
    Symphonia(Box<dyn AudioDecoder>),
    Opus {
        /// 解码器的状态有好几 KB，放在堆上。
        decoder: Box<OpusDecoder>,
        /// 开头还要丢掉多少个样本（编码器垫的「预跳过」，按 16 kHz 算）。
        skip: usize,
    },
}

impl Decoder {
    fn reset(&mut self) {
        match self {
            Decoder::Symphonia(d) => d.reset(),
            Decoder::Opus { decoder, skip } => {
                decoder.reset();
                *skip = 0;
            }
        }
    }
}

/// 解出来的一包：采样率和声道数，样本在 `interleaved` 里。
enum Decoded {
    Samples {
        rate: u32,
        channels: usize,
    },
    /// 坏包，跳过去。
    Bad,
    /// 解码器要重新开始。
    Reset,
}

pub struct AudioReader {
    format: Box<dyn FormatReader>,
    decoder: Decoder,
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
        let ext = path.extension().and_then(|e| e.to_str());
        Self::from_source(Box::new(file), ext)
    }

    /// 从任意来源读。`ext` 是文件扩展名，帮着认格式（还在下载的 `.part` 文件看不出来）。
    pub fn from_source(source: Box<dyn MediaSource>, ext: Option<&str>) -> Result<AudioReader> {
        let stream = MediaSourceStream::new(source, Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = ext {
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
        let decoder = if params.codec == CODEC_ID_OPUS {
            // 直接解成识别要的单声道 16 kHz；立体声由解码器混成单声道
            let decoder =
                OpusDecoder::new(SAMPLE_RATE, 1).map_err(|e| Error::Audio(format!("opus: {e}")))?;
            let pre_skip = track.delay.unwrap_or(0) as usize;
            Decoder::Opus {
                decoder: Box::new(decoder),
                skip: pre_skip * SAMPLE_RATE as usize / OPUS_RATE as usize,
            }
        } else {
            Decoder::Symphonia(
                symphonia::default::get_codecs()
                    .make_audio_decoder(params, &AudioDecoderOptions::default())
                    .map_err(unsupported)?,
            )
        };
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

    /// 解一包，样本放进 `interleaved`。
    fn decode(&mut self, packet: &Packet) -> Result<Decoded> {
        match &mut self.decoder {
            Decoder::Symphonia(decoder) => match decoder.decode(packet) {
                Ok(buf) => {
                    let spec = buf.spec();
                    let (rate, channels) = (spec.rate(), spec.channels().count());
                    buf.copy_to_vec_interleaved(&mut self.interleaved);
                    Ok(Decoded::Samples { rate, channels })
                }
                Err(SymError::DecodeError(_) | SymError::IoError(_)) => Ok(Decoded::Bad),
                Err(SymError::ResetRequired) => Ok(Decoded::Reset),
                Err(e) => Err(unsupported(e)),
            },
            Decoder::Opus { decoder, skip } => {
                self.interleaved
                    .resize(decoder.max_frame_size_per_channel(), 0.0);
                // 播客的音频是外来的：解码器遇到意外的包要是出了错（它不用 unsafe，出错的方式是 panic），
                // 只当这一包坏了，把解码器重置了接着往下，不让整个转写任务停掉
                let out = &mut self.interleaved;
                let decoded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    decoder.decode_float(&packet.data, out, false)
                }));
                let decoded = match decoded {
                    Ok(result) => result,
                    Err(_) => {
                        decoder.reset();
                        return Ok(Decoded::Bad);
                    }
                };
                match decoded {
                    Ok(n) => {
                        self.interleaved.truncate(n);
                        let dropped = (*skip).min(n);
                        self.interleaved.drain(..dropped);
                        *skip -= dropped;
                        Ok(Decoded::Samples {
                            rate: SAMPLE_RATE,
                            channels: 1,
                        })
                    }
                    Err(_) => Ok(Decoded::Bad),
                }
            }
        }
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
                // 来源自己出的错（比如下载失败、被取消）原样交出去
                Err(SymError::IoError(e)) => return Err(Error::Io(e)),
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
            let (rate, channels) = match self.decode(&packet)? {
                Decoded::Samples { rate, channels } => (rate, channels),
                // 个别坏包跳过去
                Decoded::Bad => {
                    bad += 1;
                    if bad > MAX_BAD_PACKETS {
                        return Err(Error::Audio("too many undecodable packets".into()));
                    }
                    continue;
                }
                Decoded::Reset => {
                    self.decoder.reset();
                    continue;
                }
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
