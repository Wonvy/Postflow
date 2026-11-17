//! 和 IMAP 服务器的一条连接，以及同步需要用到的那几个命令。

use std::fmt;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll};
use std::time::Duration;

use async_imap::extensions::idle::IdleResponse;
use async_imap::types::{Fetch, Flag, NameAttribute};
use async_imap::{Client, Session};
use futures::TryStreamExt;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::ClientConfig;
use tokio_rustls::rustls::pki_types::ServerName;

use crate::{Error, Result};

/// 收件箱在 IMAP 里的名字，不分大小写。
const INBOX_NAME: &str = "INBOX";

pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// 单个命令的超时。同步一批邮件正文可能比较慢，给得宽松一些。
const COMMAND_TIMEOUT: Duration = Duration::from_secs(90);
/// 结束 IDLE 时等服务器回应多久。
const DONE_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Security {
    /// 一连上就是 TLS，通常是 993 端口。
    Tls,
    /// 先明文连上再升级成 TLS，通常是 143 端口。
    Starttls,
    /// 不加密。只允许连本机，用于测试和本地桥接程序。
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub security: Security,
    pub username: String,
}

impl ServerConfig {
    pub fn validate(&self) -> Result<()> {
        let host = self.host.trim();
        if host.is_empty() || host.contains(char::is_whitespace) {
            return Err(Error::Config("server address is empty or invalid".into()));
        }
        if self.port == 0 {
            return Err(Error::Config("port must not be 0".into()));
        }
        if self.username.trim().is_empty() {
            return Err(Error::Config("username is empty".into()));
        }
        if self.security == Security::None && !is_loopback(host) {
            return Err(Error::Config(
                "unencrypted connections are only allowed to this machine".into(),
            ));
        }
        Ok(())
    }
}

pub(crate) fn is_loopback(host: &str) -> bool {
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    host == "localhost" || host == "::1" || host.starts_with("127.")
}

#[derive(Debug, Clone, Copy)]
pub struct MailboxInfo {
    pub uidvalidity: u32,
    pub exists: u32,
}

/// 列表阶段拿到的每封邮件的基本信息，不含正文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub uid: u32,
    pub seen: bool,
    pub flagged: bool,
    pub size: u32,
    /// 服务器收到这封邮件的时间，Unix 毫秒。
    pub internal_date: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagChange {
    Seen(bool),
    Flagged(bool),
}

/// 除了收件箱以外，按用途找的邮箱。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialMailbox {
    /// 垃圾邮件。
    Junk,
    /// 已删除的邮件。
    Trash,
    /// 已发送的邮件。
    Sent,
    /// 草稿。
    Drafts,
}

/// LIST 列出的一个邮箱。
#[derive(Debug, Clone)]
struct MailboxEntry {
    /// 服务器上的原名，选中邮箱时用。
    name: String,
    /// 最后一级的名字，解码成正常文字、转成小写，按名字找时用。
    leaf: String,
    selectable: bool,
    archive: bool,
    all: bool,
    junk: bool,
    trash: bool,
    sent: bool,
    drafts: bool,
    /// Gmail 的「已加星标」「重要」这类按标记筛出来的虚拟文件夹。
    virtual_view: bool,
    /// 层级分隔符，比如「/」或「.」。
    delimiter: Option<String>,
    /// 解码成正常文字的完整名字。
    path: String,
}

/// 邮箱里的一个普通文件夹（收件箱、已发送、草稿、垃圾邮件、已删除、归档以外的），侧栏里列出来。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FolderInfo {
    /// 服务器上的原名，打开时用。
    pub name: String,
    /// 给人看的完整名字，按 `delimiter` 分层。
    pub path: String,
    pub delimiter: Option<String>,
    /// 只是用来装子文件夹、自己打不开的。
    pub selectable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleOutcome {
    /// 服务器说邮箱有变化。
    NewData,
    /// 等够了时间，什么都没发生。
    Timeout,
    /// 被调用方打断，通常是有操作要执行。
    Interrupted,
}

