//! 离线翻译。
//!
//! 用 Firefox 翻译项目的模型（Mozilla 发布，MPL-2.0），推理在本机完成，正文不会离开这台电脑。
//! 模型按语言对下载，放在本机私有目录里，不参与同步：每台设备自己决定要哪些语言。
//! 每种语言都只和英语互译，其他语言之间先翻成英语再翻过去。

pub mod catalog;
pub mod detect;
pub mod text;

use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use fxtranslate::engine::Engine;
use fxtranslate::shortlist::Shortlist;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub use catalog::{CATALOG_URL, Catalog};
pub use text::{Mark, Segment};

use catalog::{FileKind, RemoteModel, pair_name, split_pair, valid_code};

/// 目录缓存多久以后联网更新一次。
const CATALOG_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 3600);
const INSTALLED_FILE: &str = "installed.json";
const CATALOG_FILE: &str = "catalog.json";

/// 克罗地亚语、塞尔维亚语、波斯尼亚语翻成英语共用一个模型。
const SOURCE_ALIASES: [(&str, &str); 3] = [("hr", "hbs"), ("sr", "hbs"), ("bs", "hbs")];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("translation models are not available offline: {0}")]
    CatalogUnavailable(String),
    #[error("no translation model from {0} to {1}")]
    Unsupported(String, String),
    #[error("translation model {0} is not downloaded")]
    NotInstalled(String),
    #[error("download failed: {0}")]
    Download(String),
    #[error("downloaded file for {0} is corrupt")]
    Checksum(String),
    #[error("cannot load translation model {0}: {1}")]
    Load(String, String),
    #[error("bad language code: {0}")]
    BadCode(String),
    #[error("io: {0}")]
    Io(#[from] io::Error),
}

impl Error {
    /// 稳定的错误码，界面据此显示本地化的提示。
    pub fn code(&self) -> &'static str {
        match self {
            Error::CatalogUnavailable(_) => "translate_offline",
            Error::Unsupported(..) => "translate_unsupported",
            Error::NotInstalled(_) => "translate_not_installed",
            Error::Download(_) => "translate_download_failed",
            Error::Checksum(_) => "translate_download_failed",
            Error::Load(..) => "translate_load_failed",
            Error::BadCode(_) => "bad_params",
            Error::Io(_) => "io",
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// 下载进度。`total` 是几个文件压缩后的总大小。
#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub pair: String,
    pub received: u64,
    pub total: u64,
}

/// 已经下载到本机的一个语言对。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Installed {
    pub pair: String,
    pub from: String,
    pub to: String,
    /// 占用的磁盘空间。
    pub size: u64,
}

/// 设置页和翻译按钮需要的全部信息。
#[derive(Debug, Clone, Serialize)]
pub struct Languages {
    /// 能拿到模型目录（联网，或者之前存过）。拿不到时只能用已经下载的模型。
    pub catalog: bool,
    /// 能翻译的原文语言和能翻成的语言。
    pub sources: Vec<String>,
    pub targets: Vec<String>,
    pub installed: Vec<Installed>,
    pub downloads: Vec<Progress>,
}

/// 翻译一段文字之前的检查结果。
#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    /// 识别出的原文语言。识别不出来时为空。
    pub from: Option<String>,
    pub to: String,
    /// 原文已经是目标语言了。
    pub same: bool,
    /// 还需要下载的模型。为空就可以直接翻译。
    pub missing: Vec<Missing>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Missing {
    pub pair: String,
    pub from: String,
    pub to: String,
    pub size: u64,
}

pub struct Translator {
    root: PathBuf,
    catalog_url: String,
    http: reqwest::Client,
    /// 模型目录在内存里的副本。
    catalog: Mutex<Option<Arc<Catalog>>>,
    engines: Mutex<HashMap<String, Arc<Engine>>>,
    downloads: Mutex<HashMap<String, Progress>>,
}

impl Translator {
    /// `root` 是放模型的本机目录。
    pub fn new(root: impl Into<PathBuf>) -> Result<Translator> {
        Self::with_catalog_url(root, CATALOG_URL)
    }

