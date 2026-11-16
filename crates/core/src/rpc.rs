//! 界面能调用的全部方法。桌面外壳和网页访问都把请求原样转到这里。

use std::fmt;
use std::sync::Arc;

use postflow_feed::{Conditional, FetchResult, Fetched, favicon_url, host_of, normalize_url, opml};
use postflow_store::{Cursor, Feed, FeedPatch, ItemFilter, ItemPatch, NewFeed};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::refresh::{ingest, now};
use crate::{Core, Scope, Session, home_dir};

#[derive(Debug, Clone, Serialize)]
pub struct RpcError {
    /// 稳定的错误码，界面据此显示本地化的提示。
    pub code: String,
    /// 给开发者看的英文描述。
    pub message: String,
}

impl RpcError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        RpcError {
            code: code.to_string(),
            message: message.into(),
        }
    }
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for RpcError {}

impl From<postflow_store::Error> for RpcError {
    fn from(err: postflow_store::Error) -> Self {
        let code = match &err {
            postflow_store::Error::FormatTooNew { .. } => "format_too_new",
            postflow_store::Error::Io(_) => "io",
            postflow_store::Error::Invalid(_) => "bad_params",
            _ => "store",
        };
        RpcError::new(code, err.to_string())
    }
}

impl From<postflow_feed::Error> for RpcError {
    fn from(err: postflow_feed::Error) -> Self {
        RpcError::new(err.code(), err.to_string())
    }
}

pub(crate) fn parse<T: DeserializeOwned>(params: Value) -> Result<T, RpcError> {
    let params = if params.is_null() { json!({}) } else { params };
    serde_json::from_value(params).map_err(|e| RpcError::new("bad_params", e.to_string()))
}

pub(crate) fn reply<T: Serialize>(value: T) -> Result<Value, RpcError> {
    serde_json::to_value(value).map_err(|e| RpcError::new("internal", e.to_string()))
}

#[derive(Deserialize)]
struct Setup {
    data_dir: String,
    #[serde(default)]
    device_name: String,
}

#[derive(Deserialize)]
struct NavGet {
    #[serde(default)]
    today_since: i64,
}

#[derive(Deserialize)]
struct UrlParam {
    url: String,
}

#[derive(Deserialize)]
struct AddFeed {
    url: String,
    #[serde(default)]
    folder: String,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Deserialize)]
struct UpdateFeed {
    id: String,
    #[serde(flatten)]
    patch: FeedPatch,
}

#[derive(Deserialize)]
struct GetItem {
    id: String,
    /// 这次显示邮件里的远程图片。
    #[serde(default)]
    allow_remote: bool,
}

#[derive(Deserialize)]
struct IdParam {
    id: String,
}

#[derive(Deserialize)]
struct OptionalId {
    #[serde(default)]
    id: Option<String>,
}

#[derive(Deserialize)]
struct ImportOpml {
    content: String,
}

#[derive(Deserialize)]
struct ExportOpml {
    /// 给了路径就直接写文件，桌面端用；不给就把内容返回，网页端自己触发下载。
    #[serde(default)]
    path: Option<String>,
}

#[derive(Deserialize)]
struct ListItems {
    #[serde(default)]
    filter: ItemFilter,
    #[serde(default)]
    before: Option<Cursor>,
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Deserialize)]
struct SetItems {
    ids: Vec<String>,
    #[serde(flatten)]
    patch: ItemPatch,
}

#[derive(Deserialize)]
struct MarkAll {
    #[serde(default)]
    filter: ItemFilter,
}

#[derive(Deserialize)]
struct SetSetting {
    key: String,
    value: Value,
}

#[derive(Deserialize)]
struct Rename {
    name: String,
}

#[derive(Serialize)]
struct Status {
    configured: bool,
    version: &'static str,
    data_dir: Option<String>,
    device_id: Option<String>,
    device_name: Option<String>,
    suggested_data_dir: String,
    suggested_device_name: String,
    error: Option<String>,
}