/// TLS 和明文连接的统一包装，好让 async-imap 只面对一种类型。发信也用它。
pub(crate) enum Stream {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

impl fmt::Debug for Stream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Stream::Plain(_) => "Stream::Plain",
            Stream::Tls(_) => "Stream::Tls",
        })
    }
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_read(cx, buf),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_write(cx, buf),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_flush(cx),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_shutdown(cx),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_shutdown(cx),
        }
    }
}

fn tls_config() -> Result<Arc<ClientConfig>> {
    static CONFIG: OnceLock<std::result::Result<Arc<ClientConfig>, String>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            use rustls_platform_verifier::ConfigVerifierExt;
            // 用操作系统的证书信任设置，公司内网自签的根证书也能认
            ClientConfig::with_platform_verifier()
                .map(Arc::new)
                .map_err(|e| e.to_string())
        })
        .clone()
        .map_err(Error::Tls)
}

pub(crate) async fn upgrade(host: &str, tcp: TcpStream) -> Result<Stream> {
    let name = ServerName::try_from(host.to_string())
        .map_err(|_| Error::Config(format!("invalid server name: {host}")))?;
    let tls = TlsConnector::from(tls_config()?)
        .connect(name, tcp)
        .await
        .map_err(|e| Error::Tls(e.to_string()))?;
    Ok(Stream::Tls(Box::new(tls)))
}

async fn timed<T>(fut: impl Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(COMMAND_TIMEOUT, fut)
        .await
        .map_err(|_| Error::Timeout)?
}

/// 登录方式：密码，或者 OAuth 换来的访问令牌（XOAUTH2，Outlook 和 Microsoft 365 只认这个）。
#[derive(Clone)]
pub enum Auth {
    Password(String),
    OAuth2(String),
}

impl std::fmt::Debug for Auth {
    // 不把密码和令牌打进日志
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Auth::Password(_) => "Auth::Password(***)",
            Auth::OAuth2(_) => "Auth::OAuth2(***)",
        })
    }
}

/// SASL XOAUTH2：第一次回应带上用户名和令牌；登录失败时服务器会再发一段说明，回一个空行收尾。
struct XOAuth2 {
    first: Option<Vec<u8>>,
}

impl async_imap::Authenticator for XOAuth2 {
    type Response = Vec<u8>;

    fn process(&mut self, _challenge: &[u8]) -> Vec<u8> {
        self.first.take().unwrap_or_default()
    }
}

pub struct Connection {
    session: Session<Stream>,
    idle: bool,
    mv: bool,
    uidplus: bool,
    selected: Option<String>,
    archive: Option<String>,
    mailboxes: Option<Vec<MailboxEntry>>,
}

impl Connection {
    /// 连上服务器并登录。
    pub async fn connect(cfg: &ServerConfig, auth: &Auth) -> Result<Connection> {
        cfg.validate()?;
        let host = cfg.host.trim();
        let session = tokio::time::timeout(CONNECT_TIMEOUT, async {
            let tcp = TcpStream::connect((host, cfg.port))
                .await
                .map_err(|e| Error::Connect(e.to_string()))?;
            let _ = tcp.set_nodelay(true);

            let stream = match cfg.security {
                Security::Tls => upgrade(host, tcp).await?,
                Security::None => Stream::Plain(tcp),
                Security::Starttls => {
                    let mut client = Client::new(Stream::Plain(tcp));
                    read_greeting(&mut client).await?;
                    client.run_command_and_check_ok("STARTTLS", None).await?;
                    let Stream::Plain(tcp) = client.into_inner() else {
                        unreachable!("the stream was created as plain text above");
                    };
                    upgrade(host, tcp).await?
                }
            };
            let mut client = Client::new(stream);
            // STARTTLS 之后服务器不会再发问候语
            if cfg.security != Security::Starttls {
                read_greeting(&mut client).await?;
            }
            let user = cfg.username.trim();
            let result = match auth {
                Auth::Password(password) => client.login(user, password).await,
                Auth::OAuth2(token) => {
                    let first = format!("user={user}\x01auth=Bearer {token}\x01\x01").into_bytes();
                    client
                        .authenticate("XOAUTH2", XOAuth2 { first: Some(first) })
                        .await
                }
            };
            result.map_err(|(err, _)| match err {
                async_imap::error::Error::No(msg) | async_imap::error::Error::Bad(msg) => {
                    Error::Auth(crate::server_text(&msg))
                }
                other => other.into(),
            })
        })
        .await
        .map_err(|_| Error::Timeout)??;

        let mut conn = Connection {
            session,
            idle: false,
            mv: false,
            uidplus: false,
            selected: None,
            archive: None,
            mailboxes: None,
        };
        let caps = timed(async { Ok(conn.session.capabilities().await?) }).await?;
        conn.idle = caps.has_str("IDLE");
        conn.mv = caps.has_str("MOVE");
        conn.uidplus = caps.has_str("UIDPLUS");
        if caps.has_str("ID") {
            // 网易邮箱不发 ID 就拒绝打开收件箱。对其他服务器这只是礼貌。
            let version = env!("CARGO_PKG_VERSION");
            let identity = [("name", Some("Postflow")), ("version", Some(version))];
            let _ = timed(async { Ok(conn.session.id(identity).await?) }).await;
        }
        Ok(conn)
    }

