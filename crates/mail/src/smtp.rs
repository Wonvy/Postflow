//! 发信：一个只做发信要用的那几步的 SMTP 客户端。
//!
//! 连接方式和收信一样：465 端口一连上就是 TLS，587 端口先明文连上再 STARTTLS，
//! 不加密只允许连本机。登录用密码（AUTH PLAIN 或 LOGIN）或 OAuth 令牌（XOAUTH2）。

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::imap::{Auth, CONNECT_TIMEOUT, Security, ServerConfig, Stream, upgrade};
use crate::{Error, Result, base64};

/// 等服务器回一句话最多等多久。
const REPLY_TIMEOUT: Duration = Duration::from_secs(60);
/// 信发完以后服务器要检查、转交，回应可能慢一些。
const DATA_TIMEOUT: Duration = Duration::from_secs(120);
/// 服务器回应的一行最长多少。正常的都很短，再长就是出了问题。
const MAX_LINE: usize = 8 * 1024;

/// 服务器的一次回应：三位数的状态码和说明文字（多行的合成一行）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Reply {
    code: u16,
    lines: Vec<String>,
}

impl Reply {
    fn ok(&self) -> bool {
        (200..400).contains(&self.code)
    }

    /// 给人看的说明。去掉像 5.7.8 这样的扩展状态码。
    fn text(&self) -> String {
        let text = self
            .lines
            .iter()
            .map(|l| {
                let l = l.trim();
                match l.split_once(' ') {
                    Some((code, rest)) if is_enhanced_code(code) => rest.trim(),
                    _ => l,
                }
            })
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if text.is_empty() {
            format!("SMTP {}", self.code)
        } else {
            text
        }
    }

    fn error(&self) -> Error {
        Error::Server(self.text())
    }
}

