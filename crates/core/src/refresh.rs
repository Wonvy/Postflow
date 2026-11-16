//! 抓取订阅源并把新文章存进数据目录。

use std::sync::Arc;
use std::sync::atomic::Ordering;

use futures::StreamExt;
use postflow_feed::{Conditional, FetchResult, ParsedEntry, ParsedFeed, favicon_url};
use postflow_store::{Article, FetchTarget, Store, hlc, ids};
use serde_json::{Map, Value, json};

use crate::{Core, DEFAULT_INTERVAL_MIN, Event, Scope, Session};

/// 同时抓取的订阅源数量上限。
const CONCURRENCY: usize = 6;
/// 刷新间隔的下限，防止误设成几秒把对方服务器打挂。
const MIN_INTERVAL_MIN: i64 = 5;
/// 连续失败时退避的上限。
const MAX_BACKOFF_MIN: i64 = 24 * 60;

pub(crate) fn now() -> i64 {
    hlc::now_ms() as i64
}

impl Core {
    /// 抓取一批订阅源。已经在抓的会被跳过。
    pub(crate) async fn refresh(
        self: &Arc<Self>,
        session: &Arc<Session>,
        targets: Vec<FetchTarget>,
    ) {
        let targets: Vec<FetchTarget> = {
            let mut in_flight = session.in_flight.lock().unwrap_or_else(|e| e.into_inner());
            targets
                .into_iter()
                .filter(|t| in_flight.insert(t.feed_id.clone()))
                .collect()
        };
        if targets.is_empty() {
            return;
        }
        let active = self.refreshing.fetch_add(targets.len(), Ordering::SeqCst) + targets.len();
        self.emit(Event::Refresh { active });
        let default_interval = default_interval(session).await;

        futures::stream::iter(targets)
            .for_each_concurrent(CONCURRENCY, |target| async move {
                self.refresh_one(session, &target, default_interval).await;
                session
                    .in_flight
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&target.feed_id);
                let left = self.refreshing.fetch_sub(1, Ordering::SeqCst) - 1;
                self.emit(Event::Refresh { active: left });
            })
            .await;
        // 新文章要让所有界面都看到，所以不带发起方
        self.emit_changed(&[Scope::Feeds, Scope::Items], None);
    }

    async fn refresh_one(&self, session: &Session, target: &FetchTarget, default_interval: i64) {
        let cond = Conditional {
            etag: target.etag.clone(),
            last_modified: target.last_modified.clone(),
        };
        let interval = target
            .interval_min
            .unwrap_or(default_interval)
            .max(MIN_INTERVAL_MIN);
        let result = self.client.fetch_feed(&target.url, &cond).await;
        let now = now();
        let feed_id = target.feed_id.clone();

        let outcome = match result {
            Ok(FetchResult::NotModified) => {
                let next = now + interval * 60_000;
                session
                    .db(move |s| {
                        s.record_fetch_ok(
                            &feed_id,
                            cond.etag.as_deref(),
                            cond.last_modified.as_deref(),
                            now,
                            next,
                        )
                    })
                    .await
            }
            Ok(FetchResult::Fetched(fetched)) => {
                let next = now + interval * 60_000;
                let url = target.url.clone();
                session
                    .db(move |s| {
                        ingest(s, &feed_id, &url, fetched.feed, now)?;
                        s.record_fetch_ok(
                            &feed_id,
                            fetched.etag.as_deref(),
                            fetched.last_modified.as_deref(),
                            now,
                            next,
                        )
                    })
                    .await
            }
            Err(err) => {
                // 连续失败时逐次加倍等待时间，源恢复后回到正常间隔
                let factor = 1i64 << target.error_count.clamp(0, 6);
                let next = now + (interval * factor).min(MAX_BACKOFF_MIN) * 60_000;
                let message = err.to_string();
                tracing::debug!("fetch {} failed: {message}", target.url);
                session
                    .db(move |s| s.record_fetch_err(&feed_id, &message, now, next))
                    .await
            }
        };
        if let Err(e) = outcome {
            tracing::warn!("could not record fetch of {}: {}", target.url, e.message);
        }
    }
}

async fn default_interval(session: &Session) -> i64 {
    session
        .db(|s| s.settings())
        .await
        .ok()
        .and_then(|settings| settings.get("refresh_interval_min").and_then(Value::as_i64))
        .unwrap_or(DEFAULT_INTERVAL_MIN)
}

/// 把解析好的源存进去：新文章入档，顺便补全订阅时没拿到的标题和图标。
pub(crate) fn ingest(
    store: &Store,
    feed_id: &str,
    feed_url: &str,
    feed: ParsedFeed,
    now: i64,
) -> postflow_store::Result<usize> {
    if let Some(current) = store.feed(feed_id)? {
        let mut set = Map::new();
        if current.title.is_empty() && !feed.title.is_empty() {
            set.insert("title".into(), json!(feed.title));
        }
        if current.site_url.is_none() && feed.site_url.is_some() {
            set.insert("site_url".into(), json!(feed.site_url));
        }
        if current.icon_url.is_none() {
            let site = feed.site_url.as_deref().or(current.site_url.as_deref());
            if let Some(icon) = feed
                .icon_url
                .clone()
                .or_else(|| favicon_url(site, feed_url))
            {
                set.insert("icon_url".into(), json!(icon));
            }
        }
        store.set_fields("feed", &[feed_id.to_string()], set)?;
    }
    store.add_articles(to_articles(feed_id, feed.entries, now))
}

pub(crate) fn to_articles(feed_id: &str, entries: Vec<ParsedEntry>, now: i64) -> Vec<Article> {
    entries
        .into_iter()
        .map(|e| Article {
            id: ids::item_id(feed_id, &e.guid),
            feed: feed_id.to_string(),
            guid: e.guid,
            url: e.url,
            title: e.title,
            author: e.author,
            // 没写日期的按抓到的时间算；写了未来日期的也压回现在，免得一直顶在最上面
            published: e
                .published_ms
                .filter(|p| *p > 0)
                .map_or(now, |p| p.min(now)),
            fetched: now,
            summary: e.summary,
            content: e.content,
            image: e.image,
        })
        .collect()
}
