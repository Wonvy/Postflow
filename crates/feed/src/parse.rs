use feed_rs::model::{Entry, Link, Text};

use crate::sanitize::html_to_text;
use crate::{Error, Result};

/// 列表里摘要的最大长度（字符数）。
const SUMMARY_CHARS: usize = 280;

#[derive(Debug, Clone, Default)]
pub struct ParsedFeed {
    pub title: String,
    pub site_url: Option<String>,
    pub icon_url: Option<String>,
    pub entries: Vec<ParsedEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct ParsedEntry {
    /// 条目在源内的稳定标识。两台设备解析同一个源必须得到相同的值。
    pub guid: String,
    pub url: Option<String>,
    pub title: String,
    pub author: Option<String>,
    pub published_ms: Option<i64>,
    /// 纯文本摘要。
    pub summary: String,
    /// 原始 HTML 正文，未净化。
    pub content: String,
}

/// 解析 RSS、Atom 或 JSON Feed。`base_url` 用来把相对链接补全。
pub fn parse_feed(bytes: &[u8], base_url: &str) -> Result<ParsedFeed> {
    let parser = feed_rs::parser::Builder::new()
        .base_uri(Some(base_url))
        .sanitize_content(false)
        // 源没给 guid 时用链接当标识。连链接都没有就留空，后面用内容算一个。
        // 不能用库默认的随机值，否则每次抓取都会被当成新文章。
        .id_generator(|links, _title, _uri| {
            links.first().map(|l| l.href.clone()).unwrap_or_default()
        })
        .build();
    let feed = parser
        .parse(bytes)
        .map_err(|e| Error::NotAFeed(e.to_string()))?;

    let site_url = pick_link(&feed.links).filter(|u| u != base_url);
    Ok(ParsedFeed {
        title: feed.title.as_ref().map(plain).unwrap_or_default(),
        site_url,
        icon_url: feed
            .icon
            .or(feed.logo)
            .map(|i| i.uri)
            .filter(|u| !u.is_empty()),
        entries: feed.entries.into_iter().map(convert_entry).collect(),
    })
}

fn convert_entry(entry: Entry) -> ParsedEntry {
    let url = pick_link(&entry.links);
    let title = entry.title.as_ref().map(plain).unwrap_or_default();

    let summary_html = entry.summary.as_ref().map(as_html).unwrap_or_default();
    let mut content = entry
        .content
        .as_ref()
        .and_then(|c| {
            let body = c.body.as_deref()?.trim();
            if body.is_empty() {
                return None;
            }
            Some(if c.content_type.to_string().contains("html") {
                body.to_string()
            } else {
                text_to_html(body)
            })
        })
        .unwrap_or_default();
    if content.is_empty() {
        content = summary_html.clone();
    }
    // 视频和播客源通常没有正文，只有媒体描述和封面
    if content.is_empty() {
        content = media_as_html(&entry);
    }

    let summary_source = if summary_html.is_empty() {
        &content
    } else {
        &summary_html
    };
    let summary = truncate(&html_to_text(summary_source), SUMMARY_CHARS);

    let author = entry
        .authors
        .iter()
        .filter_map(|p| p.name.as_deref())
        .map(str::trim)
        .find(|n| !n.is_empty())
        .map(str::to_string);

    let published_ms = entry
        .published
        .or(entry.updated)
        .map(|d| d.timestamp_millis());

    let mut guid = entry.id.trim().to_string();
    if guid.is_empty() {
        guid = format!("{title}\u{0}{}\u{0}{}", published_ms.unwrap_or(0), summary);
    }

    ParsedEntry {
        guid,
        url,
        title,
        author,
        published_ms,
        summary,
        content,
    }
}

/// 优先取指向网页的链接，跳过指向源自身和附件的。
fn pick_link(links: &[Link]) -> Option<String> {
    links
        .iter()
        .find(|l| matches!(l.rel.as_deref(), None | Some("alternate")))
        .or_else(|| {
            links
                .iter()
                .find(|l| !matches!(l.rel.as_deref(), Some("self" | "enclosure")))
        })
        .map(|l| l.href.trim().to_string())
        .filter(|h| !h.is_empty())
}

fn is_html(text: &Text) -> bool {
    text.content_type.to_string().contains("html")
}

fn plain(text: &Text) -> String {
    // 标题里即使声明为纯文本也常常混着实体和标签，统一按 HTML 处理最稳妥
    html_to_text(&text.content)
}

fn as_html(text: &Text) -> String {
    let body = text.content.trim();
    if body.is_empty() {
        String::new()
    } else if is_html(text) || looks_like_html(body) {
        body.to_string()
    } else {
        text_to_html(body)
    }
}

fn looks_like_html(s: &str) -> bool {
    s.contains("</") || s.contains("/>") || s.contains("<p>") || s.contains("<br")
}

fn text_to_html(text: &str) -> String {
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    escaped
        .split("\n\n")
        .map(|p| format!("<p>{}</p>", p.trim().replace('\n', "<br>")))
        .collect()
}

fn media_as_html(entry: &Entry) -> String {
    let mut html = String::new();
    for media in &entry.media {
        if let Some(thumb) = media.thumbnails.first() {
            html.push_str(&format!(
                "<p><img src=\"{}\"></p>",
                thumb.image.uri.replace('"', "&quot;")
            ));
        }
        if let Some(desc) = &media.description {
            html.push_str(&as_html(desc));
        }
    }
    html
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.truncate(out.trim_end().len());
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:content="http://purl.org/rss/1.0/modules/content/" xmlns:dc="http://purl.org/dc/elements/1.1/">
<channel>
  <title>Example &amp; Co</title>
  <link>https://example.com/</link>
  <item>
    <title>First &lt;b&gt;post&lt;/b&gt;</title>
    <link>https://example.com/first</link>
    <guid>urn:first</guid>
    <dc:creator>Ada</dc:creator>
    <pubDate>Mon, 16 Nov 2026 08:00:00 GMT</pubDate>
    <description>Short &lt;em&gt;summary&lt;/em&gt;</description>
    <content:encoded><![CDATA[<p>Full <a href="/more">content</a></p>]]></content:encoded>
  </item>
  <item>
    <title>No guid</title>
    <link>https://example.com/second</link>
    <description>Only a description</description>
  </item>
  <item>
    <title>No guid and no link</title>
    <description>Orphan</description>
  </item>
</channel>
</rss>"#;

    const ATOM: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Atom Example</title>
  <link rel="self" href="https://example.org/atom.xml"/>
  <link rel="alternate" href="https://example.org/"/>
  <icon>https://example.org/icon.png</icon>
  <entry>
    <id>tag:example.org,2026:1</id>
    <title type="html">Hello &amp;amp; welcome</title>
    <link rel="alternate" href="https://example.org/hello"/>
    <updated>2026-11-15T10:00:00Z</updated>
    <author><name>Grace</name></author>
    <content type="html">&lt;p&gt;Body&lt;/p&gt;</content>
  </entry>
</feed>"#;

    const JSON: &str = r#"{
  "version": "https://jsonfeed.org/version/1.1",
  "title": "JSON Example",
  "home_page_url": "https://example.net/",
  "items": [
    {"id": "1", "url": "https://example.net/1", "title": "One", "content_text": "Line one\n\nLine two", "date_published": "2026-11-14T00:00:00Z"}
  ]
}"#;

