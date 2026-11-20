//! 朗读用的本机声音：文字转语音模型（Kokoro）放在 `local_dir/voices`，和识别模型一样按需下载、不同步，
//! 每台设备自己决定装哪个。
//!
//! 界面一句一句地要声音（`voices.speak`），核心念出来交回一段 WAV（Base64）。界面边放这一句边要后面两句，
//! 所以念得比放得快就不会停顿（M4 上大约 4 倍于实时）。网页访问时也是这台电脑念、浏览器放。
//!
//! 模型加载要一秒左右、占几百 MB 内存：加载一次留着，十分钟没用就放掉。同一时间只念一句，
//! 免得几句抢 CPU、谁都念不完。
//!
//! ZipVoice 没有自带的声音，照着用户给的一段录音（声音样本）念。样本放在 `local_dir/voices/samples`：
//! `<ID>.wav`（24 kHz 单声道，最长 12 秒）和 `<ID>.json`（名字、录音里说的话）。录音里说的话不填的话，
//! 装了语音识别模型就自动识别出来，用户再改。

use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use postflow_speech::{AudioReader, Speaker, Speakers, Speech, Voice, tts};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::rpc::{RpcError, parse, reply};
use crate::{Core, Event, ModelState};

/// 闲了这么久就把模型从内存里放掉。
const IDLE: Duration = Duration::from_secs(10 * 60);
/// 一次最多念这么多字。界面按句子切，正常远到不了。
const MAX_TEXT: usize = 2000;
/// 声音样本至少要这么长（秒）：太短学不出音色。
const SAMPLE_MIN_SECONDS: f64 = 2.0;
/// 上传的录音文件最大多少字节（只取开头十几秒，但整个文件要传过来）。
const SAMPLE_MAX_BYTES: usize = 30 * 1024 * 1024;
/// 界面说不要了的请求最多记这么多个（念完的请求不会从这里删掉，记太多就整个清掉）。
const MAX_CANCELLED: usize = 512;

/// 一段声音样本。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Sample {
    pub id: String,
    pub name: String,
    /// 录音里说的话，要和录音一字不差。
    pub text: String,
    pub seconds: f64,
    pub created_at: i64,
}

struct Loaded {
    id: String,
    voice: Arc<Voice>,
    used: Instant,
}

pub(crate) struct Voices {
    pub(crate) models: Arc<Speech>,
    loaded: Arc<Mutex<Option<Loaded>>>,
    /// 念的时候拿着，一次只念一句。
    speaking: Arc<Mutex<()>>,
    /// 界面不要了的句子（跳过了、换了一篇、停了）：还排着队的轮到时直接跳过，不白念。
    cancelled: Arc<Mutex<HashSet<String>>>,
    samples_dir: PathBuf,
    /// 读过的声音样本：ID → 录音。念每一句都要用，不必每次读文件。
    clips: Mutex<HashMap<String, Arc<Vec<f32>>>>,
}

impl Voices {
    pub(crate) fn new(root: PathBuf) -> Result<Voices, postflow_speech::Error> {
        Ok(Voices {
            samples_dir: root.join("samples"),
            models: Arc::new(Speech::voices(root)?),
            loaded: Arc::new(Mutex::new(None)),
            speaking: Arc::new(Mutex::new(())),
            cancelled: Arc::new(Mutex::new(HashSet::new())),
            clips: Mutex::new(HashMap::new()),
        })
    }

    fn clips(&self) -> MutexGuard<'_, HashMap<String, Arc<Vec<f32>>>> {
        self.clips.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// ID 是我们自己生成的 12 位十六进制，别的一律不认（会拿来拼文件名）。
    fn sample_path(&self, id: &str, ext: &str) -> Option<PathBuf> {
        (id.len() == 12 && id.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| self.samples_dir.join(format!("{id}.{ext}")))
    }

