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
struct LogsParams {
    #[serde(default)]
    lines: Option<usize>,
}

/// 界面里出的错，记进同一份日志。
#[derive(Deserialize)]
struct UiLog {
    #[serde(default)]
    level: String,
    message: String,
}

#[derive(Deserialize)]
struct LoggingParams {
    verbose: bool,
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
    /// 预览时找到的图标（比如 YouTube 频道头像），比源里的更合适时由界面带上。
    #[serde(default)]
    icon_url: Option<String>,
}

/// 侧栏里拖动后，一个文件夹（空字符串是文件夹外）里订阅源的新顺序。
#[derive(Deserialize)]
struct ReorderFeeds {
    #[serde(default)]
    folder: String,
    ids: Vec<String>,
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

/// 一次阅读：在一篇上停留了多久、往下看到了哪里、看没看完。
#[derive(Deserialize)]
struct ReadingParams {
    item_id: String,
    dwell_ms: i64,
    depth: f64,
    #[serde(default)]
    finished: bool,
}

#[derive(Deserialize)]
struct StatsParams {
    #[serde(default = "default_stats_days")]
    days: i64,
    /// 本地时间比 UTC 快多少分钟，用来按本地的日子分天。
    #[serde(default)]
    tz_offset_min: i64,
}

fn default_stats_days() -> i64 {
    30
}

#[derive(Deserialize)]
struct SnoozeParams {
    ids: Vec<String>,
    #[serde(default)]
    until: Option<i64>,
}

#[derive(Deserialize)]
struct RefreshParams {
    #[serde(default)]
    id: Option<String>,
    /// 只刷新订阅源，不收邮件：侧栏「订阅」那边的刷新按钮。
    #[serde(default)]
    feeds_only: bool,
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
struct DomainParam {
    domain: String,
}

#[derive(Deserialize)]
struct WebCheck {
    url: String,
    /// 要把网页嵌进去的那个页面的地址，也就是界面自己的地址。
    origin: String,
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

/// 内置订阅源目录的在线版本：先走 jsDelivr（GitHub 的 CDN，国内更容易打开），不行再直接读 GitHub。
const CATALOG_URLS: &[&str] = &[
    "https://cdn.jsdelivr.net/gh/Wonvy/Postflow@main/public/catalog/catalog.json",
    "https://raw.githubusercontent.com/Wonvy/Postflow/main/public/catalog/catalog.json",
];
/// 目录再大也不会超过这么多；超过了多半不是目录。
/// 章节、文字稿文件的大小上限。
const MAX_MEDIA_TEXT: usize = 4 * 1024 * 1024;
const MAX_CATALOG: usize = 2 * 1024 * 1024;

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
            // 翻译列表要读条目，放在打开数据目录之后
            m if m.starts_with("translate.") && m != "translate.items" => {
                return self.translate_rpc(m, params).await;
            }
            "app.logs" => {
                let p: LogsParams = parse(params)?;
                return reply(
                    match crate::logging::tail(p.lines.unwrap_or(400).min(5000)) {
                        Some((dir, verbose, text)) => {
                            json!({ "available": true, "dir": dir, "verbose": verbose, "text": text })
                        }
                        None => {
                            json!({ "available": false, "dir": null, "verbose": false, "text": "" })
                        }
                    },
                );
            }
            "app.log" => {
                let p: UiLog = parse(params)?;
                let message: String = p.message.chars().take(4000).collect();
                match p.level.as_str() {
                    "error" => tracing::error!(target: "postflow_ui", "{message}"),
                    "warn" => tracing::warn!(target: "postflow_ui", "{message}"),
                    _ => tracing::info!(target: "postflow_ui", "{message}"),
                }
                return reply(json!({}));
            }
            "app.logging" => {
                let p: LoggingParams = parse(params)?;
                return reply(json!({ "ok": crate::logging::set_verbose(p.verbose) }));
            }
            // 语音识别模型：列出、下载、删除、选用
            m if m.starts_with("speech.") => return self.speech_rpc(m, params).await,
            // 插件：列出本机插件目录里的插件、读出插件的代码
            m if m.starts_with("plugins.") => return self.plugins_rpc(m, params).await,
            // RSSHub 路由：搜索、按网址猜、重新下载目录
            m if m.starts_with("rsshub.") => return self.rsshub_rpc(m, params).await,
            // 内置订阅源目录的在线版本。地址是固定的，界面不能指定
            // 离线听：下载、列出、删除下好的单集
            "episodes.list" => {
                return reply(json!({
                    "downloaded": self.episodes.list(),
                    "downloading": self.episodes.progress(),
                }));
            }
            "episodes.remove" => {
                let p: IdParam = parse(params)?;
                self.episodes.remove(&p.id);
                return reply(json!({}));
            }
            // 播客单集的章节、文字稿：由核心去取，网页里直接请求别的网站会被浏览器拦下
            "media.text" => {
                let p: UrlParam = parse(params)?;
                if !(p.url.starts_with("https://") || p.url.starts_with("http://")) {
                    return Err(RpcError::new("bad_params", "only http and https"));
                }
                let text = self.client.fetch_text(&p.url).await?;
                if text.len() > MAX_MEDIA_TEXT {
                    return Err(RpcError::new("too_large", "the file is too large"));
                }
                return reply(json!({ "text": text }));
            }
            // 搜索播客：按节目名找订阅地址
            "podcasts.search" => {
                #[derive(Deserialize)]
                struct Search {
                    term: String,
                    #[serde(default)]
                    country: String,
                }
                let p: Search = parse(params)?;
                return reply(self.search_podcasts(&p.term, &p.country).await?);
            }
            "catalog.fetch" => {
                let mut last = RpcError::new("network", "no catalog source answered");
                for url in CATALOG_URLS {
                    match self.client.fetch_text(url).await {
                        Ok(text) if text.len() <= MAX_CATALOG => {
                            return reply(json!({ "text": text, "url": url }));
                        }
                        Ok(_) => last = RpcError::new("too_large", "the catalog is too large"),
                        Err(err) => {
                            tracing::info!("catalog: {url} failed: {err}");
                            last = err.into();
                        }
                    }
                }
                return Err(last);
            }
            // 发件人头像：发件人所在网站的图标
            "mail.senderIcon" => {
                let p: DomainParam = parse(params)?;
                return reply(json!({ "url": self.sender_icon(&p.domain).await }));
            }
            // 阅读区的原网页模式：这个网页肯不肯被嵌进界面里
            "web.check" => {
                let p: WebCheck = parse(params)?;
                let lower = p.url.trim().to_ascii_lowercase();
                if !(lower.starts_with("http://") || lower.starts_with("https://")) {
                    return Err(RpcError::new("invalid_url", "only http and https pages"));
                }
                return reply(self.client.frame_policy(p.url.trim(), &p.origin).await?);
            }
            _ => {}
        }

