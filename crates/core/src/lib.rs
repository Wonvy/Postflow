//! Postflow 的核心：把存储、抓取和调度组装起来，对外只暴露一个 `rpc` 入口和一个事件流。
//!
//! 桌面外壳通过进程内调用使用它，网页访问通过 HTTP 使用它，两边走的是同一套方法。

mod mail;
mod mail_rpc;
mod refresh;
mod rpc;
mod secrets;
mod translate_rpc;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use postflow_feed::Client;
use postflow_store::{Store, ids};
use postflow_translate::Translator;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

pub use rpc::RpcError;
pub use secrets::{Keychain, MemorySecrets, Secrets};

/// 没有单独设置时，每个订阅源多久刷新一次。
pub(crate) const DEFAULT_INTERVAL_MIN: i64 = 30;
/// 数据目录有变化后等这么久再导入，让同步工具把一批文件写完。
const WATCH_DEBOUNCE: Duration = Duration::from_millis(1500);
/// 调度循环的节拍：检查到期的订阅源，并兜底扫描一次数据目录。
const TICK: Duration = Duration::from_secs(60);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Feed(#[from] postflow_feed::Error),
    #[error("{0}")]
    Translate(#[from] postflow_translate::Error),
}

/// 推给界面的事件。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// 数据变了，界面应当重新查询对应范围。
    Changed {
        scope: Vec<Scope>,
        /// 引发这次变化的客户端。发起方自己已经做了乐观更新，可以忽略。
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
    /// 正在刷新的订阅源数量，0 表示空闲。
    Refresh { active: usize },
    /// 配置状态变了，例如首次设置完成。
    Status,
    /// 翻译模型的下载进度。
    Model {
        pair: String,
        received: u64,
        total: u64,
        state: ModelState,
        /// 失败时的错误码。
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelState {
    Downloading,
    Installed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Feeds,
    Items,
    Settings,
    Devices,
}

const ALL_SCOPES: [Scope; 4] = [Scope::Feeds, Scope::Items, Scope::Settings, Scope::Devices];

/// 只存在本机的配置：数据目录在哪、这台设备是谁。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    data_dir: PathBuf,
    device_id: String,
}

/// 一次打开的数据目录，连同围绕它跑的后台任务。
pub(crate) struct Session {
    pub store: Arc<Store>,
    pub data_dir: PathBuf,
    pub device_id: String,
    /// 正在抓取的订阅源，避免同一个源被并发抓两次。
    pub in_flight: Mutex<HashSet<String>>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    watcher: Mutex<Option<RecommendedWatcher>>,
    /// 每个邮箱账户一个后台任务。
    workers: Mutex<HashMap<String, mail::Worker>>,
}

impl Session {
    /// 在阻塞线程池里跑一次数据库操作，不卡住异步运行时。
    pub async fn db<T, F>(&self, f: F) -> Result<T, RpcError>
    where
        F: FnOnce(&Store) -> postflow_store::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || f(&store))
            .await
            .map_err(|e| RpcError::new("internal", e.to_string()))?
            .map_err(RpcError::from)
    }

    pub(crate) fn workers(&self) -> MutexGuard<'_, HashMap<String, mail::Worker>> {
        self.workers.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn stop(&self) {
        // 丢掉 Worker 就会停掉它的任务
        self.workers().clear();
        self.watcher
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        for task in self
            .tasks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain(..)
        {
            task.abort();
        }
    }
}

pub struct Core {
    local_dir: PathBuf,
    pub(crate) client: Client,
    session: RwLock<Option<Arc<Session>>>,
    /// 上次打开数据目录失败的原因，展示在设置引导页上。
    last_error: Mutex<Option<String>>,
    events: broadcast::Sender<Event>,
    pub(crate) refreshing: AtomicUsize,
    pub(crate) secrets: Arc<dyn Secrets>,
    /// 离线翻译，模型放在 `local_dir/translation` 下。
    pub(crate) translator: Arc<Translator>,
}

impl Core {
    /// `local_dir` 是本机私有目录，放索引和本机配置，不参与同步。邮箱密码存在系统钥匙串里。
    pub async fn open(local_dir: impl Into<PathBuf>) -> Result<Arc<Core>, Error> {
        Self::open_with_secrets(local_dir, Arc::new(Keychain)).await
    }