    pub fn supports_idle(&self) -> bool {
        self.idle
    }

    pub async fn select(&mut self, mailbox: &str) -> Result<MailboxInfo> {
        let info = timed(async { Ok(self.session.select(mailbox).await?) }).await?;
        self.selected = Some(mailbox.to_string());
        self.drain_unsolicited();
        Ok(MailboxInfo {
            uidvalidity: info.uid_validity.unwrap_or(0),
            exists: info.exists,
        })
    }

    /// 当前选中邮箱里最新的 `count` 封邮件的基本信息。按序号取，不需要先搜索。
    pub async fn list_newest(&mut self, exists: u32, count: u32) -> Result<Vec<Listing>> {
        if exists == 0 || count == 0 {
            return Ok(Vec::new());
        }
        let start = exists.saturating_sub(count - 1).max(1);
        let set = format!("{start}:{exists}");
        timed(async {
            let fetches: Vec<Fetch> = self
                .session
                .fetch(set, "(UID FLAGS RFC822.SIZE INTERNALDATE)")
                .await?
                .try_collect()
                .await?;
            Ok(fetches.iter().filter_map(listing).collect())
        })
        .await
    }

    /// 当前选中邮箱里 UID 小于 `before` 的邮件里最新的 `count` 封的基本信息，以及更早的还有没有。
    pub async fn list_before(&mut self, before: u32, count: usize) -> Result<(Vec<Listing>, bool)> {
        if before <= 1 || count == 0 {
            return Ok((Vec::new(), false));
        }
        let found = timed(async {
            Ok(self
                .session
                .uid_search(format!("UID 1:{}", before - 1))
                .await?)
        })
        .await?;
        let mut uids: Vec<u32> = found.into_iter().filter(|u| *u < before).collect();
        uids.sort_unstable();
        let chosen = uids.split_off(uids.len().saturating_sub(count));
        let more = !uids.is_empty();
        if chosen.is_empty() {
            return Ok((Vec::new(), false));
        }
        let listing = timed(async {
            let fetches: Vec<Fetch> = self
                .session
                .uid_fetch(uid_set(&chosen), "(UID FLAGS RFC822.SIZE INTERNALDATE)")
                .await?
                .try_collect()
                .await?;
            Ok(fetches.iter().filter_map(listing).collect())
        })
        .await?;
        Ok((listing, more))
    }

    /// UID 不小于 `from` 的所有邮件的基本信息。
    pub async fn list_from(&mut self, from: u32) -> Result<Vec<Listing>> {
        let from = from.max(1);
        timed(async {
            let fetches: Vec<Fetch> = self
                .session
                .uid_fetch(format!("{from}:*"), "(UID FLAGS RFC822.SIZE INTERNALDATE)")
                .await?
                .try_collect()
                .await?;
            // 「n:*」在 n 大于现有最大 UID 时也会返回最后一封，这里按协议把它滤掉
            Ok(fetches
                .iter()
                .filter_map(listing)
                .filter(|l| l.uid >= from)
                .collect())
        })
        .await
    }

