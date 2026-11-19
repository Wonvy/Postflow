//! 朗读用的本机声音：文字转语音模型（Kokoro）放在 `local_dir/voices`，和识别模型一样按需下载、不同步，
//! 每台设备自己决定装哪个。
//!
//! 界面一句一句地要声音（`voices.speak`），核心念出来交回一段 WAV（Base64）。界面边放这一句边要后面两句，
//! 所以念得比放得快就不会停顿（M4 上大约 4 倍于实时）。网页访问时也是这台电脑念、浏览器放。
//!
//! 模型加载要一秒左右、占几百 MB 内存：加载一次留着，十分钟没用就放掉。同一时间只念一句，
//! 免得几句抢 CPU、谁都念不完。

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use postflow_speech::{Speech, Voice, tts};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::rpc::{RpcError, parse, reply};
use crate::{Core, Event, ModelState};

/// 闲了这么久就把模型从内存里放掉。
const IDLE: Duration = Duration::from_secs(10 * 60);
/// 一次最多念这么多字。界面按句子切，正常远到不了。
const MAX_TEXT: usize = 2000;

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
}

impl Voices {
    pub(crate) fn new(root: std::path::PathBuf) -> Result<Voices, postflow_speech::Error> {
        Ok(Voices {
            models: Arc::new(Speech::voices(root)?),
            loaded: Arc::new(Mutex::new(None)),
            speaking: Arc::new(Mutex::new(())),
        })
    }

    fn loaded(&self) -> MutexGuard<'_, Option<Loaded>> {
        self.loaded.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 放掉加载着的模型（删模型前、或者指定的就是它时）。
    fn unload(&self, id: Option<&str>) {
        let mut loaded = self.loaded();
        if loaded
            .as_ref()
            .is_some_and(|l| id.is_none_or(|id| l.id == id))
        {
            *loaded = None;
        }
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
struct SpeakParams {
    /// 哪个模型。
    model: String,
    /// 模型里的哪种声音（目录里 `speakers` 的一项）。
    voice: String,
    text: String,
    #[serde(default = "normal_speed")]
    speed: f32,
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
                self.voices.unload(Some(&p.id));
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
                self.voices.unload(Some(&p.id));
                self.voices.models.import(&p.id, &folder).await?;
                reply(self.voices.models.overview())
            }
            "voices.speak" => {
                let p: SpeakParams = parse(params)?;
                if p.text.chars().count() > MAX_TEXT {
                    return Err(RpcError::new("bad_params", "the text is too long"));
                }
                let voice = self.voice(&p.model).await?;
                let speaking = self.voices.speaking.clone();
                let (samples, rate) = tokio::task::spawn_blocking(move || {
                    let _turn = speaking.lock().unwrap_or_else(|e| e.into_inner());
                    voice
                        .speak(&p.text, &p.voice, p.speed)
                        .map(|s| (s, voice.sample_rate()))
                })
                .await
                .map_err(|e| RpcError::new("internal", e.to_string()))??;
                let duration = samples.len() as f64 / f64::from(rate);
                let audio = if samples.is_empty() {
                    String::new()
                } else {
                    postflow_mail::base64(&tts::wav(&samples, rate))
                };
                reply(json!({ "audio": audio, "duration": duration }))
            }
            _ => Err(RpcError::new("unknown_method", method.to_string())),
        }
    }

    /// 要用的模型：加载着的直接用，没有就加载（换了模型就换掉原来那个）。
    async fn voice(self: &Arc<Self>, id: &str) -> Result<Arc<Voice>, RpcError> {
        // 都放在阻塞线程里：加载时拿着锁要一秒，不能卡住异步线程
        let loaded = self.voices.loaded.clone();
        let models = self.voices.models.clone();
        let id = id.to_string();
        let voice = tokio::task::spawn_blocking(move || -> Result<Arc<Voice>, RpcError> {
            // 加载的时候也拿着锁：界面一下子要好几句时只加载一次
            let mut slot = loaded.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(l) = slot.as_mut().filter(|l| l.id == id) {
                l.used = Instant::now();
                return Ok(l.voice.clone());
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
            Ok(voice)
        })
        .await
        .map_err(|e| RpcError::new("internal", e.to_string()))??;
        self.watch_idle_voice();
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
                let mut slot = loaded.lock().unwrap_or_else(|e| e.into_inner());
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
            core.voices.unload(Some(&id));
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