    /// 和 `new` 一样，但从别的地址取模型目录。测试用。
    pub fn with_catalog_url(root: impl Into<PathBuf>, url: &str) -> Result<Translator> {
        let root = root.into();
        std::fs::create_dir_all(&root)?;
        let http = reqwest::Client::builder()
            .user_agent(concat!("ShibaPost/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| Error::Download(e.to_string()))?;
        Ok(Translator {
            root,
            catalog_url: url.to_string(),
            http,
            catalog: Mutex::new(None),
            engines: Mutex::new(HashMap::new()),
            downloads: Mutex::new(HashMap::new()),
        })
    }

    fn downloads(&self) -> MutexGuard<'_, HashMap<String, Progress>> {
        self.downloads.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn engines(&self) -> MutexGuard<'_, HashMap<String, Arc<Engine>>> {
        self.engines.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn keep_catalog(&self, catalog: Catalog) -> Arc<Catalog> {
        let catalog = Arc::new(catalog);
        *self.catalog.lock().unwrap_or_else(|e| e.into_inner()) = Some(catalog.clone());
        catalog
    }

    /// 模型目录。存过的副本不太旧就直接用，否则联网更新；联不上网时退回旧副本。
    pub async fn catalog(&self) -> Result<Arc<Catalog>> {
        if let Some(catalog) = self
            .catalog
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            return Ok(catalog);
        }
        let path = self.root.join(CATALOG_FILE);
        let cached = std::fs::read(&path).ok();
        let fresh = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age < CATALOG_MAX_AGE);
        if fresh && let Some(catalog) = cached.as_deref().and_then(|b| Catalog::parse(b).ok()) {
            return Ok(self.keep_catalog(catalog));
        }
        match self.fetch_catalog().await {
            Ok((bytes, catalog)) => {
                if let Err(e) = write_atomic(&path, &bytes) {
                    tracing::warn!("cannot cache translation catalog: {e}");
                }
                Ok(self.keep_catalog(catalog))
            }
            Err(e) => match cached.as_deref().and_then(|b| Catalog::parse(b).ok()) {
                Some(catalog) => {
                    tracing::info!("using cached translation catalog: {e}");
                    Ok(self.keep_catalog(catalog))
                }
                None => Err(Error::CatalogUnavailable(e)),
            },
        }
    }