fn is_enhanced_code(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// EHLO 回应里服务器说自己支持的功能。
#[derive(Debug, Default)]
struct Extensions {
    starttls: bool,
    auth: Vec<String>,
    size: Option<usize>,
    smtputf8: bool,
}

impl Extensions {
    fn parse(reply: &Reply) -> Extensions {
        let mut ext = Extensions::default();
        // 第一行是服务器的名字和问候
        for line in reply.lines.iter().skip(1) {
            let mut words = line.split_whitespace();
            let Some(keyword) = words.next() else {
                continue;
            };
            let keyword = keyword.to_ascii_uppercase();
            match keyword.as_str() {
                "STARTTLS" => ext.starttls = true,
                "SMTPUTF8" => ext.smtputf8 = true,
                "SIZE" => ext.size = words.next().and_then(|n| n.parse().ok()).filter(|n| *n > 0),
                // 老服务器写成 AUTH=LOGIN PLAIN
                k if k == "AUTH" || k.starts_with("AUTH=") => {
                    let first = k.strip_prefix("AUTH=").filter(|m| !m.is_empty());
                    for mech in first
                        .map(str::to_string)
                        .into_iter()
                        .chain(words.map(str::to_ascii_uppercase))
                    {
                        if !ext.auth.contains(&mech) {
                            ext.auth.push(mech);
                        }
                    }
                }
                _ => {}
            }
        }
        ext
    }
}

struct Client {
    stream: Stream,
    buf: Vec<u8>,
}

impl Client {
    async fn read_line(&mut self) -> Result<String> {
        loop {
            if let Some(end) = self.buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = self.buf.drain(..=end).collect();
                let line = String::from_utf8_lossy(&line);
                return Ok(line.trim_end_matches(['\r', '\n']).to_string());
            }
            if self.buf.len() > MAX_LINE {
                return Err(Error::Server("the server sent an overlong line".into()));
            }
            let mut chunk = [0u8; 4096];
            let n = self
                .stream
                .read(&mut chunk)
                .await
                .map_err(|e| Error::Io(e.to_string()))?;
            if n == 0 {
                return Err(Error::Io("server closed the connection".into()));
            }
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }

    async fn reply_within(&mut self, limit: Duration) -> Result<Reply> {
        tokio::time::timeout(limit, async {
            let mut lines = Vec::new();
            loop {
                let line = self.read_line().await?;
                let code = line
                    .get(..3)
                    .and_then(|c| c.parse::<u16>().ok())
                    .filter(|c| (200..600).contains(c))
                    .ok_or_else(|| Error::Server(format!("unexpected reply: {line}")))?;
                let more = line.as_bytes().get(3) == Some(&b'-');
                lines.push(line.get(4..).unwrap_or("").to_string());
                if !more {
                    return Ok(Reply { code, lines });
                }
            }
        })
        .await
        .map_err(|_| Error::Timeout)?
    }

    async fn reply(&mut self) -> Result<Reply> {
        self.reply_within(REPLY_TIMEOUT).await
    }

    async fn write(&mut self, data: &[u8]) -> Result<()> {
        tokio::time::timeout(DATA_TIMEOUT, async {
            self.stream.write_all(data).await?;
            self.stream.flush().await
        })
        .await
        .map_err(|_| Error::Timeout)?
        .map_err(|e| Error::Io(e.to_string()))
    }

    async fn command(&mut self, line: &str) -> Result<Reply> {
        self.write(format!("{line}\r\n").as_bytes()).await?;
        self.reply().await
    }

    /// 发一条命令，回应不是成功就报错。
    async fn expect(&mut self, line: &str) -> Result<Reply> {
        let reply = self.command(line).await?;
        if reply.ok() {
            Ok(reply)
        } else {
            Err(reply.error())
        }
    }

    async fn ehlo(&mut self, name: &str) -> Result<Extensions> {
        let reply = self.command(&format!("EHLO {name}")).await?;
        if reply.code == 250 {
            return Ok(Extensions::parse(&reply));
        }
        // 很老的服务器不认 EHLO
        self.expect(&format!("HELO {name}")).await?;
        Ok(Extensions::default())
    }
}

/// 连上发信服务器、打过招呼、登录好，返回连接和服务器支持的功能。
async fn open(cfg: &ServerConfig, auth: &Auth) -> Result<(Client, Extensions)> {
    cfg.validate()?;
    let host = cfg.host.trim();
    let (mut client, ext) = tokio::time::timeout(CONNECT_TIMEOUT, async {
        let tcp = TcpStream::connect((host, cfg.port))
            .await
            .map_err(|e| Error::Connect(e.to_string()))?;
        let _ = tcp.set_nodelay(true);
        // 用本机地址作 EHLO 的名字。不暴露电脑的名字，服务器也都接受
        let name = match tcp.local_addr() {
            Ok(addr) if addr.is_ipv4() => format!("[{}]", addr.ip()),
            Ok(addr) => format!("[IPv6:{}]", addr.ip()),
            Err(_) => "[127.0.0.1]".to_string(),
        };
        let stream = match cfg.security {
            Security::Tls => upgrade(host, tcp).await?,
            Security::Starttls | Security::None => Stream::Plain(tcp),
        };
        let mut client = Client {
            stream,
            buf: Vec::new(),
        };
        let greeting = client.reply().await?;
        if greeting.code != 220 {
            return Err(greeting.error());
        }
        let mut ext = client.ehlo(&name).await?;
        if cfg.security == Security::Starttls {
            if !ext.starttls {
                return Err(Error::Tls("the server does not offer STARTTLS".into()));
            }
            client.expect("STARTTLS").await?;
            // TLS 之前多收到的内容不能带进加密连接里（STARTTLS 注入）
            if !client.buf.is_empty() {
                return Err(Error::Tls("unexpected data before TLS started".into()));
            }
            let Stream::Plain(tcp) = client.stream else {
                unreachable!("the stream was created as plain text above");
            };
            client = Client {
                stream: upgrade(host, tcp).await?,
                buf: Vec::new(),
            };
            ext = client.ehlo(&name).await?;
        }
        Ok((client, ext))
    })
    .await
    .map_err(|_| Error::Timeout)??;

    login(&mut client, &ext, cfg.username.trim(), auth).await?;
    Ok((client, ext))
}

async fn login(client: &mut Client, ext: &Extensions, user: &str, auth: &Auth) -> Result<()> {
    let has = |m: &str| ext.auth.iter().any(|a| a == m);
    // 不要求登录的服务器（多半是本机的转发程序）不说自己支持哪些登录方式
    if ext.auth.is_empty() && matches!(auth, Auth::Password(_)) {
        return Ok(());
    }
    let auth_error = |reply: &Reply| Error::Auth(reply.text());
    match auth {
        Auth::OAuth2(token) => {
            let first = base64(format!("user={user}\x01auth=Bearer {token}\x01\x01").as_bytes());
            let mut reply = client.command(&format!("AUTH XOAUTH2 {first}")).await?;
            if reply.code == 334 {
                // 失败时服务器先发一段说明，回一个空行，它再给出最终的错误
                reply = client.command("").await?;
            }
            if reply.code != 235 {
                return Err(auth_error(&reply));
            }
        }
        Auth::Password(password) => {
            if has("PLAIN") || !has("LOGIN") {
                let token = base64(format!("\0{user}\0{password}").as_bytes());
                let reply = client.command(&format!("AUTH PLAIN {token}")).await?;
                if reply.code != 235 {
                    return Err(auth_error(&reply));
                }
            } else {
                let reply = client.command("AUTH LOGIN").await?;
                if reply.code != 334 {
                    return Err(auth_error(&reply));
                }
                let reply = client.command(&base64(user.as_bytes())).await?;
                if reply.code != 334 {
                    return Err(auth_error(&reply));
                }
                let reply = client.command(&base64(password.as_bytes())).await?;
                if reply.code != 235 {
                    return Err(auth_error(&reply));
                }
            }
        }
    }
    Ok(())
}

/// 只连上登录一下，检查发信设置对不对。
pub async fn check(cfg: &ServerConfig, auth: &Auth) -> Result<()> {
    let (mut client, _) = open(cfg, auth).await?;
    let _ = client.command("QUIT").await;
    Ok(())
}

/// 把一封信交给发信服务器。`sender` 和 `recipients` 写在信封上，`raw` 是邮件原文。
pub async fn send(
    cfg: &ServerConfig,
    auth: &Auth,
    sender: &str,
    recipients: &[String],
    raw: &[u8],
) -> Result<()> {
    if recipients.is_empty() {
        return Err(Error::Config("no recipients".into()));
    }
    for address in std::iter::once(sender).chain(recipients.iter().map(String::as_str)) {
        if address.contains(['<', '>', '\r', '\n']) || address.trim().is_empty() {
            return Err(Error::Config(format!("invalid address: {address}")));
        }
    }
    let (mut client, ext) = open(cfg, auth).await?;
    if let Some(limit) = ext.size
        && raw.len() > limit
    {
        return Err(Error::TooLarge(limit));
    }
    let utf8 = !sender.is_ascii() || recipients.iter().any(|r| !r.is_ascii());
    let mut mail_from = format!("MAIL FROM:<{sender}>");
    if ext.size.is_some() {
        mail_from.push_str(&format!(" SIZE={}", raw.len()));
    }
    if utf8 && ext.smtputf8 {
        mail_from.push_str(" SMTPUTF8");
    }
    client.expect(&mail_from).await?;
    for rcpt in recipients {
        let reply = client.command(&format!("RCPT TO:<{rcpt}>")).await?;
        if !reply.ok() {
            return Err(Error::Server(format!("{rcpt}: {}", reply.text())));
        }
    }
    let reply = client.command("DATA").await?;
    if reply.code != 354 {
        return Err(reply.error());
    }
    client.write(&dot_stuff(raw)).await?;
    let reply = client.reply_within(DATA_TIMEOUT).await?;
    if !reply.ok() {
        return Err(reply.error());
    }
    let _ = client.command("QUIT").await;
    Ok(())
}

/// DATA 里一行只有「.」表示信结束，所以以「.」开头的行要多写一个「.」。末尾补上结束标记。
fn dot_stuff(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() + 64);
    let mut line_start = true;
    let mut prev = 0u8;
    for &b in raw {
        // 单独的 LF 也按协议补成 CRLF
        if b == b'\n' && prev != b'\r' {
            out.push(b'\r');
        }
        if line_start && b == b'.' {
            out.push(b'.');
        }
        out.push(b);
        line_start = b == b'\n';
        prev = b;
    }
    if !out.ends_with(b"\r\n") {
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b".\r\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(code: u16, lines: &[&str]) -> Reply {
        Reply {
            code,
            lines: lines.iter().map(|l| l.to_string()).collect(),
        }
    }

    #[test]
    fn extensions_are_read() {
        let ext = Extensions::parse(&reply(
            250,
            &[
                "smtp.example.com at your service",
                "SIZE 35882577",
                "8BITMIME",
                "AUTH LOGIN PLAIN XOAUTH2",
                "AUTH=LOGIN",
                "STARTTLS",
                "SMTPUTF8",
            ],
        ));
        assert!(ext.starttls);
        assert!(ext.smtputf8);
        assert_eq!(ext.size, Some(35_882_577));
        assert_eq!(ext.auth, ["LOGIN", "PLAIN", "XOAUTH2"]);

        let bare = Extensions::parse(&reply(250, &["hello"]));
        assert!(!bare.starttls);
        assert!(bare.auth.is_empty());
        assert_eq!(bare.size, None);
    }

    #[test]
    fn reply_text_is_readable() {
        assert_eq!(
            reply(
                535,
                &[
                    "5.7.8 Username and Password not accepted.",
                    "5.7.8 Learn more"
                ]
            )
            .text(),
            "Username and Password not accepted. Learn more"
        );
        assert_eq!(reply(550, &[""]).text(), "SMTP 550");
        assert_eq!(
            reply(421, &["Service not available"]).text(),
            "Service not available"
        );
    }

    #[test]
    fn data_is_dot_stuffed() {
        assert_eq!(
            dot_stuff(b"a\r\n.b\r\n..c\r\n"),
            b"a\r\n..b\r\n...c\r\n.\r\n"
        );
        assert_eq!(dot_stuff(b".first\nsecond"), b"..first\r\nsecond\r\n.\r\n");
        assert_eq!(dot_stuff(b"x\r\n"), b"x\r\n.\r\n");
    }
}
