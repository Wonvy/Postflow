//! 语音转文字：给没有文字稿的播客生成字幕。
//!
//! 识别在本机完成（sherpa-onnx），音频不会离开这台电脑。模型不随安装包带，用户要用的时候再下载，
//! 放在本机私有目录里，不参与同步：每台设备自己决定装哪个。可以装好几个，随时换。
//!
//! 目录结构（`root` 是 `本机目录/speech`）：
//!
//! ```text
//! speech/
//!   catalog.json            网上取回来的模型目录
//!   state.json              现在用哪个模型
//!   silero_vad.onnx         找出哪里有人说话的小模型（随程序带的，第一次用时写出来）
//!   models/<模型 ID>/        model.json（装的时候目录里的那一条）和模型文件
//!   transcripts/<条目 ID>.json  转写出来的字幕
//! ```

pub mod audio;
pub mod catalog;
pub mod cues;
pub mod engine;
pub mod speakers;
mod store;

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub use audio::AudioReader;
pub use catalog::{Catalog, Model, RemoteFile, TASK_ASR, TASK_SPEAKER};
pub use cues::{Covered, Cue};
pub use engine::{Engine, Outcome, Step, default_threads};
pub use speakers::Speakers;
pub use store::{Transcript, Transcripts};
/// 自己实现的音频来源（比如还在下载的文件）要实现它，交给 `AudioReader::from_source`。
pub use symphonia::core::io::MediaSource;

/// 目录缓存多久以后联网更新一次。
const CATALOG_MAX_AGE: Duration = Duration::from_secs(24 * 3600);
const CATALOG_FILE: &str = "catalog.json";
const STATE_FILE: &str = "state.json";
const MODEL_FILE: &str = "model.json";
/// 一分钟收不到数据就换下一个下载地址。
const STALL: Duration = Duration::from_secs(60);
/// 下载一个模型前，每个下载源取这么多字节测速。
const PROBE_BYTES: u64 = 1024 * 1024;
/// 测速最多等这么久。没测完的按测到的算，连不上的排到最后。
const PROBE_TIME: Duration = Duration::from_secs(8);
/// 目录再大也不会超过这么多。
const MAX_CATALOG: usize = 1024 * 1024;

/// 找出哪里有人说话的小模型（Silero VAD，MIT 许可，见 assets/README.md）。只有六百多 KB，直接打进程序，
/// 省得为它单独下载一次。运行库要的是文件路径，所以第一次用时写到本机目录里。
const VAD_MODEL: &[u8] = include_bytes!("../assets/silero_vad.onnx");
const VAD_FILE: &str = "silero_vad.onnx";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no such speech model: {0}")]
    UnknownModel(String),
    #[error("speech model {0} is not downloaded")]
    NotInstalled(String),
    #[error("this version of Postflow cannot load {0} models")]
    Unsupported(String),
    #[error("download failed: {0}")]
    Download(String),
    #[error("downloaded file {0} is corrupt")]
    Checksum(String),
    #[error("{0} was not found in that folder")]
    MissingFile(String),
    #[error("cannot load the speech model: {0}")]
    Load(String),
    #[error("cannot read the audio: {0}")]
    Audio(String),
    #[error("io: {0}")]
    Io(#[from] io::Error),
}