    /// 取一批邮件的原文。`header_only` 时只取信头，用于很大的邮件。
    pub async fn fetch_raw(
        &mut self,
        uids: &[u32],
        header_only: bool,
    ) -> Result<Vec<(u32, Vec<u8>)>> {
        if uids.is_empty() {
            return Ok(Vec::new());
        }
        let query = if header_only {
            "(UID BODY.PEEK[HEADER])"
        } else {
            "(UID BODY.PEEK[])"
        };
        timed(async {
            let fetches: Vec<Fetch> = self
                .session
                .uid_fetch(uid_set(uids), query)
                .await?
                .try_collect()
                .await?;
            Ok(fetches
                .iter()
                .filter_map(|f| {
                    let bytes = if header_only { f.header() } else { f.body() }?;
                    Some((f.uid?, bytes.to_vec()))
                })
                .collect())
        })
        .await
    }

    pub async fn set_flag(&mut self, uids: &[u32], change: FlagChange) -> Result<()> {
        if uids.is_empty() {
            return Ok(());
        }
        let (flag, on) = match change {
            FlagChange::Seen(on) => ("\\Seen", on),
            FlagChange::Flagged(on) => ("\\Flagged", on),
        };
        let query = format!("{}FLAGS.SILENT ({flag})", if on { "+" } else { "-" });
        timed(async {
            let _: Vec<Fetch> = self
                .session
                .uid_store(uid_set(uids), query)
                .await?
                .try_collect()
                .await?;
            Ok(())
        })
        .await?;
        self.drain_unsolicited();
        Ok(())
    }

    /// 把当前选中邮箱里的邮件移到 `target`。服务器不支持 MOVE 时用复制加删除代替。
    pub async fn move_to(&mut self, uids: &[u32], target: &str) -> Result<()> {
        if uids.is_empty() {
            return Ok(());
        }
        let set = uid_set(uids);
        timed(async {
            if self.mv {
                self.session.uid_mv(&set, target).await?;
            } else {
                self.session.uid_copy(&set, target).await?;
                let _: Vec<Fetch> = self
                    .session
                    .uid_store(&set, "+FLAGS.SILENT (\\Deleted)")
                    .await?
                    .try_collect()
                    .await?;
                if self.uidplus {
                    let _: Vec<u32> = self.session.uid_expunge(&set).await?.try_collect().await?;
                } else {
                    let _: Vec<u32> = self.session.expunge().await?.try_collect().await?;
                }
            }
            Ok(())
        })
        .await?;
        self.drain_unsolicited();
        Ok(())
    }

    /// 服务器上所有的邮箱，第一次用到时取一次。
    async fn mailboxes(&mut self) -> Result<&[MailboxEntry]> {
        if self.mailboxes.is_none() {
            let names = timed(async {
                let names: Vec<_> = self
                    .session
                    .list(Some(""), Some("*"))
                    .await?
                    .try_collect()
                    .await?;
                Ok(names)
            })
            .await?;
            let entries = names
                .iter()
                .map(|n| {
                    let attrs = n.attributes();
                    let leaf = match n.delimiter() {
                        Some(d) => n.name().rsplit(d).next().unwrap_or(n.name()),
                        None => n.name(),
                    };
                    MailboxEntry {
                        name: n.name().to_string(),
                        leaf: decode_mailbox_name(leaf).trim().to_lowercase(),
                        selectable: !attrs.contains(&NameAttribute::NoSelect),
                        archive: attrs.contains(&NameAttribute::Archive),
                        all: attrs.contains(&NameAttribute::All),
                        junk: attrs.contains(&NameAttribute::Junk),
                        trash: attrs.contains(&NameAttribute::Trash),
                        sent: attrs.contains(&NameAttribute::Sent),
                        drafts: attrs.contains(&NameAttribute::Drafts),
                        virtual_view: attrs.iter().any(|a| match a {
                            NameAttribute::Flagged => true,
                            NameAttribute::Extension(e) => e.eq_ignore_ascii_case("\\Important"),
                            _ => false,
                        }),
                        delimiter: n.delimiter().map(str::to_string),
                        path: decode_mailbox_name(n.name()),
                    }
                })
                .collect();
            self.mailboxes = Some(entries);
        }
        Ok(self.mailboxes.as_deref().unwrap_or_default())
    }

