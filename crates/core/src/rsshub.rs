//! RSSHub：添加订阅时按网站搜索路由、按网址猜路由。
//!
//! 路由目录从 RSSHub 官方下载，存在本机目录的 `rsshub-routes.json`，一周更新一次；
//! 下载失败时继续用旧的。订阅地址用哪个 RSSHub 实例是界面的设置（`rsshub_instance`，会同步）。

use std::path::PathBuf;
use std::sync::Arc;

use postflow_feed::rsshub::Catalog;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::Core;
use crate::refresh::now;
use crate::rpc::{RpcError, parse, reply};

const FILE: &str = "rsshub-routes.json";
const MAX_AGE_MS: i64 = 7 * 24 * 3600 * 1000;
const LIMIT: usize = 40;

pub(crate) struct Rsshub {
    path: PathBuf,
    /// 读好的目录，以及它是按哪种语言读的。
    loaded: Mutex<Option<(bool, Arc<Catalog>)>>,
}

#[derive(Deserialize)]
struct SearchParams {
    #[serde(default)]
    query: String,
    #[serde(default)]
    zh: bool,
}

#[derive(Deserialize)]
struct RadarParams {
    url: String,
    #[serde(default)]
    zh: bool,
}

#[derive(Deserialize)]
struct CheckParams {
    instance: String,
    #[serde(default)]
    key: Option<String>,
}

#[derive(Deserialize)]
struct RefreshParams {
    #[serde(default)]
    zh: bool,
}

impl Rsshub {
    pub(crate) fn new(local_dir: &std::path::Path) -> Self {
        Rsshub {
            path: local_dir.join(FILE),
            loaded: Mutex::new(None),
        }
    }

    fn age_ms(&self) -> Option<i64> {
        let modified = std::fs::metadata(&self.path).ok()?.modified().ok()?;
        let ms = modified
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_millis() as i64;
        Some(now() - ms)
    }
}

impl Core {
    /// 路由目录。本机的太旧或者没有就下载；`force` 时一定重新下载。
    async fn rsshub_catalog(&self, zh: bool, force: bool) -> Result<Arc<Catalog>, RpcError> {
        let state = &self.rsshub;
        let mut loaded = state.loaded.lock().await;
        let stale = state.age_ms().is_none_or(|age| age > MAX_AGE_MS);
        if !force
            && !stale
            && let Some((lang, catalog)) = loaded.as_ref()
            && *lang == zh
        {
            return Ok(catalog.clone());
        }

        if force || stale {
            match self.client.rsshub_catalog().await {
                Ok(bytes) => {
                    // 先确认能读，再替换本机的
                    if Catalog::parse(&bytes, false).is_ok_and(|c| !c.is_empty()) {
                        let tmp = state.path.with_extension("json.tmp");
                        if std::fs::write(&tmp, &bytes).is_ok() {
                            let _ = std::fs::rename(&tmp, &state.path);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("could not download the RSSHub catalog: {e}");
                    if !state.path.exists() {
                        return Err(RpcError::new("rsshub_unavailable", e.to_string()));
                    }
                }
            }
        }

        let path = state.path.clone();
        let catalog = tokio::task::spawn_blocking(move || {
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            Catalog::parse(&bytes, zh).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| RpcError::new("internal", e.to_string()))?
        .map_err(|e| RpcError::new("rsshub_unavailable", e))?;
        let catalog = Arc::new(catalog);
        *loaded = Some((zh, catalog.clone()));
        Ok(catalog)
    }

    pub(crate) async fn rsshub_rpc(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        match method {
            "rsshub.search" => {
                let p: SearchParams = parse(params)?;
                let catalog = self.rsshub_catalog(p.zh, false).await?;
                reply(json!({ "routes": catalog.search(&p.query, LIMIT), "total": catalog.len() }))
            }
            "rsshub.radar" => {
                let p: RadarParams = parse(params)?;
                let catalog = self.rsshub_catalog(p.zh, false).await?;
                reply(json!({ "matches": catalog.radar(&p.url, 8) }))
            }
            // 测试一个实例能不能用（设置里的「测试连接」）
            "rsshub.check" => {
                let p: CheckParams = parse(params)?;
                reply(
                    self.client
                        .check_rsshub(&p.instance, p.key.as_deref())
                        .await,
                )
            }
            "rsshub.refresh" => {
                let p: RefreshParams = parse(params)?;
                let catalog = self.rsshub_catalog(p.zh, true).await?;
                reply(json!({ "total": catalog.len() }))
            }
            _ => Err(RpcError::new("unknown_method", method.to_string())),
        }
    }
}
