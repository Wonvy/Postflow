//! 每个邮箱账户一个后台任务：保持一条 IMAP 连接，同步收件箱，把本机的改动写回服务器，
//! 服务器支持 IDLE 时用它实时收到新邮件。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use postflow_mail::{
    Auth, Connection, FlagChange, FolderInfo, Listing, Security, ServerConfig, SpecialMailbox,
};
use postflow_store::{
    Account, ItemPatch, MailFolder, MailRef, MailSync, MailSyncStats, NewMail, ids,
};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::{Core, Event, RpcError, Scope, Session};

const INBOX: &str = "INBOX";
/// 第一次同步取最新的这么多封。
const INITIAL_COUNT: u32 = 200;
/// 垃圾邮件和已删除第一次同步取最新的这么多封。
const OTHER_INITIAL_COUNT: u32 = 100;
/// 一次同步最多新取这么多封，防止一下子涌进几万封时卡住。
const MAX_NEW_PER_SYNC: usize = 500;
/// 只核对最新这么多封的状态。更早的邮件很少会被改动。
const FLAG_WINDOW: usize = 1000;
/// 不超过这个大小的邮件同步时就把原文取回来，更大的打开时再取。
const EAGER_BODY_LIMIT: u32 = 2 * 1024 * 1024;
const FETCH_BATCH: usize = 20;
/// IDLE 最多等这么久就结束，重新同步一次再接着等。大多数服务器会在 30 分钟左右断开空闲的
/// IDLE；有的服务器从不推送新邮件，只发「还在」，这时它就是收到新邮件的最长延迟。
/// 多久做一次完整同步（所有文件夹）。IDLE 收到新邮件时会提前。
const FULL_SYNC_INTERVAL: Duration = Duration::from_secs(5 * 60);
/// 服务器不支持 IDLE 时多久查一次。
/// 连接最多安静多久就要跟服务器说句话（重新开始 IDLE，或者发 NOOP）。
/// 很多服务器、网关和代理会悄悄关掉一段时间没动静的连接。
const KEEPALIVE_START: Duration = Duration::from_secs(2 * 60);
/// 学到的间隔最短这么多，免得遇到真正的网络故障时越缩越短。
const KEEPALIVE_MIN: Duration = Duration::from_secs(20);
const MIN_BACKOFF: Duration = Duration::from_secs(15);
const MAX_BACKOFF: Duration = Duration::from_secs(15 * 60);
/// 服务器拒绝登录后，多久自己再试一次。有的拒绝是临时的（比如登录太频繁），密码真错了也只多试这一次。
const AUTH_RETRY: Duration = Duration::from_secs(15 * 60);

pub(crate) enum Command {
    /// 马上同步一次，界面上显示正在刷新。用户点了刷新。
    Sync,
    /// 应用自己发起的同步（窗口回到前台时）。和 `Sync` 一样，但不显示正在刷新，
    /// 也不会叫醒因为登录被拒而停着的任务：免得用户一切换窗口就去试一次错的密码。
    CatchUp,
    /// 把本机改过的状态写回服务器。
    Apply(Vec<MailRef>, ItemPatch),
    /// 取一封邮件的原文，同步时没取的大邮件打开时用。
    FetchBody(MailRef, oneshot::Sender<Result<Vec<u8>, RpcError>>),
    /// 把一封草稿存进服务器的「草稿」，再删掉它替换的旧版本（`replace` 是旧版本的 Message-ID）。
    SaveDraft {
        raw: Vec<u8>,
        replace: Option<String>,
        reply: oneshot::Sender<Result<(), RpcError>>,
    },
    /// 删掉一封草稿：发出去了，或者用户不要了。
    DeleteDraft { message_id: String },
    /// 列出邮箱里的普通文件夹。
    ListFolders(oneshot::Sender<Result<Vec<FolderInfo>, RpcError>>),
    /// 用户打开了一个普通文件夹：马上同步它，这次连接期间每次同步都带上它。
    WatchFolder(String),
    /// 把邮件移到另一个文件夹（服务器上的名字）。本机已经先删掉了，移成以后由目标文件夹的同步带回来。
    MoveTo(Vec<MailRef>, String),
    /// 从服务器取更早的邮件：新加了几封、更早的还有没有。
    LoadOlder(OlderFrom, oneshot::Sender<Result<(usize, bool), RpcError>>),
}

/// 同时跟着同步的普通文件夹最多这么多个，多了先不管最早打开的。
const MAX_WATCHED: usize = 8;

pub(crate) struct Worker {
    tx: mpsc::UnboundedSender<Command>,
    task: JoinHandle<()>,
}