    async fn fetch_catalog(&self) -> Result<(Vec<u8>, Catalog), String> {
        let response = self
            .http
            .get(&self.catalog_url)
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| e.to_string())?;
        let bytes = response.bytes().await.map_err(|e| e.to_string())?.to_vec();
        let catalog = Catalog::parse(&bytes)?;
        Ok((bytes, catalog))
    }

    /// 本机已经下载的模型。
    pub fn installed(&self) -> Vec<Installed> {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut list: Vec<Installed> = entries
            .flatten()
            .filter_map(|e| {
                let bytes = std::fs::read(e.path().join(INSTALLED_FILE)).ok()?;
                let mut info: Installed = serde_json::from_slice(&bytes).ok()?;
                // 目录名才是真正的语言对，以它为准
                let name = e.file_name().to_string_lossy().into_owned();
                let (from, to) = split_pair(&name)?;
                info.from = from.to_string();
                info.to = to.to_string();
                info.pair = name;
                Some(info)
            })
            .collect();
        list.sort_by(|a, b| a.pair.cmp(&b.pair));
        list
    }

    fn is_installed(&self, pair: &str) -> bool {
        self.root.join(pair).join(INSTALLED_FILE).is_file()
    }

    pub async fn languages(&self) -> Languages {
        let installed = self.installed();
        let downloads: Vec<Progress> = self.downloads().values().cloned().collect();
        let catalog = self.catalog().await.ok();
        let mut sources = vec!["en".to_string()];
        let mut targets = vec!["en".to_string()];
        let pairs: Vec<(String, String)> = match &catalog {
            Some(c) => c.models().map(|m| (m.from.clone(), m.to.clone())).collect(),
            None => installed
                .iter()
                .map(|i| (i.from.clone(), i.to.clone()))
                .collect(),
        };
        for (from, to) in pairs {
            if to == "en" {
                // 一个模型管几种语言时，把它们都列出来
                let aliases = SOURCE_ALIASES
                    .iter()
                    .filter(|(_, a)| *a == from)
                    .map(|(l, _)| *l);
                if aliases.clone().next().is_some() {
                    sources.extend(aliases.map(String::from));
                } else {
                    sources.push(from);
                }
            } else if from == "en" {
                targets.push(to);
            }
        }
        sources.sort();
        sources.dedup();
        targets.sort();
        targets.dedup();
        Languages {
            catalog: catalog.is_some(),
            sources,
            targets,
            installed,
            downloads,
        }
    }

    /// 从 `from` 到 `to` 要经过的模型。`exists` 判断某个语言对有没有模型。
    fn legs(
        from: &str,
        to: &str,
        exists: impl Fn(&str, &str) -> bool,
    ) -> Result<Vec<(String, String)>> {
        let source = |lang: &str| -> String {
            if exists(lang, "en") {
                return lang.to_string();
            }
            SOURCE_ALIASES
                .iter()
                .find(|(l, a)| *l == lang && exists(a, "en"))
                .map_or(lang.to_string(), |(_, a)| a.to_string())
        };
        let legs = if from == "en" {
            vec![("en".to_string(), to.to_string())]
        } else if to == "en" {
            vec![(source(from), "en".to_string())]
        } else {
            vec![
                (source(from), "en".to_string()),
                ("en".to_string(), to.to_string()),
            ]
        };
        Ok(legs)
    }

    /// 翻译前的检查：识别原文语言，看还缺哪些模型。
    pub async fn plan(&self, sample: &str, from: Option<&str>, to: &str) -> Result<Plan> {
        check_code(to)?;
        let from = match from {
            Some(f) => {
                check_code(f)?;
                Some(f.to_string())
            }
            None => detect::detect(sample),
        };
        let Some(from_code) = from.clone() else {
            return Ok(Plan {
                from,
                to: to.to_string(),
                same: false,
                missing: Vec::new(),
            });
        };
        if same_language(&from_code, to) {
            return Ok(Plan {
                from,
                to: to.to_string(),
                same: true,
                missing: Vec::new(),
            });
        }
        let installed_legs =
            Self::legs(&from_code, to, |a, b| self.is_installed(&pair_name(a, b)))?;
        if installed_legs
            .iter()
            .all(|(a, b)| self.is_installed(&pair_name(a, b)))
        {
            return Ok(Plan {
                from,
                to: to.to_string(),
                same: false,
                missing: Vec::new(),
            });
        }
        let catalog = self.catalog().await?;
        let legs = Self::legs(&from_code, to, |a, b| catalog.get(a, b).is_some())?;
        let mut missing = Vec::new();
        for (a, b) in legs {
            let pair = pair_name(&a, &b);
            if self.is_installed(&pair) {
                continue;
            }
            let model = catalog
                .get(&a, &b)
                .ok_or_else(|| Error::Unsupported(from_code.clone(), to.to_string()))?;
            missing.push(Missing {
                pair,
                from: a,
                to: b,
                size: model.size,
            });
        }
        Ok(Plan {
            from,
            to: to.to_string(),
            same: false,
            missing,
        })
    }

    /// 正在下载的话返回 `false`，调用方不必再开一个。
    pub fn begin_download(&self, pair: &str) -> Result<bool> {
        split_pair(pair).ok_or_else(|| Error::BadCode(pair.to_string()))?;
        let mut downloads = self.downloads();
        if downloads.contains_key(pair) {
            return Ok(false);
        }
        downloads.insert(
            pair.to_string(),
            Progress {
                pair: pair.to_string(),
                received: 0,
                total: 0,
            },
        );
        Ok(true)
    }

    /// 下载一个语言对。先调用 `begin_download` 占位；不管成功与否，结束后都会清掉占位。
    pub async fn download(
        &self,
        pair: &str,
        mut on_progress: impl FnMut(&Progress),
    ) -> Result<Installed> {
        let result = self.download_inner(pair, &mut on_progress).await;
        self.downloads().remove(pair);
        result
    }

    async fn download_inner(
        &self,
        pair: &str,
        on_progress: &mut impl FnMut(&Progress),
    ) -> Result<Installed> {
        let (from, to) = split_pair(pair).ok_or_else(|| Error::BadCode(pair.to_string()))?;
        let catalog = self.catalog().await?;
        let model = catalog
            .get(from, to)
            .ok_or_else(|| Error::Unsupported(from.to_string(), to.to_string()))?
            .clone();

        // 先问清每个文件多大，进度条才准
        let mut sizes = Vec::new();
        for file in &model.files {
            let len = self
                .http
                .head(&file.url)
                .send()
                .await
                .ok()
                .and_then(|r| r.error_for_status().ok())
                .and_then(|r| r.content_length())
                .unwrap_or(0);
            sizes.push(len);
        }
        let mut progress = Progress {
            pair: pair.to_string(),
            received: 0,
            total: sizes.iter().sum(),
        };
        self.report(&progress, on_progress);

        let partial = self.root.join(format!(".{pair}.partial"));
        if partial.exists() {
            std::fs::remove_dir_all(&partial)?;
        }
        std::fs::create_dir_all(&partial)?;
        let result = self
            .fetch_files(&model, &partial, &mut progress, on_progress)
            .await;
        if let Err(e) = result {
            let _ = std::fs::remove_dir_all(&partial);
            return Err(e);
        }

        let size = dir_size(&partial);
        let info = Installed {
            pair: pair.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            size,
        };
        let json = serde_json::to_vec_pretty(&info).map_err(|e| Error::Io(io::Error::other(e)))?;
        std::fs::write(partial.join(INSTALLED_FILE), json)?;
        // 换掉旧版本。旧模型可能正被映射在内存里，先放掉
        let target = self.root.join(pair);
        self.engines().remove(pair);
        if target.exists() {
            std::fs::remove_dir_all(&target)?;
        }
        std::fs::rename(&partial, &target)?;
        Ok(info)
    }

    fn report(&self, progress: &Progress, on_progress: &mut impl FnMut(&Progress)) {
        if let Some(slot) = self.downloads().get_mut(&progress.pair) {
            *slot = progress.clone();
        }
        on_progress(progress);
    }

    async fn fetch_files(
        &self,
        model: &RemoteModel,
        dir: &Path,
        progress: &mut Progress,
        on_progress: &mut impl FnMut(&Progress),
    ) -> Result<()> {
        // 进度最多报一百来次
        let step = (progress.total / 100).max(256 * 1024);
        let mut reported = progress.received;
        for file in &model.files {
            let mut response = self
                .http
                .get(&file.url)
                .send()
                .await
                .and_then(|r| r.error_for_status())
                .map_err(|e| Error::Download(e.to_string()))?;
            let out = std::fs::File::create(dir.join(file.kind.file_name()))?;
            let mut sink = Sink::new(io::BufWriter::new(out));
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|e| Error::Download(e.to_string()))?
            {
                sink.write_all(&chunk)?;
                progress.received += chunk.len() as u64;
                if progress.received - reported >= step {
                    reported = progress.received;
                    self.report(progress, on_progress);
                }
            }
            let digest = sink.finish()?;
            if let Some(expected) = &file.sha256
                && *expected != digest
            {
                return Err(Error::Checksum(model.pair()));
            }
        }
        self.report(progress, on_progress);
        Ok(())
    }

    /// 删掉一个语言对。
    pub fn remove(&self, pair: &str) -> Result<()> {
        split_pair(pair).ok_or_else(|| Error::BadCode(pair.to_string()))?;
        self.engines().remove(pair);
        let dir = self.root.join(pair);
        if dir.exists() {
            std::fs::remove_dir_all(dir)?;
        }
        Ok(())
    }

    fn engine(&self, pair: &str) -> Result<Arc<Engine>> {
        if let Some(engine) = self.engines().get(pair) {
            return Ok(engine.clone());
        }
        let dir = self.root.join(pair);
        if !dir.join(INSTALLED_FILE).is_file() {
            return Err(Error::NotInstalled(pair.to_string()));
        }
        let path = |k: FileKind| dir.join(k.file_name());
        let (src, trg) = if path(FileKind::Vocab).is_file() {
            (path(FileKind::Vocab), path(FileKind::Vocab))
        } else {
            (path(FileKind::SrcVocab), path(FileKind::TrgVocab))
        };
        let fail = |e: String| Error::Load(pair.to_string(), e);
        // 文件坏了时引擎会直接 panic，接住它，当作加载失败
        let loaded = std::panic::catch_unwind(|| -> Result<Engine, String> {
            let shortlist = Shortlist::load(path(FileKind::Lex)).map_err(|e| e.to_string())?;
            Ok(Engine::load_mmapped(path(FileKind::Model), &src, &trg)?.with_shortlist(shortlist))
        });
        let engine = match loaded {
            Ok(result) => result.map_err(fail)?,
            Err(_) => return Err(fail("model files are damaged".into())),
        };
        let engine = Arc::new(engine);
        self.engines().insert(pair.to_string(), engine.clone());
        Ok(engine)
    }

    /// 翻译一组段落，顺序和数量不变。很耗 CPU，异步代码里请放到阻塞线程池里调用。
    pub fn translate(&self, from: &str, to: &str, segments: Vec<Segment>) -> Result<Vec<Segment>> {
        check_code(from)?;
        check_code(to)?;
        if same_language(from, to) {
            return Ok(segments);
        }
        let legs = Self::legs(from, to, |a, b| self.is_installed(&pair_name(a, b)))?;
        let engines = legs
            .iter()
            .map(|(a, b)| Ok((self.engine(&pair_name(a, b))?, b.clone())))
            .collect::<Result<Vec<_>>>()?;

        let todo: Vec<usize> = (0..segments.len())
            .filter(|&i| text::needs_translation(&segments[i].text, from, to))
            .collect();
        let mut work: Vec<Segment> = todo.iter().map(|&i| segments[i].clone()).collect();
        for (engine, leg_to) in &engines {
            work = par_map(&work, |s| text::translate_segment(engine, leg_to, s));
        }
        let mut out = segments;
        for (i, translated) in todo.into_iter().zip(work) {
            out[i] = translated;
        }
        Ok(out)
    }
}