impl Core {
    /// 调用一个方法。`client` 标识发起调用的界面实例，会原样带在它引发的变更事件里。
    pub async fn rpc(
        self: &Arc<Self>,
        method: &str,
        params: Value,
        client: Option<String>,
    ) -> Result<Value, RpcError> {
        match method {
            "app.status" => return reply(self.status().await),
            "app.setup" => {
                let p: Setup = parse(params)?;
                self.setup(&p.data_dir, &p.device_name).await?;
                return reply(self.status().await);
            }
            _ => {}
        }

        let session = self
            .session()
            .ok_or_else(|| RpcError::new("not_configured", "no data directory has been set up"))?;

        match method {
            "nav.get" => {
                let p: NavGet = parse(params)?;
                let (counts, feeds, accounts) = session
                    .db(move |s| Ok((s.counts(p.today_since)?, s.feeds()?, s.accounts()?)))
                    .await?;
                reply(json!({ "counts": counts, "feeds": feeds, "accounts": accounts }))
            }

            "feeds.discover" => {
                let p: UrlParam = parse(params)?;
                reply(self.client.discover(&p.url).await?)
            }
            "feeds.add" => {
                let feed = self.add_feed(&session, parse(params)?).await?;
                self.emit_changed(&[Scope::Feeds, Scope::Items], None);
                reply(feed)
            }
            "feeds.update" => {
                let p: UpdateFeed = parse(params)?;
                session.db(move |s| s.update_feed(&p.id, p.patch)).await?;
                self.emit_changed(&[Scope::Feeds, Scope::Items], client);
                reply(json!({}))
            }
            "feeds.remove" => {
                let p: IdParam = parse(params)?;
                session.db(move |s| s.remove_feed(&p.id)).await?;
                self.emit_changed(&[Scope::Feeds, Scope::Items], None);
                reply(json!({}))
            }
            "feeds.refresh" => {
                let p: OptionalId = parse(params)?;
                // 刷新全部时邮箱也一起收；指定的是邮箱账户时只收那一个
                match &p.id {
                    None => self.sync_all_mail(&session),
                    Some(id) => {
                        if self.send_mail(&session, id, crate::mail::Command::Sync) {
                            return reply(json!({ "count": 1 }));
                        }
                    }
                }
                let targets = session
                    .db(move |s| match p.id {
                        Some(id) => Ok(s.fetch_target(&id)?.into_iter().collect()),
                        None => s.all_fetch_targets(),
                    })
                    .await?;
                let count = targets.len();
                // 抓取可能要很久，放到后台，进度通过事件告诉界面
                let (core, session) = (self.clone(), session.clone());
                tokio::spawn(async move { core.refresh(&session, targets).await });
                reply(json!({ "count": count }))
            }
            "feeds.import_opml" => {
                let p: ImportOpml = parse(params)?;
                let parsed = opml::parse(&p.content)?;
                let total = parsed.len();
                let added = session
                    .db(move |s| {
                        let before = s.feeds()?.len();
                        for f in parsed {
                            let Ok(url) = normalize_url(&f.url) else {
                                continue;
                            };
                            let url = url.to_string();
                            let title = if f.title.is_empty() {
                                host_of(&url).unwrap_or_default()
                            } else {
                                f.title
                            };
                            s.put_feed(NewFeed {
                                icon_url: favicon_url(f.site_url.as_deref(), &url),
                                url,
                                title,
                                site_url: f.site_url,
                                folder: f.folder,
                            })?;
                        }
                        Ok(s.feeds()?.len() - before)
                    })
                    .await?;
                self.emit_changed(&[Scope::Feeds], None);
                // 新导入的源还没抓过，立刻开始
                let targets = session.db(|s| s.due_feeds(now())).await?;
                let (core, session) = (self.clone(), session.clone());
                tokio::spawn(async move { core.refresh(&session, targets).await });
                reply(json!({ "added": added, "total": total }))
            }
            "feeds.export_opml" => {
                let p: ExportOpml = parse(params)?;
                let feeds = session.db(|s| s.feeds()).await?;
                let feeds: Vec<opml::OpmlFeed> = feeds
                    .into_iter()
                    .map(|f| opml::OpmlFeed {
                        url: f.url,
                        title: f.title,
                        site_url: f.site_url,
                        folder: f.folder,
                    })
                    .collect();
                let content = opml::write("Postflow", &feeds)?;
                match p.path {
                    Some(path) => {
                        tokio::fs::write(&path, content)
                            .await
                            .map_err(|e| RpcError::new("io", e.to_string()))?;
                        reply(json!({ "path": path, "count": feeds.len() }))
                    }
                    None => reply(json!({ "content": content, "count": feeds.len() })),
                }
            }

            "items.list" => {
                let p: ListItems = parse(params)?;
                let limit = p.limit.unwrap_or(100).clamp(1, 500);
                let items = session
                    .db(move |s| s.items(&p.filter, p.before.as_ref(), limit))
                    .await?;
                let next = (items.len() as u32 == limit)
                    .then(|| items.last())
                    .flatten()
                    .map(|i| Cursor {
                        published_at: i.published_at,
                        id: i.id.clone(),
                    });
                reply(json!({ "items": items, "next": next }))
            }
            "items.get" => {
                let p: GetItem = parse(params)?;
                let detail = session
                    .db(move |s| s.item(&p.id))
                    .await?
                    .ok_or_else(|| RpcError::new("not_found", "no such item"))?;
                if detail.item.kind == "mail" {
                    return self
                        .mail_detail(&session, detail.item, p.allow_remote)
                        .await;
                }
                let base = detail.item.url.as_deref();
                let content = postflow_feed::sanitize_html(&detail.content, base);
                let full_text = detail
                    .full_text
                    .as_deref()
                    .map(|h| postflow_feed::sanitize_html(h, base));
                let mut value = reply(&detail.item)?;
                value["content"] = json!(content);
                value["full_text"] = json!(full_text);
                Ok(value)
            }
            "items.set" => {
                let p: SetItems = parse(params)?;
                let patch = p.patch.clone();
                let refs = session
                    .db(move |s| s.set_item_state(&p.ids, &p.patch))
                    .await?;
                self.dispatch(&session, refs, &patch);
                self.emit_changed(&[Scope::Feeds, Scope::Items], client);
                reply(json!({}))
            }
            "items.mark_all_read" => {
                let p: MarkAll = parse(params)?;
                let (count, refs) = session.db(move |s| s.mark_all_read(&p.filter)).await?;
                self.dispatch(
                    &session,
                    refs,
                    &ItemPatch {
                        read: Some(true),
                        ..Default::default()
                    },
                );
                self.emit_changed(&[Scope::Feeds, Scope::Items], client);
                reply(json!({ "count": count }))
            }
            "items.full_text" => {
                let p: IdParam = parse(params)?;
                let id = p.id.clone();
                let detail = session
                    .db(move |s| s.item(&id))
                    .await?
                    .ok_or_else(|| RpcError::new("not_found", "no such item"))?;
                let url = detail
                    .item
                    .url
                    .ok_or_else(|| RpcError::new("extract_failed", "item has no link"))?;
                let raw = self.client.full_text(&url).await?;
                let clean = postflow_feed::sanitize_html(&raw, Some(&url));
                session.db(move |s| s.set_full_text(&p.id, &raw)).await?;
                reply(json!({ "full_text": clean }))
            }

            "settings.get" => reply(session.db(|s| s.settings()).await?),
            "settings.set" => {
                let p: SetSetting = parse(params)?;
                session.db(move |s| s.set_setting(&p.key, p.value)).await?;
                self.emit_changed(&[Scope::Settings], client);
                reply(json!({}))
            }

            "devices.list" => reply(session.db(|s| s.devices()).await?),
            "devices.rename" => {
                let p: Rename = parse(params)?;
                let name = p.name.trim().to_string();
                if name.is_empty() {
                    return Err(RpcError::new("bad_params", "device name is empty"));
                }
                session.db(move |s| s.set_device_name(&name)).await?;
                self.emit_changed(&[Scope::Devices], None);
                reply(json!({}))
            }

            "index.rebuild" => {
                let stats = session.db(|s| s.rebuild_index()).await?;
                self.emit_changed(&crate::ALL_SCOPES, None);
                reply(stats)
            }

            _ => match self.mail_rpc(method, params, &session).await {
                Some(result) => result,
                None => Err(RpcError::new(
                    "unknown_method",
                    format!("unknown method: {method}"),
                )),
            },
        }
    }

