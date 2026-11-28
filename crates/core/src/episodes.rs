//! 下载播客单集离线听。
//!
//! 文件放在本机目录的 `episodes/` 下，文件名是条目 ID 加扩展名，不参与同步（每台设备自己下）。
//! 下载在后台进行，界面轮询 `episodes.list` 看进度。下完以后只保留最近下载的几期（数量由界面给），
//! 早下的先删。网页端从 `/api/episodes/<ID>` 取文件（见 shibapost-web），桌面应用直接读这个目录。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use serde::Serialize;

use crate::rpc::RpcError;
use crate::{Core, Session};

/// 一期最大多少（视频播客也够用）。
const LIMIT: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Downloaded {
    pub id: String,
    /// 文件的完整路径，桌面应用拿它直接播放。
    pub file: String,
    pub size: u64,
    /// 下载完成的时间，毫秒。
    pub at: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct Progress {
    pub received: u64,
    pub total: Option<u64>,
    pub error: Option<String>,
    /// 下完以后的文件名（下载中的内容在旁边的 `.part` 里）。字幕边下边转时读它。
    #[serde(skip)]
    pub file: Option<PathBuf>,
}

impl Progress {
    /// 正在写的那个文件。
    pub(crate) fn partial(&self) -> Option<PathBuf> {
        self.file.as_ref().map(|f| f.with_extension("part"))
    }
}

pub(crate) struct Episodes {
    dir: PathBuf,
    active: Mutex<HashMap<String, Progress>>,
}

/// 条目 ID 只能是字母数字，免得拿它拼出别的路径。
fn safe_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn extension(mime: &str, url: &str) -> &'static str {
    match mime {
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/mp4" | "audio/x-m4a" | "audio/m4a" => "m4a",
        "audio/aac" => "aac",
        "audio/ogg" | "audio/opus" => "ogg",
        "audio/wav" | "audio/x-wav" => "wav",
        "audio/flac" => "flac",
        "video/mp4" => "mp4",
        "video/quicktime" => "mov",
        "video/webm" => "webm",
        _ => {
            let path = url
                .split(['?', '#'])
                .next()
                .unwrap_or(url)
                .to_ascii_lowercase();
            match path.rsplit('.').next() {
                Some("m4a") => "m4a",
                Some("mp4") => "mp4",
                Some("ogg" | "opus") => "ogg",
                _ => "mp3",
            }
        }
    }
}

impl Episodes {
    pub(crate) fn new(local_dir: &Path) -> Self {
        Episodes {
            dir: local_dir.join("episodes"),
            active: Mutex::new(HashMap::new()),
        }
    }

    /// 下好的一期在哪里。
    pub(crate) fn file(&self, id: &str) -> Option<PathBuf> {
        if !safe_id(id) {
            return None;
        }
        std::fs::read_dir(&self.dir)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.file_stem().and_then(|s| s.to_str()) == Some(id)
                    && p.extension().and_then(|e| e.to_str()) != Some("part")
            })
    }

    pub(crate) fn list(&self) -> Vec<Downloaded> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut out: Vec<Downloaded> = entries
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                if path.extension().and_then(|x| x.to_str()) == Some("part") {
                    return None;
                }
                let id = path.file_stem()?.to_str()?.to_string();
                if !safe_id(&id) {
                    return None;
                }
                let meta = e.metadata().ok()?;
                let at = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_millis() as i64);
                Some(Downloaded {
                    id,
                    file: path.to_string_lossy().into_owned(),
                    size: meta.len(),
                    at,
                })
            })
            .collect();
        out.sort_by_key(|d| std::cmp::Reverse(d.at));
        out
    }

    pub(crate) fn progress(&self) -> HashMap<String, Progress> {
        self.active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub(crate) fn remove(&self, id: &str) {
        if let Some(path) = self.file(id) {
            let _ = std::fs::remove_file(path);
        }
        self.active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
    }

    /// 只留最近下载的 `keep` 期，`except` 这一期不删。
    fn prune(&self, keep: usize, except: &str) {
        for old in self
            .list()
            .into_iter()
            .filter(|d| d.id != except)
            .skip(keep.saturating_sub(1))
        {
            let _ = std::fs::remove_file(old.file);
        }
    }

    fn set(&self, id: &str, progress: Progress) {
        self.active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.to_string(), progress);
    }
}

