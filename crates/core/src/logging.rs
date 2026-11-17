//! 日志：写到本机目录下的 `logs/postflow.log`，满 5 MB 换一个新文件，保留最近三个旧文件。
//!
//! 只记发生了什么（连接、同步、抓取、出错），不记邮件内容、密码和密钥。写进文件之前，
//! 邮箱地址只留第一个字和域名，像 API Key 的字符串整个遮掉，用户目录换成 `~`，
//! 日志可以放心发给别人帮忙排查。

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Registry, fmt, reload};

const FILE: &str = "postflow.log";
const SETTINGS: &str = "settings.json";
const MAX_BYTES: u64 = 5 * 1024 * 1024;
/// 保留几个旧文件：postflow.log.1、.2、.3。
const KEEP: usize = 3;
/// 自己的代码。平时记 info，打开详细日志时记 debug；其他库只记警告以上。
const CRATES: &[&str] = &[
    "postflow_core",
    "postflow_mail",
    "postflow_feed",
    "postflow_store",
    "postflow_ai",
    "postflow_translate",
    "postflow_desktop_lib",
    "postflow_web",
    "postflow_ui",
];

fn directives(verbose: bool) -> String {
    let level = if verbose { "debug" } else { "info" };
    let mut out = String::from(if verbose { "info" } else { "warn" });
    for c in CRATES {
        out.push_str(&format!(",{c}={level}"));
    }
    out
}

struct Logs {
    dir: PathBuf,
    reload: reload::Handle<EnvFilter, Registry>,
    verbose: Mutex<bool>,
}

static LOGS: OnceLock<Logs> = OnceLock::new();
static HOME: OnceLock<Option<String>> = OnceLock::new();

/// 开始写日志。`stderr` 为真时同时打到终端（开发时、网页服务）。只有第一次调用起作用。
/// 设置了 `RUST_LOG` 时按它的级别来。
pub fn init(local_dir: &Path, stderr: bool) {
    let dir = local_dir.join("logs");
    let _ = fs::create_dir_all(&dir);
    HOME.get_or_init(|| crate::home_dir().map(|h| h.to_string_lossy().into_owned()));
    let verbose = read_verbose(&dir);
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(directives(verbose)));
    let (filter, handle) = reload::Layer::new(filter);
    let file = fmt::layer()
        .with_ansi(false)
        .with_writer(RotatingFile::new(dir.clone()));
    let terminal = stderr.then(|| fmt::layer().with_writer(io::stderr));
    if tracing_subscriber::registry()
        .with(filter)
        .with(file)
        .with(terminal)
        .try_init()
        .is_ok()
    {
        let _ = LOGS.set(Logs {
            dir,
            reload: handle,
            verbose: Mutex::new(verbose),
        });
        tracing::info!(
            version = env!("CARGO_PKG_VERSION"),
            os = std::env::consts::OS,
            "Postflow started"
        );
    }
}

fn read_verbose(dir: &Path) -> bool {
    fs::read(dir.join(SETTINGS))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| v.get("verbose").and_then(serde_json::Value::as_bool))
        .unwrap_or(false)
}

/// 日志的位置、是否详细，以及最近的若干行。没有初始化日志（比如测试里）时为 `None`。
pub fn tail(lines: usize) -> Option<(String, bool, String)> {
    let logs = LOGS.get()?;
    let mut text = String::new();
    for name in [format!("{FILE}.1"), FILE.to_string()] {
        if let Ok(content) = fs::read_to_string(logs.dir.join(name)) {
            text.push_str(&content);
        }
    }
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(lines);
    let verbose = *logs.verbose.lock().unwrap_or_else(|e| e.into_inner());
    Some((
        logs.dir.to_string_lossy().into_owned(),
        verbose,
        all[start..].join("\n"),
    ))
}

/// 打开或关掉详细日志，立即生效，并记住设置。
pub fn set_verbose(on: bool) -> bool {
    let Some(logs) = LOGS.get() else {
        return false;
    };
    let _ = logs.reload.modify(|f| *f = EnvFilter::new(directives(on)));
    *logs.verbose.lock().unwrap_or_else(|e| e.into_inner()) = on;
    let _ = fs::write(
        logs.dir.join(SETTINGS),
        serde_json::json!({ "verbose": on }).to_string(),
    );
    tracing::info!(verbose = on, "log level changed");
    true
}

/// 写日志文件。每条日志攒成一行，遮盖之后一次写进去；文件太大就换一个。
struct RotatingFile {
    dir: PathBuf,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    file: Option<File>,
    size: u64,
}

