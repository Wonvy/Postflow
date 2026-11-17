//! 一个只实现了 Postflow 用到的那部分命令的假 IMAP 服务器，跑在本机明文端口上。
//!
//! 它不追求完整，只追求在协议细节上足够真实，让客户端代码在测试里走的是和真实服务器一样的路径：
//! 序号与 UID 的区别、「n:*」的边界行为、MOVE 之后的 EXPUNGE、IDLE 期间推送新邮件。

pub mod ai;

pub use ai::FakeAi;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

const CAPS: &str = "IMAP4rev1 IDLE MOVE UIDPLUS ID";

#[derive(Debug, Clone)]
struct Msg {
    uid: u32,
    flags: BTreeSet<String>,
    raw: Vec<u8>,
}

#[derive(Debug, Clone)]
struct Mbox {
    uidvalidity: u32,
    uidnext: u32,
    special: Option<&'static str>,
    msgs: Vec<Msg>,
}

impl Mbox {
    fn new(uidvalidity: u32, special: Option<&'static str>) -> Self {
        Mbox {
            uidvalidity,
            uidnext: 1,
            special,
            msgs: Vec::new(),
        }
    }

    fn push(&mut self, raw: Vec<u8>, flags: BTreeSet<String>) -> u32 {
        let uid = self.uidnext;
        self.uidnext += 1;
        self.msgs.push(Msg { uid, flags, raw });
        uid
    }
}

#[derive(Debug)]
struct State {
    user: String,
    pass: String,
    mailboxes: BTreeMap<String, Mbox>,
    commands: Vec<String>,
    logins: usize,
    /// IDLE 期间不通知新邮件，只每隔这么久发一句「还在」。有的邮件服务器就是这样。
    quiet_idle: Option<Duration>,
}

#[derive(Clone)]
pub struct FakeImap {
    state: Arc<Mutex<State>>,
    changes: watch::Sender<u64>,
    pub port: u16,
}

impl FakeImap {
    /// 起一个服务器，带一个空的 INBOX 和一个标了 \Archive 的 Archive。端口随机。
    pub async fn start(user: &str, pass: &str) -> FakeImap {
        Self::start_on(user, pass, 0).await
    }

