//! 订阅源的图标：源自己没带图标时，找网站真正的收藏图标。
//!
//! 订阅时没拿到图标的源先用猜的站点根目录 `/favicon.ico`，很多网站那里没有图标，界面只能显示首字母。
//! 每次刷新完，给还没有图标、或者用的还是猜的那个地址的源，找一下网站声明的图标（apple-touch-icon
//! 这类大图标优先，没有再看 favicon.ico 是否真的存在），找到就换上。找的结果和发件人头像共用
//! 本机缓存（`sender-icons.json`），找到的一个月、找不到的一周内不重复上网找。
//! 源自己带的图标（RSS 的 `<image>`、Atom 的 `<icon>`）和订阅时指定的图标不动。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::StreamExt;
use postflow_feed::{favicon_url, host_of};
use postflow_store::Feed;
use serde_json::{Map, json};

use crate::{Core, Scope, Session};

/// 同时找图标的源的数量（发件人头像那边另外最多 4 个网站）。
const PARALLEL: usize = 4;

/// 正在找：上一轮没找完时不再开一轮。
static RUNNING: AtomicBool = AtomicBool::new(false);

/// 这个源要不要找网站图标；要的话返回要找的网站域名。
pub(crate) fn icon_domain(feed: &Feed) -> Option<String> {
    let guessed = favicon_url(feed.site_url.as_deref(), &feed.url);
    let needs = match &feed.icon_url {
        None => true,
        Some(icon) => icon.trim().is_empty() || Some(icon) == guessed.as_ref(),
    };
    if !needs {
        return None;
    }
    let site = feed.site_url.as_deref().unwrap_or(&feed.url);
    let domain = host_of(site)
        .or_else(|| host_of(&feed.url))?
        .to_ascii_lowercase();
    postflow_feed::valid_domain(&domain).then_some(domain)
}

impl Core {
    /// 在后台给源找图标。上一轮还没完就不开新的。
    pub(crate) fn spawn_fill_feed_icons(self: &Arc<Self>, session: &Arc<Session>) {
        if RUNNING.swap(true, Ordering::SeqCst) {
            return;
        }
        let core = self.clone();
        let session = session.clone();
        tokio::spawn(async move {
            core.fill_feed_icons(&session).await;
            RUNNING.store(false, Ordering::SeqCst);
        });
    }

    /// 给没有图标（或者只有猜的图标）的源找网站图标，找到了就存下来。
    async fn fill_feed_icons(self: &Arc<Self>, session: &Arc<Session>) {
        let Ok(feeds) = session.db(|s| s.feeds()).await else {
            return;
        };
        let todo: Vec<(String, Option<String>, String)> = feeds
            .into_iter()
            .filter_map(|f| icon_domain(&f).map(|d| (f.id, f.icon_url, d)))
            .collect();
        if todo.is_empty() {
            return;
        }
        let found: Vec<(String, String)> = futures::stream::iter(todo)
            .map(|(id, current, domain)| async move {
                let icon = self.sender_icon(&domain).await?;
                (Some(&icon) != current.as_ref()).then_some((id, icon))
            })
            .buffer_unordered(PARALLEL)
            .filter_map(|x| async move { x })
            .collect()
            .await;
        if found.is_empty() {
            return;
        }
        let changed = found.len();
        let saved = session
            .db(move |s| {
                for (id, icon) in found {
                    let mut set = Map::new();
                    set.insert("icon_url".into(), json!(icon));
                    s.set_fields("feed", &[id], set)?;
                }
                Ok(())
            })
            .await;
        match saved {
            Ok(()) => {
                tracing::info!(feeds = changed, "found site icons for feeds");
                self.emit_changed(&[Scope::Feeds, Scope::Items], None);
            }
            Err(e) => tracing::warn!("could not save feed icons: {}", e.message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::icon_domain;
    use postflow_store::Feed;

    fn feed(url: &str, site: Option<&str>, icon: Option<&str>) -> Feed {
        Feed {
            id: "f".into(),
            url: url.into(),
            title: String::new(),
            site_url: site.map(String::from),
            icon_url: icon.map(String::from),
            kind: "article".into(),
            kind_set: None,
            full_text: false,
            folder: String::new(),
            interval_min: None,
            added_at: 0,
            unread: 0,
            last_fetch_at: None,
            error: None,
            error_code: None,
        }
    }

    #[test]
    fn looks_up_feeds_without_a_real_icon() {
        // 没有图标
        assert_eq!(
            icon_domain(&feed(
                "https://example.com/feed.xml",
                Some("https://www.Example.com/"),
                None
            ))
            .as_deref(),
            Some("example.com")
        );
        // 只有猜的站点根目录 favicon
        assert_eq!(
            icon_domain(&feed(
                "https://blog.example.org/rss",
                Some("https://blog.example.org/"),
                Some("https://blog.example.org/favicon.ico")
            ))
            .as_deref(),
            Some("blog.example.org")
        );
        // 没写网站地址时看订阅地址
        assert_eq!(
            icon_domain(&feed("https://news.example.net/atom.xml", None, None)).as_deref(),
            Some("news.example.net")
        );
    }

    #[test]
    fn keeps_icons_the_feed_brought() {
        assert_eq!(
            icon_domain(&feed(
                "https://example.com/feed.xml",
                Some("https://example.com/"),
                Some("https://example.com/static/logo.png")
            )),
            None
        );
        assert_eq!(
            icon_domain(&feed(
                "https://example.com/feed.xml",
                None,
                Some("data:image/png;base64,AAAA")
            )),
            None
        );
        // 本机地址、IP 不去找
        assert_eq!(
            icon_domain(&feed("http://127.0.0.1:8080/feed", None, None)),
            None
        );
        assert_eq!(
            icon_domain(&feed("http://localhost/feed", None, None)),
            None
        );
    }
}