        let session = self
            .session()
            .ok_or_else(|| RpcError::new("not_configured", "no data directory has been set up"))?;

        match method {
            "episodes.download" => {
                #[derive(Deserialize)]
                struct Download {
                    id: String,
                    #[serde(default = "default_keep")]
                    keep: usize,
                }
                fn default_keep() -> usize {
                    10
                }
                let p: Download = parse(params)?;
                self.download_episode(&session, p.id, p.keep).await?;
                reply(json!({}))
            }
            // 划线和笔记：存在同步日志里
            "highlights.list" => {
                #[derive(Deserialize)]
                struct List {
                    #[serde(default)]
                    item: Option<String>,
                }
                let p: List = parse(params)?;
                match p.item {
                    Some(item) => reply(
                        json!({ "highlights": session.db(move |s| s.highlights(Some(&item))).await? }),
                    ),
                    None => {
                        reply(json!({ "highlights": session.db(|s| s.all_highlights()).await? }))
                    }
                }
            }
            "highlights.add" => {
                let mut h: postflow_store::Highlight = parse(params)?;
                if h.quote.trim().is_empty() || h.item.is_empty() {
                    return Err(RpcError::new("bad_params", "nothing to highlight"));
                }
                h.quote = h.quote.chars().take(5000).collect();
                h.created_at = crate::refresh::now();
                let saved = h.clone();
                session.db(move |s| s.put_highlight(&h)).await?;
                reply(saved)
            }
            "highlights.note" => {
                #[derive(Deserialize)]
                struct Note {
                    id: String,
                    note: String,
                }
                let p: Note = parse(params)?;
                session
                    .db(move |s| s.set_highlight_note(&p.id, &p.note))
                    .await?;
                reply(json!({}))
            }
            "highlights.remove" => {
                let p: IdParam = parse(params)?;
                session.db(move |s| s.remove_highlight(&p.id)).await?;
                reply(json!({}))
            }
            // 发件人卡片：和这个人的往来
            "mail.sender" => {
                #[derive(Deserialize)]
                struct Sender {
                    address: String,
                }
                let p: Sender = parse(params)?;
                reply(session.db(move |s| s.sender_profile(&p.address)).await?)
            }
            // 附件总览：最近邮件里的附件
            "mail.attachments" => {
                #[derive(Deserialize)]
                struct List {
                    #[serde(default)]
                    query: Option<String>,
                    #[serde(default)]
                    limit: Option<u32>,
                }
                let p: List = parse(params)?;
                let limit = p.limit.unwrap_or(500).clamp(1, 5000);
                reply(
                    session
                        .db(move |s| s.attachments(p.query.as_deref(), limit))
                        .await?,
                )
            }
            // 稍后读：存一个网页、导入 Pocket / Instapaper / 书签
            "later.save" => {
                let p: UrlParam = parse(params)?;
                reply(self.save_later(&session, &p.url).await?)
            }
            "later.import" => {
                #[derive(Deserialize)]
                struct Import {
                    text: String,
                }
                let p: Import = parse(params)?;
                reply(self.import_later(&session, &p.text).await?)
            }
            "nav.get" => {
                let p: NavGet = parse(params)?;
                let (counts, feeds, accounts, labels, newsletters) = session
                    .db(move |s| {
                        Ok((
                            s.counts(p.today_since)?,
                            s.feeds()?,
                            s.accounts()?,
                            s.label_counts()?,
                            s.newsletters()?,
                        ))
                    })
                    .await?;
                reply(json!({
                    "counts": counts,
                    "feeds": feeds,
                    "accounts": accounts,
                    "labels": labels,
                    "newsletters": newsletters,
                    "later_id": postflow_store::later_feed_id(),
                }))
            }

            "feeds.discover" => {
                let p: UrlParam = parse(params)?;
                reply(self.client.discover(&p.url).await?)
            }
            // 添加订阅前的预览：图标、标题、介绍和最近几篇
            "feeds.preview" => {
                let p: UrlParam = parse(params)?;
                reply(self.client.preview(&p.url).await?)
            }
            "feeds.add" => {
                let feed = self.add_feed(&session, parse(params)?).await?;
                self.emit_changed(&[Scope::Feeds, Scope::Items], None);
                reply(feed)
            }
            "feeds.update" => {
                let mut p: UpdateFeed = parse(params)?;
                let moved = match p.patch.url.take().filter(|u| !u.trim().is_empty()) {
                    Some(input) => {
                        self.change_feed_url(&session, &p.id, &input, &mut p.patch)
                            .await?
                    }
                    None => None,
                };
                let id = p.id.clone();
                session
                    .db(move |s| {
                        s.update_feed(&p.id, p.patch)?;
                        // 新地址：旧的校验值和错误都作废，把刚抓到的内容收进来
                        if let Some((url, fetched)) = moved {
                            s.reset_fetch(&p.id)?;
                            let at = now();
                            let interval = crate::DEFAULT_INTERVAL_MIN * 60_000;
                            ingest(s, &p.id, &url, fetched.feed, at)?;
                            s.record_fetch_ok(
                                &p.id,
                                fetched.etag.as_deref(),
                                fetched.last_modified.as_deref(),
                                at,
                                at + interval,
                            )?;
                        }
                        Ok(())
                    })
                    .await?;
                tracing::debug!(feed = %id, "feed updated");
                self.emit_changed(&[Scope::Feeds, Scope::Items], client);
                reply(json!({}))
            }
            "feeds.reorder" => {
                let p: ReorderFeeds = parse(params)?;
                let folder = p.folder.trim().to_string();
                session
                    .db(move |s| s.reorder_feeds(&folder, &p.ids))
                    .await?;
                // 其他窗口要跟着变；发起方已经先在本地排好了
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
                let p: RefreshParams = parse(params)?;
                // 刷新全部时邮箱也一起收（除非只要订阅）；指定的是邮箱账户时只收那一个
                match &p.id {
                    None if p.feeds_only => {}
                    None => self.sync_all_mail(&session, true).await,
                    Some(id) => {
                        if self.sync_account(&session, id, true) {
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
                // 搜索前先把刚到的内容收进索引，免得刚收到的邮件搜不到
                if p.filter
                    .query
                    .as_deref()
                    .is_some_and(|q| !q.trim().is_empty())
                {
                    session
                        .db(|s| {
                            for _ in 0..20 {
                                if s.index_search(crate::SEARCH_BATCH, &crate::mail_search_text)?
                                    == 0
                                {
                                    break;
                                }
                            }
                            Ok(())
                        })
                        .await?;
                }
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
                // YouTube、B 站的视频页：正文里没有播放器时补上一个
                let show = |html: &str| match postflow_feed::with_player(html, base) {
                    Some(with) => postflow_feed::sanitize_html(&with, base),
                    None => postflow_feed::sanitize_html(html, base),
                };
                let content = show(&detail.content);
                let full_text = detail.full_text.as_deref().map(show);
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
            // 稍后提醒：`until` 之前不出现在收件箱里；`until` 为空是取消
            "items.snooze" => {
                let p: SnoozeParams = parse(params)?;
                session.db(move |s| s.set_snooze(&p.ids, p.until)).await?;
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
                let clean = match postflow_feed::with_player(&raw, Some(&url)) {
                    Some(with) => postflow_feed::sanitize_html(&with, Some(&url)),
                    None => postflow_feed::sanitize_html(&raw, Some(&url)),
                };
                // 原网页上的正文比源里给的还短：多半只给了没登录的人看开头，不如源里的内容
                let chars = |html: &str| {
                    postflow_feed::html_to_text(html)
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .count()
                };
                if chars(&clean) * 10 < chars(&detail.content) * 8 {
                    return Err(RpcError::new(
                        "fulltext_shorter",
                        "the page has less text than the feed",
                    ));
                }
                session.db(move |s| s.set_full_text(&p.id, &raw)).await?;
                reply(json!({ "full_text": clean }))
            }

            // 查看源代码：邮件是原始邮件，文章是它在订阅源里的那一段 XML（重新抓一次源找出来）
            "items.source" => {
                let p: IdParam = parse(params)?;
                let id = p.id.clone();
                let detail = session
                    .db(move |s| s.item(&id))
                    .await?
                    .ok_or_else(|| RpcError::new("not_found", "no such item"))?;
                if detail.item.kind == "mail" {
                    let id = p.id.clone();
                    let raw = session
                        .db(move |s| s.mail_body(&id))
                        .await?
                        .ok_or_else(|| {
                            RpcError::new("not_found", "the message isn't downloaded yet")
                        })?;
                    return reply(json!({
                        "kind": "mail",
                        "source": String::from_utf8_lossy(&raw),
                        "whole": false,
                    }));
                }
                let source_id = detail.item.source_id.clone();
                let id = p.id.clone();
                let (feed, guid) = session
                    .db(move |s| Ok((s.feed(&source_id)?, s.item_guid(&id)?)))
                    .await?;
                let feed = feed.ok_or_else(|| RpcError::new("not_found", "the feed is gone"))?;
                let xml = self.client.fetch_text(&feed.url).await?;
                let found = postflow_feed::item_xml(
                    &xml,
                    guid.as_deref().unwrap_or(""),
                    detail.item.url.as_deref(),
                );
                // 这一篇已经不在源里了（源只保留最近几篇），给整个源
                let whole = found.is_none();
                let source = found.unwrap_or_else(|| postflow_feed::whole_feed(xml));
                reply(json!({ "kind": "xml", "source": source, "whole": whole, "url": feed.url }))
            }

            // 订阅统计。阅读记录只在本机
            "stats.reading" => {
                let p: ReadingParams = parse(params)?;
                let at = now();
                let saved = session
                    .db(move |s| s.record_reading(&p.item_id, at, p.dwell_ms, p.depth, p.finished))
                    .await?;
                reply(json!({ "saved": saved }))
            }
            "stats.feeds" => {
                let p: StatsParams = parse(params)?;
                let at = now();
                let stats = session
                    .db(move |s| s.feed_stats(at, p.days, p.tz_offset_min * 60_000))
                    .await?;
                reply(stats)
            }
            "stats.clear" => {
                session.db(|s| s.clear_reading()).await?;
                reply(json!({}))
            }

            "translate.items" => self.translate_items(params, &session).await,
            // 分类的方法也在 AI 模块里
            m if m.starts_with("ai.") || m == "items.label" => {
                self.ai_rpc(m, params, &session, client).await
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

            m if m.starts_with("storage.") => self.storage_rpc(m, params, &session).await,
            // 字幕：在本机把播客单集转成文字
            m if m.starts_with("subtitles.") => self.subtitles_rpc(&session, m, params).await,
            m if m.starts_with("rules.") => self.rules_rpc(m, params, &session).await,
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

    /// 把用户给的地址变成能订阅的源：本身就是源就直接用；是网页时只有一个候选就采用它，
    /// 免得用户多点一步。返回源的地址和抓到的内容。
    async fn resolve_feed(&self, input: &str) -> Result<(String, Fetched), RpcError> {
        let requested = normalize_url(input)?.to_string();
        match self.fetch_new(&requested).await {
            // 地址已经永久跳到别处的，直接订阅新地址
            Ok(fetched) => Ok((fetched.moved_to.clone().unwrap_or(requested), fetched)),
            Err(postflow_feed::Error::NotAFeed(_)) => {
                let mut candidates = self.client.discover(&requested).await?;
                if candidates.len() != 1 {
                    return Err(postflow_feed::Error::NoFeedFound.into());
                }
                let url = candidates.remove(0).url;
                let fetched = self.fetch_new(&url).await?;
                Ok((fetched.moved_to.clone().unwrap_or(url), fetched))
            }
            Err(e) => Err(e.into()),
        }
    }

    /// 换订阅地址：先确认新地址能抓到源，订阅源的 ID 不变，已读、星标和以前的文章都留着，
    /// 然后马上把新地址的内容收进来。返回实际用的地址（给的是网页时是找到的源）。
    async fn change_feed_url(
        &self,
        session: &Session,
        id: &str,
        input: &str,
        patch: &mut FeedPatch,
    ) -> Result<Option<(String, Fetched)>, RpcError> {
        let feed_id = id.to_string();
        let feeds = session.db(|s| s.feeds()).await?;
        let current = feeds
            .iter()
            .find(|f| f.id == feed_id)
            .ok_or_else(|| RpcError::new("not_found", "no such feed"))?;
        let (url, fetched) = self.resolve_feed(input).await?;
        if url == current.url {
            return Ok(None);
        }
        if feeds.iter().any(|f| f.id != feed_id && f.url == url) {
            return Err(RpcError::new(
                "feed_exists",
                "another feed already uses this address",
            ));
        }
        patch.url = Some(url.clone());
        patch.site_url = fetched.feed.site_url.clone();
        // 新源自己带图标才换，否则留着原来的（多半还是同一个网站）
        patch.icon_url = fetched.feed.icon_url.clone();
        Ok(Some((url, fetched)))
    }

    async fn add_feed(&self, session: &Session, p: AddFeed) -> Result<Feed, RpcError> {
        let (url, fetched) = self.resolve_feed(&p.url).await?;

        let title = p
            .title
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .or_else(|| Some(fetched.feed.title.clone()).filter(|t| !t.is_empty()))
            .or_else(|| host_of(&url))
            .unwrap_or_default();
        let given_icon = p
            .icon_url
            .filter(|u| u.starts_with("https://") || u.starts_with("http://"));
        let new = NewFeed {
            icon_url: given_icon
                .or_else(|| fetched.feed.icon_url.clone())
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
