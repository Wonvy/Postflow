//! 转写出来的字幕存在本机，一期一个 JSON 文件，不参与同步。
//!
//! 边播边转写时一期可能只转了几段（从听到的地方开始），所以除了字幕本身，还记着哪些时间段已经转过。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::cues::{Covered, Cue};
use crate::write_atomic;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Transcript {
    /// 用哪个模型转的。
    pub model: String,
    /// 整期多长（秒）。读到过文件结尾才知道得准。
    #[serde(default)]
    pub duration: Option<f64>,
    /// 已经转过的时间段。
    #[serde(default)]
    pub covered: Covered,
    #[serde(default)]
    pub cues: Vec<Cue>,
}

impl Transcript {
    /// 整期都转完了。
    pub fn complete(&self) -> bool {
        self.duration.is_some_and(|d| self.covered.complete(d))
    }
}

pub struct Transcripts {
    dir: PathBuf,
}

/// 条目 ID 只能是字母数字，免得拿它拼出别的路径。
fn safe_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

impl Transcripts {
    pub fn new(dir: PathBuf) -> Self {
        Transcripts { dir }
    }

    fn path(&self, id: &str) -> Option<PathBuf> {
        safe_id(id).then(|| self.dir.join(format!("{id}.json")))
    }

    pub fn load(&self, id: &str) -> Option<Transcript> {
        let bytes = std::fs::read(self.path(id)?).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub fn save(&self, id: &str, transcript: &Transcript) -> std::io::Result<()> {
        let path = self
            .path(id)
            .ok_or_else(|| std::io::Error::other("invalid id"))?;
        let json = serde_json::to_vec(transcript).map_err(std::io::Error::other)?;
        write_atomic(&path, &json)
    }

    pub fn remove(&self, id: &str) {
        if let Some(path) = self.path(id) {
            let _ = std::fs::remove_file(path);
        }
    }

    /// 有字幕的条目 ID。
    pub fn list(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                if path.extension().and_then(|x| x.to_str()) != Some("json") {
                    return None;
                }
                let id = path.file_stem()?.to_str()?.to_string();
                safe_id(&id).then_some(id)
            })
            .collect()
    }

    pub fn clear(&self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_and_loads() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Transcripts::new(tmp.path().join("transcripts"));
        assert!(store.load("abc123").is_none());
        let mut t = Transcript {
            model: "m".into(),
            duration: Some(60.0),
            ..Default::default()
        };
        t.covered.add(0.0, 30.0);
        t.cues.push(Cue {
            start: 1.0,
            end: 3.0,
            text: "你好".into(),
        });
        assert!(!t.complete());
        store.save("abc123", &t).unwrap();
        assert_eq!(store.load("abc123"), Some(t.clone()));
        assert_eq!(store.list(), ["abc123"]);
        t.covered.add(30.0, 60.0);
        assert!(t.complete());
        // 不是字母数字的 ID 不碰文件系统
        assert!(store.save("../x", &t).is_err());
        assert!(store.load("../x").is_none());
        store.remove("abc123");
        assert!(store.list().is_empty());
    }
}
