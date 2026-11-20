//! 字幕：给没有文字稿的播客单集在本机转写。
//!
//! 识别模型和转写出来的字幕都在本机目录的 `speech/` 下，不参与同步（见 postflow-speech）。
//!
//! 边播边出字幕的做法：不去录正在放的声音，而是直接读下载到本机的音频文件，从播放器现在的位置
//! 往后转。不用等整期下完：下了一点就开始转，读到还没下到的地方就等一等（`GrowingFile`）。识别比播放快得多（几倍到几十倍），所以开了字幕等一小会儿就能跟上，之后一直走在播放前面；
//! 转到结尾再回头把前面没听的部分补上。用户拖动进度到还没转的地方，就改从那里接着转。
//! 同一时间只转一期：换了一期听，上一期的停下，转了多少存多少，下次接着来。

use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use postflow_speech::{AudioReader, Cue, MediaSource, Transcript, cues};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::rpc::{RpcError, parse, reply};
use crate::{Core, Event, ModelState, Session};

/// 新转出来的句子攒这么久推给界面一次。
const EMIT_EVERY: Duration = Duration::from_millis(300);
/// 转写过程中每隔这么久存一次盘，程序中途退出也不会白转。
const SAVE_EVERY: Duration = Duration::from_secs(10);
/// 播放位置在「正在转的地方往后这么多秒」以内就不改道：马上就转到了。
const LOOKAHEAD: f64 = 45.0;
/// 等单集下载时多久看一次进度。
const POLL: Duration = Duration::from_millis(500);
/// 单集下了这么多就开始转，不等整期下完。
const START_BYTES: u64 = 256 * 1024;
/// 读到还没下到的地方时，隔多久再看一次。
const WAIT: Duration = Duration::from_millis(200);
/// 等下载时，隔多久告诉界面一次下载进度。
const WAIT_REPORT: Duration = Duration::from_secs(1);

/// 转写读的是哪个文件。
enum Source {
    /// 下好了的。
    File(PathBuf),
    /// 还在下载的：`part` 是正在写的文件，`ext` 是下完以后的扩展名（帮着认格式）。
    Growing { part: PathBuf, ext: Option<String> },
}

/// 还在下载的单集：读到还没下到的地方就等一等，下完了才算到头。下载失败、任务取消时报错，
/// 转写跟着停下。下完改名不影响已经打开的文件。
struct GrowingFile {
    file: std::fs::File,
    core: Arc<Core>,
    job: Arc<Job>,
}

impl Read for GrowingFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut reported = Instant::now() - WAIT_REPORT;
        loop {
            let n = self.file.read(buf)?;
            if n > 0 || buf.is_empty() {
                return Ok(n);
            }
            if self.job.cancelled() {
                // 不能用 Interrupted：标准库的 read_exact 和 symphonia 遇到它都当「再试一次」，
                // 文件不再变长时就一直空转，占满一个核、线程和已加载的模型都不释放。
                // 是不是取消由调用方看 job.cancelled()，这里只要让读取停下来
                return Err(io::Error::other("cancelled"));
            }
            match self.core.episodes.progress().remove(&self.job.id) {
                Some(p) if p.error.is_some() => {
                    return Err(io::Error::other(p.error.unwrap_or_default()));
                }
                Some(p) => {
                    // 在等下载：让界面显示下到哪儿了
                    if reported.elapsed() >= WAIT_REPORT {
                        reported = Instant::now();
                        {
                            let mut live = self.job.live();
                            live.received = p.received;
                            live.total = p.total;
                        }
                        self.job.emit(&self.core, Vec::new(), None);
                    }
                }
                // 下完了（不在下载列表里了）：再读一次，还没有就是真的到头了
                None => {
                    let n = self.file.read(buf)?;
                    let mut live = self.job.live();
                    if let Some(total) = live.total {
                        live.received = total;
                    }
                    return Ok(n);
                }
            }
            std::thread::sleep(WAIT);
        }
    }
}