    /// 和 `open` 一样，但密码存到指定的地方。测试用。
    pub async fn open_with_secrets(
        local_dir: impl Into<PathBuf>,
        secrets: Arc<dyn Secrets>,
    ) -> Result<Arc<Core>, Error> {
        let local_dir = local_dir.into();
        std::fs::create_dir_all(&local_dir)?;
        let (events, _) = broadcast::channel(256);
        let translator = Arc::new(Translator::new(local_dir.join("translation"))?);
        let core = Arc::new(Core {
            translator,
            local_dir,
            client: Client::new()?,
            session: RwLock::new(None),
            last_error: Mutex::new(None),
            events,
            refreshing: AtomicUsize::new(0),
            secrets,
        });
        if let Some(config) = core.read_config()
            && let Err(e) = core.start_session(&config).await
        {
            tracing::warn!("could not open data directory: {}", e.message);
            core.set_last_error(Some(e.message));
        }
        Ok(core)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    pub(crate) fn emit(&self, event: Event) {
        // 没有订阅者时发送会失败，属于正常情况
        let _ = self.events.send(event);
    }

    pub(crate) fn emit_changed(&self, scope: &[Scope], source: Option<String>) {
        self.emit(Event::Changed {
            scope: scope.to_vec(),
            source,
        });
    }

    pub(crate) fn session(&self) -> Option<Arc<Session>> {
        self.session
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn set_last_error(&self, error: Option<String>) {
        *self.last_error.lock().unwrap_or_else(|e| e.into_inner()) = error;
    }

    pub(crate) fn last_error(&self) -> Option<String> {
        self.last_error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn config_path(&self) -> PathBuf {
        self.local_dir.join("config.json")
    }

    fn read_config(&self) -> Option<Config> {
        let bytes = std::fs::read(self.config_path()).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// 首次设置，或换一个数据目录。
    pub(crate) async fn setup(
        self: &Arc<Self>,
        data_dir: &str,
        device_name: &str,
    ) -> Result<(), RpcError> {
        let data_dir = expand_home(data_dir.trim());
        if !data_dir.is_absolute() {
            return Err(RpcError::new(
                "bad_params",
                "data directory must be an absolute path",
            ));
        }
        // 设备 ID 一旦生成就不再变，换数据目录也沿用
        let device_id = self
            .read_config()
            .map(|c| c.device_id)
            .unwrap_or_else(ids::new_device_id);
        let config = Config {
            data_dir,
            device_id,
        };

        let previous = self
            .session
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(previous) = previous {
            previous.stop();
        }
        let session = self.start_session(&config).await?;
        let body = serde_json::to_vec_pretty(&config)
            .map_err(|e| RpcError::new("internal", e.to_string()))?;
        std::fs::write(self.config_path(), body).map_err(|e| RpcError::new("io", e.to_string()))?;
        self.set_last_error(None);

        let name = device_name.trim().to_string();
        if !name.is_empty() {
            session.db(move |s| s.set_device_name(&name)).await?;
        }
        self.emit(Event::Status);
        Ok(())
    }

    async fn start_session(self: &Arc<Self>, config: &Config) -> Result<Arc<Session>, RpcError> {
        let data_dir = config.data_dir.clone();
        let index = self.local_dir.join("index.db");
        let device_id = config.device_id.clone();
        let store = {
            let (data_dir, device_id) = (data_dir.clone(), device_id.clone());
            tokio::task::spawn_blocking(move || Store::open(&data_dir, &index, &device_id))
                .await
                .map_err(|e| RpcError::new("internal", e.to_string()))??
        };
        let session = Arc::new(Session {
            store: Arc::new(store),
            data_dir: data_dir.clone(),
            device_id,
            in_flight: Mutex::new(HashSet::new()),
            tasks: Mutex::new(Vec::new()),
            watcher: Mutex::new(None),
            workers: Mutex::new(HashMap::new()),
        });

        // 监听数据目录：同步工具写入其他设备的日志时，尽快把它们并进来
        let (tx, rx) = mpsc::unbounded_channel::<()>();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                // 新设备第一次同步过来时，先出现它的目录，再出现里面的文件。Linux 上新目录的监听
                // 是收到目录事件后才补上的，紧接着写进去的文件可能没有事件，所以新目录也算变化。
                let new_dir = matches!(event.kind, notify::EventKind::Create(_))
                    && event.paths.iter().any(|p| p.is_dir());
                let relevant = new_dir
                    || event
                        .paths
                        .iter()
                        .any(|p| p.extension().and_then(|e| e.to_str()) == Some("jsonl"));
                if relevant {
                    let _ = tx.send(());
                }
            }
        })
        .and_then(|mut w| w.watch(&data_dir, RecursiveMode::Recursive).map(|_| w));
        match watcher {
            Ok(w) => *session.watcher.lock().unwrap_or_else(|e| e.into_inner()) = Some(w),
            // 监听不上不致命，调度循环每分钟还会兜底扫描一次
            Err(e) => tracing::warn!("cannot watch data directory: {e}"),
        }

        let tasks = vec![
            tokio::spawn(run_watcher(self.clone(), session.clone(), rx)),
            tokio::spawn(run_scheduler(self.clone(), session.clone())),
        ];
        *session.tasks.lock().unwrap_or_else(|e| e.into_inner()) = tasks;
        *self.session.write().unwrap_or_else(|e| e.into_inner()) = Some(session.clone());
        self.reconcile_workers(&session).await;
        Ok(session)
    }

    /// 导入其他设备写入的内容，有变化就通知界面。
    pub(crate) async fn import_external(self: &Arc<Self>, session: &Arc<Session>) {
        match session.db(|s| s.import_all()).await {
            Ok(stats) if stats.changed() => {
                // 可能是别的设备加了或删了邮箱
                self.reconcile_workers(session).await;
                self.emit_changed(&ALL_SCOPES, None);
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("import failed: {}", e.message),
        }
    }

    /// 停掉后台任务。进程退出前调用。
    pub fn shutdown(&self) {
        if let Some(session) = self
            .session
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            session.stop();
        }
    }
}

async fn run_watcher(core: Arc<Core>, session: Arc<Session>, mut rx: mpsc::UnboundedReceiver<()>) {
    while rx.recv().await.is_some() {
        tokio::time::sleep(WATCH_DEBOUNCE).await;
        while rx.try_recv().is_ok() {}
        core.import_external(&session).await;
    }
}

async fn run_scheduler(core: Arc<Core>, session: Arc<Session>) {
    let mut tick = tokio::time::interval(TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        core.import_external(&session).await;
        let now = refresh::now();
        match session.db(move |s| s.due_feeds(now)).await {
            Ok(due) if !due.is_empty() => core.refresh(&session, due).await,
            Ok(_) => {}
            Err(e) => tracing::warn!("scheduler: {}", e.message),
        }
    }
}

pub(crate) fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\"))
        && let Some(home) = home_dir()
    {
        return home.join(rest);
    }
    Path::new(path).to_path_buf()
}