/// 简体和繁体之间、挪威语两种写法之间不经过英语转译，当作同一种语言。
fn same_language(a: &str, b: &str) -> bool {
    let base = |c: &str| {
        match c {
            "zh_hant" => "zh",
            "no" => "nb",
            other => other,
        }
        .to_string()
    };
    base(a) == base(b)
}

fn check_code(code: &str) -> Result<()> {
    if valid_code(code) {
        Ok(())
    } else {
        Err(Error::BadCode(code.to_string()))
    }
}

/// 在几个线程上并行处理，结果顺序不变。段落少时就在当前线程做。
fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(4)
        .min(items.len());
    if threads <= 1 {
        return items.iter().map(&f).collect();
    }
    let chunk = items.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let handles: Vec<_> = items
            .chunks(chunk)
            .map(|part| scope.spawn(|| part.iter().map(&f).collect::<Vec<R>>()))
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
            .collect()
    })
}

/// 写下载的文件：认出 gzip 就边收边解压，同时算出解压后内容的 SHA-256。
struct Sink<W: Write> {
    state: SinkState<W>,
}

enum SinkState<W: Write> {
    /// 还没收到足够判断格式的字节。
    Start(Vec<u8>, Hashing<W>),
    Gzip(flate2::write::GzDecoder<Hashing<W>>),
    Plain(Hashing<W>),
    Done,
}