impl Seek for GrowingFile {
    // 跳到还没下到的地方也可以：之后读的时候再等。
    // 「从末尾往前」要按整个文件的大小算，不能按已经下到的：解码器靠跳到末尾知道文件多长，
    // 按下了一半的长度算，索引很大的 M4A（比如故事FM 的，开头的 moov 有 800 多 KB）
    // 会被当成在索引中间就结束了，报「missing moov atom」。
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let pos = from_start(pos, self.byte_len())?;
        self.file.seek(pos)
    }
}

/// 「从末尾往前」换成从开头算，按整个文件的大小 `total`（不知道就照原样）。
fn from_start(pos: SeekFrom, total: Option<u64>) -> io::Result<SeekFrom> {
    match (pos, total) {
        (SeekFrom::End(delta), Some(total)) => total
            .checked_add_signed(delta)
            .map(SeekFrom::Start)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before the start")),
        (pos, _) => Ok(pos),
    }
}

/// 等这一期下载完，返回下好的文件。边下边读不行的时候用（见 `prepare`）。等的时候让界面显示下载进度。
fn wait_for_download(core: &Arc<Core>, job: &Arc<Job>) -> Result<PathBuf, RpcError> {
    let mut reported = Instant::now() - WAIT_REPORT;
    loop {
        if job.cancelled() {
            return Err(RpcError::new("cancelled", "cancelled"));
        }
        if let Some(file) = core.episodes.file(&job.id) {
            return Ok(file);
        }
        match core.episodes.progress().remove(&job.id) {
            Some(p) if p.error.is_some() => {
                return Err(RpcError::new(
                    "speech_episode_failed",
                    p.error.unwrap_or_default(),
                ));
            }
            // 在等下载：每秒让界面看一次下到哪儿了
            Some(p) if reported.elapsed() >= WAIT_REPORT => {
                reported = Instant::now();
                {
                    let mut live = job.live();
                    live.state = SubtitleState::Downloading;
                    live.received = p.received;
                    live.total = p.total;
                }
                job.emit(core, Vec::new(), None);
            }
            Some(_) => {}
            // 刚好下完、正在改名：再看一圈
            None => {}
        }
        std::thread::sleep(WAIT);
    }
}

impl MediaSource for GrowingFile {
    fn is_seekable(&self) -> bool {
        true
    }

    /// 整个文件多大（服务器说了的话）：有的格式靠它估算时长、跳转位置。
    fn byte_len(&self) -> Option<u64> {
        self.core
            .episodes
            .progress()
            .get(&self.job.id)
            .and_then(|p| p.total)
            .or_else(|| self.file.metadata().ok().map(|m| m.len()))
    }
}