impl Error {
    /// 稳定的错误码，界面据此显示本地化的提示。
    pub fn code(&self) -> &'static str {
        match self {
            Error::UnknownModel(_) => "speech_unknown_model",
            Error::NotInstalled(_) => "speech_not_installed",
            Error::Unsupported(_) => "speech_unsupported",
            Error::Download(_) => "speech_download_failed",
            Error::Checksum(_) => "speech_download_failed",
            Error::MissingFile(_) => "speech_import_missing",
            Error::Load(_) => "speech_load_failed",
            Error::Audio(_) => "speech_audio_failed",
            Error::Io(_) => "io",
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// 下载进度，几个文件加在一起。
#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub id: String,
    pub received: u64,
    pub total: u64,
}

/// 设置页里的一行。
#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    #[serde(flatten)]
    pub model: Model,
    /// 要下载的大小。
    pub size: u64,
    pub installed: bool,
    /// 这个版本的程序加载不了（模型类型太新），要等更新。
    pub supported: bool,
    /// 目录里已经没有它了（本机还装着，照样能用）。
    pub retired: bool,
    /// 正在下载的话，已经下了多少。
    pub received: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Overview {
    pub models: Vec<ModelInfo>,
    /// 现在用哪个模型。一个都没装就是空。
    pub selected: Option<String>,
    /// 模型放在哪儿。
    pub dir: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    #[serde(default)]
    selected: Option<String>,
}

pub struct Speech {
    root: PathBuf,
    catalog_urls: Vec<String>,
    http: reqwest::Client,
    catalog: Mutex<Option<Arc<Catalog>>>,
    /// 这次运行里联网更新过目录了（不管成没成）。
    refreshed: AtomicBool,
    downloads: Mutex<HashMap<String, Progress>>,
    /// 下载源（`https://主机`）按上次测的速度排好，快的在前。
    sources: Mutex<Vec<String>>,
}

impl Speech {
    /// `root` 是放模型和字幕的本机目录。
    pub fn new(root: impl Into<PathBuf>) -> Result<Speech> {
        Self::with_catalog_urls(
            root,
            catalog::CATALOG_URLS
                .iter()
                .map(|s| s.to_string())
                .collect(),
        )
    }

    /// 和 `new` 一样，但从别的地址取模型目录（空的话只用内置的）。测试用。
    pub fn with_catalog_urls(root: impl Into<PathBuf>, urls: Vec<String>) -> Result<Speech> {
        let root = root.into();
        // 模型目录先建好：设置里的「打开模型文件夹」在下载第一个模型之前也能用
        std::fs::create_dir_all(root.join("models"))?;
        let http = reqwest::Client::builder()
            .user_agent(concat!("Postflow/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| Error::Download(e.to_string()))?;
        Ok(Speech {
            root,
            catalog_urls: urls,
            http,
            catalog: Mutex::new(None),
            refreshed: AtomicBool::new(false),
            downloads: Mutex::new(HashMap::new()),
            sources: Mutex::new(Vec::new()),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn transcripts(&self) -> Transcripts {
        Transcripts::new(self.root.join("transcripts"))
    }

    fn models_dir(&self) -> PathBuf {
        self.root.join("models")
    }

    fn model_dir(&self, id: &str) -> PathBuf {
        self.models_dir().join(id)
    }

    fn downloads(&self) -> MutexGuard<'_, HashMap<String, Progress>> {
        self.downloads.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn keep_catalog(&self, catalog: Catalog) -> Arc<Catalog> {
        let catalog = Arc::new(catalog);
        *self.catalog.lock().unwrap_or_else(|e| e.into_inner()) = Some(catalog.clone());
        catalog
    }

    /// 模型目录，不联网、不等待：这次运行里更新过的，否则上次存下的副本，否则随程序带的那一份。
    /// 存下的副本比随程序带的还旧（刚升级了应用）就用带的。
    pub fn catalog(&self) -> Arc<Catalog> {
        if let Some(catalog) = self
            .catalog
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            return catalog;
        }
        let builtin = Catalog::builtin();
        let cached = std::fs::read(self.root.join(CATALOG_FILE))
            .ok()
            .and_then(|b| Catalog::parse(&b).ok())
            .filter(|c| c.updated >= builtin.updated);
        self.keep_catalog(cached.unwrap_or(builtin))
    }

    /// 该联网看看目录有没有更新了：一天看一次，这次运行里看过就不再看。
    pub fn catalog_due(&self) -> bool {
        if self.catalog_urls.is_empty() || self.refreshed.load(Ordering::Relaxed) {
            return false;
        }
        !std::fs::metadata(self.root.join(CATALOG_FILE))
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age < CATALOG_MAX_AGE)
    }

    /// 联网更新模型目录。拿到了比手上更新的返回 `true`。联不上、读不懂都没关系，接着用手上的。
    pub async fn refresh_catalog(&self) -> bool {
        self.refreshed.store(true, Ordering::Relaxed);
        let current = self.catalog();
        for url in &self.catalog_urls {
            match self.fetch_catalog(url).await {
                Ok((bytes, catalog)) => {
                    // 存下来，哪怕内容没变：一天之内不用再问
                    if let Err(e) = write_atomic(&self.root.join(CATALOG_FILE), &bytes) {
                        tracing::warn!("cannot cache the speech model catalog: {e}");
                    }
                    if catalog.updated < current.updated || catalog == *current {
                        return false;
                    }
                    self.keep_catalog(catalog);
                    return true;
                }
                Err(e) => tracing::info!(%url, "speech model catalog not updated: {e}"),
            }
        }
        false
    }

    async fn fetch_catalog(&self, url: &str) -> std::result::Result<(Vec<u8>, Catalog), String> {
        let response = self
            .http
            .get(url)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| e.to_string())?;
        let bytes = response.bytes().await.map_err(|e| e.to_string())?.to_vec();
        if bytes.len() > MAX_CATALOG {
            return Err("too large".into());
        }
        let catalog = Catalog::parse(&bytes)?;
        Ok((bytes, catalog))
    }

    /// 本机装了哪些模型：读每个模型目录里的 `model.json`（装的时候目录里的那一条）。
    pub fn installed(&self) -> Vec<Model> {
        let Ok(entries) = std::fs::read_dir(self.models_dir()) else {
            return Vec::new();
        };
        let mut list: Vec<Model> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                if !catalog::safe_id(&name) {
                    return None;
                }
                let bytes = std::fs::read(e.path().join(MODEL_FILE)).ok()?;
                let mut model: Model = serde_json::from_slice(&bytes).ok()?;
                // 目录名才是真正的 ID
                model.id = name;
                Some(model)
            })
            .collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }

    pub fn installed_model(&self, id: &str) -> Option<Model> {
        if !catalog::safe_id(id) {
            return None;
        }
        let bytes = std::fs::read(self.model_dir(id).join(MODEL_FILE)).ok()?;
        let mut model: Model = serde_json::from_slice(&bytes).ok()?;
        model.id = id.to_string();
        Some(model)
    }

    fn read_state(&self) -> State {
        std::fs::read(self.root.join(STATE_FILE))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// 现在用哪个识别模型：用户选的那个（还装着的话），否则装着的第一个。
    pub fn selected(&self) -> Option<String> {
        let installed: Vec<Model> = self
            .installed()
            .into_iter()
            .filter(|m| m.task == TASK_ASR)
            .collect();
        self.read_state()
            .selected
            .filter(|id| installed.iter().any(|m| &m.id == id))
            .or_else(|| installed.first().map(|m| m.id.clone()))
    }

    /// 装着的声纹模型（区分说话人用）。
    pub fn speaker_model(&self) -> Option<Model> {
        self.installed()
            .into_iter()
            .find(|m| m.task == TASK_SPEAKER && m.supported())
    }

    pub fn select(&self, id: &str) -> Result<()> {
        if self.installed_model(id).is_none_or(|m| m.task != TASK_ASR) {
            return Err(Error::NotInstalled(id.to_string()));
        }
        let state = State {
            selected: Some(id.to_string()),
        };
        let json = serde_json::to_vec_pretty(&state).map_err(io::Error::other)?;
        write_atomic(&self.root.join(STATE_FILE), &json)?;
        Ok(())
    }

    /// 设置页要的全部信息。
    pub fn overview(&self) -> Overview {
        let catalog = self.catalog();
        let installed = self.installed();
        let downloads = self.downloads().clone();
        let info = |model: &Model, retired: bool| ModelInfo {
            size: model.size(),
            installed: installed.iter().any(|m| m.id == model.id),
            supported: model.supported(),
            retired,
            received: downloads.get(&model.id).map(|p| p.received),
            model: model.clone(),
        };
        let mut models: Vec<ModelInfo> = catalog.models.iter().map(|m| info(m, false)).collect();
        // 装着、但目录里已经撤掉的，照样列出来
        for m in &installed {
            if catalog.get(&m.id).is_none() {
                models.push(info(m, true));
            }
        }
        Overview {
            models,
            selected: self.selected(),
            dir: std::fs::canonicalize(self.models_dir())
                .unwrap_or_else(|_| self.models_dir())
                .to_string_lossy()
                .into_owned(),
        }
    }

    /// 正在下载的话返回 `false`，调用方不必再开一个。
    pub fn begin_download(&self, id: &str) -> Result<bool> {
        let catalog = self.catalog();
        let model = catalog
            .get(id)
            .ok_or_else(|| Error::UnknownModel(id.to_string()))?;
        if !model.supported() {
            return Err(Error::Unsupported(model.kind.clone()));
        }
        let mut downloads = self.downloads();
        if downloads.contains_key(id) {
            return Ok(false);
        }
        downloads.insert(
            id.to_string(),
            Progress {
                id: id.to_string(),
                received: 0,
                total: model.size(),
            },
        );
        Ok(true)
    }

    /// 下载一个模型。先调用 `begin_download` 占位；不管成功与否，结束后都会清掉占位。
    /// 下到一半失败的，已经下的部分留着，下次接着下。
    pub async fn download(
        &self,
        id: &str,
        mut on_progress: impl FnMut(&Progress),
    ) -> Result<Model> {
        let result = self.download_inner(id, &mut on_progress).await;
        self.downloads().remove(id);
        result
    }

    async fn download_inner(
        &self,
        id: &str,
        on_progress: &mut impl FnMut(&Progress),
    ) -> Result<Model> {
        let catalog = self.catalog();
        let model = catalog
            .get(id)
            .ok_or_else(|| Error::UnknownModel(id.to_string()))?
            .clone();
        let mut progress = Progress {
            id: id.to_string(),
            received: 0,
            total: model.size(),
        };
        self.report(&progress, on_progress);
        let step = (progress.total / 200).max(256 * 1024);

        // 同一个文件放在好几个地方（Hugging Face、魔搭……）：先测哪儿快，连不上的放到最后。
        // 国内连不上 Hugging Face，国外从魔搭下又慢，按顺序挨个试要白等好久
        if let Some(largest) = model.files.iter().max_by_key(|f| f.size) {
            let urls: Vec<String> = largest.sources().iter().map(|s| s.0.to_string()).collect();
            self.rank_sources(&urls).await;
        }

        let partial = self.models_dir().join(format!(".{id}.partial"));
        std::fs::create_dir_all(&partial)?;
        for file in &model.files {
            self.fetch_file(
                file,
                &partial.join(&file.name),
                &mut progress,
                step,
                on_progress,
            )
            .await?;
        }
        self.finish_install(&model, &partial)?;
        Ok(model)
    }

    /// 把装好文件的临时目录换成正式的模型目录。
    fn finish_install(&self, model: &Model, partial: &Path) -> Result<()> {
        let json = serde_json::to_vec_pretty(model).map_err(io::Error::other)?;
        std::fs::write(partial.join(MODEL_FILE), json)?;
        let target = self.model_dir(&model.id);
        if target.exists() {
            std::fs::remove_dir_all(&target)?;
        }
        std::fs::rename(partial, &target)?;
        // 还没选过用哪个识别模型（第一次装），就用刚装的这个；以后再装别的不会悄悄换掉
        let chosen = self.read_state().selected;
        if model.task == TASK_ASR && chosen.is_none_or(|id| self.installed_model(&id).is_none()) {
            self.select(&model.id)?;
        }
        Ok(())
    }

    fn report(&self, progress: &Progress, on_progress: &mut impl FnMut(&Progress)) {
        if let Some(slot) = self.downloads().get_mut(&progress.id) {
            *slot = progress.clone();
        }
        on_progress(progress);
    }

    /// 把一个文件下到 `to`：几个地址挨个试，已经下了一部分的接着下，下完校验 SHA-256。
    async fn fetch_file(
        &self,
        file: &RemoteFile,
        to: &Path,
        progress: &mut Progress,
        step: u64,
        on_progress: &mut impl FnMut(&Progress),
    ) -> Result<()> {
        let before = progress.received;
        let sources = file.sources();
        // 上次已经下完整的不用再下
        if std::fs::metadata(to).is_ok_and(|m| sources.iter().any(|s| s.1 == m.len()))
            && file.accepts(&sha256_of(to)?)
        {
            progress.received = before + file.size;
            self.report(progress, on_progress);
            return Ok(());
        }
        let mut last = Error::Download("no download address".into());
        let urls: Vec<String> = sources.iter().map(|s| s.0.to_string()).collect();
        for url in &self.ordered(&urls) {
            // 这个地址下来的文件应有的大小和校验值（主文件或者某个等价副本）
            let Some(&(_, size, sha256)) = sources.iter().find(|s| s.0 == url) else {
                continue;
            };
            match self
                .fetch_from(url, file, size, to, before, progress, step, on_progress)
                .await
            {
                Ok(()) => {
                    if sha256_of(to)?.eq_ignore_ascii_case(sha256) {
                        progress.received = before + file.size;
                        self.report(progress, on_progress);
                        return Ok(());
                    }
                    // 内容不对：整个重来，换下一个地址
                    let _ = std::fs::remove_file(to);
                    tracing::warn!(%url, file = %file.name, "downloaded file does not match its checksum");
                    last = Error::Checksum(file.name.clone());
                }
                Err(e) => {
                    tracing::info!(%url, file = %file.name, "download failed: {e}");
                    last = e;
                }
            }
            progress.received =
                before + std::fs::metadata(to).map_or(0, |m| m.len()).min(file.size);
        }
        Err(last)
    }

    #[allow(clippy::too_many_arguments)]
    async fn fetch_from(
        &self,
        url: &str,
        file: &RemoteFile,
        size: u64,
        to: &Path,
        before: u64,
        progress: &mut Progress,
        step: u64,
        on_progress: &mut impl FnMut(&Progress),
    ) -> Result<()> {
        let failed = |e: reqwest::Error| Error::Download(e.to_string());
        let mut have = std::fs::metadata(to).map_or(0, |m| m.len());
        if have >= size {
            // 比应有的还大，肯定不对
            let _ = std::fs::remove_file(to);
            have = 0;
        }
        let mut request = self.http.get(url);
        if have > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let mut response = request
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(failed)?;
        // 接着上次的下：看服务器给的是从哪儿开始的。有的服务器（魔搭的小文件）只给了要的那一段，
        // 状态码却是 200，所以按 Content-Range 判断，不只看状态码。不认断点续传的就从头来
        let resumed = match content_range_start(&response) {
            Some(start) if have > 0 && start == have => true,
            Some(start) if start != 0 => {
                return Err(Error::Download(format!(
                    "asked for bytes from {have} but got bytes from {start}"
                )));
            }
            Some(_) => false,
            None => have > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT,
        };
        if !resumed {
            have = 0;
        }
        let mut out = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(resumed)
            .truncate(!resumed)
            .open(to)?;
        progress.received = before + have;
        let mut reported = progress.received;
        loop {
            let chunk = tokio::time::timeout(STALL, response.chunk())
                .await
                .map_err(|_| Error::Download("download stalled".into()))?
                .map_err(failed)?;
            let Some(chunk) = chunk else { break };
            have += chunk.len() as u64;
            if have > size {
                return Err(Error::Checksum(file.name.clone()));
            }
            out.write_all(&chunk)?;
            progress.received = before + have;
            if progress.received - reported >= step {
                reported = progress.received;
                self.report(progress, on_progress);
            }
        }
        out.flush()?;
        if have != size {
            return Err(Error::Download("the connection closed early".into()));
        }
        Ok(())
    }

    /// 给几个下载地址测速：每个网站取一个地址，下开头一小段，按快慢给网站排序，记下来。
    /// 只有一个网站时不用测。
    async fn rank_sources(&self, urls: &[String]) {
        let mut origins: Vec<String> = Vec::new();
        for url in urls {
            let host = origin(url);
            if !origins.contains(&host) {
                origins.push(host);
            }
        }
        if origins.len() < 2 {
            return;
        }
        let mut tasks = tokio::task::JoinSet::new();
        for host in &origins {
            let Some(url) = urls.iter().find(|u| origin(u) == *host) else {
                continue;
            };
            let (http, url, host) = (self.http.clone(), url.clone(), host.clone());
            tasks.spawn(async move { (host, probe(&http, &url).await) });
        }
        let mut speeds: Vec<(String, f64)> = Vec::new();
        while let Some(done) = tasks.join_next().await {
            if let Ok((host, speed)) = done {
                tracing::debug!(%host, speed, "download source speed");
                speeds.push((host, speed));
            }
        }
        // 快的在前；一样快（比如都连不上）的保持目录里的顺序
        let at = |h: &str| origins.iter().position(|o| o == h);
        speeds.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| at(&a.0).cmp(&at(&b.0))));
        if let Some((best, speed)) = speeds.first() {
            tracing::info!(source = %best, bytes_per_second = *speed as u64, "downloading the speech model from the fastest source");
        }
        *self.sources.lock().unwrap_or_else(|e| e.into_inner()) =
            speeds.into_iter().map(|(h, _)| h).collect();
    }