    /// 和 `start` 一样，但监听指定端口。开发时手动试用。
    pub async fn start_on(user: &str, pass: &str, port: u16) -> FakeImap {
        let mut mailboxes = BTreeMap::new();
        mailboxes.insert("INBOX".to_string(), Mbox::new(1000, None));
        mailboxes.insert("Archive".to_string(), Mbox::new(2000, Some("\\Archive")));
        let state = Arc::new(Mutex::new(State {
            user: user.to_string(),
            pass: pass.to_string(),
            mailboxes,
            commands: Vec::new(),
            logins: 0,
            quiet_idle: None,
        }));
        let (changes, _) = watch::channel(0);
        let listener = TcpListener::bind(("127.0.0.1", port))
            .await
            .expect("bind fake imap");
        let port = listener.local_addr().expect("local addr").port();
        let server = FakeImap {
            state,
            changes,
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

    fn changed(&self) {
        self.changes.send_modify(|v| *v += 1);
    }

    /// 之后的 IDLE 不再通知新邮件，只每隔 `keepalive` 发一句 `* OK Still here`。
    pub fn quiet_idle(&self, keepalive: Duration) {
        self.lock().quiet_idle = Some(keepalive);
    }

    /// 投递一封邮件，返回它的 UID。正在 IDLE 的连接会收到通知。
    pub fn deliver(&self, mailbox: &str, raw: &str, flags: &[&str]) -> u32 {
        let uid = {
            let mut s = self.lock();
            let mbox = s.mailboxes.get_mut(mailbox).expect("mailbox exists");
            mbox.push(
                raw.replace("\r\n", "\n").replace('\n', "\r\n").into_bytes(),
                flags.iter().map(|f| f.to_string()).collect(),
            )
        };
        self.changed();
        uid
    }

    /// 原文里包含 `needle` 的那封邮件当前的标记。
    pub fn flags(&self, mailbox: &str, needle: &str) -> Option<Vec<String>> {
        let s = self.lock();
        let msg = find(&s.mailboxes.get(mailbox)?.msgs, needle)?;
        Some(msg.flags.iter().cloned().collect())
    }

    pub fn count(&self, mailbox: &str) -> usize {
        self.lock()
            .mailboxes
            .get(mailbox)
            .map_or(0, |m| m.msgs.len())
    }

    pub fn contains(&self, mailbox: &str, needle: &str) -> bool {
        self.lock()
            .mailboxes
            .get(mailbox)
            .is_some_and(|m| find(&m.msgs, needle).is_some())
    }

    /// 模拟另一个客户端改了标记。
    pub fn set_flag(&self, mailbox: &str, needle: &str, flag: &str, on: bool) {
        {
            let mut s = self.lock();
            let mbox = s.mailboxes.get_mut(mailbox).expect("mailbox exists");
            let msg = mbox
                .msgs
                .iter_mut()
                .find(|m| contains(&m.raw, needle))
                .expect("message exists");
            if on {
                msg.flags.insert(flag.to_string());
            } else {
                msg.flags.remove(flag);
            }
        }
        self.changed();
    }

    /// 模拟另一个客户端删除了邮件。
    pub fn remove(&self, mailbox: &str, needle: &str) {
        {
            let mut s = self.lock();
            let mbox = s.mailboxes.get_mut(mailbox).expect("mailbox exists");
            mbox.msgs.retain(|m| !contains(&m.raw, needle));
        }
        self.changed();
    }

    /// 模拟服务器重建了邮箱：UIDVALIDITY 变了，所有 UID 重新编号。
    pub fn renumber(&self, mailbox: &str) {
        let mut s = self.lock();
        let mbox = s.mailboxes.get_mut(mailbox).expect("mailbox exists");
        let msgs = std::mem::take(&mut mbox.msgs);
        let mut fresh = Mbox::new(mbox.uidvalidity + 1, mbox.special);
        fresh.uidnext = 500;
        for m in msgs {
            fresh.push(m.raw, m.flags);
        }
        *mbox = fresh;
    }

    pub fn logins(&self) -> usize {
        self.lock().logins
    }

    /// 收到过的命令（不含标签），用来检查客户端发了什么。
    pub fn commands(&self) -> Vec<String> {
        self.lock().commands.clone()
    }

    async fn serve(&self, stream: TcpStream) -> std::io::Result<()> {
        let (read, mut w) = stream.into_split();
        let mut reader = BufReader::new(read);
        let mut changes = self.changes.subscribe();
        w.write_all(format!("* OK [CAPABILITY {CAPS}] Fake IMAP ready\r\n").as_bytes())
            .await?;

        let mut authed = false;
        let mut selected: Option<String> = None;
        let mut reported = 0usize;
        let mut line = String::new();

        loop {
            line.clear();
            if reader.read_line(&mut line).await? == 0 {
                return Ok(());
            }
            let text = line.trim_end_matches(['\r', '\n']).to_string();
            let Some((tag, rest)) = text.split_once(' ') else {
                continue;
            };
            let tokens = tokenize(rest);
            let Some(first) = tokens.first() else {
                continue;
            };
            let mut cmd = first.to_ascii_uppercase();
            let mut args = &tokens[1..];
            if cmd == "UID" && !args.is_empty() {
                cmd = format!("UID {}", args[0].to_ascii_uppercase());
                args = &args[1..];
            }
            self.lock().commands.push(redact(&cmd, rest));

            let mut out = String::new();
            match cmd.as_str() {
                "CAPABILITY" => {
                    out.push_str(&format!("* CAPABILITY {CAPS}\r\n{tag} OK done\r\n"));
                }
                "LOGIN" => {
                    let mut s = self.lock();
                    if args.len() == 2 && args[0] == s.user && args[1] == s.pass {
                        s.logins += 1;
                        authed = true;
                        out.push_str(&format!("{tag} OK [CAPABILITY {CAPS}] Logged in\r\n"));
                    } else {
                        out.push_str(&format!(
                            "{tag} NO [AUTHENTICATIONFAILED] Invalid credentials\r\n"
                        ));
                    }
                }
                "LOGOUT" => {
                    w.write_all(format!("* BYE logging out\r\n{tag} OK done\r\n").as_bytes())
                        .await?;
                    return Ok(());
                }
                "NOOP" => {
                    if let Some(mb) = &selected {
                        let n = self.count(mb);
                        out.push_str(&format!("* {n} EXISTS\r\n"));
                        reported = n;
                    }
                    out.push_str(&format!("{tag} OK done\r\n"));
                }
                _ if !authed => out.push_str(&format!("{tag} NO not authenticated\r\n")),
                "ID" => out.push_str(&format!(
                    "* ID (\"name\" \"fake-imap\")\r\n{tag} OK done\r\n"
                )),
                "LIST" => {
                    for (name, mbox) in &self.lock().mailboxes {
                        let special = mbox.special.map(|s| format!(" {s}")).unwrap_or_default();
                        out.push_str(&format!(
                            "* LIST (\\HasNoChildren{special}) \"/\" \"{name}\"\r\n"
                        ));
                    }
                    out.push_str(&format!("{tag} OK done\r\n"));
                }
                "CREATE" => {
                    let name = args.first().cloned().unwrap_or_default();
                    let mut s = self.lock();
                    let validity = 3000 + s.mailboxes.len() as u32;
                    s.mailboxes
                        .entry(name)
                        .or_insert_with(|| Mbox::new(validity, None));
                    out.push_str(&format!("{tag} OK done\r\n"));
                }
                "SELECT" | "EXAMINE" => {
                    let name = args.first().cloned().unwrap_or_default();
                    let s = self.lock();
                    match s.mailboxes.get(&name) {
                        Some(mbox) => {
                            out.push_str(
                                "* FLAGS (\\Answered \\Flagged \\Deleted \\Seen \\Draft)\r\n",
                            );
                            out.push_str("* OK [PERMANENTFLAGS (\\Answered \\Flagged \\Deleted \\Seen \\Draft \\*)] ok\r\n");
                            out.push_str(&format!(
                                "* {} EXISTS\r\n* 0 RECENT\r\n",
                                mbox.msgs.len()
                            ));
                            out.push_str(&format!(
                                "* OK [UIDVALIDITY {}] ok\r\n",
                                mbox.uidvalidity
                            ));
                            out.push_str(&format!("* OK [UIDNEXT {}] ok\r\n", mbox.uidnext));
                            out.push_str(&format!("{tag} OK [READ-WRITE] done\r\n"));
                            reported = mbox.msgs.len();
                            selected = Some(name);
                        }
                        None => out.push_str(&format!("{tag} NO no such mailbox\r\n")),
                    }
                }
                "FETCH" | "UID FETCH" => {
                    let by_uid = cmd.starts_with("UID");
                    let (Some(set), Some(mb)) = (args.first(), &selected) else {
                        out.push_str(&format!("{tag} BAD bad fetch\r\n"));
                        w.write_all(out.as_bytes()).await?;
                        continue;
                    };
                    let items = args[1..].join(" ").to_ascii_uppercase();
                    let buf = {
                        let s = self.lock();
                        let msgs = &s.mailboxes[mb].msgs;
                        let mut buf: Vec<u8> = Vec::new();
                        for (i, msg) in msgs.iter().enumerate() {
                            let seq = i as u32 + 1;
                            let key = if by_uid { msg.uid } else { seq };
                            let max = if by_uid {
                                msgs.last().map_or(0, |m| m.uid)
                            } else {
                                msgs.len() as u32
                            };
                            if !in_set(set, key, max) {
                                continue;
                            }
                            buf.extend_from_slice(
                                fetch_response(seq, msg, &items, by_uid).as_slice(),
                            );
                        }
                        buf
                    };
                    w.write_all(&buf).await?;
                    out.push_str(&format!("{tag} OK done\r\n"));
                }
                "UID STORE" => {
                    let (Some(set), Some(op), Some(mb)) = (args.first(), args.get(1), &selected)
                    else {
                        out.push_str(&format!("{tag} BAD bad store\r\n"));
                        w.write_all(out.as_bytes()).await?;
                        continue;
                    };
                    let flags: Vec<String> = args[2..]
                        .join(" ")
                        .trim_matches(['(', ')'])
                        .split_whitespace()
                        .map(str::to_string)
                        .collect();
                    let add = op.starts_with('+');
                    {
                        let mut s = self.lock();
                        let mbox = s.mailboxes.get_mut(mb).expect("selected mailbox exists");
                        let max = mbox.msgs.last().map_or(0, |m| m.uid);
                        for msg in mbox.msgs.iter_mut().filter(|m| in_set(set, m.uid, max)) {
                            for f in &flags {
                                if add {
                                    msg.flags.insert(f.clone());
                                } else {
                                    msg.flags.remove(f);
                                }
                            }
                        }
                    }
                    out.push_str(&format!("{tag} OK done\r\n"));
                }
                "UID MOVE" => {
                    let (Some(set), Some(dest), Some(mb)) =
                        (args.first(), args.get(1), selected.clone())
                    else {
                        out.push_str(&format!("{tag} BAD bad move\r\n"));
                        w.write_all(out.as_bytes()).await?;
                        continue;
                    };
                    let mut s = self.lock();
                    if !s.mailboxes.contains_key(dest) {
                        out.push_str(&format!("{tag} NO [TRYCREATE] no such mailbox\r\n"));
                    } else {
                        let src = s.mailboxes.get_mut(&mb).expect("selected mailbox exists");
                        let max = src.msgs.last().map_or(0, |m| m.uid);
                        let mut moved = Vec::new();
                        let mut expunged = Vec::new();
                        let mut i = 0;
                        while i < src.msgs.len() {
                            if in_set(set, src.msgs[i].uid, max) {
                                expunged.push(i as u32 + 1);
                                moved.push(src.msgs.remove(i));
                            } else {
                                i += 1;
                            }
                        }
                        let target = s.mailboxes.get_mut(dest).expect("checked above");
                        let (mut from, mut to) = (Vec::new(), Vec::new());
                        for m in moved {
                            from.push(m.uid.to_string());
                            to.push(target.push(m.raw, m.flags).to_string());
                        }
                        if !from.is_empty() {
                            out.push_str(&format!(
                                "* OK [COPYUID {} {} {}] moved\r\n",
                                target.uidvalidity,
                                from.join(","),
                                to.join(",")
                            ));
                        }
                        // 序号在每次删除后都会前移，所以从后往前报
                        for seq in expunged.iter().rev() {
                            out.push_str(&format!("* {seq} EXPUNGE\r\n"));
                        }
                        reported = s.mailboxes[&mb].msgs.len();
                        out.push_str(&format!("{tag} OK done\r\n"));
                    }
                }
                "UID SEARCH" => {
                    let Some(mb) = &selected else {
                        out.push_str(&format!("{tag} BAD nothing selected\r\n"));
                        w.write_all(out.as_bytes()).await?;
                        continue;
                    };
                    let s = self.lock();
                    let msgs = &s.mailboxes[mb].msgs;
                    let hits: Vec<String> =
                        if args.len() >= 3 && args[0].eq_ignore_ascii_case("HEADER") {
                            let header = args[1].to_ascii_lowercase();
                            let value = &args[2];
                            msgs.iter()
                                .filter(|m| {
                                    header_value(&m.raw, &header)
                                        .is_some_and(|v| v.contains(value.as_str()))
                                })
                                .map(|m| m.uid.to_string())
                                .collect()
                        } else {
                            msgs.iter().map(|m| m.uid.to_string()).collect()
                        };
                    let list = if hits.is_empty() {
                        String::new()
                    } else {
                        format!(" {}", hits.join(" "))
                    };
                    out.push_str(&format!("* SEARCH{list}\r\n{tag} OK done\r\n"));
                }
                "IDLE" => {
                    w.write_all(b"+ idling\r\n").await?;
                    let quiet = self.lock().quiet_idle;
                    loop {
                        // 进入 IDLE 前就已经到了的新邮件，也要马上告诉客户端
                        if quiet.is_none()
                            && let Some(mb) = &selected
                        {
                            let n = self.count(mb);
                            if n != reported {
                                w.write_all(format!("* {n} EXISTS\r\n").as_bytes()).await?;
                                reported = n;
                            }
                        }
                        line.clear();
                        tokio::select! {
                            read = reader.read_line(&mut line) => {
                                if read? == 0 {
                                    return Ok(());
                                }
                                if line.trim().eq_ignore_ascii_case("DONE") {
                                    break;
                                }
                            }
                            _ = keepalive(quiet) => {
                                w.write_all(b"* OK Still here\r\n").await?;
                            }
                            changed = changes.changed() => {
                                if changed.is_err() {
                                    return Ok(());
                                }
                                // 标记变化也推一个 EXISTS，让客户端去同步
                                if quiet.is_none()
                                    && let Some(mb) = &selected
                                {
                                    let n = self.count(mb);
                                    w.write_all(format!("* {n} EXISTS\r\n").as_bytes()).await?;
                                    reported = n;
                                }
                            }
                        }
                    }
                    out.push_str(&format!("{tag} OK IDLE terminated\r\n"));
                }
                _ => out.push_str(&format!("{tag} BAD unknown command\r\n")),
            }
            write(&mut w, &out).await?;
        }
    }
}

/// 安静模式下每隔一会儿醒一次，发「还在」；平时永远不醒。
async fn keepalive(every: Option<Duration>) {
    match every {
        Some(every) => tokio::time::sleep(every).await,
        None => std::future::pending().await,
    }
}

async fn write(w: &mut OwnedWriteHalf, out: &str) -> std::io::Result<()> {
    if !out.is_empty() {
        w.write_all(out.as_bytes()).await?;
    }
    Ok(())
}

fn redact(cmd: &str, rest: &str) -> String {
    if cmd == "LOGIN" {
        "LOGIN ***".to_string()
    } else {
        rest.to_string()
    }
}

fn contains(raw: &[u8], needle: &str) -> bool {
    raw.windows(needle.len()).any(|w| w == needle.as_bytes())
}

fn find<'a>(msgs: &'a [Msg], needle: &str) -> Option<&'a Msg> {
    msgs.iter().find(|m| contains(&m.raw, needle))
}

