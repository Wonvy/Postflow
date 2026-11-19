//! 设置里的「存储空间」：数据目录和本机目录各占多少、哪些订阅源和邮箱占得最多，以及清理本机缓存。
//!
//! 数据目录是同步的事实来源，只显示不清理；能清的都是本机缓存，清掉后用到时会重新下载。

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::refresh::now;
use crate::rpc::{RpcError, parse, reply};
use crate::{Core, Session};

/// 清邮件原文缓存时保留最近这么多天的。
const KEEP_MAIL_DAYS: i64 = 30;
const DAY_MS: i64 = 86_400_000;

#[derive(Serialize, Default)]
struct Part {
    key: &'static str,
    bytes: u64,
}

#[derive(Deserialize)]
struct ClearParams {
    what: String,
}

/// 一个目录（含子目录）里文件的总大小。读不到的文件跳过。
fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    };
    entries
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

/// 把目录按指定的几个子项分开算，剩下的归到 `other`。
fn split(root: &Path, named: &[(&'static str, &[&str])]) -> (u64, Vec<Part>) {
    let total = dir_size(root);
    let mut parts: Vec<Part> = named
        .iter()
        .map(|(key, names)| Part {
            key,
            bytes: names.iter().map(|n| dir_size(&root.join(n))).sum(),
        })
        .collect();
    let named_sum: u64 = parts.iter().map(|p| p.bytes).sum();
    parts.push(Part {
        key: "other",
        bytes: total.saturating_sub(named_sum),
    });
    (total, parts)
}

impl Core {
    pub(crate) async fn storage_rpc(
        self: &Arc<Self>,
        method: &str,
        params: Value,
        session: &Arc<Session>,
    ) -> Result<Value, RpcError> {
        match method {
            "storage.usage" => {
                let data_dir = session.data_dir.clone();
                let local_dir = self.local_dir.clone();
                let (data, local) = tokio::task::spawn_blocking(move || {
                    let data = split(
                        &data_dir,
                        &[("articles", &["articles"]), ("oplog", &["oplog"])],
                    );
                    let local = split(
                        &local_dir,
                        &[
                            ("index", &["index.db", "index.db-wal", "index.db-shm"]),
                            ("translation", &["translation"]),
                            ("speech", &["speech"]),
                            ("voices", &["voices"]),
                            ("episodes", &["episodes"]),
                            ("logs", &["logs"]),
                            ("icons", &["sender-icons.json"]),
                            ("rsshub", &["rsshub-routes.json"]),
                        ],
                    );
                    (data, local)
                })
                .await
                .map_err(|e| RpcError::new("internal", e.to_string()))?;
                let keep_since = now() - KEEP_MAIL_DAYS * DAY_MS;
                let index = session.db(move |s| s.index_usage(keep_since)).await?;
                let absolute =
                    |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
                reply(json!({
                    "data_dir": absolute(&session.data_dir),
                    "data": { "total": data.0, "parts": data.1 },
                    "local_dir": absolute(&self.local_dir),
                    "local": { "total": local.0, "parts": local.1 },
                    "index": index,
                    "keep_mail_days": KEEP_MAIL_DAYS,
                }))
            }
            "storage.clear" => {
                let p: ClearParams = parse(params)?;
                let count = match p.what.as_str() {
                    "fulltext" => session.db(|s| s.clear_full_text()).await?,
                    "mail" => {
                        let before = now() - KEEP_MAIL_DAYS * DAY_MS;
                        session.db(move |s| s.clear_mail_bodies(before)).await?
                    }
                    "icons" => {
                        self.sender_icons.clear();
                        0
                    }
                    other => {
                        return Err(RpcError::new("invalid", format!("unknown cache: {other}")));
                    }
                };
                // 清完顺手压缩，空出来的地方才会真正还给磁盘
                session.db(|s| s.compact()).await?;
                reply(json!({ "count": count }))
            }
            "storage.compact" => {
                session.db(|s| s.compact()).await?;
                reply(json!({}))
            }
            _ => Err(RpcError::new("unknown_method", method.to_string())),
        }
    }
}