struct Hashing<W: Write> {
    inner: W,
    hasher: Sha256,
}

impl<W: Write> Write for Hashing<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<W: Write> Sink<W> {
    fn new(inner: W) -> Sink<W> {
        Sink {
            state: SinkState::Start(
                Vec::new(),
                Hashing {
                    inner,
                    hasher: Sha256::new(),
                },
            ),
        }
    }

    fn decide(&mut self, force: bool) -> io::Result<()> {
        let SinkState::Start(head, _) = &self.state else {
            return Ok(());
        };
        if head.len() < 2 && !force {
            return Ok(());
        }
        let SinkState::Start(head, hashing) = std::mem::replace(&mut self.state, SinkState::Done)
        else {
            unreachable!()
        };
        self.state = if head.starts_with(&[0x1f, 0x8b]) {
            let mut decoder = flate2::write::GzDecoder::new(hashing);
            decoder.write_all(&head)?;
            SinkState::Gzip(decoder)
        } else {
            let mut plain = hashing;
            plain.write_all(&head)?;
            SinkState::Plain(plain)
        };
        Ok(())
    }

    /// 写完以后调用，返回解压后内容的 SHA-256（小写十六进制）。
    fn finish(mut self) -> io::Result<String> {
        self.decide(true)?;
        let mut hashing = match std::mem::replace(&mut self.state, SinkState::Done) {
            SinkState::Gzip(decoder) => decoder.finish()?,
            SinkState::Plain(plain) => plain,
            _ => unreachable!(),
        };
        hashing.inner.flush()?;
        let digest = hashing.hasher.finalize();
        Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
    }
}