impl Worker {
    pub(crate) fn send(&self, cmd: Command) -> bool {
        self.tx.send(cmd).is_ok()
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// 错误码给界面翻译成人话，message 里放服务器的原话，界面会一起显示。
pub(crate) fn mail_error(err: &postflow_mail::Error) -> RpcError {
    let detail = err.detail();
    RpcError::new(
        err.code(),
        if detail.is_empty() {
            err.to_string()
        } else {
            detail.to_string()
        },
    )
}

/// 连接本身出了问题（断线、超时），重连可能就好了。服务器拒绝某个命令则不是。
/// 一个邮箱的「多久说一句话」。连接在安静等待时被关掉，就按安静了多久把间隔缩短，
/// 之后每次重连都用新的间隔（应用重启后从头学）。
#[derive(Debug)]
pub(crate) struct Keepalive {
    pub(crate) window: Duration,
    /// 正在安静等待（IDLE 或两次收信之间）时，从什么时候开始的。
    waiting_since: Option<std::time::Instant>,
}

impl Keepalive {
    pub(crate) fn new() -> Self {
        Keepalive {
            window: KEEPALIVE_START,
            waiting_since: None,
        }
    }

    /// 连接在安静了 `quiet` 之后被关掉了：以后在那之前就说话。返回新的间隔（没变返回 None）。
    pub(crate) fn dropped_after(&mut self, quiet: Duration) -> Option<Duration> {
        let next = (quiet * 7 / 10).min(self.window / 2).max(KEEPALIVE_MIN);
        if next < self.window {
            self.window = next;
            Some(next)
        } else {
            None
        }
    }
}

fn is_connection_error(err: &postflow_mail::Error) -> bool {
    use postflow_mail::Error as E;
    matches!(err, E::Io(_) | E::Timeout | E::Connect(_) | E::Tls(_))
}

pub(crate) fn parse_security(s: &str) -> Result<Security, RpcError> {
    match s {
        "tls" => Ok(Security::Tls),
        "starttls" => Ok(Security::Starttls),
        "none" => Ok(Security::None),
        other => Err(RpcError::new(
            "bad_params",
            format!("unknown security mode: {other}"),
        )),
    }
}

pub(crate) fn security_str(s: Security) -> &'static str {
    match s {
        Security::Tls => "tls",
        Security::Starttls => "starttls",
        Security::None => "none",
    }
}

pub(crate) fn server_config(account: &Account) -> Result<ServerConfig, RpcError> {
    let cfg = ServerConfig {
        host: account.imap_host.clone(),
        port: account.imap_port,
        security: parse_security(&account.imap_security)?,
        username: account.username.clone(),
    };
    cfg.validate().map_err(|e| mail_error(&e))?;
    Ok(cfg)
}

/// 发信服务器：自己填过就用填的，否则按服务商预设或收信服务器推断。
pub(crate) fn smtp_config(account: &Account) -> Result<ServerConfig, RpcError> {
    let imap = ServerConfig {
        host: account.imap_host.clone(),
        port: account.imap_port,
        security: parse_security(&account.imap_security)?,
        username: account.username.clone(),
    };
    let mut cfg = match account.smtp_host.as_deref().map(str::trim) {
        Some(host) if !host.is_empty() => ServerConfig {
            host: host.to_string(),
            port: account.smtp_port.unwrap_or(465),
            security: parse_security(account.smtp_security.as_deref().unwrap_or("tls"))?,
            username: account.username.clone(),
        },
        _ => postflow_mail::smtp_for(&account.email, &imap),
    };
    // iCloud 收信用 @ 前面的名字，发信要完整的地址
    if cfg.host.eq_ignore_ascii_case("smtp.mail.me.com") && !cfg.username.contains('@') {
        cfg.username = account.email.trim().to_string();
    }
    cfg.validate().map_err(|e| mail_error(&e))?;
    Ok(cfg)
}

/// 用户名有两种写法的服务商。iCloud 的说明里写着用户名「通常」是不带 @icloud.com 的名字，
/// 有的客户端要用完整地址才行，所以登录被拒时换另一种写法再试一次。
fn alternate_username(cfg: &ServerConfig, email: &str) -> Option<String> {
    if !cfg.host.trim().eq_ignore_ascii_case("imap.mail.me.com") {
        return None;
    }
    let email = email.trim();
    match cfg.username.split_once('@') {
        Some((local, _)) if !local.is_empty() => Some(local.to_string()),
        _ => (email.contains('@') && email != cfg.username).then(|| email.to_string()),
    }
}

/// 确认能登录。被拒时，对用户名有两种写法的服务商换一种再试，返回真正能用的设置。
pub(crate) async fn verify_login(
    cfg: ServerConfig,
    email: &str,
    password: &str,
) -> Result<ServerConfig, RpcError> {
    let password = Auth::Password(password.to_string());
    let first = match verify(&cfg, &password).await {
        Ok(()) => return Ok(cfg),
        Err(e) => e,
    };
    tracing::warn!(host = %cfg.host, port = cfg.port, code = %first.code, "login check failed: {}", first.message);
    if first.code == "mail_auth"
        && let Some(username) = alternate_username(&cfg, email)
    {
        tracing::info!(host = %cfg.host, "login rejected, trying the other form of the user name");
        let retry = ServerConfig { username, ..cfg };
        if verify(&retry, &password).await.is_ok() {
            return Ok(retry);
        }
    }
    Err(first)
}

/// 试着连上并打开收件箱，确认设置和密码（或令牌）都对。
pub(crate) async fn verify(cfg: &ServerConfig, auth: &Auth) -> Result<(), RpcError> {
    let mut conn = Connection::connect(cfg, auth)
        .await
        .map_err(|e| mail_error(&e))?;
    let result = conn.select(INBOX).await.map_err(|e| mail_error(&e));
    conn.logout().await;
    result.map(|_| ())
}

/// 新邮件通知最多列几封。
const NOTIFY_LIST: usize = 3;
/// 比这更早的邮件不弹通知：比如电脑关了几天，开机后补收的那些。
const NOTIFY_MAX_AGE_MS: i64 = 2 * 24 * 60 * 60 * 1000;

impl Core {
    /// 告诉界面来了新邮件。补收的旧邮件不算，太多时只列最新的几封。
    fn announce_new_mail(&self, account_id: &str, fresh: Vec<postflow_store::FreshMail>) {
        let since = crate::refresh::now() - NOTIFY_MAX_AGE_MS;
        let mut mails: Vec<_> = fresh.into_iter().filter(|m| m.date >= since).collect();
        if mails.is_empty() {
            return;
        }
        mails.sort_by_key(|m| std::cmp::Reverse(m.date));
        let total = mails.len();
        let senders = mails
            .iter()
            .map(|m| m.address.clone().unwrap_or_default())
            .collect();
        mails.truncate(NOTIFY_LIST);
        tracing::debug!(account = %account_id, total, "new mail");
        self.emit(Event::NewMail {
            account_id: account_id.to_string(),
            mails,
            total,
            senders,
        });
    }