    /// 上次测速排出来的下载源，快的在前。排查下载问题用。
    pub fn sources(&self) -> Vec<String> {
        self.sources
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// 按测出来的快慢排好的下载地址。没测过的网站排在测过的后面，各自保持原来的顺序。
    fn ordered(&self, urls: &[String]) -> Vec<String> {
        let ranking = self
            .sources
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let mut out = urls.to_vec();
        out.sort_by_key(|u| {
            let host = origin(u);
            ranking
                .iter()
                .position(|h| *h == host)
                .unwrap_or(ranking.len())
        });
        out
    }

    /// 从本机的一个文件夹装模型：给下载不动的人用。文件夹里（或者下一层）要有目录里列的那几个文件，
    /// 内容照样校验。
    pub async fn import(&self, id: &str, folder: &Path) -> Result<Model> {
        let catalog = self.catalog();
        let model = catalog
            .get(id)
            .ok_or_else(|| Error::UnknownModel(id.to_string()))?
            .clone();
        if !model.supported() {
            return Err(Error::Unsupported(model.kind.clone()));
        }
        let folder = folder.to_path_buf();
        let partial = self.models_dir().join(format!(".{id}.import"));
        let copied = {
            let (model, partial) = (model.clone(), partial.clone());
            tokio::task::spawn_blocking(move || -> Result<()> {
                let find = |name: &str| -> Option<PathBuf> {
                    let direct = folder.join(name);
                    if direct.is_file() {
                        return Some(direct);
                    }
                    std::fs::read_dir(&folder)
                        .ok()?
                        .flatten()
                        .map(|e| e.path().join(name))
                        .find(|p| p.is_file())
                };
                if partial.exists() {
                    std::fs::remove_dir_all(&partial)?;
                }
                std::fs::create_dir_all(&partial)?;
                for file in &model.files {
                    let from =
                        find(&file.name).ok_or_else(|| Error::MissingFile(file.name.clone()))?;
                    if !file.accepts(&sha256_of(&from)?) {
                        return Err(Error::Checksum(file.name.clone()));
                    }
                    std::fs::copy(&from, partial.join(&file.name))?;
                }
                Ok(())
            })
            .await
            .map_err(|e| Error::Io(io::Error::other(e)))?
        };
        if let Err(e) = copied {
            let _ = std::fs::remove_dir_all(&partial);
            return Err(e);
        }
        self.finish_install(&model, &partial)?;
        Ok(model)
    }

    /// 删掉一个模型。字幕不受影响。
    pub fn remove(&self, id: &str) -> Result<()> {
        if !catalog::safe_id(id) {
            return Err(Error::UnknownModel(id.to_string()));
        }
        for dir in [
            self.model_dir(id),
            self.models_dir().join(format!(".{id}.partial")),
        ] {
            if dir.exists() {
                std::fs::remove_dir_all(&dir)?;
            }
        }
        Ok(())
    }

    /// VAD 小模型在本机的位置。还没写出来（或者不完整）就先写出来。
    pub fn vad_path(&self) -> Result<PathBuf> {
        let path = self.root.join(VAD_FILE);
        if std::fs::metadata(&path).is_ok_and(|m| m.len() == VAD_MODEL.len() as u64) {
            return Ok(path);
        }
        write_atomic(&path, VAD_MODEL)?;
        Ok(path)
    }

    /// 加载一个装好的模型。要一两秒，放在阻塞线程里调用。
    pub fn load(&self, id: &str) -> Result<Engine> {
        let model = self
            .installed_model(id)
            .ok_or_else(|| Error::NotInstalled(id.to_string()))?;
        let speaker = self.speaker_model().map(|m| (self.model_dir(&m.id), m));
        Engine::load(
            &self.model_dir(id),
            &model,
            &self.vad_path()?,
            speaker.as_ref().map(|(dir, m)| (dir.as_path(), m)),
            default_threads(),
        )
    }

    /// 模型和字幕各占多少磁盘空间。
    pub fn disk_usage(&self) -> (u64, u64) {
        let transcripts = dir_size(&self.root.join("transcripts"));
        (
            dir_size(&self.root).saturating_sub(transcripts),
            transcripts,
        )
    }
}

/// 网址的协议、主机和端口：`https://www.modelscope.cn`。按它给下载源排序。
fn origin(url: &str) -> String {
    let (scheme, rest) = url.split_once("://").unwrap_or(("", url));
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    format!(
        "{}://{}",
        scheme.to_ascii_lowercase(),
        host.to_ascii_lowercase()
    )
}

/// `Content-Range: bytes 100-199/315894` 里的起点。没有这个头、或者读不懂时为空。
fn content_range_start(response: &reqwest::Response) -> Option<u64> {
    let value = response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)?
        .to_str()
        .ok()?;
    parse_content_range_start(value)
}