    pub(crate) fn samples(&self) -> Vec<Sample> {
        let Ok(entries) = std::fs::read_dir(&self.samples_dir) else {
            return Vec::new();
        };
        let mut list: Vec<Sample> = entries
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok())
            .filter(|s: &Sample| self.sample_path(&s.id, "wav").is_some_and(|p| p.is_file()))
            .collect();
        list.sort_by_key(|s| s.created_at);
        list
    }

    fn sample(&self, id: &str) -> Option<Sample> {
        let path = self.sample_path(id, "json")?;
        serde_json::from_slice(&std::fs::read(path).ok()?).ok()
    }

    fn save_sample(&self, sample: &Sample) -> std::io::Result<()> {
        let path = self
            .sample_path(&sample.id, "json")
            .ok_or_else(|| std::io::Error::other("bad sample id"))?;
        let json = serde_json::to_vec_pretty(sample).map_err(std::io::Error::other)?;
        write_atomic(&path, &json)
    }

    /// 念的时候用的录音：读过的直接给。
    fn clip(&self, id: &str) -> Result<Arc<Vec<f32>>, RpcError> {
        if let Some(clip) = self.clips().get(id) {
            return Ok(clip.clone());
        }
        let path = self
            .sample_path(id, "wav")
            .filter(|p| p.is_file())
            .ok_or_else(|| RpcError::new("voice_sample_missing", "no such voice sample"))?;
        let reader = AudioReader::open(&path)?;
        let clip = Arc::new(tts::read_sample(reader)?);
        self.clips().insert(id.to_string(), clip.clone());
        Ok(clip)
    }

    /// 放掉加载着的模型（删模型前、或者指定的就是它时）。加载模型时一直拿着锁（要一两秒），
    /// 所以在阻塞线程里做，不卡异步线程。
    async fn unload(&self, id: Option<&str>) {
        let loaded = self.loaded.clone();
        let id = id.map(str::to_string);
        let _ = tokio::task::spawn_blocking(move || {
            let mut loaded = loaded.lock().unwrap_or_else(|e| e.into_inner());
            if loaded
                .as_ref()
                .is_some_and(|l| id.as_deref().is_none_or(|id| l.id == id))
            {
                *loaded = None;
            }
        })
        .await;
    }

    /// 这个请求界面已经不要了（顺便从记录里拿掉）。
    fn take_cancelled(cancelled: &Mutex<HashSet<String>>, request: Option<&str>) -> bool {
        request.is_some_and(|r| {
            cancelled
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(r)
        })
    }
}

#[derive(Deserialize)]
struct IdParam {
    id: String,
}

#[derive(Deserialize)]
struct ImportParams {
    id: String,
    path: String,
}