    /// 为一个账户启动后台任务。已有的会先停掉，这样改了设置或密码会立刻生效。
    pub(crate) fn start_worker(self: &Arc<Self>, session: &Arc<Session>, account_id: &str) {
        let (tx, rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(run(
            self.clone(),
            session.clone(),
            account_id.to_string(),
            rx,
        ));
        let old = session
            .workers()
            .insert(account_id.to_string(), Worker { tx, task });
        drop(old);
    }

    /// 用现在的解码重新整理以前同步下来、列表里是乱码的邮件。每份索引只做一次。
    pub(crate) async fn repair_mail_text(self: &Arc<Self>, session: &Arc<Session>) {
        let fixed = session
            .db(|s| {
                s.repair_mail_text(|raw| {
                    let summary = postflow_mail::summarize(raw);
                    (summary.subject, summary.snippet, summary.from.name)
                })
            })
            .await;
        match fixed {
            Ok(0) => {}
            Ok(count) => {
                tracing::info!(count, "mail: repaired garbled text");
                self.emit_changed(&[Scope::Feeds, Scope::Items], None);
            }
            Err(e) => tracing::warn!("mail: could not repair garbled text: {}", e.message),
        }
    }

    pub(crate) fn stop_worker(&self, session: &Session, account_id: &str) {
        let worker = session.workers().remove(account_id);
        drop(worker);
    }

    /// 让后台任务和账户列表对上：其他设备加了或删了邮箱时调用。
    pub(crate) async fn reconcile_workers(self: &Arc<Self>, session: &Arc<Session>) {
        let Ok(accounts) = session.db(|s| s.accounts()).await else {
            return;
        };
        let wanted: HashSet<String> = accounts.into_iter().map(|a| a.id).collect();
        let running: HashSet<String> = session.workers().keys().cloned().collect();
        for id in running.difference(&wanted) {
            self.stop_worker(session, id);
        }
        for id in wanted.difference(&running) {
            self.start_worker(session, id);
        }
    }

    /// 让一个邮箱悄悄同步一次（不显示正在刷新），比如刚发了信，要把「已发送」里的新邮件收进来。
    pub(crate) fn nudge_mail(session: &Session, account_id: &str) -> bool {
        session
            .workers()
            .get(account_id)
            .is_some_and(|w| w.send(Command::CatchUp))
    }

    pub(crate) fn send_mail(&self, session: &Session, account_id: &str, cmd: Command) -> bool {
        session
            .workers()
            .get(account_id)
            .is_some_and(|w| w.send(cmd))
    }

    /// 所有邮箱马上收一次信。没有后台任务的、任务已经结束的，重新启动。
    pub(crate) async fn sync_all_mail(self: &Arc<Self>, session: &Arc<Session>, by_user: bool) {
        self.reconcile_workers(session).await;
        let ids: Vec<String> = session.workers().keys().cloned().collect();
        for id in ids {
            self.sync_account(session, &id, by_user);
        }
    }

    /// 让一个邮箱马上收一次信。`by_user` 为假时是应用自己发起的。任务已经结束了就重新启动。
    /// 不是邮箱账户时返回 false。
    pub(crate) fn sync_account(
        self: &Arc<Self>,
        session: &Arc<Session>,
        account_id: &str,
        by_user: bool,
    ) -> bool {
        if !session.workers().contains_key(account_id) {
            return false;
        }
        let command = || {
            if by_user {
                Command::Sync
            } else {
                Command::CatchUp
            }
        };
        if !self.send_mail(session, account_id, command()) {
            tracing::warn!(account = %account_id, "mail task had stopped; starting it again");
            self.start_worker(session, account_id);
            self.send_mail(session, account_id, command());
        }
        true
    }

    /// 把本机改过状态的邮件按账户分组，交给各自的后台任务写回服务器。
    pub(crate) fn dispatch(&self, session: &Session, refs: Vec<MailRef>, patch: &ItemPatch) {
        let mut by_account: HashMap<String, Vec<MailRef>> = HashMap::new();
        for r in refs {
            by_account.entry(r.account_id.clone()).or_default().push(r);
        }
        for (account, refs) in by_account {
            self.send_mail(session, &account, Command::Apply(refs, patch.clone()));
        }
    }

    async fn record_mail(
        &self,
        session: &Session,
        account_id: &str,
        error: Option<(&str, String)>,
    ) {
        let id = account_id.to_string();
        let error = error.map(|(c, m)| (c.to_string(), m));
        let result = session
            .db(move |s| {
                s.record_mail_sync(&id, error.as_ref().map(|(c, m)| (c.as_str(), m.as_str())))
            })
            .await;
        if let Err(e) = result {
            tracing::warn!("could not record mail state: {}", e.message);
        }
        self.emit_changed(&[Scope::Feeds], None);
    }
}

/// 后台任务的主循环：连上、干活、出错就退避重连。
async fn run(
    core: Arc<Core>,
    session: Arc<Session>,
    account_id: String,
    mut rx: mpsc::UnboundedReceiver<Command>,
) {
    let mut pending: Vec<Command> = Vec::new();
    let mut backoff = MIN_BACKOFF;
    // 用户打开过的普通文件夹，最近打开的在最后。断线重连后接着同步它们
    let mut watched: Vec<String> = Vec::new();
    // 这个邮箱多久要跟服务器说一句话，重连之间保留（连接在安静时被关掉会把它缩短）
    let mut keepalive = Keepalive::new();
    loop {
        let id = account_id.clone();
        let Ok(Some(account)) = session.db(move |s| s.account(&id)).await else {
            return;
        };
        let auth = match core.mail_auth(&account_id).await {
            Ok(auth) => auth,
            // 换 OAuth 令牌失败：登录过期了就等用户重新登录，网络问题等一会儿再试
            Err(e) => {
                core.record_mail(
                    &session,
                    &account_id,
                    Some((e.code.as_str(), e.message.clone())),
                )
                .await;
                if crate::oauth::needs_sign_in(&e.code) {
                    if park(&mut rx, &mut pending, &e.code, None).await {
                        continue;
                    }
                    return;
                }
                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    cmd = rx.recv() => match cmd {
                        Some(cmd) => pending.push(cmd),
                        None => return,
                    }
                }
                backoff = (backoff * 2).min(MAX_BACKOFF);
                continue;
            }
        };
        let Some(auth) = auth else {
            tracing::warn!(account = %account_id, "no password on this device; waiting for the user");
            core.record_mail(
                &session,
                &account_id,
                Some(("mail_no_password", "no password on this device".into())),
            )
            .await;
            // 等用户在这台设备上输入密码（那时会重新启动这个任务），或者点刷新再读一次钥匙串
            if park(&mut rx, &mut pending, "mail_no_password", None).await {
                continue;
            }
            return;
        };
        let cfg = match server_config(&account) {
            Ok(cfg) => cfg,
            Err(e) => {
                core.record_mail(
                    &session,
                    &account_id,
                    Some((e.code.as_str(), e.message.clone())),
                )
                .await;
                if park(&mut rx, &mut pending, &e.code, None).await {
                    continue;
                }
                return;
            }
        };

        tracing::debug!(account = %account_id, host = %cfg.host, port = cfg.port, "connecting");
        let conn = match Connection::connect(&cfg, &auth).await {
            Ok(conn) => {
                tracing::info!(account = %account_id, host = %cfg.host, idle = conn.supports_idle(), "mail connected");
                conn
            }
            Err(err) => {
                tracing::warn!(
                    account = %account_id,
                    host = %cfg.host,
                    code = err.code(),
                    retry_in = ?(!err.needs_user()).then_some(backoff),
                    "mail connect failed: {err}"
                );
                core.record_mail(
                    &session,
                    &account_id,
                    Some((err.code(), mail_error(&err).message)),
                )
                .await;
                fail_fetches(&mut pending, &err);
                // 令牌被拒（刚好过期、被吊销）：丢掉缓存的，下次重新换
                if matches!(auth, Auth::OAuth2(_)) && matches!(err, postflow_mail::Error::Auth(_)) {
                    core.drop_access_token(&account_id);
                }
                if err.needs_user() {
                    // 服务器拒绝登录：可能是密码错了，也可能只是暂时的（登录太频繁、系统忙）。
                    // 用户点刷新就再试；不点也过一阵自己再试一次
                    let retry = matches!(err, postflow_mail::Error::Auth(_)).then_some(AUTH_RETRY);
                    if park(&mut rx, &mut pending, err.code(), retry).await {
                        continue;
                    }
                    return;
                }
                // 等一会儿再连；用户点刷新就立刻重试
                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    cmd = rx.recv() => match cmd {
                        Some(cmd) => pending.push(cmd),
                        None => return,
                    }
                }
                backoff = (backoff * 2).min(MAX_BACKOFF);
                continue;
            }
        };
        backoff = MIN_BACKOFF;

