//! 每个邮箱账户一个后台任务：保持一条 IMAP 连接，同步收件箱，把本机的改动写回服务器，
//! 服务器支持 IDLE 时用它实时收到新邮件。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use postflow_mail::{Connection, FlagChange, Listing, Security, ServerConfig};
use postflow_store::{Account, ItemPatch, MailRef, MailSync, MailSyncStats, NewMail, ids};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::{Core, Event, RpcError, Scope, Session};

const INBOX: &str = "INBOX";
/// 第一次同步取最新的这么多封。
const INITIAL_COUNT: u32 = 200;
/// 一次同步最多新取这么多封，防止一下子涌进几万封时卡住。
const MAX_NEW_PER_SYNC: usize = 500;
/// 只核对最新这么多封的状态。更早的邮件很少会被改动。
const FLAG_WINDOW: usize = 1000;
/// 不超过这个大小的邮件同步时就把原文取回来，更大的打开时再取。
const EAGER_BODY_LIMIT: u32 = 2 * 1024 * 1024;
const FETCH_BATCH: usize = 20;
/// 大多数服务器会在 30 分钟左右断开空闲的 IDLE，提前一些重新发起。
const IDLE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// 服务器不支持 IDLE 时多久查一次。
const POLL_INTERVAL: Duration = Duration::from_secs(5 * 60);
const MIN_BACKOFF: Duration = Duration::from_secs(15);
const MAX_BACKOFF: Duration = Duration::from_secs(15 * 60);

pub(crate) enum Command {
    /// 马上同步一次，界面上显示正在刷新。
    Sync,
    /// 把本机改过的状态写回服务器。
    Apply(Vec<MailRef>, ItemPatch),
    /// 取一封邮件的原文，同步时没取的大邮件打开时用。
    FetchBody(MailRef, oneshot::Sender<Result<Vec<u8>, RpcError>>),
}

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

/// 试着连上并打开收件箱，确认设置和密码都对。
pub(crate) async fn verify(cfg: &ServerConfig, password: &str) -> Result<(), RpcError> {
    let mut conn = Connection::connect(cfg, password)
        .await
        .map_err(|e| mail_error(&e))?;
    let result = conn.select(INBOX).await.map_err(|e| mail_error(&e));
    conn.logout().await;
    result.map(|_| ())
}

impl Core {
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

    pub(crate) fn send_mail(&self, session: &Session, account_id: &str, cmd: Command) -> bool {
        session
            .workers()
            .get(account_id)
            .is_some_and(|w| w.send(cmd))
    }

