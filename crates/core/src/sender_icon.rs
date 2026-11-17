//! 发件人头像：发件人所在网站的图标。
//!
//! 找到的地址（找不到也记下）存在本机目录的 `sender-icons.json` 里，不参与同步。
//! 找到的一个月后重新找，找不到的一周后再试。同时最多找 4 个网站。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;

use crate::Core;
use crate::refresh::now;

const FILE: &str = "sender-icons.json";
const FOUND_TTL_MS: i64 = 30 * 24 * 3600 * 1000;
const MISSING_TTL_MS: i64 = 7 * 24 * 3600 * 1000;
const PARALLEL: usize = 4;

#[derive(Clone, Serialize, Deserialize)]
struct Entry {
    url: Option<String>,
    at: i64,
}

impl Entry {
    fn fresh(&self, now: i64) -> bool {
        let ttl = if self.url.is_some() {
            FOUND_TTL_MS
        } else {
            MISSING_TTL_MS
        };
        now - self.at < ttl
    }
}

pub(crate) struct SenderIcons {
    path: PathBuf,
    cache: Mutex<Option<HashMap<String, Entry>>>,
    limit: Arc<Semaphore>,
}

impl SenderIcons {
    pub(crate) fn new(local_dir: &std::path::Path) -> Self {
        SenderIcons {
            path: local_dir.join(FILE),
            cache: Mutex::new(None),
            limit: Arc::new(Semaphore::new(PARALLEL)),
        }
    }

    fn with_cache<T>(&self, f: impl FnOnce(&mut HashMap<String, Entry>) -> T) -> T {
        let mut guard = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let cache = guard.get_or_insert_with(|| {
            std::fs::read(&self.path)
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default()
        });
        f(cache)
    }

    fn cached(&self, domain: &str) -> Option<Entry> {
        let now = now();
        self.with_cache(|c| c.get(domain).filter(|e| e.fresh(now)).cloned())
    }

    fn store(&self, domain: &str, url: Option<String>) {
        let snapshot = self.with_cache(|c| {
            c.insert(domain.to_string(), Entry { url, at: now() });
            serde_json::to_vec(c).ok()
        });
        if let Some(bytes) = snapshot {
            let tmp = self.path.with_extension("json.tmp");
            if std::fs::write(&tmp, bytes).is_ok() {
                let _ = std::fs::rename(&tmp, &self.path);
            }
        }
    }
}

impl Core {
    /// 发件人所在网站的图标地址，没有返回 `None`。
    pub(crate) async fn sender_icon(&self, domain: &str) -> Option<String> {
        let domain = domain.trim().to_ascii_lowercase();
        if !postflow_feed::valid_domain(&domain) {
            return None;
        }
        if let Some(entry) = self.sender_icons.cached(&domain) {
            return entry.url;
        }
        let _permit = self.sender_icons.limit.acquire().await.ok()?;
        // 排队的时候别人可能已经找过了
        if let Some(entry) = self.sender_icons.cached(&domain) {
            return entry.url;
        }
        let url = self.client.site_icon(&domain).await;
        self.sender_icons.store(&domain, url.clone());
        url
    }
}
