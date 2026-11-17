//! 一个假的发信服务器（SMTP），跑在本机明文端口上。收下的信记在内存里，测试拿来检查；
//! 演示服务器把它们投进收件人的假邮箱。
//!
//! 和假 IMAP 一样：XOAUTH2 的访问令牌和密码相同就算登录成功。

use std::sync::{Arc, Mutex, MutexGuard};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use crate::base64_decode;

/// 收下的一封信。
#[derive(Debug, Clone)]
pub struct Delivered {
    /// 登录用的用户名。
    pub user: String,
    pub from: String,
    pub to: Vec<String>,
    pub raw: Vec<u8>,
}

type Hook = Arc<dyn Fn(&Delivered) + Send + Sync>;

struct State {
    users: Vec<(String, String)>,
    delivered: Vec<Delivered>,
    hook: Option<Hook>,
}

#[derive(Clone)]
pub struct FakeSmtp {
    state: Arc<Mutex<State>>,
    pub port: u16,
}

impl FakeSmtp {
    /// 起一个服务器，只认这一个用户。端口随机。
    pub async fn start(user: &str, pass: &str) -> FakeSmtp {
        let server = Self::start_on(0).await;
        server.add_user(user, pass);
        server
    }

    /// 在指定端口起一个还没有用户的服务器。开发时手动试用。
    pub async fn start_on(port: u16) -> FakeSmtp {
        let listener = TcpListener::bind(("127.0.0.1", port))
            .await
            .expect("bind fake smtp");
        let port = listener.local_addr().expect("local addr").port();
        let server = FakeSmtp {
            state: Arc::new(Mutex::new(State {
                users: Vec::new(),
                delivered: Vec::new(),
                hook: None,
            })),
            port,
        };
        let accept = server.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let conn = accept.clone();
                tokio::spawn(async move {
                    let _ = conn.serve(stream).await;
                });
            }
        });
        server
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn add_user(&self, user: &str, pass: &str) {
        self.lock().users.push((user.to_string(), pass.to_string()));
    }

    /// 每收下一封信就调用一次。
    pub fn on_deliver(&self, hook: impl Fn(&Delivered) + Send + Sync + 'static) {
        self.lock().hook = Some(Arc::new(hook));
    }

    pub fn delivered(&self) -> Vec<Delivered> {
        self.lock().delivered.clone()
    }

    fn check(&self, user: &str, secret: &str) -> bool {
        self.lock()
            .users
            .iter()
            .any(|(u, p)| u == user && p == secret)
    }

    async fn serve(&self, stream: TcpStream) -> std::io::Result<()> {
        let (read, mut w) = stream.into_split();
        let mut reader = BufReader::new(read);
        w.write_all(b"220 fake-smtp ESMTP ready\r\n").await?;

        let mut user: Option<String> = None;
        let mut from: Option<String> = None;
        let mut to: Vec<String> = Vec::new();
        let mut line = String::new();

        macro_rules! read_line {
            () => {{
                line.clear();
                if reader.read_line(&mut line).await? == 0 {
                    return Ok(());
                }
                line.trim_end_matches(['\r', '\n']).to_string()
            }};
        }

        loop {
            let text = read_line!();
            let (verb, rest) = text.split_once(' ').unwrap_or((&text, ""));
            let verb = verb.to_ascii_uppercase();
            let reply: String = match verb.as_str() {
                "EHLO" => "250-fake-smtp\r\n250-SIZE 10485760\r\n250-8BITMIME\r\n\
                           250-AUTH PLAIN LOGIN XOAUTH2\r\n250 SMTPUTF8"
                    .into(),
                "HELO" => "250 fake-smtp".into(),
                "NOOP" => "250 OK".into(),
                "RSET" => {
                    from = None;
                    to.clear();
                    "250 OK".into()
                }
                "QUIT" => {
                    w.write_all(b"221 2.0.0 Bye\r\n").await?;
                    return Ok(());
                }
                "AUTH" => {
                    let mut words = rest.split_whitespace();
                    let mech = words.next().unwrap_or("").to_ascii_uppercase();
                    let initial = words.next().map(str::to_string);
                    let (name, secret) = match mech.as_str() {
                        "PLAIN" => {
                            let token = match initial {
                                Some(t) => t,
                                None => {
                                    w.write_all(b"334 \r\n").await?;
                                    read_line!()
                                }
                            };
                            let decoded = base64_decode(&token);
                            let parts: Vec<String> = decoded
                                .split(|b| *b == 0)
                                .map(|p| String::from_utf8_lossy(p).to_string())
                                .collect();
                            match parts.as_slice() {
                                [_, u, p] => (u.clone(), p.clone()),
                                _ => (String::new(), String::new()),
                            }
                        }
                        "LOGIN" => {
                            w.write_all(b"334 VXNlcm5hbWU6\r\n").await?;
                            let u = read_line!();
                            w.write_all(b"334 UGFzc3dvcmQ6\r\n").await?;
                            let p = read_line!();
                            (
                                String::from_utf8_lossy(&base64_decode(&u)).to_string(),
                                String::from_utf8_lossy(&base64_decode(&p)).to_string(),
                            )
                        }
                        "XOAUTH2" => {
                            let text = String::from_utf8_lossy(&base64_decode(
                                initial.as_deref().unwrap_or(""),
                            ))
                            .to_string();
                            let field = |name: &str| {
                                text.split('\x01')
                                    .find_map(|part| part.strip_prefix(name))
                                    .map(str::to_string)
                                    .unwrap_or_default()
                            };
                            let (u, t) = (field("user="), field("auth=Bearer "));
                            if !self.check(&u, &t) {
                                // 和真服务器一样：先发一段错误说明，等客户端回一个空行，再拒绝
                                w.write_all(b"334 eyJzdGF0dXMiOiI0MDEifQ==\r\n").await?;
                                read_line!();
                            }
                            (u, t)
                        }
                        _ => {
                            w.write_all(b"504 5.5.4 Unrecognized authentication type\r\n")
                                .await?;
                            continue;
                        }
                    };
                    if self.check(&name, &secret) {
                        user = Some(name);
                        "235 2.7.0 Accepted".into()
                    } else {
                        "535 5.7.8 Username and Password not accepted.".into()
                    }
                }
                _ if user.is_none() => "530 5.7.0 Authentication required".into(),
                "MAIL" => match angle(rest) {
                    Some(address) => {
                        from = Some(address);
                        to.clear();
                        "250 2.1.0 OK".into()
                    }
                    None => "501 5.5.4 Syntax error".into(),
                },
                "RCPT" => match angle(rest) {
                    // 测试发信被拒的情况：地址里带 reject 的都不收
                    Some(address) if address.contains("reject") => {
                        "550 5.1.1 The email account that you tried to reach does not exist.".into()
                    }
                    Some(address) if from.is_some() => {
                        to.push(address);
                        "250 2.1.5 OK".into()
                    }
                    _ => "503 5.5.1 Bad sequence of commands".into(),
                },
                "DATA" if from.is_some() && !to.is_empty() => {
                    w.write_all(b"354 Go ahead\r\n").await?;
                    let mut raw: Vec<u8> = Vec::new();
                    loop {
                        line.clear();
                        if reader.read_line(&mut line).await? == 0 {
                            return Ok(());
                        }
                        if line == ".\r\n" || line == ".\n" {
                            break;
                        }
                        let content = line.strip_prefix('.').unwrap_or(&line);
                        raw.extend_from_slice(content.as_bytes());
                    }
                    let mail = Delivered {
                        user: user.clone().unwrap_or_default(),
                        from: from.take().unwrap_or_default(),
                        to: std::mem::take(&mut to),
                        raw,
                    };
                    let hook = {
                        let mut s = self.lock();
                        s.delivered.push(mail.clone());
                        s.hook.clone()
                    };
                    if let Some(hook) = hook {
                        hook(&mail);
                    }
                    "250 2.0.0 OK queued".into()
                }
                "DATA" => "503 5.5.1 Bad sequence of commands".into(),
                _ => "502 5.5.2 Command not recognized".into(),
            };
            w.write_all(format!("{reply}\r\n").as_bytes()).await?;
        }
    }
}

/// 从 `FROM:<a@example.com> SIZE=12` 里取出尖括号里的地址。
fn angle(rest: &str) -> Option<String> {
    let start = rest.find('<')?;
    let end = rest[start..].find('>')? + start;
    Some(rest[start + 1..end].to_string())
}