        let synced_before = account.last_sync_at.is_some();
        match serve(
            &core,
            &session,
            &account_id,
            conn,
            &mut rx,
            &mut pending,
            &mut watched,
            synced_before,
            &mut keepalive,
        )
        .await
        {
            Ok(()) => return,
            Err(err) => {
                // 安静等待时被关掉：多半是服务器或网关清理没动静的连接，不算出错，下次早点说话
                let quiet = keepalive
                    .waiting_since
                    .take()
                    .filter(|_| is_connection_error(&err))
                    .map(|since| since.elapsed());
                if let Some(quiet) = quiet {
                    match keepalive.dropped_after(quiet) {
                        Some(window) => tracing::info!(
                            account = %account_id,
                            quiet_secs = quiet.as_secs(),
                            keepalive_secs = window.as_secs(),
                            "mail: the server closes quiet connections; checking in more often"
                        ),
                        None => {
                            tracing::debug!(account = %account_id, "mail: quiet connection closed, reconnecting: {err}")
                        }
                    }
                } else {
                    tracing::warn!(account = %account_id, code = err.code(), "mail connection dropped: {err}");
                    core.record_mail(
                        &session,
                        &account_id,
                        Some((err.code(), mail_error(&err).message)),
                    )
                    .await;
                }
                fail_fetches(&mut pending, &err);
                // 用户正等着刷新就马上重连，否则稍等一下
                if !pending.iter().any(|c| matches!(c, Command::Sync)) {
                    tokio::time::sleep(MIN_BACKOFF / 3).await;
                }
            }
        }
    }
}

/// 没法工作（没有密码、设置有误、服务器拒绝登录）时停在这里：取正文的请求直接回错误，
/// 要写回服务器的改动留着。用户点刷新、或者过了 `retry_after`，返回 true，调用方重新连；
/// 任务被替换或停掉时返回 false。
async fn park(
    rx: &mut mpsc::UnboundedReceiver<Command>,
    pending: &mut Vec<Command>,
    code: &str,
    retry_after: Option<Duration>,
) -> bool {
    let fail = |reply: oneshot::Sender<Result<Vec<u8>, RpcError>>| {
        let _ = reply.send(Err(RpcError::new(code, "mail account is not available")));
    };
    let fail_draft = |reply: oneshot::Sender<Result<(), RpcError>>| {
        let _ = reply.send(Err(RpcError::new(code, "mail account is not available")));
    };
    let mut keep = Vec::new();
    for cmd in pending.drain(..) {
        match cmd {
            Command::FetchBody(_, reply) => fail(reply),
            Command::SaveDraft { reply, .. } => fail_draft(reply),
            Command::ListFolders(reply) => {
                let _ = reply.send(Err(RpcError::new(code, "mail account is not available")));
            }
            Command::LoadOlder(_, reply) => {
                let _ = reply.send(Err(RpcError::new(code, "mail account is not available")));
            }
            // 这次连接就是为它连的，已经失败了，不能拿它当再试的理由，否则会一直空转
            Command::Sync | Command::CatchUp => {}
            apply => keep.push(apply),
        }
    }
    *pending = keep;
    let timer = async {
        match retry_after {
            Some(after) => tokio::time::sleep(after).await,
            None => std::future::pending().await,
        }
    };
    tokio::pin!(timer);
    loop {
        tokio::select! {
            cmd = rx.recv() => match cmd {
                None => return false,
                Some(Command::Sync) => {
                    pending.push(Command::Sync);
                    return true;
                }
                Some(Command::CatchUp) => {}
                Some(Command::FetchBody(_, reply)) => fail(reply),
                Some(Command::SaveDraft { reply, .. }) => fail_draft(reply),
                Some(Command::ListFolders(reply)) => {
                    let _ = reply.send(Err(RpcError::new(code, "mail account is not available")));
                }
                Some(Command::LoadOlder(_, reply)) => {
                    let _ = reply.send(Err(RpcError::new(code, "mail account is not available")));
                }
                Some(apply) => pending.push(apply),
            },
            _ = &mut timer => return true,
        }
    }
}

fn fail_fetches(pending: &mut Vec<Command>, err: &postflow_mail::Error) {
    let mut keep = Vec::new();
    for cmd in pending.drain(..) {
        match cmd {
            Command::FetchBody(_, reply) => {
                let _ = reply.send(Err(mail_error(err)));
            }
            Command::SaveDraft { reply, .. } => {
                let _ = reply.send(Err(mail_error(err)));
            }
            Command::ListFolders(reply) => {
                let _ = reply.send(Err(mail_error(err)));
            }
            Command::LoadOlder(_, reply) => {
                let _ = reply.send(Err(mail_error(err)));
            }
            other => keep.push(other),
        }
    }
    *pending = keep;
}