    /// 普通文件夹：去掉收件箱、已发送、草稿、垃圾邮件、已删除、归档，以及 Gmail 的「所有邮件」
    /// 「已加星标」「重要」这些另有地方看的。打不开、下面也没有文件夹的不要。
    pub async fn folders(&mut self) -> Result<Vec<FolderInfo>> {
        let mut special = Vec::new();
        for kind in [
            SpecialMailbox::Junk,
            SpecialMailbox::Trash,
            SpecialMailbox::Sent,
            SpecialMailbox::Drafts,
        ] {
            if let Some(name) = self.special_mailbox(kind).await? {
                special.push(name);
            }
        }
        let list = self.mailboxes().await?;
        let kept: Vec<&MailboxEntry> = list
            .iter()
            .filter(|m| {
                !m.name.eq_ignore_ascii_case(INBOX_NAME)
                    && !special.contains(&m.name)
                    && !m.archive
                    && !matches!(m.leaf.as_str(), "archive" | "archives")
                    && !m.all
                    && !m.virtual_view
            })
            .collect();
        let has_children = |parent: &MailboxEntry| {
            let Some(d) = &parent.delimiter else {
                return false;
            };
            let prefix = format!("{}{d}", parent.name);
            kept.iter()
                .any(|m| m.selectable && m.name.starts_with(&prefix))
        };
        let mut out: Vec<FolderInfo> = kept
            .iter()
            .filter(|m| m.selectable || has_children(m))
            .map(|m| FolderInfo {
                name: m.name.clone(),
                path: m.path.clone(),
                delimiter: m.delimiter.clone(),
                selectable: m.selectable,
            })
            .collect();
        out.sort_by_key(|f| f.path.to_lowercase());
        Ok(out)
    }

    /// 找到用来存放归档邮件的邮箱。找不到就建一个叫 Archive 的。
    pub async fn archive_mailbox(&mut self) -> Result<String> {
        if let Some(name) = &self.archive {
            return Ok(name.clone());
        }
        let list = self.mailboxes().await?;
        let by_name = || {
            list.iter()
                .find(|m| m.selectable && matches!(m.leaf.as_str(), "archive" | "archives"))
        };
        // Gmail 没有归档文件夹，「所有邮件」就是归档：移出收件箱即可
        let found = list
            .iter()
            .find(|m| m.selectable && m.archive)
            .or_else(by_name)
            .or_else(|| list.iter().find(|m| m.selectable && m.all))
            .map(|m| m.name.clone());
        let name = match found {
            Some(name) => name,
            None => {
                timed(async { Ok(self.session.create("Archive").await?) }).await?;
                "Archive".to_string()
            }
        };
        self.archive = Some(name.clone());
        Ok(name)
    }

    /// 找到垃圾邮件或已删除邮件所在的邮箱。服务器标了用途的优先，否则按常见的名字找。
    /// 都没有就返回 `None`，比如 Gmail 设置里关掉了在 IMAP 中显示这个标签。
    pub async fn special_mailbox(&mut self, kind: SpecialMailbox) -> Result<Option<String>> {
        let list = self.mailboxes().await?;
        let names: &[&str] = match kind {
            SpecialMailbox::Junk => &[
                "junk",
                "spam",
                "junk e-mail",
                "junk email",
                "junk mail",
                "bulk mail",
                "垃圾邮件",
                "垃圾箱",
            ],
            SpecialMailbox::Trash => &[
                "trash",
                "bin",
                "deleted",
                "deleted items",
                "deleted messages",
                "已删除",
                "已删除邮件",
                "废纸篓",
            ],
            SpecialMailbox::Sent => &[
                "sent",
                "sent items",
                "sent messages",
                "sent mail",
                "已发送",
                "已发送邮件",
                "已发邮件",
            ],
            SpecialMailbox::Drafts => &["drafts", "draft", "草稿", "草稿箱", "草稿夹"],
        };
        let marked = |m: &MailboxEntry| match kind {
            SpecialMailbox::Junk => m.junk,
            SpecialMailbox::Trash => m.trash,
            SpecialMailbox::Sent => m.sent,
            SpecialMailbox::Drafts => m.drafts,
        };
        Ok(list
            .iter()
            .filter(|m| m.selectable)
            .find(|m| marked(m))
            .or_else(|| {
                list.iter()
                    .filter(|m| m.selectable)
                    .find(|m| names.contains(&m.leaf.as_str()))
            })
            .map(|m| m.name.clone()))
    }