impl RotatingFile {
    fn new(dir: PathBuf) -> RotatingFile {
        RotatingFile {
            dir,
            state: Mutex::new(State::default()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn append(&self, bytes: &[u8]) {
        let mut state = self.lock();
        let path = self.dir.join(FILE);
        if state.file.is_none() {
            let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) else {
                return;
            };
            state.size = file.metadata().map(|m| m.len()).unwrap_or(0);
            state.file = Some(file);
        }
        if state.size + bytes.len() as u64 > MAX_BYTES {
            state.file = None;
            for i in (1..KEEP).rev() {
                let _ = fs::rename(
                    self.dir.join(format!("{FILE}.{i}")),
                    self.dir.join(format!("{FILE}.{}", i + 1)),
                );
            }
            let _ = fs::rename(&path, self.dir.join(format!("{FILE}.1")));
            let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) else {
                return;
            };
            state.file = Some(file);
            state.size = 0;
        }
        if let Some(file) = state.file.as_mut()
            && file.write_all(bytes).is_ok()
        {
            state.size += bytes.len() as u64;
        }
    }
}

impl<'a> fmt::MakeWriter<'a> for RotatingFile {
    type Writer = EventWriter<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        EventWriter {
            target: self,
            buf: Vec::new(),
        }
    }
}

/// 一条日志。写完（被丢掉）时遮盖敏感信息，再追加到文件里。
struct EventWriter<'a> {
    target: &'a RotatingFile,
    buf: Vec<u8>,
}

impl Write for EventWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for EventWriter<'_> {
    fn drop(&mut self) {
        if !self.buf.is_empty() {
            let text = String::from_utf8_lossy(&self.buf);
            let home = HOME.get().and_then(|h| h.as_deref());
            self.target.append(redact(&text, home).as_bytes());
        }
    }
}

/// 遮盖日志里的敏感信息：邮箱地址只留第一个字和域名，像密钥的字符串遮掉，用户目录换成 `~`。
pub fn redact(text: &str, home: Option<&str>) -> String {
    let text = match home.filter(|h| h.len() > 1) {
        Some(h) => text.replace(h, "~"),
        None => text.to_string(),
    };
    let chars: Vec<char> = text.chars().collect();
    let local = |c: char| c.is_ascii_alphanumeric() || "._%+-".contains(c);
    let domain = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '.';
    // 要遮掉的范围（字符下标），以及换成什么
    let mut cuts: Vec<(usize, usize, String)> = Vec::new();
    for (at, c) in chars.iter().enumerate() {
        if *c != '@' {
            continue;
        }
        let mut start = at;
        while start > 0 && local(chars[start - 1]) {
            start -= 1;
        }
        let mut end = at + 1;
        while end < chars.len() && domain(chars[end]) {
            end += 1;
        }
        while end > at + 1 && chars[end - 1] == '.' {
            end -= 1;
        }
        let host: String = chars[at + 1..end].iter().collect();
        if start < at && host.contains('.') && !host.starts_with('.') {
            let first: String = chars[start..start + 1].iter().collect();
            cuts.push((start, at, format!("{first}***")));
        }
    }
    // 像 API Key 的：sk- 开头的一长串，以及 Bearer 后面的令牌
    let token = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.';
    let joined: String = chars.iter().collect();
    for (marker, keep) in [("sk-", "sk-"), ("Bearer ", "Bearer ")] {
        let mut from = 0;
        while let Some(found) = joined[from..].find(marker) {
            let byte = from + found;
            let start = joined[..byte].chars().count() + keep.chars().count();
            let mut end = start;
            while end < chars.len() && token(chars[end]) {
                end += 1;
            }
            if end - start >= 8 {
                cuts.push((start, end, "***".to_string()));
            }
            from = byte + marker.len();
        }
    }
    cuts.sort_by_key(|c| c.0);
    let mut out = String::with_capacity(text.len());
    let mut pos = 0;
    for (start, end, with) in cuts {
        if start < pos {
            continue;
        }
        out.extend(&chars[pos..start]);
        out.push_str(&with);
        pos = end;
    }
    out.extend(&chars[pos..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_addresses_keys_and_home() {
        let line = "login failed for jane.doe@example.com at /Users/someone/Library with Bearer abcdefghijk and sk-1234567890abcdef";
        let out = redact(line, Some("/Users/someone"));
        assert_eq!(
            out,
            "login failed for j***@example.com at ~/Library with Bearer *** and sk-***"
        );
    }

    #[test]
    fn leaves_ordinary_text_alone() {
        let line = "rpc method=items.list code=mail_auth 用户@ 不是地址 a@b";
        assert_eq!(redact(line, None), line);
        assert_eq!(redact("short sk-abc stays", None), "short sk-abc stays");
    }

    #[test]
    fn rotates_when_full() {
        let dir = tempfile::tempdir().unwrap();
        let file = RotatingFile::new(dir.path().to_path_buf());
        let chunk = vec![b'x'; 1024 * 1024];
        for _ in 0..12 {
            file.append(&chunk);
        }
        let names: Vec<String> = {
            let mut v: Vec<String> = fs::read_dir(dir.path())
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            v.sort();
            v
        };
        assert_eq!(
            names,
            vec!["postflow.log", "postflow.log.1", "postflow.log.2"]
        );
        assert!(fs::metadata(dir.path().join(FILE)).unwrap().len() <= MAX_BYTES);
    }
}