/// 连接期间的循环。正常结束（任务被停掉）返回 Ok，连接出错返回 Err。
#[allow(clippy::too_many_arguments)]
async fn serve(
    core: &Arc<Core>,
    session: &Arc<Session>,
    account_id: &str,
    mut conn: Connection,
    rx: &mut mpsc::UnboundedReceiver<Command>,
    pending: &mut Vec<Command>,
    watched: &mut Vec<String>,
    synced_before: bool,
    keepalive: &mut Keepalive,
) -> Result<(), postflow_mail::Error> {
    // 刚连上的第一次同步也在界面上显示，让用户知道在收信
    let mut announce = true;
    // 刚添加的邮箱第一次同步会收进一大批旧邮件，那些不算「新邮件」
    let mut notify = synced_before;
    let mut healthy = false;
    loop {
        // 先执行用户的操作，再同步，免得同步把刚做的改动又改回去
        let commands = std::mem::take(pending);
        let mut iter = commands.into_iter();
        while let Some(cmd) = iter.next() {
            match cmd {
                Command::Sync => announce = true,
                Command::CatchUp => {}
                Command::Apply(refs, patch) => {
                    if let Err(err) = apply(&mut conn, session, &refs, &patch).await {
                        if is_connection_error(&err) {
                            // 连接断了，留着重连后再写
                            pending.push(Command::Apply(refs, patch));
                            pending.extend(iter);
                            return Err(err);
                        }
                        tracing::warn!("mail: server rejected a change: {err}");
                    }
                }
                Command::FetchBody(r, reply) => match fetch_body(&mut conn, session, &r).await {
                    Ok(raw) => {
                        let _ = reply.send(Ok(raw));
                    }
                    Err(err) => {
                        let _ = reply.send(Err(mail_error(&err)));
                        if is_connection_error(&err) {
                            pending.extend(iter);
                            return Err(err);
                        }
                    }
                },
                Command::SaveDraft {
                    raw,
                    replace,
                    reply,
                } => {
                    let result = save_draft(&mut conn, &raw, replace.as_deref()).await;
                    let failed = result.as_ref().err().is_some_and(is_connection_error);
                    let _ = reply.send(result.as_ref().map_err(mail_error).map(|_| ()));
                    if let (true, Err(err)) = (failed, result) {
                        pending.extend(iter);
                        return Err(err);
                    }
                }
                Command::ListFolders(reply) => {
                    let result = conn.folders().await;
                    let failed = result.as_ref().err().is_some_and(is_connection_error);
                    let _ = reply.send(result.as_ref().map(|f| f.clone()).map_err(mail_error));
                    if let (true, Err(err)) = (failed, result) {
                        pending.extend(iter);
                        return Err(err);
                    }
                }
                Command::MoveTo(refs, target) => {
                    if let Err(err) = move_mail(&mut conn, &refs, &target).await {
                        if is_connection_error(&err) {
                            pending.push(Command::MoveTo(refs, target));
                            pending.extend(iter);
                            return Err(err);
                        }
                        // 没移成（目标文件夹没了之类）：下次同步原来的文件夹时它会回到列表里
                        tracing::warn!(account = %account_id, "mail: could not move mail: {err}");
                    }
                }
                Command::LoadOlder(from, reply) => {
                    let result = load_older(&mut conn, session, account_id, from).await;
                    let failed = result.as_ref().err().is_some_and(is_connection_error);
                    let _ = reply.send(result.as_ref().map(|r| *r).map_err(mail_error));
                    if result.as_ref().is_ok_and(|(added, _)| *added > 0) {
                        core.emit_changed(&[Scope::Feeds, Scope::Items], None);
                    }
                    if let (true, Err(err)) = (failed, result) {
                        pending.extend(iter);
                        return Err(err);
                    }
                }
                Command::WatchFolder(name) => {
                    watched.retain(|w| *w != name);
                    watched.push(name);
                    if watched.len() > MAX_WATCHED {
                        watched.remove(0);
                    }
                    announce = true;
                }
                Command::DeleteDraft { message_id } => {
                    if let Err(err) = delete_draft(&mut conn, &message_id).await {
                        if is_connection_error(&err) {
                            // 连接断了，重连后再删
                            pending.push(Command::DeleteDraft { message_id });
                            pending.extend(iter);
                            return Err(err);
                        }
                        tracing::warn!(account = %account_id, "mail: could not delete a draft: {err}");
                    }
                }
            }
        }

        if announce {
            let mail = core.syncing_mail.fetch_add(1, Ordering::SeqCst) + 1;
            let active = core.refreshing.fetch_add(1, Ordering::SeqCst) + 1;
            core.emit(Event::Refresh { active, mail });
        }
        let result = sync_all_folders(&mut conn, session, account_id, watched).await;
        if announce {
            let mail = core.syncing_mail.fetch_sub(1, Ordering::SeqCst) - 1;
            let active = core.refreshing.fetch_sub(1, Ordering::SeqCst) - 1;
            core.emit(Event::Refresh { active, mail });
            announce = false;
        }
        let stats = result?;
        if stats.changed() {
            tracing::info!(
                account = %account_id,
                added = stats.added,
                updated = stats.updated,
                removed = stats.removed,
                "mail synced"
            );
        } else {
            tracing::debug!(account = %account_id, "mail synced, nothing new");
        }
        if !healthy {
            healthy = true;
            core.record_mail(session, account_id, None).await;
        } else {
            let id = account_id.to_string();
            let _ = session.db(move |s| s.record_mail_sync(&id, None)).await;
        }
        if stats.changed() {
            core.emit_changed(&[Scope::Feeds, Scope::Items], None);
        }
        // 新邮件按自动规则处理（分类、标已读……），然后再通知
        if stats.added > 0 {
            core.run_rules_on_new(session).await;
        }
        if notify {
            core.announce_new_mail(account_id, stats.fresh);
        }
        notify = true;

        let synced_at = std::time::Instant::now();
        // 等新邮件或用户的操作。每隔一会儿跟服务器说句话保持连接；没新邮件时不用每次都完整同步
        loop {
            keepalive.waiting_since = Some(std::time::Instant::now());
            let mut closed = false;
            let mut new_data = false;
            if conn.supports_idle() {
                let (next, outcome) = conn
                    .idle(keepalive.window, async {
                        match rx.recv().await {
                            Some(cmd) => pending.push(cmd),
                            None => closed = true,
                        }
                    })
                    .await?;
                conn = next;
                new_data = matches!(outcome, postflow_mail::IdleOutcome::NewData);
            } else {
                tokio::select! {
                    cmd = rx.recv() => match cmd {
                        Some(cmd) => pending.push(cmd),
                        None => closed = true,
                    },
                    _ = tokio::time::sleep(keepalive.window) => {}
                }
                if !closed && pending.is_empty() {
                    conn.noop().await?;
                }
            }
            keepalive.waiting_since = None;
            if closed {
                conn.logout().await;
                return Ok(());
            }
            if new_data || !pending.is_empty() || synced_at.elapsed() >= FULL_SYNC_INTERVAL {
                break;
            }
        }
        // 用户连续操作时，攒一小会儿一起写回去
        tokio::time::sleep(Duration::from_millis(150)).await;
        while let Ok(cmd) = rx.try_recv() {
            pending.push(cmd);
        }
    }
}