fn parse_content_range_start(value: &str) -> Option<u64> {
    let range = value.trim().strip_prefix("bytes")?.trim_start();
    let (start, _) = range.split_once('-')?;
    start.trim().parse().ok()
}

/// 从一个地址下开头一小段，返回每秒多少字节。连不上、出错是 0。
async fn probe(http: &reqwest::Client, url: &str) -> f64 {
    let started = Instant::now();
    let mut got = 0u64;
    let read = async {
        let mut response = http
            .get(url)
            .header(
                reqwest::header::RANGE,
                format!("bytes=0-{}", PROBE_BYTES - 1),
            )
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?;
        while got < PROBE_BYTES {
            match response.chunk().await.ok()? {
                Some(chunk) => got += chunk.len() as u64,
                None => break,
            }
        }
        Some(())
    };
    // 超时了也按已经收到的算：慢，但是通的
    let _ = tokio::time::timeout(PROBE_TIME, read).await;
    if got == 0 {
        return 0.0;
    }
    got as f64 / started.elapsed().as_secs_f64().max(0.001)
}

/// 文件内容的 SHA-256（小写十六进制）。
fn sha256_of(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins_and_content_ranges() {
        assert_eq!(
            origin("https://www.ModelScope.cn/models/a/b/resolve/master/x.onnx"),
            "https://www.modelscope.cn"
        );
        assert_eq!(origin("http://127.0.0.1:9/good/x"), "http://127.0.0.1:9");
        assert_eq!(origin("https://hf-mirror.com?x=1"), "https://hf-mirror.com");
        assert_eq!(parse_content_range_start("bytes 100-199/315894"), Some(100));
        assert_eq!(parse_content_range_start(" bytes  0-0/1"), Some(0));
        assert_eq!(parse_content_range_start("bytes */315894"), None);
        assert_eq!(parse_content_range_start("items 1-2/3"), None);
    }
}