#[derive(Deserialize)]
struct AddSampleParams {
    name: String,
    /// 录音文件，Base64。
    audio: String,
    /// 文件扩展名，帮着认格式。
    #[serde(default)]
    ext: Option<String>,
    /// 录音里说的话。不给就试着识别。
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct UpdateSampleParams {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct CancelParams {
    requests: Vec<String>,
}

#[derive(Deserialize)]
struct SpeakParams {
    /// 哪个模型。
    model: String,
    /// 模型里的哪种声音（目录里 `speakers` 的一项）；ZipVoice 是声音样本的 ID。
    voice: String,
    text: String,
    #[serde(default = "normal_speed")]
    speed: f32,
    /// 界面给这次请求起的名字：之后可以用 `voices.cancel` 说不要了。
    #[serde(default)]
    request: Option<String>,
}

fn cancelled_error() -> RpcError {
    RpcError::new("voice_cancelled", "the sentence is no longer needed")
}

fn normal_speed() -> f32 {
    1.0
}

impl Core {
    /// 朗读模型：列出、下载、删除、从文件夹导入，以及念一句。不需要打开数据目录。
    pub(crate) async fn voices_rpc(
        self: &Arc<Self>,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        match method {
            "voices.overview" => {
                if self.voices.models.catalog_due() {
                    let models = self.voices.models.clone();
                    tokio::spawn(async move {
                        if models.refresh_catalog().await {
                            tracing::info!("voice model catalog updated");
                        }
                    });
                }
                reply(self.voices.models.overview())
            }
            "voices.download" => {
                let p: IdParam = parse(params)?;
                if self.voices.models.begin_download(&p.id)? {
                    tokio::spawn(download_model(self.clone(), p.id));
                }
                reply(json!({}))
            }
            "voices.remove" => {
                let p: IdParam = parse(params)?;
                self.voices.unload(Some(&p.id)).await;
                let models = self.voices.models.clone();
                tokio::task::spawn_blocking(move || models.remove(&p.id))
                    .await
                    .map_err(|e| RpcError::new("internal", e.to_string()))??;
                reply(self.voices.models.overview())
            }
            "voices.import" => {
                let p: ImportParams = parse(params)?;
                let folder = crate::expand_home(p.path.trim());
                if !folder.is_dir() {
                    return Err(RpcError::new("speech_import_missing", "no such folder"));
                }
                self.voices.unload(Some(&p.id)).await;
                self.voices.models.import(&p.id, &folder).await?;
                reply(self.voices.models.overview())
            }
            "voices.cancel" => {
                let p: CancelParams = parse(params)?;
                let mut cancelled = self
                    .voices
                    .cancelled
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                if cancelled.len() + p.requests.len() > MAX_CANCELLED {
                    cancelled.clear();
                }
                cancelled.extend(p.requests.into_iter().take(MAX_CANCELLED));
                reply(json!({}))
            }
            "voices.speak" => {
                let p: SpeakParams = parse(params)?;
                if p.text.chars().count() > MAX_TEXT {
                    return Err(RpcError::new("bad_params", "the text is too long"));
                }
                if Voices::take_cancelled(&self.voices.cancelled, p.request.as_deref()) {
                    return Err(cancelled_error());
                }
                let voice = self.voice(&p.model).await?;
                // ZipVoice 照着声音样本念：录音和录音里说的话
                let sample = if voice.needs_sample() {
                    let info = self.voices.sample(&p.voice).ok_or_else(|| {
                        RpcError::new("voice_sample_missing", "no such voice sample")
                    })?;
                    if info.text.trim().is_empty() {
                        return Err(RpcError::new(
                            "voice_sample_text",
                            "the voice sample has no transcript",
                        ));
                    }
                    let voices = self.clone();
                    let id = p.voice.clone();
                    let clip = tokio::task::spawn_blocking(move || voices.voices.clip(&id))
                        .await
                        .map_err(|e| RpcError::new("internal", e.to_string()))??;
                    Some((clip, info.text))
                } else {
                    None
                };
                let speaking = self.voices.speaking.clone();
                let cancelled = self.voices.cancelled.clone();
                let (spoken, rate) = tokio::task::spawn_blocking(move || {
                    let _turn = speaking.lock().unwrap_or_else(|e| e.into_inner());
                    // 排队等着的时候界面已经不要了（用户跳过了这几句）：不念，让后面要的那句早点轮到
                    if Voices::take_cancelled(&cancelled, p.request.as_deref()) {
                        return Err(cancelled_error());
                    }
                    let who = match &sample {
                        Some((clip, text)) => Speaker::Sample { audio: clip, text },
                        None => Speaker::Preset(&p.voice),
                    };
                    voice
                        .speak(&p.text, who, p.speed)
                        .map(|s| (s, voice.sample_rate()))
                        .map_err(RpcError::from)
                })
                .await
                .map_err(|e| RpcError::new("internal", e.to_string()))??;
                let duration = spoken.samples.len() as f64 / f64::from(rate);
                let audio = if spoken.samples.is_empty() {
                    String::new()
                } else {
                    postflow_mail::base64(&tts::wav(&spoken.samples, rate))
                };
                // speed：实际按什么语速念的，比要的慢时界面放快一些
                reply(json!({ "audio": audio, "duration": duration, "speed": spoken.speed }))
            }
            "voices.samples" => reply(json!({ "samples": self.voices.samples() })),
            "voices.samples.add" => {
                let p: AddSampleParams = parse(params)?;
                let bytes = postflow_mail::base64_decode(&p.audio)
                    .ok_or_else(|| RpcError::new("bad_params", "the audio is not Base64"))?;
                if bytes.len() > SAMPLE_MAX_BYTES {
                    return Err(RpcError::new(
                        "voice_sample_large",
                        "the recording is too large",
                    ));
                }
                let core = self.clone();
                let sample = tokio::task::spawn_blocking(move || core.add_sample(p, bytes))
                    .await
                    .map_err(|e| RpcError::new("internal", e.to_string()))??;
                reply(sample)
            }
            "voices.samples.update" => {
                let p: UpdateSampleParams = parse(params)?;
                let mut sample = self
                    .voices
                    .sample(&p.id)
                    .ok_or_else(|| RpcError::new("voice_sample_missing", "no such voice sample"))?;
                if let Some(name) = p
                    .name
                    .map(|n| n.trim().to_string())
                    .filter(|n| !n.is_empty())
                {
                    sample.name = name;
                }
                if let Some(text) = p.text {
                    sample.text = text.trim().to_string();
                }
                self.voices
                    .save_sample(&sample)
                    .map_err(|e| RpcError::new("io", e.to_string()))?;
                reply(sample)
            }
            "voices.samples.remove" => {
                let p: IdParam = parse(params)?;
                for ext in ["wav", "json"] {
                    if let Some(path) = self.voices.sample_path(&p.id, ext) {
                        let _ = std::fs::remove_file(path);
                    }
                }
                self.voices.clips().remove(&p.id);
                reply(json!({ "samples": self.voices.samples() }))
            }
            // 原来的录音，试听用
            "voices.samples.audio" => {
                let p: IdParam = parse(params)?;
                let path = self
                    .voices
                    .sample_path(&p.id, "wav")
                    .filter(|p| p.is_file())
                    .ok_or_else(|| RpcError::new("voice_sample_missing", "no such voice sample"))?;
                let bytes = std::fs::read(path).map_err(|e| RpcError::new("io", e.to_string()))?;
                reply(json!({ "audio": postflow_mail::base64(&bytes) }))
            }
            _ => Err(RpcError::new("unknown_method", method.to_string())),
        }
    }

    /// 存一段声音样本：解码、去掉前后的静音、只留开头 12 秒，存成 24 kHz 的 WAV；没给文字就试着识别。
    fn add_sample(&self, p: AddSampleParams, bytes: Vec<u8>) -> Result<Sample, RpcError> {
        let ext = p
            .ext
            .as_deref()
            .map(|e| e.trim_start_matches('.').to_ascii_lowercase());
        let reader = AudioReader::from_source(Box::new(Cursor::new(bytes)), ext.as_deref())
            .map_err(|_| RpcError::new("voice_sample_unreadable", "cannot read the recording"))?;
        let clip = tts::read_sample(reader).map_err(|e| match e {
            postflow_speech::Error::Silent => {
                RpcError::new("voice_sample_silent", "the recording is silent")
            }
            _ => RpcError::new("voice_sample_unreadable", "cannot read the recording"),
        })?;
        let seconds = clip.len() as f64 / f64::from(tts::SAMPLE_RATE);
        if seconds < SAMPLE_MIN_SECONDS {
            return Err(RpcError::new(
                "voice_sample_short",
                "the recording is too short",
            ));
        }
        std::fs::create_dir_all(&self.voices.samples_dir)
            .map_err(|e| RpcError::new("io", e.to_string()))?;
        let id = postflow_store::ids::new_device_id();
        let wav = self
            .voices
            .sample_path(&id, "wav")
            .ok_or_else(|| RpcError::new("internal", "bad sample id"))?;
        write_atomic(&wav, &tts::wav(&clip, tts::SAMPLE_RATE))
            .map_err(|e| RpcError::new("io", e.to_string()))?;
        let text = match p
            .text
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
        {
            Some(text) => text,
            None => transcribe(&self.speech, &wav).unwrap_or_default(),
        };
        let name = p.name.trim();
        let sample = Sample {
            id,
            name: if name.is_empty() {
                "Voice".into()
            } else {
                name.chars().take(60).collect()
            },
            text,
            seconds: (seconds * 10.0).round() / 10.0,
            created_at: crate::refresh::now(),
        };
        self.voices
            .save_sample(&sample)
            .map_err(|e| RpcError::new("io", e.to_string()))?;
        self.voices
            .clips()
            .insert(sample.id.clone(), Arc::new(clip));
        Ok(sample)
    }

    /// 要用的模型：加载着的直接用，没有就加载（换了模型就换掉原来那个）。
    async fn voice(self: &Arc<Self>, id: &str) -> Result<Arc<Voice>, RpcError> {
        // 都放在阻塞线程里：加载时拿着锁要一秒，不能卡住异步线程
        let loaded = self.voices.loaded.clone();
        let models = self.voices.models.clone();
        let id = id.to_string();
        let (voice, fresh) = tokio::task::spawn_blocking(move || -> Result<(Arc<Voice>, bool), RpcError> {
            // 加载的时候也拿着锁：界面一下子要好几句时只加载一次
            let mut slot = loaded.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(l) = slot.as_mut().filter(|l| l.id == id) {
                l.used = Instant::now();
                return Ok((l.voice.clone(), false));
            }
            *slot = None;
            let started = Instant::now();
            let voice = Arc::new(models.load_voice(&id)?);
            tracing::info!(model = %id, seconds = started.elapsed().as_secs_f64(), "voice model loaded");
            *slot = Some(Loaded {
                id: id.clone(),
                voice: voice.clone(),
                used: Instant::now(),
            });
            Ok((voice, true))
        })
        .await
        .map_err(|e| RpcError::new("internal", e.to_string()))??;
        // 刚加载的才派一个看着它的任务（以前每念一句派一个，连听一小时攒下几百个）
        if fresh {
            self.watch_idle_voice();
        }
        Ok(voice)
    }

    /// 每分钟看一次：加载着的模型闲了 `IDLE` 就放掉。模型放掉（或者换了）以后这个任务就结束。
    fn watch_idle_voice(self: &Arc<Self>) {
        let loaded = self.voices.loaded.clone();
        let Some(id) = loaded
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|l| l.id.clone())
        else {
            return;
        };
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                // 正在加载模型时锁要拿一两秒：不在异步线程上等，下一分钟再看
                let mut slot = match loaded.try_lock() {
                    Ok(slot) => slot,
                    Err(std::sync::TryLockError::Poisoned(e)) => e.into_inner(),
                    Err(std::sync::TryLockError::WouldBlock) => continue,
                };
                match slot.as_ref() {
                    Some(l) if l.id == id => {
                        if l.used.elapsed() >= IDLE {
                            tracing::info!(model = %id, "voice model unloaded after being idle");
                            *slot = None;
                            return;
                        }
                    }
                    _ => return,
                }
            }
        });
    }
}