    #[test]
    fn rss() {
        let feed = parse_feed(RSS.as_bytes(), "https://example.com/feed.xml").unwrap();
        assert_eq!(feed.title, "Example & Co");
        assert_eq!(feed.site_url.as_deref(), Some("https://example.com/"));
        assert_eq!(feed.entries.len(), 3);

        let first = &feed.entries[0];
        assert_eq!(first.guid, "urn:first");
        assert_eq!(first.title, "First post");
        assert_eq!(first.url.as_deref(), Some("https://example.com/first"));
        assert_eq!(first.author.as_deref(), Some("Ada"));
        assert_eq!(first.published_ms, Some(1_794_816_000_000));
        assert_eq!(first.summary, "Short summary");
        assert!(first.content.contains("<a href=\"/more\">content</a>"));

        // 没有 guid 时用链接
        assert_eq!(feed.entries[1].guid, "https://example.com/second");
        assert_eq!(feed.entries[1].content, "Only a description");
    }

    #[test]
    fn entries_without_guid_or_link_get_a_stable_id() {
        let a = parse_feed(RSS.as_bytes(), "https://example.com/feed.xml").unwrap();
        let b = parse_feed(RSS.as_bytes(), "https://example.com/feed.xml").unwrap();
        assert!(!a.entries[2].guid.is_empty());
        assert_eq!(a.entries[2].guid, b.entries[2].guid);
        assert_ne!(a.entries[2].guid, a.entries[1].guid);
    }

    #[test]
    fn atom() {
        let feed = parse_feed(ATOM.as_bytes(), "https://example.org/atom.xml").unwrap();
        assert_eq!(feed.title, "Atom Example");
        assert_eq!(feed.site_url.as_deref(), Some("https://example.org/"));
        assert_eq!(
            feed.icon_url.as_deref(),
            Some("https://example.org/icon.png")
        );
        let entry = &feed.entries[0];
        assert_eq!(entry.guid, "tag:example.org,2026:1");
        assert_eq!(entry.title, "Hello & welcome");
        assert_eq!(entry.author.as_deref(), Some("Grace"));
        assert_eq!(entry.content, "<p>Body</p>");
        assert_eq!(entry.summary, "Body");
    }

    #[test]
    fn json_feed() {
        let feed = parse_feed(JSON.as_bytes(), "https://example.net/feed.json").unwrap();
        assert_eq!(feed.title, "JSON Example");
        let entry = &feed.entries[0];
        assert_eq!(entry.guid, "1");
        assert_eq!(entry.content, "<p>Line one</p><p>Line two</p>");
    }

    #[test]
    fn html_page_is_not_a_feed() {
        let err = parse_feed(
            b"<!doctype html><html><body>hi</body></html>",
            "https://x.test/",
        );
        assert!(matches!(err, Err(Error::NotAFeed(_))));
    }

    #[test]
    fn truncation_is_char_safe() {
        let s = "汉".repeat(300);
        let t = truncate(&s, 280);
        assert_eq!(t.chars().count(), 281);
        assert!(t.ends_with('…'));
    }
}