/// 同步收件箱，再同步垃圾邮件、已删除、草稿、已发送和用户打开过的文件夹，最后回到收件箱，
/// 好接着用 IDLE 等新邮件。别的文件夹服务器不让打开时跳过，不影响收件箱。
async fn sync_all_folders(
    conn: &mut Connection,
    session: &Arc<Session>,
    account_id: &str,
    watched: &[String],
) -> Result<MailSyncStats, postflow_mail::Error> {
    let mut stats = sync_folder(conn, session, account_id, MailFolder::Inbox, INBOX).await?;
    // 每次都重新看一下有哪些文件夹，连接期间新建、改名的（比如发信时才建的「已发送」）也能找到
    conn.forget_mailboxes();
    let mut others: Vec<(MailFolder, Option<SpecialMailbox>, Option<&str>)> = vec![
        (MailFolder::Junk, Some(SpecialMailbox::Junk), None),
        (MailFolder::Trash, Some(SpecialMailbox::Trash), None),
        (MailFolder::Drafts, Some(SpecialMailbox::Drafts), None),
        (MailFolder::Sent, Some(SpecialMailbox::Sent), None),
    ];
    others.extend(
        watched
            .iter()
            .map(|name| (MailFolder::Other, None, Some(name.as_str()))),
    );
    for (folder, kind, name) in others {
        let mailbox = match (kind, name) {
            (Some(kind), _) => conn.special_mailbox(kind).await,
            (None, name) => Ok(name.map(str::to_string)),
        };
        let result = match mailbox {
            Ok(Some(mailbox)) => sync_folder(conn, session, account_id, folder, &mailbox).await,
            Ok(None) => {
                // 找不到「已发送」这类文件夹时，把服务器列出的文件夹记下来，好知道它叫什么
                if kind.is_some() {
                    let names = conn.mailbox_names().await.unwrap_or_default();
                    tracing::debug!(account = %account_id, ?folder, ?names, "mail: special folder not found");
                }
                continue;
            }
            Err(err) => Err(err),
        };
        match result {
            Ok(s) => {
                stats.added += s.added;
                stats.updated += s.updated;
                stats.removed += s.removed;
            }
            Err(err) if is_connection_error(&err) => return Err(err),
            Err(err) => {
                tracing::warn!(account = %account_id, ?folder, "mail: skipped a folder: {err}")
            }
        }
    }
    if conn.selected() != Some(INBOX) {
        conn.select(INBOX).await?;
    }
    Ok(stats)
}

/// 同步一个文件夹：新邮件取回来，状态变了的更新，服务器上没了的删掉。
async fn sync_folder(
    conn: &mut Connection,
    session: &Arc<Session>,
    account_id: &str,
    folder: MailFolder,
    mailbox: &str,
) -> Result<MailSyncStats, postflow_mail::Error> {
    let started_at = crate::refresh::now();
    let info = conn.select(mailbox).await?;
    let id = account_id.to_string();
    let name = mailbox.to_string();
    let previous = session
        .db(move |s| s.mail_folder(&id, folder, &name))
        .await
        .ok()
        .flatten();
    // 文件夹被服务器重建了，或者这次找到的是另一个文件夹
    if let Some((old_name, v)) = previous
        && (old_name != mailbox || v != info.uidvalidity)
    {
        let id = account_id.to_string();
        let _ = session
            .db(move |s| s.reset_mail_folder(&id, folder, &old_name))
            .await;
    }

    let (id, name) = (account_id.to_string(), mailbox.to_string());
    let known = session
        .db(move |s| s.known_mail(&id, &name))
        .await
        .unwrap_or_default();
    let mut window: Vec<u32> = known.iter().map(|k| k.uid).collect();
    window.sort_unstable_by(|a, b| b.cmp(a));
    window.truncate(FLAG_WINDOW);

    let initial = match folder {
        MailFolder::Inbox => INITIAL_COUNT,
        _ => OTHER_INITIAL_COUNT,
    };
    let listing: Vec<Listing> = match window.last() {
        None => conn.list_newest(info.exists, initial).await?,
        Some(&from) => conn.list_from(from).await?,
    };

    let known_by_uid: HashMap<u32, &postflow_store::KnownMail> =
        known.iter().map(|k| (k.uid, k)).collect();
    let listed: HashSet<u32> = listing.iter().map(|l| l.uid).collect();

    let mut flags = Vec::new();
    let mut new: Vec<&Listing> = Vec::new();
    for l in &listing {
        match known_by_uid.get(&l.uid) {
            Some(k) if k.read != l.seen || k.starred != l.flagged => {
                flags.push((l.uid, l.seen, l.flagged))
            }
            Some(_) => {}
            None => new.push(l),
        }
    }
    new.sort_unstable_by_key(|l| std::cmp::Reverse(l.uid));
    new.truncate(MAX_NEW_PER_SYNC);
    let vanished: Vec<u32> = window
        .iter()
        .copied()
        .filter(|uid| !listed.contains(uid))
        .collect();

    let place = Place {
        account_id,
        folder,
        mailbox,
        uidvalidity: info.uidvalidity,
    };
    let (mails, parts) = fetch_mails(conn, &place, &new, &known, started_at).await?;

    let sync = MailSync {
        uidvalidity: info.uidvalidity,
        started_at,
        new: mails,
        flags,
        vanished,
    };
    let (id, name) = (account_id.to_string(), mailbox.to_string());
    session
        .db(move |s| {
            let stats = s.apply_folder_sync(&id, folder, &name, sync)?;
            save_parts(s, &parts)?;
            Ok(stats)
        })
        .await
        .map_err(|e| postflow_mail::Error::Io(format!("could not save mail: {}", e.message)))
}

/// 一个文件夹：哪个账户、什么用途、服务器上叫什么、这次的 UIDVALIDITY。
struct Place<'a> {
    account_id: &'a str,
    folder: MailFolder,
    mailbox: &'a str,
    uidvalidity: u32,
}

/// 只取了信头的大邮件，从邮件结构里认出来的附件：记下来，附件总览里先列出名字。
type Parts = Vec<(String, Vec<postflow_store::AttachmentMeta>)>;

fn save_parts(s: &postflow_store::Store, parts: &Parts) -> postflow_store::Result<()> {
    for (id, list) in parts {
        s.set_structure_attachments(id, list)?;
    }
    Ok(())
}

/// 取回这些邮件的原文（大邮件只取信头和结构），整理成要存的样子。
async fn fetch_mails(
    conn: &mut Connection,
    place: &Place<'_>,
    listing: &[&Listing],
    known: &[postflow_store::KnownMail],
    now: i64,
) -> Result<(Vec<NewMail>, Parts), postflow_mail::Error> {
    let mut parts: Parts = Vec::new();
    // 同一个 Message-ID 在收件箱里出现两次时，给后来的那封换个 ID，免得互相覆盖
    let mut taken: HashMap<String, u32> = known.iter().map(|k| (k.id.clone(), k.uid)).collect();
    let mut mails = Vec::with_capacity(listing.len());
    let (small, large): (Vec<&Listing>, Vec<&Listing>) =
        listing.iter().partition(|l| l.size <= EAGER_BODY_LIMIT);
    for (group, header_only) in [(small, false), (large, true)] {
        for chunk in group.chunks(FETCH_BATCH) {
            let uids: Vec<u32> = chunk.iter().map(|l| l.uid).collect();
            let mut structure: HashMap<u32, Vec<postflow_mail::AttachmentInfo>> = HashMap::new();
            let fetched: HashMap<u32, Vec<u8>> = if header_only {
                match conn.fetch_headers(&uids).await {
                    Ok(list) => list
                        .into_iter()
                        .map(|(uid, raw, found)| {
                            structure.insert(uid, found);
                            (uid, raw)
                        })
                        .collect(),
                    // 有的服务器给的结构读不懂：只取信头，附件等打开邮件再说
                    Err(e) => {
                        tracing::info!("mail structure unavailable, fetching headers only: {e}");
                        conn.fetch_raw(&uids, true).await?.into_iter().collect()
                    }
                }
            } else {
                conn.fetch_raw(&uids, false).await?.into_iter().collect()
            };
            for l in chunk {
                let Some(raw) = fetched.get(&l.uid) else {
                    continue;
                };
                let mail = new_mail(
                    place.account_id,
                    place.folder,
                    place.mailbox,
                    place.uidvalidity,
                    l,
                    raw,
                    !header_only,
                    now,
                    &mut taken,
                );
                if let Some(found) = structure.remove(&l.uid).filter(|f| !f.is_empty()) {
                    parts.push((
                        mail.id.clone(),
                        found
                            .into_iter()
                            .map(|a| postflow_store::AttachmentMeta {
                                index: a.index,
                                name: a.name,
                                mime: a.mime,
                                size: a.size as u64,
                            })
                            .collect(),
                    ));
                }
                mails.push(mail);
            }
        }
    }
    Ok((mails, parts))
}