async fn download_model(core: Arc<Core>, id: String) {
    tracing::info!(model = %id, "downloading voice model");
    let result = core
        .voices
        .models
        .download(&id, |p| {
            core.emit(Event::VoiceModel {
                id: p.id.clone(),
                received: p.received,
                total: p.total,
                state: ModelState::Downloading,
                error: None,
            })
        })
        .await;
    let (state, error) = match result {
        Ok(_) => {
            tracing::info!(model = %id, "voice model installed");
            // 重新装的：内存里那份旧的放掉
            core.voices.unload(Some(&id)).await;
            (ModelState::Installed, None)
        }
        Err(e) => {
            tracing::warn!("voice model {id}: {e}");
            (ModelState::Failed, Some(e.code().to_string()))
        }
    };
    core.emit(Event::VoiceModel {
        id,
        received: 0,
        total: 0,
        state,
        error,
    });
}

/// 先写到旁边的临时文件再改名，写到一半断电也不会留下坏文件。
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// 用装着的语音识别模型把声音样本识别成文字。没装、识别不出来时是空的，让用户自己填。
fn transcribe(speech: &Speech, wav: &Path) -> Option<String> {
    let id = speech.selected()?;
    let engine = speech.load(&id).ok()?;
    let mut reader = AudioReader::open(wav).ok()?;
    let mut speakers = Speakers::default();
    let mut text = String::new();
    engine
        .run(&mut reader, f64::INFINITY, &mut speakers, &mut |step| {
            for cue in step.cues {
                text.push_str(cue.text.trim());
            }
            true
        })
        .ok()?;
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}
