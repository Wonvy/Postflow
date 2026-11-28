//! 抓取订阅源并把新文章存进数据目录。

use std::sync::Arc;
use std::sync::atomic::Ordering;

use futures::StreamExt;
use serde_json::{Map, Value, json};
use shibapost_feed::{Conditional, FetchResult, ParsedEntry, ParsedFeed, favicon_url};
use shibapost_store::{Article, FeedPatch, FetchTarget, Media, Store, Transcript, hlc, ids};

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
        let mail = self.syncing_mail.load(Ordering::SeqCst);
        self.emit(Event::Refresh { active, mail });
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
                let mail = self.syncing_mail.load(Ordering::SeqCst);
                self.emit(Event::Refresh { active: left, mail });
            })
            .await;
        // 新文章要让所有界面都看到，所以不带发起方
        self.emit_changed(&[Scope::Feeds, Scope::Items], None);
        // 新文章按自动规则处理
        self.run_rules_on_new(session).await;
        // 没有图标的源在后台找一下网站的收藏图标，不耽误这次刷新结束
        self.spawn_fill_feed_icons(session);
    }

    /// 被拒绝的订阅源在苹果目录里的地址：按订阅时记下的节目名去搜。
    async fn apple_lookup(&self, session: &Session, target: &FetchTarget) -> Option<String> {
        let id = target.feed_id.clone();
        let title = session
            .db(move |s| Ok(s.feed(&id)?.map(|f| f.title)))
            .await
            .ok()
            .flatten()?;
        self.apple_lookup_for(&title, &target.url).await
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
        let mut result = self.client.fetch_feed(&target.url, &cond).await;
        // 小宇宙这类只给白名单客户端发订阅源的平台：在苹果播客目录里找到这个节目，改从那里读，
        // 以后也直接用那个地址（和源永久搬家一样处理）
        if matches!(result, Err(shibapost_feed::Error::Refused(_)))
            && shibapost_feed::apple::feed_is_blocked(&target.url)
            && let Some(lookup) = self.apple_lookup(session, target).await
        {
            tracing::info!(feed = %target.feed_id, to = %lookup, "feed refused; reading it from Apple Podcasts");
            result = self
                .client
                .fetch_feed(&lookup, &Conditional::default())
                .await
                .map(|r| match r {
                    FetchResult::Fetched(mut f) => {
                        f.moved_to = Some(lookup);
                        FetchResult::Fetched(f)
                    }
                    other => other,
                });
        }
        let now = now();
        let feed_id = target.feed_id.clone();

        let outcome = match result {
            Ok(FetchResult::NotModified) => {
                tracing::debug!(feed = %target.feed_id, "not modified");
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
                tracing::debug!(feed = %target.feed_id, entries = fetched.feed.entries.len(), "fetched");
                let next = now + interval * 60_000;
                let old = target.url.clone();
                session
                    .db(move |s| {
                        // 源永久搬了家：以后直接用新地址。别的订阅源已经在用那个地址时不动
                        let mut url = old.clone();
                        if let Some(moved) = fetched.moved_to.clone()
                            && !s.feeds()?.iter().any(|f| f.id != feed_id && f.url == moved)
                        {
                            tracing::info!(feed = %feed_id, from = %old, to = %moved, "feed moved permanently");
                            s.update_feed(
                                &feed_id,
                                FeedPatch {
                                    url: Some(moved.clone()),
                                    ..Default::default()
                                },
                            )?;
                            url = moved;
                        }
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
                let code = err.code();
                tracing::warn!(feed = %target.feed_id, failures = target.error_count + 1, "fetch {} failed: {message}", target.url);
                session
                    .db(move |s| s.record_fetch_err(&feed_id, code, &message, now, next))
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
) -> shibapost_store::Result<usize> {
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
            media: e.media.map(|m| Media {
                url: m.url,
                mime: m.mime,
                duration: m.duration,
                size: m.size,
                chapters: m.chapters,
                transcript: m.transcript.map(|(url, mime)| Transcript { url, mime }),
            }),
        })
        .collect()
}