/// 「加载更早的邮件」从哪个文件夹取。
#[derive(Debug, Clone)]
pub(crate) enum OlderFrom {
    Inbox,
    Special(SpecialMailbox),
    Folder(String),
}

/// 每次「加载更早的邮件」取这么多封。
const OLDER_BATCH: usize = 100;

/// 取本机已有的最早一封之前的 `OLDER_BATCH` 封。返回新加了几封、更早的还有没有。
async fn load_older(
    conn: &mut Connection,
    session: &Arc<Session>,
    account_id: &str,
    from: OlderFrom,
) -> Result<(usize, bool), postflow_mail::Error> {
    let (folder, mailbox) = match from {
        OlderFrom::Inbox => (MailFolder::Inbox, INBOX.to_string()),
        OlderFrom::Folder(name) => (MailFolder::Other, name),
        OlderFrom::Special(kind) => {
            let folder = match kind {
                SpecialMailbox::Junk => MailFolder::Junk,
                SpecialMailbox::Trash => MailFolder::Trash,
                SpecialMailbox::Sent => MailFolder::Sent,
                SpecialMailbox::Drafts => MailFolder::Drafts,
            };
            match conn.special_mailbox(kind).await? {
                Some(name) => (folder, name),
                None => return Ok((0, false)),
            }
        }
    };
    let info = conn.select(&mailbox).await?;
    let (id, name) = (account_id.to_string(), mailbox.clone());
    let known = session
        .db(move |s| s.known_mail(&id, &name))
        .await
        .unwrap_or_default();
    // 还没同步过的文件夹交给平常的同步
    let Some(oldest) = known.iter().map(|k| k.uid).min() else {
        conn.select(INBOX).await?;
        return Ok((0, false));
    };
    let started_at = crate::refresh::now();
    let (listing, more) = conn.list_before(oldest, OLDER_BATCH).await?;
    let place = Place {
        account_id,
        folder,
        mailbox: &mailbox,
        uidvalidity: info.uidvalidity,
    };
    let refs: Vec<&Listing> = listing.iter().collect();
    let (mails, parts) = fetch_mails(conn, &place, &refs, &known, started_at).await?;
    let added = mails.len();
    let sync = MailSync {
        uidvalidity: info.uidvalidity,
        started_at,
        new: mails,
        flags: Vec::new(),
        vanished: Vec::new(),
    };
    let (id, name) = (account_id.to_string(), mailbox.clone());
    session
        .db(move |s| {
            s.apply_folder_sync(&id, folder, &name, sync)?;
            save_parts(s, &parts)
        })
        .await
        .map_err(|e| postflow_mail::Error::Io(format!("could not save mail: {}", e.message)))?;
    conn.select(INBOX).await?;
    Ok((added, more))
}

#[allow(clippy::too_many_arguments)]
fn new_mail(
    account_id: &str,
    folder: MailFolder,
    mailbox: &str,
    uidvalidity: u32,
    l: &Listing,
    raw: &[u8],
    full: bool,
    now: i64,
    taken: &mut HashMap<String, u32>,
) -> NewMail {
    let summary = postflow_mail::summarize(raw);
    let key = summary
        .message_id
        .clone()
        .unwrap_or_else(|| format!("uid:{uidvalidity}:{}", l.uid));
    // 有的服务器删除时是复制一份到已删除，同一封邮件可能同时在收件箱和已删除里。
    // 各用各的 ID，免得两边的同步互相覆盖
    let key = match folder {
        MailFolder::Inbox => key,
        MailFolder::Junk => format!("junk:{key}"),
        MailFolder::Trash => format!("trash:{key}"),
        MailFolder::Drafts => format!("draft:{key}"),
        MailFolder::Sent => format!("sent:{key}"),
        MailFolder::Other => format!("box:{mailbox}:{key}"),
    };
    let mut id = ids::mail_id(account_id, &key);
    if taken.get(&id).is_some_and(|uid| *uid != l.uid) {
        id = ids::mail_id(account_id, &format!("{key}#{}", l.uid));
    }
    taken.insert(id.clone(), l.uid);
    let date = summary
        .date
        .filter(|d| *d > 0)
        .or(l.internal_date)
        .unwrap_or(now)
        .min(now);
    // 草稿和已发送在列表里显示写给谁；发件人是自己，不进联系人，收件人进
    let mine = matches!(folder, MailFolder::Drafts | MailFolder::Sent);
    let recipients: Vec<(Option<String>, String)> = if folder == MailFolder::Sent {
        summary
            .to
            .iter()
            .chain(&summary.cc)
            .filter_map(|m| Some((m.name.clone(), m.address.clone()?)))
            .collect()
    } else {
        Vec::new()
    };
    let (from_name, from_address) = if mine {
        let to: Vec<String> = summary
            .to
            .iter()
            .filter_map(|m| m.name.clone().or_else(|| m.address.clone()))
            .collect();
        ((!to.is_empty()).then(|| to.join(", ")), None)
    } else {
        (summary.from.name, summary.from.address)
    };
    NewMail {
        id,
        uid: l.uid,
        message_id: summary.message_id,
        subject: summary.subject,
        from_name,
        from_address,
        snippet: summary.snippet,
        date,
        read: l.seen,
        starred: l.flagged,
        size: l.size,
        // 只取了信头的大邮件，十有八九带着附件
        has_attachments: summary.has_attachments || !full,
        list_unsubscribe: summary.list_unsubscribe,
        raw: full.then(|| raw.to_vec()),
        recipients,
    }
}