    pub(crate) fn sync_all_mail(&self, session: &Session) {
        for worker in session.workers().values() {
            worker.send(Command::Sync);
        }
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
    loop {
        let id = account_id.clone();
        let Ok(Some(account)) = session.db(move |s| s.account(&id)).await else {
            return;
        };
        let secrets = core.secrets.clone();
        let id = account_id.clone();
        let password = tokio::task::spawn_blocking(move || secrets.get(&id))
            .await
            .ok()
            .flatten();
        let Some(password) = password else {
            core.record_mail(
                &session,
                &account_id,
                Some(("mail_no_password", "no password on this device".into())),
            )
            .await;
            // 等用户在这台设备上输入密码，那时会重新启动这个任务
            park(&mut rx, &mut pending, "mail_no_password").await;
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
                park(&mut rx, &mut pending, &e.code).await;
                return;
            }
        };

        let conn = match Connection::connect(&cfg, &password).await {
            Ok(conn) => conn,
            Err(err) => {
                tracing::debug!("mail {}: connect failed: {err}", account.email);
                core.record_mail(
                    &session,
                    &account_id,
                    Some((err.code(), mail_error(&err).message)),
                )
                .await;
                fail_fetches(&mut pending, &err);
                if err.needs_user() {
                    park(&mut rx, &mut pending, err.code()).await;
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

        match serve(&core, &session, &account_id, conn, &mut rx, &mut pending).await {
            Ok(()) => return,
            Err(err) => {
                tracing::debug!("mail {}: connection dropped: {err}", account.email);
                core.record_mail(
                    &session,
                    &account_id,
                    Some((err.code(), mail_error(&err).message)),
                )
                .await;
                fail_fetches(&mut pending, &err);
                tokio::time::sleep(MIN_BACKOFF / 3).await;
            }
        }
    }
}

/// 没法工作（没有密码、设置有误）时停在这里，对取正文的请求直接回错误，直到任务被替换。
async fn park(rx: &mut mpsc::UnboundedReceiver<Command>, pending: &mut Vec<Command>, code: &str) {
    let fail = |cmd: Command| {
        if let Command::FetchBody(_, reply) = cmd {
            let _ = reply.send(Err(RpcError::new(code, "mail account is not available")));
        }
    };
    pending.drain(..).for_each(fail);
    while let Some(cmd) = rx.recv().await {
        fail(cmd);
    }
}

fn fail_fetches(pending: &mut Vec<Command>, err: &postflow_mail::Error) {
    let mut keep = Vec::new();
    for cmd in pending.drain(..) {
        match cmd {
            Command::FetchBody(_, reply) => {
                let _ = reply.send(Err(mail_error(err)));
            }
            other => keep.push(other),
        }
    }
    *pending = keep;
}

/// 连接期间的循环。正常结束（任务被停掉）返回 Ok，连接出错返回 Err。
async fn serve(
    core: &Arc<Core>,
    session: &Arc<Session>,
    account_id: &str,
    mut conn: Connection,
    rx: &mut mpsc::UnboundedReceiver<Command>,
    pending: &mut Vec<Command>,
) -> Result<(), postflow_mail::Error> {
    // 刚连上的第一次同步也在界面上显示，让用户知道在收信
    let mut announce = true;
    let mut healthy = false;
    loop {
        // 先执行用户的操作，再同步，免得同步把刚做的改动又改回去
        let commands = std::mem::take(pending);
        let mut iter = commands.into_iter();
        while let Some(cmd) = iter.next() {
            match cmd {
                Command::Sync => announce = true,
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
            }
        }

        if announce {
            let active = core.refreshing.fetch_add(1, Ordering::SeqCst) + 1;
            core.emit(Event::Refresh { active });
        }
        let result = sync_inbox(&mut conn, session, account_id).await;
        if announce {
            let active = core.refreshing.fetch_sub(1, Ordering::SeqCst) - 1;
            core.emit(Event::Refresh { active });
            announce = false;
        }
        let stats = result?;
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

        if conn.supports_idle() {
            let mut closed = false;
            let (next, _) = conn
                .idle(IDLE_TIMEOUT, async {
                    match rx.recv().await {
                        Some(cmd) => pending.push(cmd),
                        None => closed = true,
                    }
                })
                .await?;
            conn = next;
            if closed {
                conn.logout().await;
                return Ok(());
            }
        } else {
            tokio::select! {
                cmd = rx.recv() => match cmd {
                    Some(cmd) => pending.push(cmd),
                    None => {
                        conn.logout().await;
                        return Ok(());
                    }
                },
                _ = tokio::time::sleep(POLL_INTERVAL) => {}
            }
        }
        // 用户连续操作时，攒一小会儿一起写回去
        tokio::time::sleep(Duration::from_millis(150)).await;
        while let Ok(cmd) = rx.try_recv() {
            pending.push(cmd);
        }
    }
}

/// 同步收件箱：新邮件取回来，状态变了的更新，服务器上没了的删掉。
async fn sync_inbox(
    conn: &mut Connection,
    session: &Arc<Session>,
    account_id: &str,
) -> Result<MailSyncStats, postflow_mail::Error> {
    let started_at = crate::refresh::now();
    let info = conn.select(INBOX).await?;
    let id = account_id.to_string();
    let previous = session
        .db(move |s| s.mail_uidvalidity(&id))
        .await
        .ok()
        .flatten();
    if previous.is_some_and(|v| v != info.uidvalidity) {
        let id = account_id.to_string();
        let _ = session.db(move |s| s.reset_inbox(&id)).await;
    }

    let id = account_id.to_string();
    let known = session
        .db(move |s| s.known_mail(&id))
        .await
        .unwrap_or_default();
    let mut window: Vec<u32> = known.iter().map(|k| k.uid).collect();
    window.sort_unstable_by(|a, b| b.cmp(a));
    window.truncate(FLAG_WINDOW);

    let listing: Vec<Listing> = match window.last() {
        None => conn.list_newest(info.exists, INITIAL_COUNT).await?,
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

    // 同一个 Message-ID 在收件箱里出现两次时，给后来的那封换个 ID，免得互相覆盖
    let mut taken: HashMap<String, u32> = known.iter().map(|k| (k.id.clone(), k.uid)).collect();
    let mut mails = Vec::with_capacity(new.len());
    let (small, large): (Vec<&Listing>, Vec<&Listing>) =
        new.iter().partition(|l| l.size <= EAGER_BODY_LIMIT);
    for (group, header_only) in [(small, false), (large, true)] {
        for chunk in group.chunks(FETCH_BATCH) {
            let uids: Vec<u32> = chunk.iter().map(|l| l.uid).collect();
            let fetched: HashMap<u32, Vec<u8>> = conn
                .fetch_raw(&uids, header_only)
                .await?
                .into_iter()
                .collect();
            for l in chunk {
                let Some(raw) = fetched.get(&l.uid) else {
                    continue;
                };
                let mail = new_mail(
                    account_id,
                    info.uidvalidity,
                    l,
                    raw,
                    !header_only,
                    started_at,
                    &mut taken,
                );
                mails.push(mail);
            }
        }
    }

    let sync = MailSync {
        uidvalidity: info.uidvalidity,
        started_at,
        new: mails,
        flags,
        vanished,
    };
    let id = account_id.to_string();
    session
        .db(move |s| s.apply_mail_sync(&id, sync))
        .await
        .map_err(|e| postflow_mail::Error::Io(format!("could not save mail: {}", e.message)))
}

fn new_mail(
    account_id: &str,
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
    NewMail {
        id,
        uid: l.uid,
        message_id: summary.message_id,
        subject: summary.subject,
        from_name: summary.from.name,
        from_address: summary.from.address,
        snippet: summary.snippet,
        date,
        read: l.seen,
        starred: l.flagged,
        size: l.size,
        // 只取了信头的大邮件，十有八九带着附件
        has_attachments: summary.has_attachments || !full,
        list_unsubscribe: summary.list_unsubscribe,
        raw: full.then(|| raw.to_vec()),
    }
}

/// 找出一批邮件在某个邮箱里的 UID。收件箱里有记录的直接用，其他的按 Message-ID 搜。
async fn resolve(
    conn: &mut Connection,
    mailbox: &str,
    refs: &[&MailRef],
) -> Result<Vec<(String, u32)>, postflow_mail::Error> {
    conn.select(mailbox).await?;
    let mut out = Vec::new();
    for r in refs {
        let uid = match (mailbox == INBOX, r.uid, &r.message_id) {
            (true, Some(uid), _) => Some(uid),
            (_, _, Some(mid)) => conn.find_by_message_id(mid).await?.into_iter().next(),
            (_, uid, None) => uid,
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