impl Core {
    /// 开始在后台下载一期。已经下好、或者正在下的不重复。
    pub(crate) async fn download_episode(
        self: &Arc<Self>,
        session: &Arc<Session>,
        id: String,
        keep: usize,
    ) -> Result<(), RpcError> {
        if !safe_id(&id) {
            return Err(RpcError::new("bad_params", "invalid id"));
        }
        if self.episodes.file(&id).is_some() {
            return Ok(());
        }
        {
            let active = self
                .episodes
                .active
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if active.get(&id).is_some_and(|p| p.error.is_none()) {
                return Ok(());
            }
        }
        let lookup = id.clone();
        let media = session
            .db(move |s| s.item(&lookup))
            .await?
            .and_then(|d| d.item.media)
            .ok_or_else(|| RpcError::new("not_found", "this item has no audio or video"))?;
        if !(media.url.starts_with("https://") || media.url.starts_with("http://")) {
            return Err(RpcError::new("bad_params", "only http and https"));
        }
        std::fs::create_dir_all(&self.episodes.dir)
            .map_err(|e| RpcError::new("io", e.to_string()))?;
        let to = self
            .episodes
            .dir
            .join(format!("{id}.{}", extension(&media.mime, &media.url)));
        self.episodes.set(
            &id,
            Progress {
                file: Some(to.clone()),
                ..Progress::default()
            },
        );
        let target = to.clone();
        let core = self.clone();
        tokio::spawn(async move {
            let result = core
                .client
                .download(&media.url, &to, LIMIT, |received, total| {
                    core.episodes.set(
                        &id,
                        Progress {
                            received,
                            total,
                            error: None,
                            file: Some(target.clone()),
                        },
                    );
                })
                .await;
            match result {
                Ok(size) => {
                    tracing::info!(item = %id, size, "episode downloaded");
                    core.episodes
                        .active
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .remove(&id);
                    core.episodes.prune(keep.max(1), &id);
                }
                Err(err) => {
                    tracing::warn!(item = %id, "episode download failed: {err}");
                    core.episodes.set(
                        &id,
                        Progress {
                            error: Some(err.to_string()),
                            ..Default::default()
                        },
                    );
                }
            }
        });
        Ok(())
    }

    /// 下好的一期在本机的位置（网页端从 `/api/episodes/<ID>` 取）。
    pub fn episode_file(&self, id: &str) -> Option<PathBuf> {
        self.episodes.file(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_extensions() {
        assert!(safe_id("3e1c00d32e77132c0a08f38a"));
        assert!(!safe_id("../etc/passwd"));
        assert!(!safe_id(""));
        assert_eq!(extension("audio/mpeg", "https://x/a"), "mp3");
        assert_eq!(
            extension("application/octet-stream", "https://x/a.m4a?x=1"),
            "m4a"
        );
        assert_eq!(extension("video/mp4", "https://x/a"), "mp4");
    }

    #[test]
    fn keeps_the_latest_downloads() {
        let tmp = tempfile::tempdir().unwrap();
        let eps = Episodes::new(tmp.path());
        std::fs::create_dir_all(&eps.dir).unwrap();
        for (i, id) in ["aaa", "bbb", "ccc", "ddd"].iter().enumerate() {
            let path = eps.dir.join(format!("{id}.mp3"));
            std::fs::write(&path, b"x").unwrap();
            let t = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000 + i as u64);
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(t)
                .unwrap();
        }
        std::fs::write(eps.dir.join("eee.part"), b"half").unwrap();
        assert_eq!(
            eps.list().iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
            ["ddd", "ccc", "bbb", "aaa"]
        );
        // 刚下好的是 aaa（最早的时间也不删），另外留最新的一期
        eps.prune(2, "aaa");
        let mut left: Vec<String> = eps.list().into_iter().map(|d| d.id).collect();
        left.sort();
        assert_eq!(left, ["aaa", "ddd"]);
        assert!(eps.file("ddd").is_some());
        assert!(eps.file("eee").is_none());
        eps.remove("ddd");
        assert!(eps.file("ddd").is_none());
    }
}