/// 找出一批邮件在某个邮箱里的 UID。同步过的文件夹里有记录的直接用；
/// 移到归档以后不知道新 UID 的，按 Message-ID 搜。
async fn resolve(
    conn: &mut Connection,
    mailbox: &str,
    refs: &[&MailRef],
) -> Result<Vec<(String, u32)>, postflow_mail::Error> {
    conn.select(mailbox).await?;
    let mut out = Vec::new();
    for r in refs {
        let uid = match (r.uid, &r.message_id) {
            (Some(uid), _) => Some(uid),
            (None, Some(mid)) => conn.find_by_message_id(mid).await?.into_iter().next(),
            (None, None) => None,
        };
        if let Some(uid) = uid {
            out.push((r.id.clone(), uid));
        }
    }
    Ok(out)
}

async fn apply(
    conn: &mut Connection,
    session: &Arc<Session>,
    refs: &[MailRef],
    patch: &ItemPatch,
) -> Result<(), postflow_mail::Error> {
    let mut by_mailbox: BTreeMap<&str, Vec<&MailRef>> = BTreeMap::new();
    for r in refs {
        by_mailbox.entry(r.mailbox.as_str()).or_default().push(r);
    }
    for (mailbox, refs) in by_mailbox {
        let found = resolve(conn, mailbox, &refs).await?;
        let uids: Vec<u32> = found.iter().map(|(_, uid)| *uid).collect();
        if uids.is_empty() {
            continue;
        }
        if let Some(read) = patch.read {
            conn.set_flag(&uids, FlagChange::Seen(read)).await?;
        }
        if let Some(starred) = patch.starred {
            conn.set_flag(&uids, FlagChange::Flagged(starred)).await?;
        }
        let target = match patch.archived {
            Some(true) if mailbox == INBOX => Some(conn.archive_mailbox().await?),
            Some(false) if mailbox != INBOX => Some(INBOX.to_string()),
            _ => None,
        };
        if let Some(target) = target {
            // archive_mailbox 可能执行过 LIST/CREATE，确保还在原来的邮箱里
            if conn.selected() != Some(mailbox) {
                conn.select(mailbox).await?;
            }
            conn.move_to(&uids, &target).await?;
            for (id, _) in found {
                let target = target.clone();
                let _ = session.db(move |s| s.mail_moved(&id, &target, None)).await;
            }
        }
    }
    conn.select(INBOX).await?;
    Ok(())
}

/// 把邮件移到 `target`，按原来所在的文件夹分批。
async fn move_mail(
    conn: &mut Connection,
    refs: &[MailRef],
    target: &str,
) -> Result<(), postflow_mail::Error> {
    let mut by_mailbox: BTreeMap<&str, Vec<&MailRef>> = BTreeMap::new();
    for r in refs.iter().filter(|r| r.mailbox != target) {
        by_mailbox.entry(r.mailbox.as_str()).or_default().push(r);
    }
    for (mailbox, refs) in by_mailbox {
        let found = resolve(conn, mailbox, &refs).await?;
        let uids: Vec<u32> = found.iter().map(|(_, uid)| *uid).collect();
        conn.move_to(&uids, target).await?;
    }
    conn.select(INBOX).await?;
    Ok(())
}

/// 存草稿：先存新的，再删旧的，中间出错也不会两份都没了。
async fn save_draft(
    conn: &mut Connection,
    raw: &[u8],
    replace: Option<&str>,
) -> Result<(), postflow_mail::Error> {
    let mailbox = conn.drafts_mailbox().await?;
    conn.append_draft(&mailbox, raw).await?;
    if let Some(old) = replace {
        conn.delete_by_message_id(&mailbox, old).await?;
    }
    conn.select(INBOX).await?;
    Ok(())
}

async fn delete_draft(conn: &mut Connection, message_id: &str) -> Result<(), postflow_mail::Error> {
    if let Some(mailbox) = conn.special_mailbox(SpecialMailbox::Drafts).await? {
        conn.delete_by_message_id(&mailbox, message_id).await?;
    }
    conn.select(INBOX).await?;
    Ok(())
}

async fn fetch_body(
    conn: &mut Connection,
    session: &Arc<Session>,
    r: &MailRef,
) -> Result<Vec<u8>, postflow_mail::Error> {
    let found = resolve(conn, &r.mailbox, &[r]).await?;
    let raw = match found.first() {
        Some((_, uid)) => conn
            .fetch_raw(&[*uid], false)
            .await?
            .into_iter()
            .next()
            .map(|(_, raw)| raw),
        None => None,
    };
    conn.select(INBOX).await?;
    let raw = raw.ok_or_else(|| {
        postflow_mail::Error::Server("the message is no longer on the server".into())
    })?;
    let (id, copy) = (r.id.clone(), raw.clone());
    let _ = session.db(move |s| s.set_mail_body(&id, &copy)).await;
    Ok(raw)
}

#[cfg(test)]
mod keepalive_tests {
    use super::*;

    #[test]
    fn learns_to_check_in_before_the_server_hangs_up() {
        let mut k = Keepalive::new();
        assert_eq!(k.window, KEEPALIVE_START);
        // 搜狐那样，安静 56 秒就被关掉
        assert_eq!(
            k.dropped_after(Duration::from_secs(56)),
            Some(Duration::from_millis(39_200))
        );
        // 再被关一次也不会缩得比安静时间的七成还短太多
        assert_eq!(
            k.dropped_after(Duration::from_secs(39)),
            Some(Duration::from_millis(19_600).max(KEEPALIVE_MIN))
        );
        // 不会低于下限
        assert_eq!(k.dropped_after(Duration::from_secs(5)), None);
        assert_eq!(k.window, KEEPALIVE_MIN);
    }

    #[test]
    fn a_dead_connection_at_the_deadline_halves_the_window() {
        // QQ、Gmail 那样：到点结束 IDLE 时连接已经断了，等 DONE 等了 15 秒
        let mut k = Keepalive::new();
        let quiet = KEEPALIVE_START + Duration::from_secs(15);
        assert_eq!(k.dropped_after(quiet), Some(KEEPALIVE_START / 2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icloud(username: &str) -> ServerConfig {
        ServerConfig {
            host: "imap.mail.me.com".into(),
            port: 993,
            security: postflow_mail::Security::Tls,
            username: username.into(),
        }
    }

    #[test]
    fn icloud_tries_both_forms_of_the_user_name() {
        assert_eq!(
            alternate_username(&icloud("john@icloud.com"), "john@icloud.com").as_deref(),
            Some("john")
        );
        assert_eq!(
            alternate_username(&icloud("john"), "john@icloud.com").as_deref(),
            Some("john@icloud.com")
        );
        assert_eq!(alternate_username(&icloud("john"), "john"), None);
        let mut other = icloud("me@qq.com");
        other.host = "imap.qq.com".into();
        assert_eq!(
            alternate_username(&other, "me@qq.com"),
            None,
            "only iCloud gets a second try"
        );
    }
}