impl From<postflow_speech::Error> for RpcError {
    fn from(err: postflow_speech::Error) -> Self {
        RpcError::new(err.code(), err.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleState {
    /// 没在转。可能一句都没有，可能转了一部分，也可能转完了。
    Idle,
    /// 在等这一期下载到本机。
    Downloading,
    /// 在加载识别模型。
    Loading,
    /// 正在转。
    Running,
    Failed,
}

struct Live {
    state: SubtitleState,
    transcript: Transcript,
    /// 失败时的错误码和说明。
    error: Option<RpcError>,
    /// 等下载时的进度。
    received: u64,
    total: Option<u64>,
}

struct Job {
    id: String,
    /// 用哪个模型转。
    model: String,
    cancel: AtomicBool,
    /// 播放器现在放到第几秒（f64 的位模式）。转写优先从这里往后。
    want: AtomicU64,
    live: Mutex<Live>,
}

impl Job {
    fn live(&self) -> MutexGuard<'_, Live> {
        self.live.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 边下边转时，把单集的下载进度抄过来，界面据此说「在等下载」还是「在识别」。
    fn track_download(&self, core: &Core) {
        let progress = core.episodes.progress().remove(&self.id);
        let mut live = self.live();
        match progress {
            Some(p) if p.error.is_none() => {
                live.received = p.received;
                live.total = p.total;
            }
            // 下完了
            _ => {
                if let Some(total) = live.total {
                    live.received = total;
                }
            }
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn want(&self) -> f64 {
        f64::from_bits(self.want.load(Ordering::Relaxed))
    }

    fn set_want(&self, seconds: f64) {
        let seconds = if seconds.is_finite() {
            seconds.max(0.0)
        } else {
            0.0
        };
        self.want.store(seconds.to_bits(), Ordering::Relaxed);
    }

    fn set_state(&self, state: SubtitleState) {
        self.live().state = state;
    }

    /// 给界面的完整状态。
    fn snapshot(&self) -> Value {
        let live = self.live();
        snapshot(
            &self.id,
            live.state,
            &live.transcript,
            live.error.as_ref(),
            live.received,
            live.total,
        )
    }

    /// 推一次进展：新出来的句子（落在 `replaced` 这段时间里的旧句子由它们替换），和现在的状态。
    fn emit(&self, core: &Core, cues: Vec<Cue>, replaced: Option<(f64, f64)>) {
        let live = self.live();
        core.emit(Event::Subtitles {
            id: self.id.clone(),
            state: live.state,
            cues,
            from: replaced.map(|r| r.0),
            to: replaced.map(|r| r.1),
            covered: live.transcript.covered.clone(),
            duration: live.transcript.duration,
            complete: live.transcript.complete(),
            model: live.transcript.model.clone(),
            voices: live.transcript.speakers.genders(),
            received: live.received,
            total: live.total,
            error: live.error.as_ref().map(|e| e.code.clone()),
            message: live.error.as_ref().map(|e| e.message.clone()),
        });
    }
}

fn snapshot(
    id: &str,
    state: SubtitleState,
    transcript: &Transcript,
    error: Option<&RpcError>,
    received: u64,
    total: Option<u64>,
) -> Value {
    json!({
        "id": id,
        "state": state,
        "model": transcript.model,
        "duration": transcript.duration,
        "covered": transcript.covered,
        "complete": transcript.complete(),
        "cues": transcript.cues,
        "voices": transcript.speakers.genders(),
        "received": received,
        "total": total,
        "error": error.map(|e| e.code.clone()),
        "message": error.map(|e| e.message.clone()),
    })
}

/// 正在进行的转写。同一时间最多一个。
#[derive(Default)]
pub(crate) struct Subtitles {
    job: Mutex<Option<Arc<Job>>>,
}

impl Subtitles {
    fn slot(&self) -> MutexGuard<'_, Option<Arc<Job>>> {
        self.job.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn current(&self, id: &str) -> Option<Arc<Job>> {
        self.slot().as_ref().filter(|j| j.id == id).cloned()
    }

    /// 停掉正在转的（`id` 给了就只停这一期的）。
    pub(crate) fn stop(&self, id: Option<&str>) {
        let mut slot = self.slot();
        if let Some(job) = slot.as_ref()
            && id.is_none_or(|id| id == job.id)
        {
            job.cancel.store(true, Ordering::Relaxed);
            *slot = None;
        }
    }
}

#[derive(Deserialize)]
struct IdParam {
    id: String,
}

#[derive(Deserialize)]
struct StartParams {
    id: String,
    /// 播放器现在放到第几秒。
    #[serde(default)]
    position: f64,
    /// 要先下载这一期的话，本机一共留几期（和「下载离线听」同一个设置）。
    #[serde(default = "default_keep")]
    keep: usize,
}

fn default_keep() -> usize {
    10
}

#[derive(Deserialize)]
struct TranslationsParams {
    id: String,
    lang: String,
    /// 保存时：原文的句子（`<开始的百分之一秒>|<原文>`）到译文。
    #[serde(default)]
    entries: std::collections::BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct ImportParams {
    id: String,
    path: String,
}

impl Core {
    /// 识别模型：列出、下载、删除、选用、从文件夹导入。不需要打开数据目录。
    pub(crate) async fn speech_rpc(
        self: &Arc<Self>,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        match method {
            "speech.overview" => {
                // 先用手上的目录回答，要不要联网更新放到后台：离线时设置页也不用等
                if self.speech.catalog_due() {
                    let speech = self.speech.clone();
                    tokio::spawn(async move {
                        if speech.refresh_catalog().await {
                            tracing::info!("speech model catalog updated");
                        }
                    });
                }
                reply(self.speech.overview())
            }
            "speech.download" => {
                let p: IdParam = parse(params)?;
                if self.speech.begin_download(&p.id)? {
                    tokio::spawn(download_model(self.clone(), p.id));
                }
                reply(json!({}))
            }
            "speech.remove" => {
                let p: IdParam = parse(params)?;
                // 正在用它转的先停下
                self.subtitles.stop(None);
                let speech = self.speech.clone();
                tokio::task::spawn_blocking(move || speech.remove(&p.id))
                    .await
                    .map_err(|e| RpcError::new("internal", e.to_string()))??;
                reply(self.speech.overview())
            }
            "speech.select" => {
                let p: IdParam = parse(params)?;
                self.speech.select(&p.id)?;
                reply(self.speech.overview())
            }
            "speech.import" => {
                let p: ImportParams = parse(params)?;
                let folder = crate::expand_home(p.path.trim());
                if !folder.is_dir() {
                    return Err(RpcError::new("speech_import_missing", "no such folder"));
                }
                self.speech.import(&p.id, &folder).await?;
                reply(self.speech.overview())
            }
            _ => Err(RpcError::new("unknown_method", method.to_string())),
        }
    }

    /// 字幕：读、开始转、停、删。
    pub(crate) async fn subtitles_rpc(
        self: &Arc<Self>,
        session: &Arc<Session>,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        match method {
            "subtitles.get" => {
                let p: IdParam = parse(params)?;
                reply(self.subtitles_snapshot(&p.id))
            }
            "subtitles.start" => {
                let p: StartParams = parse(params)?;
                let model = self.speech.selected();
                let job = {
                    let mut slot = self.subtitles.slot();
                    if let Some(job) = slot.as_ref() {
                        if job.id == p.id {
                            // 已经在转这一期：只告诉它现在放到哪儿了
                            job.set_want(p.position);
                            return reply(job.snapshot());
                        }
                        job.cancel.store(true, Ordering::Relaxed);
                    }
                    let transcript = self.speech.transcripts().load(&p.id).unwrap_or_default();
                    // 转完了的直接用。模型被删了的话，以前转过的部分照样能看，只是没法接着转
                    if transcript.complete() || (model.is_none() && !transcript.cues.is_empty()) {
                        *slot = None;
                        return reply(snapshot(
                            &p.id,
                            SubtitleState::Idle,
                            &transcript,
                            None,
                            0,
                            None,
                        ));
                    }
                    let Some(model) = model else {
                        *slot = None;
                        return Err(RpcError::new(
                            "speech_not_installed",
                            "no speech model is downloaded",
                        ));
                    };
                    let job = Arc::new(Job {
                        model,
                        id: p.id.clone(),
                        cancel: AtomicBool::new(false),
                        want: AtomicU64::new(0),
                        live: Mutex::new(Live {
                            state: SubtitleState::Loading,
                            transcript,
                            error: None,
                            received: 0,
                            total: None,
                        }),
                    });
                    job.set_want(p.position);
                    *slot = Some(job.clone());
                    job
                };
                tokio::spawn(run_job(self.clone(), session.clone(), job.clone(), p.keep));
                reply(job.snapshot())
            }
            "subtitles.stop" => {
                let p: IdParam = parse(params)?;
                self.subtitles.stop(Some(&p.id));
                reply(self.subtitles_snapshot(&p.id))
            }
            // 删掉这一期的字幕（想换个模型重新转时用）
            "subtitles.remove" => {
                let p: IdParam = parse(params)?;
                self.subtitles.stop(Some(&p.id));
                self.speech.transcripts().remove(&p.id);
                reply(self.subtitles_snapshot(&p.id))
            }
            // 字幕的译文：存在本机，再听、重开都不用再翻（AI 翻译要花钱）
            "subtitles.translations" => {
                let p: TranslationsParams = parse(params)?;
                reply(json!({ "entries": self.speech.transcripts().translations(&p.id, &p.lang) }))
            }
            "subtitles.translations.save" => {
                let p: TranslationsParams = parse(params)?;
                self.speech
                    .transcripts()
                    .add_translations(&p.id, &p.lang, p.entries)
                    .map_err(|e| RpcError::new("bad_params", e.to_string()))?;
                reply(json!({}))
            }
            // 本机哪些条目有转写出来的字幕
            "subtitles.list" => reply(json!({ "ids": self.speech.transcripts().list() })),
            "subtitles.clear" => {
                self.subtitles.stop(None);
                self.speech.transcripts().clear();
                reply(json!({}))
            }
            _ => Err(RpcError::new("unknown_method", method.to_string())),
        }
    }

    fn subtitles_snapshot(&self, id: &str) -> Value {
        if let Some(job) = self.subtitles.current(id) {
            return job.snapshot();
        }
        let transcript = self.speech.transcripts().load(id).unwrap_or_default();
        snapshot(id, SubtitleState::Idle, &transcript, None, 0, None)
    }
}

/// 在后台下载一个识别模型，进度通过事件告诉界面。
async fn download_model(core: Arc<Core>, id: String) {
    tracing::info!(model = %id, "downloading speech model");
    let result = core
        .speech
        .download(&id, |p| {
            core.emit(Event::SpeechModel {
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
            tracing::info!(model = %id, "speech model installed");
            (ModelState::Installed, None)
        }
        Err(e) => {
            tracing::warn!("speech model {id}: {e}");
            (ModelState::Failed, Some(e.code().to_string()))
        }
    };
    core.emit(Event::SpeechModel {
        id,
        received: 0,
        total: 0,
        state,
        error,
    });
}

async fn run_job(core: Arc<Core>, session: Arc<Session>, job: Arc<Job>, keep: usize) {
    let result = prepare_and_transcribe(&core, &session, &job, keep).await;
    {
        let mut live = job.live();
        match result {
            Ok(()) => live.state = SubtitleState::Idle,
            Err(e) => {
                tracing::warn!(item = %job.id, "subtitles failed: {e}");
                live.state = SubtitleState::Failed;
                live.error = Some(e);
            }
        }
    }
    // 被别的任务顶掉的不再出声：界面已经在看另一期了
    if !job.cancelled() {
        job.emit(&core, Vec::new(), None);
    }
    let mut slot = core.subtitles.slot();
    if slot.as_ref().is_some_and(|j| Arc::ptr_eq(j, &job)) {
        *slot = None;
    }
}

async fn prepare_and_transcribe(
    core: &Arc<Core>,
    session: &Arc<Session>,
    job: &Arc<Job>,
    keep: usize,
) -> Result<(), RpcError> {
    // 音频要在本机：还没下载就先下载，下了一点就开始转
    let source = match core.episodes.file(&job.id) {
        Some(file) => Source::File(file),
        None => {
            job.set_state(SubtitleState::Downloading);
            job.emit(core, Vec::new(), None);
            core.download_episode(session, job.id.clone(), keep).await?;
            loop {
                if job.cancelled() {
                    return Ok(());
                }
                if let Some(file) = core.episodes.file(&job.id) {
                    break Source::File(file);
                }
                match core.episodes.progress().get(&job.id) {
                    Some(p) if p.error.is_some() => {
                        return Err(RpcError::new(
                            "speech_episode_failed",
                            p.error.clone().unwrap_or_default(),
                        ));
                    }
                    Some(p) => {
                        {
                            let mut live = job.live();
                            live.received = p.received;
                            live.total = p.total;
                        }
                        job.emit(core, Vec::new(), None);
                        // 已经下了一点：边下边转
                        if p.received >= START_BYTES
                            && let Some(part) = p.partial().filter(|f| f.is_file())
                        {
                            let ext = p
                                .file
                                .as_ref()
                                .and_then(|f| f.extension())
                                .and_then(|e| e.to_str())
                                .map(str::to_string);
                            break Source::Growing { part, ext };
                        }
                    }
                    // 刚好下完：下一圈就能找到文件
                    None if core.episodes.file(&job.id).is_some() => continue,
                    None => {
                        return Err(RpcError::new(
                            "speech_episode_failed",
                            "the episode could not be downloaded",
                        ));
                    }
                }
                tokio::time::sleep(POLL).await;
            }
        }
    };
    if job.cancelled() {
        return Ok(());
    }
    {
        let mut live = job.live();
        live.state = SubtitleState::Loading;
        if matches!(source, Source::File(_)) {
            live.received = 0;
            live.total = None;
        }
    }
    job.emit(core, Vec::new(), None);
    let (core, job) = (core.clone(), job.clone());
    tokio::task::spawn_blocking(move || {
        let growing = matches!(source, Source::Growing { .. });
        let mut result = transcribe(&core, &job, source);
        // 边下边读不了的文件（个别格式要整个下完才认得出来）：不报错，等下完了自动从头转，
        // 不用用户点「重试」
        if growing
            && !job.cancelled()
            && result.as_ref().is_err_and(|e| e.code == "speech_audio_failed")
        {
            if let Err(e) = &result {
                tracing::info!(item = %job.id, "cannot read while downloading ({}); waiting for the whole episode", e.message);
            }
            result = wait_for_download(&core, &job).and_then(|file| {
                {
                    let mut live = job.live();
                    live.state = SubtitleState::Loading;
                    live.received = 0;
                    live.total = None;
                }
                job.emit(&core, Vec::new(), None);
                transcribe(&core, &job, Source::File(file))
            });
        }
        // 被取消的（换了一期、关了字幕）不算出错
        if job.cancelled() { Ok(()) } else { result }
    })
    .await
    .map_err(|e| RpcError::new("internal", e.to_string()))?
}

/// 转写的主循环，跑在阻塞线程里。
fn transcribe(core: &Arc<Core>, job: &Arc<Job>, source: Source) -> Result<(), RpcError> {
    let model = job.model.as_str();
    let store = core.speech.transcripts();
    let engine = core.speech.load(model)?;
    let mut reader = match source {
        Source::File(file) => AudioReader::open(&file)?,
        Source::Growing { part, ext } => match std::fs::File::open(&part) {
            Ok(file) => {
                let growing = GrowingFile {
                    file,
                    core: core.clone(),
                    job: job.clone(),
                };
                AudioReader::from_source(Box::new(growing), ext.as_deref())?
            }
            // 加载模型的这一会儿刚好下完了，.part 已经改名：直接读下好的文件
            Err(e) if e.kind() == io::ErrorKind::NotFound => match core.episodes.file(&job.id) {
                Some(file) => AudioReader::open(&file)?,
                None => return Err(RpcError::new("io", e.to_string())),
            },
            Err(e) => return Err(RpcError::new("io", e.to_string())),
        },
    };
    {
        let mut live = job.live();
        live.transcript.model = model.to_string();
        if let Some(d) = reader.duration() {
            live.transcript.duration = Some(d);
        }
        live.state = SubtitleState::Running;
    }
    job.emit(core, Vec::new(), None);
    tracing::info!(item = %job.id, model, "transcribing");
    let started = Instant::now();
    let mut last_save = Instant::now();
    let save = |job: &Job| {
        let transcript = job.live().transcript.clone();
        if let Err(e) = store.save(&job.id, &transcript) {
            tracing::warn!(item = %job.id, "cannot save subtitles: {e}");
        }
    };

    while !job.cancelled() {
        // 先转播放位置后面还没转的；后面都转完了，再从头补前面的
        let (duration, gap) = {
            let live = job.live();
            let duration = live.transcript.duration.unwrap_or(f64::INFINITY);
            let covered = &live.transcript.covered;
            let gap = covered
                .next_gap(job.want(), duration)
                .or_else(|| covered.next_gap(0.0, duration));
            (duration, gap)
        };
        let Some((from, to)) = gap else { break };

        if (from > 0.0 || reader.position() > 0.0)
            && let Err(e) = reader.seek(from)
        {
            // 文件头里写的时长比实际长时，最后一小段跳不过去：就当到头了
            if duration.is_finite() && duration - from < 5.0 {
                job.live().transcript.duration = Some(from);
                continue;
            }
            return Err(e.into());
        }

        let before = job.live().transcript.covered.clone();
        let mut emitted = from;
        let mut batch: Vec<Cue> = Vec::new();
        let mut batch_from = from;
        let mut last_emit = Instant::now();
        // 认出来的说话人在任务和字幕文件里各有一份：转写时用本地的，每一步抄回去
        let mut speakers = job.live().transcript.speakers.clone();
        let outcome = engine.run(&mut reader, to, &mut speakers, &mut |step| {
            let reached = step.reached.max(emitted);
            // 跳转通常落在要的位置前面一点，那一小段已经转过了，不要重复的句子
            let new: Vec<Cue> = step
                .cues
                .into_iter()
                .filter(|c| c.start >= from - 0.01 && c.start < to)
                .collect();
            {
                let mut live = job.live();
                cues::merge(&mut live.transcript.cues, emitted, reached, new.clone());
                live.transcript.covered.add(from, reached);
                live.transcript.speakers = step.speakers;
            }
            batch.extend(new);
            emitted = reached;
            if last_emit.elapsed() >= EMIT_EVERY {
                last_emit = Instant::now();
                job.track_download(core);
                job.emit(
                    core,
                    std::mem::take(&mut batch),
                    Some((batch_from, emitted)),
                );
                batch_from = emitted;
            }
            if last_save.elapsed() >= SAVE_EVERY {
                last_save = Instant::now();
                save(job);
            }
            if job.cancelled() {
                return false;
            }
            // 用户拖到了别处、那里还没转：停下，改从那里转
            let want = job.want();
            let nearby = want >= from - 1.0 && want <= reached + LOOKAHEAD;
            nearby || job.live().transcript.covered.end_of(want).is_some()
        })?;

        {
            let mut live = job.live();
            if outcome.eof {
                // 读到结尾才知道确切的长度
                let end = reader.position();
                live.transcript.duration = Some(end);
                live.transcript.covered.add(from, end);
            } else if live.transcript.covered == before && !job.cancelled() {
                // 一点没往前走（这一小段读不出东西）：记成转过了，免得原地打转
                let end = if to.is_finite() { to } else { from + 1.0 };
                live.transcript.covered.add(from, end);
                emitted = emitted.max(end);
            }
        }
        job.emit(core, batch, Some((batch_from, emitted.max(batch_from))));
        last_save = Instant::now();
        save(job);
    }
    save(job);
    let live = job.live();
    tracing::info!(
        item = %job.id,
        cues = live.transcript.cues.len(),
        complete = live.transcript.complete(),
        seconds = started.elapsed().as_secs(),
        "transcription stopped"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_cancelled_read_stops_instead_of_spinning() {
        let dir = tempfile::tempdir().unwrap();
        let core = Core::open_with_secrets(dir.path(), Arc::new(crate::MemorySecrets::default()))
            .await
            .unwrap();
        let part = dir.path().join("ep.part");
        std::fs::write(&part, b"").unwrap();
        let job = Arc::new(Job {
            id: "ep".into(),
            model: "m".into(),
            cancel: AtomicBool::new(true),
            want: AtomicU64::new(0),
            live: Mutex::new(Live {
                state: SubtitleState::Running,
                transcript: Default::default(),
                error: None,
                received: 0,
                total: None,
            }),
        });
        let mut file = GrowingFile {
            file: std::fs::File::open(&part).unwrap(),
            core,
            job,
        };
        // 解码器用 read_exact 读：取消后要报错返回，不能当成「再试一次」一直转
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4];
            let _ = tx.send(file.read_exact(&mut buf).is_err());
        });
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(true));
    }

    #[test]
    fn seeking_from_the_end_counts_the_whole_file() {
        // 下了 512 KB、整个 48 MB：跳到末尾是 48 MB 那儿，不是已经下到的地方
        let total = Some(48_877_686);
        assert_eq!(
            from_start(SeekFrom::End(0), total).unwrap(),
            SeekFrom::Start(48_877_686)
        );
        assert_eq!(
            from_start(SeekFrom::End(-128), total).unwrap(),
            SeekFrom::Start(48_877_558)
        );
        assert!(from_start(SeekFrom::End(-50_000_000), total).is_err());
        // 从开头、从当前位置跳的不用换；不知道多大的照原样
        assert_eq!(
            from_start(SeekFrom::Start(10), total).unwrap(),
            SeekFrom::Start(10)
        );
        assert_eq!(
            from_start(SeekFrom::Current(-4), total).unwrap(),
            SeekFrom::Current(-4)
        );
        assert_eq!(
            from_start(SeekFrom::End(0), None).unwrap(),
            SeekFrom::End(0)
        );
    }
}
