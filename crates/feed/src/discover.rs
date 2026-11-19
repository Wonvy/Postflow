use std::time::Duration;

use scraper::{Html, Selector};
use serde::Serialize;
use url::Url;

use crate::client::{Client, decode_html};
use crate::known;
use crate::parse::{parse_feed, parse_source};
use crate::sanitize::collapse_whitespace;
use crate::{Error, Result, normalize_url};

/// 网页没有声明订阅源时，依次试探这些常见路径。
const COMMON_PATHS: &[&str] = &[
    "feed",
    "rss",
    "atom.xml",
    "feed.xml",
    "rss.xml",
    "index.xml",
    "feed.json",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Candidate {
    pub url: String,
    pub title: String,
}

impl Client {
    /// 用户可能贴的是订阅地址，也可能只是网站首页。找出可订阅的源。
    pub async fn discover(&self, input: &str) -> Result<Vec<Candidate>> {
        let url = normalize_url(input)?;
        // YouTube 频道页上声明的源只叫「RSS」，直接换算成频道的源，用频道名作标题
        if let Some(found) = self.youtube(&url).await? {
            let title = found.title.unwrap_or_default();
            return Ok(vec![Candidate {
                url: found.feed_url,
                title,
            }]);
        }
        // 已知网站（Telegram 频道、PyPI、Steam……）：直接换成订阅地址，读一次拿到标题
        if let Some(feed_url) = known::feed_for(&url) {
            let page = self
                .get(&feed_url, None, None)
                .await?
                .ok_or(Error::NoFeedFound)?;
            let feed = parse_source(&page.body, page.final_url.as_str())?;
            return Ok(vec![Candidate {
                url: feed_url,
                title: feed.title,
            }]);
        }

        let page = self
            .get(url.as_str(), None, None)
            .await?
            .ok_or(Error::NoFeedFound)?;

        if let Ok(feed) = parse_feed(&page.body, page.final_url.as_str()) {
            return Ok(vec![Candidate {
                url: url.to_string(),
                title: feed.title,
            }]);
        }

        let declared = links_in_html(&decode_html(&page), &page.final_url);
        if !declared.is_empty() {
            return Ok(declared);
        }
        match self.probe_common_paths(&page.final_url).await {
            Some(found) => Ok(vec![found]),
            None => Err(Error::NoFeedFound),
        }
    }

    pub(crate) async fn probe_common_paths(&self, page_url: &Url) -> Option<Candidate> {
        let mut urls: Vec<Url> = Vec::new();
        let mut bases = vec![page_url.join("/").ok()?];
        // 博客挂在子路径下时，源往往也在子路径下
        if page_url.path() != "/" {
            let mut dir = page_url.clone();
            if !dir.path().ends_with('/') {
                dir.set_path(&format!("{}/", dir.path()));
            }
            bases.push(dir);
        }
        for base in &bases {
            for path in COMMON_PATHS {
                if let Ok(candidate) = base.join(path)
                    && !urls.contains(&candidate)
                {
                    urls.push(candidate);
                }
            }
        }

        let probes = urls.iter().map(|u| async move {
            let page = self
                .get(u.as_str(), None, Some(Duration::from_secs(8)))
                .await
                .ok()??;
            let feed = parse_feed(&page.body, u.as_str()).ok()?;
            Some(Candidate {
                url: u.to_string(),
                title: feed.title,
            })
        });
        // 按上面的优先顺序取第一个能解析的
        futures::future::join_all(probes)
            .await
            .into_iter()
            .flatten()
            .next()
    }
}

/// 读出网页 `<head>` 里用 `<link rel="alternate">` 声明的订阅源。
pub(crate) fn links_in_html(html: &str, base: &Url) -> Vec<Candidate> {
    let doc = Html::parse_document(html);
    let link_selector = Selector::parse("link[href]").expect("valid selector");
    let title_selector = Selector::parse("title").expect("valid selector");
    let page_title = doc
        .select(&title_selector)
        .next()
        .map(|t| collapse_whitespace(&t.text().collect::<String>()))
        .unwrap_or_default();

    let mut out: Vec<Candidate> = Vec::new();
    for link in doc.select(&link_selector) {
        let rel = link.attr("rel").unwrap_or_default().to_ascii_lowercase();
        if !rel.split_whitespace().any(|r| r == "alternate") {
            continue;
        }
        let kind = link.attr("type").unwrap_or_default().to_ascii_lowercase();
        let is_feed = kind.contains("rss")
            || kind.contains("atom")
            || kind == "application/feed+json"
            || kind == "application/json";
        if !is_feed {
            continue;
        }
        let Some(url) = link.attr("href").and_then(|h| base.join(h.trim()).ok()) else {
            continue;
        };
        if !matches!(url.scheme(), "http" | "https") {
            continue;
        }
        let url = url.to_string();
        if out.iter().any(|c| c.url == url) {
            continue;
        }
        let title = link
            .attr("title")
            .map(collapse_whitespace)
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| page_title.clone());
        out.push(Candidate { url, title });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_declared_feeds() {
        let html = r#"<!doctype html><html><head>
            <title> My   Blog </title>
            <link rel="stylesheet" href="/style.css">
            <link rel="alternate" type="application/rss+xml" title="Posts" href="/feed.xml">
            <link rel="ALTERNATE home" type="application/atom+xml" href="atom.xml">
            <link rel="alternate" type="application/rss+xml" title="Posts again" href="/feed.xml">
            <link rel="alternate" hreflang="de" href="/de/">
            <link rel="alternate" type="application/rss+xml" href="javascript:alert(1)">
        </head><body></body></html>"#;
        let base = Url::parse("https://example.com/blog/").unwrap();
        let found = links_in_html(html, &base);
        assert_eq!(
            found,
            vec![
                Candidate {
                    url: "https://example.com/feed.xml".into(),
                    title: "Posts".into()
                },
                Candidate {
                    url: "https://example.com/blog/atom.xml".into(),
                    title: "My Blog".into()
                },
            ]
        );
    }

    #[test]
    fn page_without_feeds() {
        let base = Url::parse("https://example.com/").unwrap();
        assert!(links_in_html("<html><head><title>x</title></head></html>", &base).is_empty());
    }
}