impl<W: Write> Write for Sink<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let SinkState::Start(head, _) = &mut self.state {
            head.extend_from_slice(buf);
            self.decide(false)?;
            return Ok(buf.len());
        }
        match &mut self.state {
            SinkState::Gzip(decoder) => decoder.write(buf),
            SinkState::Plain(plain) => plain.write(buf),
            _ => Err(io::Error::other("write after finish")),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.state {
            SinkState::Gzip(decoder) => decoder.flush(),
            SinkState::Plain(plain) => plain.flush(),
            _ => Ok(()),
        }
    }
}

fn dir_size(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| e.metadata().ok())
                .filter(|m| m.is_file())
                .map(|m| m.len())
                .sum()
        })
        .unwrap_or(0)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sink_unpacks_gzip_and_hashes_the_content() {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(b"hello model").unwrap();
        let packed = gz.finish().unwrap();

        let mut out = Vec::new();
        let mut sink = Sink::new(&mut out);
        // 一个字节一个字节地喂，也要能认出 gzip
        for b in &packed {
            sink.write_all(std::slice::from_ref(b)).unwrap();
        }
        let digest = sink.finish().unwrap();
        assert_eq!(out, b"hello model");
        let expected: String = Sha256::digest(b"hello model")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(digest, expected);
    }

    #[test]
    fn sink_passes_plain_files_through() {
        let mut out = Vec::new();
        let mut sink = Sink::new(&mut out);
        sink.write_all(b"x").unwrap();
        sink.finish().unwrap();
        assert_eq!(out, b"x");
    }

    #[test]
    fn routes_through_english() {
        let all = |_: &str, _: &str| true;
        assert_eq!(
            Translator::legs("en", "zh", all).unwrap(),
            vec![("en".into(), "zh".into())]
        );
        assert_eq!(
            Translator::legs("ja", "zh", all).unwrap(),
            vec![("ja".into(), "en".into()), ("en".into(), "zh".into())]
        );
        let hbs = |a: &str, _: &str| a == "hbs" || a == "en";
        assert_eq!(
            Translator::legs("hr", "en", hbs).unwrap(),
            vec![("hbs".into(), "en".into())]
        );
    }

    #[test]
    fn same_language_needs_no_model() {
        assert!(same_language("zh", "zh_hant"));
        assert!(same_language("no", "nb"));
        assert!(!same_language("zh", "ja"));
    }
}
