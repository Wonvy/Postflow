//! 添加订阅前的预览：贴进一个地址，先看看是哪个网站、什么图标、最近都发了些什么。

use scraper::{Html, Selector};
use serde::Serialize;
use url::Url;

use crate::client::{Client, decode_html};
use crate::discover::{Candidate, links_in_html};
use crate::parse::{ParsedFeed, parse_feed, truncate};
use crate::sanitize::collapse_whitespace;
use crate::{Error, Result, favicon_url, normalize_url};

/// 预览里列出几篇。
const SAMPLE: usize = 6;
/// 预览里每篇摘要的长度。
const SAMPLE_CHARS: usize = 140;

#[derive(Debug, Clone, Serialize)]
pub struct PreviewEntry {
    pub title: String,
    pub url: Option<String>,
    pub published_ms: Option<i64>,
    pub summary: String,
    pub image: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeedPreview {
    /// 订阅的地址。
    pub url: String,
    pub title: String,
    pub site_url: Option<String>,
    pub icon_url: Option<String>,
    pub description: Option<String>,
    /// 源里一共有几篇，以及最新、最早一篇的时间，界面据此估算更新频率。
    pub entry_count: usize,
    pub newest_ms: Option<i64>,
    pub oldest_ms: Option<i64>,
    /// 最新的几篇。
    pub entries: Vec<PreviewEntry>,
    /// 特殊来源，比如 `youtube`。
    pub kind: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Preview {
    pub feed: FeedPreview,
    /// 网页上声明了不止一个源时，全部列出来，`feed` 是第一个。
    pub candidates: Vec<Candidate>,
}

/// 网页自己声明的图标和介绍，源里没有时用它们。
#[derive(Debug, Default)]
struct PageMeta {
    icon: Option<String>,
    description: Option<String>,
}

fn page_meta(html: &str, base: &Url) -> PageMeta {
    let doc = Html::parse_document(html);
    let icons = Selector::parse("link[rel][href]").expect("valid selector");
    // 大图标优先：apple-touch-icon 一般是 180px，普通 icon 常常只有 16px
    let mut best: Option<(u8, String)> = None;
    for link in doc.select(&icons) {
        let rel = link.attr("rel").unwrap_or_default().to_ascii_lowercase();
        let rank = if rel.contains("apple-touch-icon") {
            3
        } else if rel.split_whitespace().any(|r| r == "icon") {
            if link.attr("type").is_some_and(|t| t.contains("svg")) {
                2
            } else {
                1
            }
        } else {
            continue;
        };
        let Some(href) = link.attr("href").and_then(|h| base.join(h.trim()).ok()) else {
            continue;
        };
        if !matches!(href.scheme(), "http" | "https") {
            continue;
        }
        if best.as_ref().is_none_or(|(r, _)| rank > *r) {
            best = Some((rank, href.to_string()));
        }
    }
    let description =
        Selector::parse(r#"meta[name="description"], meta[property="og:description"]"#)
            .ok()
            .and_then(|s| {
                doc.select(&s)
                    .find_map(|m| m.attr("content"))
                    .map(collapse_whitespace)
            })
            .filter(|d| !d.is_empty())
            .map(|d| truncate(&d, 280));
    PageMeta {
        icon: best.map(|(_, href)| href),
        description,
    }
}

fn summarize(
    url: String,
    feed: ParsedFeed,
    meta: PageMeta,
    kind: Option<&'static str>,
) -> FeedPreview {
    let mut dates: Vec<i64> = feed.entries.iter().filter_map(|e| e.published_ms).collect();
    dates.sort_unstable();
    let mut entries = feed.entries;
    // 大多数源从新到旧，但也有反过来的
    entries.sort_by_key(|e| std::cmp::Reverse(e.published_ms.unwrap_or(i64::MIN)));
    let icon_url = feed
        .icon_url
        .or(meta.icon)
        .or_else(|| favicon_url(feed.site_url.as_deref(), &url));
    FeedPreview {
        title: if feed.title.is_empty() {
            crate::host_of(&url).unwrap_or_default()
        } else {
            feed.title
        },
        site_url: feed.site_url,
        icon_url,
        description: feed.description.or(meta.description),
        entry_count: entries.len(),
        newest_ms: dates.last().copied(),
        oldest_ms: dates.first().copied(),
        entries: entries
            .into_iter()
            .take(SAMPLE)
            .map(|e| PreviewEntry {
                title: e.title,
                url: e.url,
                published_ms: e.published_ms,
                summary: truncate(&e.summary, SAMPLE_CHARS),
                image: e.image,
            })
            .collect(),
        kind,
        url,
    }
}

impl Client {
    async fn fetch_parsed(&self, url: &str) -> Result<ParsedFeed> {
        let page = self.get(url, None, None).await?.ok_or(Error::NoFeedFound)?;
        parse_feed(&page.body, page.final_url.as_str())
    }

    /// 预览用户贴进来的地址：可以是源，也可以是网站首页或 YouTube 频道。
    pub async fn preview(&self, input: &str) -> Result<Preview> {
        let url = normalize_url(input)?;

        if let Some(found) = self.youtube(&url).await? {
            let feed = self.fetch_parsed(&found.feed_url).await?;
            // 源里的标题是频道名，和订阅后侧栏里显示的一致；频道页上的名字只在源没有标题时用
            let title = Some(feed.title.clone())
                .filter(|t| !t.is_empty())
                .or(found.title)
                .unwrap_or_default();
            let mut preview = summarize(
                found.feed_url.clone(),
                feed,
                PageMeta::default(),
                Some("youtube"),
            );
            // 频道头像比源里的任何图标都合适
            if found.avatar.is_some() {
                preview.icon_url = found.avatar;
            }
            preview.title = title.clone();
            return Ok(Preview {
                candidates: vec![Candidate {
                    url: found.feed_url,
                    title,
                }],
                feed: preview,
            });
        }

        let page = self
            .get(url.as_str(), None, None)
            .await?
            .ok_or(Error::NoFeedFound)?;
        if let Ok(feed) = parse_feed(&page.body, page.final_url.as_str()) {
            let title = feed.title.clone();
            let preview = summarize(url.to_string(), feed, PageMeta::default(), None);
            return Ok(Preview {
                candidates: vec![Candidate {
                    url: url.to_string(),
                    title,
                }],
                feed: preview,
            });
        }

        let html = decode_html(&page);
        let meta = page_meta(&html, &page.final_url);
        let mut candidates = links_in_html(&html, &page.final_url);
        if candidates.is_empty() {
            candidates.extend(self.probe_common_paths(&page.final_url).await);
        }
        let first = candidates.first().ok_or(Error::NoFeedFound)?.url.clone();
        let feed = self.fetch_parsed(&first).await?;
        Ok(Preview {
            feed: summarize(first, feed, meta, None),
            candidates,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_large_page_icons() {
        let base = Url::parse("https://example.com/blog/").unwrap();
        let html = r#"<html><head>
            <link rel="icon" href="/favicon-16.png">
            <link rel="apple-touch-icon" href="/touch.png">
            <link rel="icon" type="image/svg+xml" href="/icon.svg">
            <meta name="description" content="  A   blog about things ">
        </head></html>"#;
        let meta = page_meta(html, &base);
        assert_eq!(meta.icon.as_deref(), Some("https://example.com/touch.png"));
        assert_eq!(meta.description.as_deref(), Some("A blog about things"));
    }

    #[test]
    fn summary_sorts_newest_first_and_counts() {
        let entry = |title: &str, ms: Option<i64>| crate::ParsedEntry {
            title: title.into(),
            published_ms: ms,
            summary: "x".repeat(400),
            ..Default::default()
        };
        let feed = ParsedFeed {
            title: String::new(),
            entries: vec![
                entry("old", Some(1)),
                entry("new", Some(3)),
                entry("undated", None),
            ],
            ..Default::default()
        };
        let p = summarize(
            "https://example.com/feed".into(),
            feed,
            PageMeta::default(),
            None,
        );
        assert_eq!(p.title, "example.com");
        assert_eq!(p.entry_count, 3);
        assert_eq!((p.oldest_ms, p.newest_ms), (Some(1), Some(3)));
        assert_eq!(p.entries[0].title, "new");
        assert!(p.entries[0].summary.chars().count() <= SAMPLE_CHARS + 1);
        assert_eq!(
            p.icon_url.as_deref(),
            Some("https://example.com/favicon.ico")
        );
    }
}