    /// 服务器上所有邮箱的名字（解码后的完整路径），找不到特殊文件夹时写进日志排查用。
    pub async fn mailbox_names(&mut self) -> Result<Vec<String>> {
        Ok(self
            .mailboxes()
            .await?
            .iter()
            .map(|m| {
                let flags = [
                    (m.sent, "\\Sent"),
                    (m.drafts, "\\Drafts"),
                    (m.junk, "\\Junk"),
                    (m.trash, "\\Trash"),
                    (!m.selectable, "\\NoSelect"),
                ]
                .iter()
                .filter(|(on, _)| *on)
                .map(|(_, f)| *f)
                .collect::<Vec<_>>()
                .join(" ");
                let name = decode_mailbox_name(&m.name);
                if flags.is_empty() {
                    name
                } else {
                    format!("{name} ({flags})")
                }
            })
            .collect())
    }

    /// 下次用到时重新列一遍邮箱：连接期间服务器上可能新建或改名了文件夹
    /// （比如发信时才建出来的「已发送」）。
    pub fn forget_mailboxes(&mut self) {
        self.mailboxes = None;
    }

    /// 找到存放已发送邮件的邮箱。找不到就建一个叫 Sent 的。
    pub async fn sent_mailbox(&mut self) -> Result<String> {
        self.special_or_create(SpecialMailbox::Sent, "Sent").await
    }

    /// 找到草稿所在的邮箱。找不到就建一个叫 Drafts 的。
    pub async fn drafts_mailbox(&mut self) -> Result<String> {
        self.special_or_create(SpecialMailbox::Drafts, "Drafts")
            .await
    }

    async fn special_or_create(&mut self, kind: SpecialMailbox, name: &str) -> Result<String> {
        if let Some(found) = self.special_mailbox(kind).await? {
            return Ok(found);
        }
        timed(async { Ok(self.session.create(name).await?) }).await?;
        self.mailboxes = None;
        Ok(name.to_string())
    }

    /// 在当前选中的邮箱里按 Message-ID 找邮件。
    pub async fn find_by_message_id(&mut self, message_id: &str) -> Result<Vec<u32>> {
        let query = format!("HEADER Message-ID {}", quote(message_id));
        let uids = timed(async { Ok(self.session.uid_search(query).await?) }).await?;
        let mut uids: Vec<u32> = uids.into_iter().collect();
        uids.sort_unstable();
        Ok(uids)
    }