fn header_value(raw: &[u8], name: &str) -> Option<String> {
    let text = String::from_utf8_lossy(raw);
    let header = text.split("\r\n\r\n").next()?;
    header.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        k.trim()
            .eq_ignore_ascii_case(name)
            .then(|| v.trim().to_string())
    })
}

fn fetch_response(seq: u32, msg: &Msg, items: &str, by_uid: bool) -> Vec<u8> {
    let mut parts: Vec<String> = Vec::new();
    if by_uid || items.contains("UID") {
        parts.push(format!("UID {}", msg.uid));
    }
    if items.contains("FLAGS") {
        parts.push(format!(
            "FLAGS ({})",
            msg.flags.iter().cloned().collect::<Vec<_>>().join(" ")
        ));
    }
    if items.contains("INTERNALDATE") {
        parts.push("INTERNALDATE \"16-Nov-2026 08:00:00 +0000\"".to_string());
    }
    if items.contains("RFC822.SIZE") {
        parts.push(format!("RFC822.SIZE {}", msg.raw.len()));
    }
    let literal: Option<(&str, &[u8])> = if items.contains("BODY.PEEK[HEADER]") {
        let end = msg
            .raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .map_or(msg.raw.len(), |p| p + 4);
        Some(("BODY[HEADER]", &msg.raw[..end]))
    } else if items.contains("BODY.PEEK[]") {
        Some(("BODY[]", &msg.raw[..]))
    } else {
        None
    };
    let mut out = format!("* {seq} FETCH ({}", parts.join(" ")).into_bytes();
    if let Some((name, bytes)) = literal {
        if !parts.is_empty() {
            out.push(b' ');
        }
        out.extend_from_slice(format!("{name} {{{}}}\r\n", bytes.len()).as_bytes());
        out.extend_from_slice(bytes);
    }
    out.extend_from_slice(b")\r\n");
    out
}

