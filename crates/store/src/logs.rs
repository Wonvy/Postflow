//! 数据目录里的日志文件。
//!
//! ```text
//! <数据目录>/
//! ├─ meta.json
//! ├─ oplog/<设备ID>/2026-11.jsonl      状态变更
//! └─ articles/<设备ID>/2026-11.jsonl   文章存档
//! ```
//!
//! 每台设备只往自己的子目录里追加，从不修改别的设备的文件，
//! 所以文件同步工具不会遇到两边改同一个文件的情况。

use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::hlc::now_ms;

/// 数据目录的格式版本。读到更高版本时拒绝打开，避免旧版本写坏新格式。
pub const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogKind {
    Ops,
    Articles,
}

impl LogKind {
    pub fn dir(self) -> &'static str {
        match self {
            LogKind::Ops => "oplog",
            LogKind::Articles => "articles",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogFile {
    pub kind: LogKind,
    /// 相对数据目录的路径，统一用正斜杠，作为导入进度的键。
    pub rel: String,
    pub path: PathBuf,
    pub len: u64,
}

#[derive(Debug, Default)]
pub struct Chunk {
    pub lines: Vec<String>,
    pub new_offset: u64,
    /// 文件比上次记录的进度还短，说明被同步工具换成了别的版本，已从头重读。
    pub reset: bool,
}

#[derive(Serialize, Deserialize)]
struct Meta {
    format: u32,
}

#[derive(Debug, Clone)]
pub struct DataDir {
    root: PathBuf,
}

impl DataDir {
    pub fn open(root: &Path) -> Result<Self> {
        fs::create_dir_all(root.join(LogKind::Ops.dir()))?;
        fs::create_dir_all(root.join(LogKind::Articles.dir()))?;

        let meta_path = root.join("meta.json");
        match fs::read(&meta_path) {
            Ok(bytes) => {
                let meta: Meta = serde_json::from_slice(&bytes)?;
                if meta.format > FORMAT_VERSION {
                    return Err(Error::FormatTooNew {
                        found: meta.format,
                        supported: FORMAT_VERSION,
                    });
                }
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {
                let body = serde_json::to_vec_pretty(&Meta {
                    format: FORMAT_VERSION,
                })?;
                fs::write(&meta_path, body)?;
            }
            Err(e) => return Err(e.into()),
        }
        Ok(DataDir {
            root: root.to_path_buf(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 把若干行追加到本设备当月的日志文件，返回该文件。
    pub fn append(&self, kind: LogKind, device: &str, lines: &[String]) -> Result<LogFile> {
        let dir = self.root.join(kind.dir()).join(device);
        fs::create_dir_all(&dir)?;
        let (year, month) = year_month(now_ms());
        let name = format!("{year:04}-{month:02}.jsonl");
        let path = dir.join(&name);

        let mut buf = String::with_capacity(lines.iter().map(|l| l.len() + 1).sum());
        // 上次如果崩溃在写到一半，文件末尾会留下没有换行的半行。
        // 先补一个换行，新内容就不会被拼到那半行后面一起作废。
        if ends_with_partial_line(&path)? {
            buf.push('\n');
        }
        for line in lines {
            debug_assert!(!line.contains('\n'));
            buf.push_str(line);
            buf.push('\n');
        }

        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        file.write_all(buf.as_bytes())?;
        file.sync_data()?;
        let len = file.metadata()?.len();

        Ok(LogFile {
            kind,
            rel: format!("{}/{device}/{name}", kind.dir()),
            path,
            len,
        })
    }

    /// 列出某类日志下所有设备的文件，包括同步工具留下的冲突副本。
    pub fn list(&self, kind: LogKind) -> Result<Vec<LogFile>> {
        let base = self.root.join(kind.dir());
        let mut out = Vec::new();
        for device_dir in read_dir_sorted(&base)? {
            if !device_dir.is_dir() || is_hidden(&device_dir) {
                continue;
            }
            let device = file_name(&device_dir);
            for path in read_dir_sorted(&device_dir)? {
                // 隐藏文件是同步工具的临时文件或云盘占位符，内容不完整，跳过
                if is_hidden(&path) || path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                let Ok(meta) = fs::metadata(&path) else {
                    continue;
                };
                if !meta.is_file() {
                    continue;
                }
                out.push(LogFile {
                    kind,
                    rel: format!("{}/{device}/{}", kind.dir(), file_name(&path)),
                    path,
                    len: meta.len(),
                });
            }
        }
        Ok(out)
    }
}

/// 从 `offset` 起读出所有完整的行。末尾没有换行的半行留到下次。
pub fn read_new(path: &Path, offset: u64) -> Result<Chunk> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let (start, reset) = if offset > len {
        (0, true)
    } else {
        (offset, false)
    };
    if start == len {
        return Ok(Chunk {
            lines: Vec::new(),
            new_offset: start,
            reset,
        });
    }

    file.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::with_capacity((len - start) as usize);
    file.read_to_end(&mut buf)?;

    let Some(last_newline) = buf.iter().rposition(|b| *b == b'\n') else {
        return Ok(Chunk {
            lines: Vec::new(),
            new_offset: start,
            reset,
        });
    };
    let lines = buf[..last_newline]
        .split(|b| *b == b'\n')
        .filter(|l| !l.iter().all(u8::is_ascii_whitespace))
        .map(|l| String::from_utf8_lossy(l).into_owned())
        .collect();
    Ok(Chunk {
        lines,
        new_offset: start + last_newline as u64 + 1,
        reset,
    })
}

fn ends_with_partial_line(path: &Path) -> Result<bool> {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
    };
    if file.metadata()?.len() == 0 {
        return Ok(false);
    }
    file.seek(SeekFrom::End(-1))?;
    let mut last = [0u8; 1];
    file.read_exact(&mut last)?;
    Ok(last[0] != b'\n')
}

fn read_dir_sorted(dir: &Path) -> Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    Ok(paths)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn is_hidden(path: &Path) -> bool {
    file_name(path).starts_with('.')
}

/// 由 Unix 毫秒时间戳算出 UTC 的年和月。
pub fn year_month(ms: u64) -> (i64, u32) {
    let days = (ms / 86_400_000) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    (year, month as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn year_month_known_dates() {
        assert_eq!(year_month(0), (1970, 1));
        // 2026-11-16T00:00:00Z
        assert_eq!(year_month(1_794_787_200_000), (2026, 11));
        // 2024-02-29T23:59:59Z 与 2024-03-01T00:00:00Z
        assert_eq!(year_month(1_709_251_199_000), (2024, 2));
        assert_eq!(year_month(1_709_251_200_000), (2024, 3));
        // 2026-12-31T23:59:59Z 与 2027-01-01T00:00:00Z
        assert_eq!(year_month(1_798_761_599_000), (2026, 12));
        assert_eq!(year_month(1_798_761_600_000), (2027, 1));
    }

    #[test]
    fn append_then_read_incrementally() {
        let tmp = tempfile::tempdir().unwrap();
        let data = DataDir::open(tmp.path()).unwrap();

        let file = data
            .append(LogKind::Ops, "dev1", &["one".into(), "two".into()])
            .unwrap();
        let chunk = read_new(&file.path, 0).unwrap();
        assert_eq!(chunk.lines, vec!["one", "two"]);
        assert_eq!(chunk.new_offset, file.len);

        let file = data
            .append(LogKind::Ops, "dev1", &["three".into()])
            .unwrap();
        let chunk = read_new(&file.path, chunk.new_offset).unwrap();
        assert_eq!(chunk.lines, vec!["three"]);
        assert_eq!(chunk.new_offset, file.len);
    }

    #[test]
    fn partial_trailing_line_is_left_for_later() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("log.jsonl");
        fs::write(&path, "complete\nhal").unwrap();
        let chunk = read_new(&path, 0).unwrap();
        assert_eq!(chunk.lines, vec!["complete"]);
        assert_eq!(chunk.new_offset, 9);

        // 同步工具把剩下的内容补齐
        fs::write(&path, "complete\nhalf-done\n").unwrap();
        let chunk = read_new(&path, 9).unwrap();
        assert_eq!(chunk.lines, vec!["half-done"]);
    }

    #[test]
    fn append_after_crash_does_not_glue_onto_partial_line() {
        let tmp = tempfile::tempdir().unwrap();
        let data = DataDir::open(tmp.path()).unwrap();
        let file = data.append(LogKind::Ops, "dev1", &["good".into()]).unwrap();
        // 模拟写到一半崩溃
        OpenOptions::new()
            .append(true)
            .open(&file.path)
            .unwrap()
            .write_all(b"{\"brok")
            .unwrap();

        let file = data.append(LogKind::Ops, "dev1", &["next".into()]).unwrap();
        let chunk = read_new(&file.path, 0).unwrap();
        assert_eq!(chunk.lines, vec!["good", "{\"brok", "next"]);
    }

    #[test]
    fn shrunken_file_is_reread_from_start() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("log.jsonl");
        fs::write(&path, "a\n").unwrap();
        let chunk = read_new(&path, 100).unwrap();
        assert!(chunk.reset);
        assert_eq!(chunk.lines, vec!["a"]);
    }

    #[test]
    fn list_skips_hidden_and_includes_conflict_copies() {
        let tmp = tempfile::tempdir().unwrap();
        let data = DataDir::open(tmp.path()).unwrap();
        data.append(LogKind::Ops, "dev1", &["x".into()]).unwrap();
        let dir = tmp.path().join("oplog/dev1");
        fs::write(
            dir.join("2026-10.sync-conflict-20261101-101010-ABCDEFG.jsonl"),
            "y\n",
        )
        .unwrap();
        fs::write(dir.join(".syncthing.2026-11.jsonl.tmp"), "z").unwrap();
        fs::write(dir.join(".2026-09.jsonl.icloud"), "").unwrap();
        fs::write(dir.join("notes.txt"), "ignored").unwrap();

        let files = data.list(LogKind::Ops).unwrap();
        assert_eq!(files.len(), 2);
        assert!(
            files
                .iter()
                .all(|f| f.rel.starts_with("oplog/dev1/") && f.rel.ends_with(".jsonl"))
        );
        assert!(files.iter().any(|f| f.rel.contains("sync-conflict")));
    }

    #[test]
    fn newer_format_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("meta.json"), r#"{"format": 99}"#).unwrap();
        assert!(matches!(
            DataDir::open(tmp.path()),
            Err(Error::FormatTooNew { found: 99, .. })
        ));
    }
}