    pub fn selected(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    /// 把一封邮件的原文存进 `mailbox`，标成已读。发出去的信存到「已发送」用。
    pub async fn append(&mut self, mailbox: &str, raw: &[u8]) -> Result<()> {
        self.append_flagged(mailbox, raw, "(\\Seen)").await
    }

    /// 存一封草稿：标成已读的草稿，别的邮件程序也认得出来。
    pub async fn append_draft(&mut self, mailbox: &str, raw: &[u8]) -> Result<()> {
        self.append_flagged(mailbox, raw, "(\\Seen \\Draft)").await
    }

    async fn append_flagged(&mut self, mailbox: &str, raw: &[u8], flags: &str) -> Result<()> {
        timed(async {
            self.session.append(mailbox, Some(flags), None, raw).await?;
            Ok(())
        })
        .await?;
        self.drain_unsolicited();
        Ok(())
    }

    /// 从 `mailbox` 里彻底删掉这个 Message-ID 的邮件（替换、丢弃草稿时用）。返回删了几封。
    /// 之后选中的是 `mailbox`。
    pub async fn delete_by_message_id(&mut self, mailbox: &str, message_id: &str) -> Result<usize> {
        self.select(mailbox).await?;
        let uids = self.find_by_message_id(message_id).await?;
        if uids.is_empty() {
            return Ok(0);
        }
        let set = uid_set(&uids);
        timed(async {
            let _: Vec<Fetch> = self
                .session
                .uid_store(&set, "+FLAGS.SILENT (\\Deleted)")
                .await?
                .try_collect()
                .await?;
            if self.uidplus {
                let _: Vec<u32> = self.session.uid_expunge(&set).await?.try_collect().await?;
            } else {
                let _: Vec<u32> = self.session.expunge().await?.try_collect().await?;
            }
            Ok(())
        })
        .await?;
        self.drain_unsolicited();
        Ok(uids.len())
    }

    /// 在 `mailbox` 里按 Message-ID 找邮件，找完回到原来选中的邮箱。
    pub async fn contains_message_id(&mut self, mailbox: &str, message_id: &str) -> Result<bool> {
        let previous = self.selected.clone();
        self.select(mailbox).await?;
        let found = !self.find_by_message_id(message_id).await?.is_empty();
        if let Some(previous) = previous.filter(|p| p != mailbox) {
            self.select(&previous).await?;
        }
        Ok(found)
    }

    /// 进入 IDLE，等服务器通知、等到超时，或者等 `interrupt` 完成。
    pub async fn idle(
        self,
        timeout: Duration,
        interrupt: impl Future<Output = ()>,
    ) -> Result<(Connection, IdleOutcome)> {
        let Connection {
            session,
            idle,
            mv,
            uidplus,
            selected,
            archive,
            mailboxes,
        } = self;
        let mut handle = session.idle();
        timed(async { Ok(handle.init().await?) }).await?;

        // async-imap 的超时每收到一句服务器的回应就重新计时，连「还在」（`* OK Still here`）也算。
        // 有的服务器 IDLE 期间只发「还在」、从不通知新邮件，只靠它的超时会一直停在 IDLE 里，
        // 新邮件永远收不到。所以另外定一个到点就结束的时间，结束后由调用方重新同步。
        let mut timed_out = false;
        let outcome = {
            let (wait, stop) = handle.wait_with_timeout(timeout);
            tokio::pin!(wait);
            tokio::pin!(interrupt);
            let deadline = tokio::time::sleep(timeout);
            tokio::pin!(deadline);
            let mut stop = Some(stop);
            loop {
                tokio::select! {
                    res = &mut wait => break res?,
                    _ = &mut interrupt, if stop.is_some() => {
                        // 丢掉 StopSource 就会让等待提前结束
                        stop.take();
                    }
                    _ = &mut deadline, if stop.is_some() => {
                        timed_out = true;
                        stop.take();
                    }
                }
            }
        };
        // 正常的服务器马上回应 DONE。连接在睡眠或断网时已经死了的，别等满 90 秒，早点重连
        let session = tokio::time::timeout(DONE_TIMEOUT, handle.done())
            .await
            .map_err(|_| Error::Timeout)??;
        let mut conn = Connection {
            session,
            idle,
            mv,
            uidplus,
            selected,
            archive,
            mailboxes,
        };
        conn.drain_unsolicited();
        let outcome = match outcome {
            IdleResponse::NewData(_) => IdleOutcome::NewData,
            IdleResponse::Timeout => IdleOutcome::Timeout,
            IdleResponse::ManualInterrupt if timed_out => IdleOutcome::Timeout,
            IdleResponse::ManualInterrupt => IdleOutcome::Interrupted,
        };
        Ok((conn, outcome))
    }

    pub async fn logout(mut self) {
        let _ = tokio::time::timeout(Duration::from_secs(5), self.session.logout()).await;
    }

    /// 服务器主动推来的通知（新邮件、标记变化）由下一次同步统一处理，这里只是清掉。
    fn drain_unsolicited(&mut self) {
        while self.session.unsolicited_responses.try_recv().is_ok() {}
    }
}

async fn read_greeting(client: &mut Client<Stream>) -> Result<()> {
    match client.read_response().await {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(Error::Io("server closed the connection".into())),
        Err(e) => Err(Error::Io(e.to_string())),
    }
}

fn listing(f: &Fetch) -> Option<Listing> {
    let mut seen = false;
    let mut flagged = false;
    for flag in f.flags() {
        match flag {
            Flag::Seen => seen = true,
            Flag::Flagged => flagged = true,
            _ => {}
        }
    }
    Some(Listing {
        uid: f.uid?,
        seen,
        flagged,
        size: f.size.unwrap_or(0),
        internal_date: f.internal_date().map(|d| d.timestamp_millis()),
    })
}

/// 把一组 UID 压成 IMAP 的集合写法，连续的合并成区间：1:3,7,9:10。
pub(crate) fn uid_set(uids: &[u32]) -> String {
    let mut sorted = uids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut parts = Vec::new();
    let mut i = 0;
    while i < sorted.len() {
        let start = sorted[i];
        let mut end = start;
        while i + 1 < sorted.len() && sorted[i + 1] == end + 1 {
            i += 1;
            end = sorted[i];
        }
        parts.push(if start == end {
            start.to_string()
        } else {
            format!("{start}:{end}")
        });
        i += 1;
    }
    parts.join(",")
}

/// 把 IMAP 邮箱名用的改良 UTF-7（RFC 3501 5.1.3）解码成正常文字。
/// 网易等服务器的中文邮箱名是这样写的。格式不对时原样返回。
fn decode_mailbox_name(name: &str) -> String {
    let mut out = String::new();
    let mut rest = name;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('-') else {
            return name.to_string();
        };
        let encoded = &after[..end];
        if encoded.is_empty() {
            out.push('&');
        } else {
            match utf16_base64(encoded).and_then(|units| String::from_utf16(&units).ok()) {
                Some(text) => out.push_str(&text),
                None => return name.to_string(),
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// 改良 base64（用「,」代替「/」，不补「=」）里装着的 UTF-16BE。
fn utf16_base64(s: &str) -> Option<Vec<u16>> {
    let mut bits: u32 = 0;
    let mut count = 0;
    let mut bytes = Vec::new();
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b',' => 63,
            _ => return None,
        };
        bits = (bits << 6) | u32::from(v);
        count += 6;
        if count >= 8 {
            count -= 8;
            bytes.push((bits >> count) as u8);
            bits &= (1 << count) - 1;
        }
    }
    let (pairs, rest) = bytes.as_chunks::<2>();
    if !rest.is_empty() {
        return None;
    }
    Some(pairs.iter().map(|p| u16::from_be_bytes(*p)).collect())
}

fn quote(s: &str) -> String {
    let cleaned: String = s.chars().filter(|c| !c.is_control()).collect();
    format!("\"{}\"", cleaned.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uid_sets() {
        assert_eq!(uid_set(&[5, 1, 2, 3, 9, 10, 7, 3]), "1:3,5,7,9:10");
        assert_eq!(uid_set(&[42]), "42");
    }

    #[test]
    fn mailbox_names_are_decoded() {
        assert_eq!(decode_mailbox_name("&V4NXPpCuTvY-"), "垃圾邮件");
        assert_eq!(decode_mailbox_name("&XfJSIJZk-"), "已删除");
        assert_eq!(decode_mailbox_name("Sent &- Received"), "Sent & Received");
        assert_eq!(decode_mailbox_name("INBOX"), "INBOX");
        assert_eq!(decode_mailbox_name("bad &!!-"), "bad &!!-");
        assert_eq!(decode_mailbox_name("open &V4NX"), "open &V4NX");
    }

    #[test]
    fn quoting() {
        assert_eq!(quote("abc@x"), "\"abc@x\"");
        assert_eq!(quote("a\"b\\c\r\n"), "\"a\\\"b\\\\c\"");
    }

    #[test]
    fn plaintext_only_to_loopback() {
        let mut cfg = ServerConfig {
            host: "127.0.0.1".into(),
            port: 143,
            security: Security::None,
            username: "me".into(),
        };
        assert!(cfg.validate().is_ok());
        cfg.host = "imap.example.com".into();
        assert!(matches!(cfg.validate(), Err(Error::Config(_))));
        cfg.security = Security::Tls;
        assert!(cfg.validate().is_ok());
        cfg.username = " ".into();
        assert!(cfg.validate().is_err());
    }
}