/// 判断 `n` 是否落在 IMAP 集合里，`*` 代表 `max`。
fn in_set(set: &str, n: u32, max: u32) -> bool {
    let value = |s: &str| if s == "*" { Some(max) } else { s.parse().ok() };
    set.split(',').any(|part| match part.split_once(':') {
        Some((a, b)) => match (value(a), value(b)) {
            (Some(a), Some(b)) => (a.min(b)..=a.max(b)).contains(&n),
            _ => false,
        },
        None => value(part) == Some(n),
    })
}

/// 按 IMAP 的规则切分参数：引号字符串（支持转义）、括号列表整体保留、其余按空格分。
fn tokenize(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c == ' ' {
            chars.next();
        } else if c == '"' {
            chars.next();
            let mut tok = String::new();
            while let Some(c) = chars.next() {
                match c {
                    '\\' => {
                        if let Some(n) = chars.next() {
                            tok.push(n);
                        }
                    }
                    '"' => break,
                    _ => tok.push(c),
                }
            }
            out.push(tok);
        } else if c == '(' {
            let mut depth = 0;
            let mut tok = String::new();
            for c in chars.by_ref() {
                tok.push(c);
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            out.push(tok);
        } else {
            let mut tok = String::new();
            while let Some(&c) = chars.peek() {
                if c == ' ' {
                    break;
                }
                tok.push(c);
                chars.next();
            }
            out.push(tok);
        }
    }
    out
}

/// 拼一封简单的测试邮件。
pub fn mail(message_id: &str, from: &str, subject: &str, body: &str) -> String {
    format!(
        "From: {from}\r\nTo: me@example.com\r\nSubject: {subject}\r\n\
         Date: Mon, 16 Nov 2026 08:00:00 +0000\r\nMessage-ID: <{message_id}>\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\r\n{body}\r\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets() {
        assert!(in_set("1:3", 2, 10));
        assert!(in_set("5:*", 10, 10));
        // n:* 在 n 大于最大值时仍然包含最大值
        assert!(in_set("50:*", 10, 10));
        assert!(!in_set("50:*", 9, 10));
        assert!(in_set("1,4,7:8", 8, 10));
        assert!(!in_set("1,4,7:8", 5, 10));
    }

    #[test]
    fn tokens() {
        assert_eq!(
            tokenize(r#"LOGIN "me@example.com" "p\"w d""#),
            vec!["LOGIN", "me@example.com", "p\"w d"]
        );
        assert_eq!(
            tokenize("FETCH 1:* (UID FLAGS BODY.PEEK[])"),
            vec!["FETCH", "1:*", "(UID FLAGS BODY.PEEK[])"]
        );
    }
}