    async fn status(&self) -> Status {
        let session = self.session();
        let device_name = match &session {
            Some(session) => session
                .db(|s| s.devices())
                .await
                .ok()
                .and_then(|d| d.into_iter().find(|d| d.current))
                .map(|d| d.name),
            None => None,
        };
        let suggested_data_dir = home_dir()
            .map(|h| h.join("Postflow").to_string_lossy().into_owned())
            .unwrap_or_default();
        Status {
            configured: session.is_some(),
            version: env!("CARGO_PKG_VERSION"),
            data_dir: session
                .as_ref()
                .map(|s| s.data_dir.to_string_lossy().into_owned()),
            device_id: session.as_ref().map(|s| s.device_id.clone()),
            device_name,
            suggested_data_dir,
            suggested_device_name: gethostname::gethostname()
                .to_string_lossy()
                .trim_end_matches(".local")
                .to_string(),
            error: if session.is_some() {
                None
            } else {
                self.last_error()
            },
        }
    }

    async fn add_feed(&self, session: &Session, p: AddFeed) -> Result<Feed, RpcError> {
        let requested = normalize_url(&p.url)?.to_string();
        let (url, fetched) = match self.fetch_new(&requested).await {
            Ok(fetched) => (requested, fetched),
            // 给的是网页而不是源：只有一个候选时直接采用，免得用户多点一步
            Err(postflow_feed::Error::NotAFeed(_)) => {
                let mut candidates = self.client.discover(&requested).await?;
                if candidates.len() != 1 {
                    return Err(postflow_feed::Error::NoFeedFound.into());
                }
                let url = candidates.remove(0).url;
                let fetched = self.fetch_new(&url).await?;
                (url, fetched)
            }
            Err(e) => return Err(e.into()),
        };

        let title = p
            .title
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .or_else(|| Some(fetched.feed.title.clone()).filter(|t| !t.is_empty()))
            .or_else(|| host_of(&url))
            .unwrap_or_default();
        let new = NewFeed {
            icon_url: fetched
                .feed
                .icon_url
                .clone()
                .or_else(|| favicon_url(fetched.feed.site_url.as_deref(), &url)),
            site_url: fetched.feed.site_url.clone(),
            url: url.clone(),
            title,
            folder: p.folder.trim().to_string(),
        };
        let interval = crate::DEFAULT_INTERVAL_MIN * 60_000;
        session
            .db(move |s| {
                let feed = s.put_feed(new)?;
                let at = now();
                ingest(s, &feed.id, &url, fetched.feed, at)?;
                s.record_fetch_ok(
                    &feed.id,
                    fetched.etag.as_deref(),
                    fetched.last_modified.as_deref(),
                    at,
                    at + interval,
                )?;
                // 重新读一遍，带上刚入库的未读数
                Ok(s.feed(&feed.id)?.unwrap_or(feed))
            })
            .await
    }

    async fn fetch_new(&self, url: &str) -> Result<Fetched, postflow_feed::Error> {
        match self.client.fetch_feed(url, &Conditional::default()).await? {
            FetchResult::Fetched(fetched) => Ok(fetched),
            // 没带校验值却收到 304，只可能是服务器或中间代理有问题
            FetchResult::NotModified => Err(postflow_feed::Error::Status(304)),
        }
    }
}
