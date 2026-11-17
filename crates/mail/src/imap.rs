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

const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// 单个命令的超时。同步一批邮件正文可能比较慢，给得宽松一些。
const COMMAND_TIMEOUT: Duration = Duration::from_secs(90);

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

fn is_loopback(host: &str) -> bool {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleOutcome {
    /// 服务器说邮箱有变化。
    NewData,
    /// 等够了时间，什么都没发生。
    Timeout,
    /// 被调用方打断，通常是有操作要执行。
    Interrupted,
}

/// TLS 和明文连接的统一包装，好让 async-imap 只面对一种类型。
enum Stream {
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

async fn upgrade(host: &str, tcp: TcpStream) -> Result<Stream> {
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

pub struct Connection {
    session: Session<Stream>,
    idle: bool,
    mv: bool,
    uidplus: bool,
    selected: Option<String>,
    archive: Option<String>,
}

impl Connection {
    /// 连上服务器并登录。
    pub async fn connect(cfg: &ServerConfig, password: &str) -> Result<Connection> {
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
            client
                .login(cfg.username.trim(), password)
                .await
                .map_err(|(err, _)| match err {
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

    /// 找到用来存放归档邮件的邮箱。找不到就建一个叫 Archive 的。
    pub async fn archive_mailbox(&mut self) -> Result<String> {
        if let Some(name) = &self.archive {
            return Ok(name.clone());
        }
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
        let by_attr = |attr: &NameAttribute<'_>| {
            names
                .iter()
                .find(|n| n.attributes().contains(attr))
                .map(|n| n.name().to_string())
        };
        let by_name = || {
            names
                .iter()
                .find(|n| {
                    let leaf = match n.delimiter() {
                        Some(d) => n.name().rsplit(d).next().unwrap_or(n.name()),
                        None => n.name(),
                    };
                    matches!(leaf.to_ascii_lowercase().as_str(), "archive" | "archives")
                })
                .map(|n| n.name().to_string())
        };
        // Gmail 没有归档文件夹，「所有邮件」就是归档：移出收件箱即可
        let found = by_attr(&NameAttribute::Archive)
            .or_else(by_name)
            .or_else(|| by_attr(&NameAttribute::All));
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
        let session = timed(async { Ok(handle.done().await?) }).await?;
        let mut conn = Connection {
            session,
            idle,
            mv,
            uidplus,
            selected,
            archive,
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
