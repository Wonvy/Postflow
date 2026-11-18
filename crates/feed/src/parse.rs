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
    /// 源自己的一句话介绍，纯文本。
    pub description: Option<String>,
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
    /// 源里明确给出的封面图：媒体缩略图或图片附件。
    pub image: Option<String>,
    /// 播客、视频的音频或视频附件。
    pub media: Option<ParsedMedia>,
}

/// 条目附带的音频或视频（RSS 的 `<enclosure>`、Media RSS、Atom 的 `rel="enclosure"`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMedia {
    pub url: String,
    /// MIME 类型，比如 `audio/mpeg`。源里没写时按文件扩展名猜。
    pub mime: String,
    /// 时长，秒。
    pub duration: Option<u64>,
    /// 文件大小，字节。
    pub size: Option<u64>,
    /// 章节（Podcasting 2.0 的 `<podcast:chapters>`，JSON 文件地址）。
    pub chapters: Option<String>,
    /// 文字稿（`<podcast:transcript>`）：地址和类型。有好几种格式时优先 VTT、SRT，再是 JSON、HTML、纯文本。
    pub transcript: Option<(String, String)>,
}

impl ParsedMedia {
    pub fn is_audio(&self) -> bool {
        self.mime.starts_with("audio/")
    }
}

/// 解析 RSS、Atom 或 JSON Feed。`base_url` 用来把相对链接补全。
pub fn parse_feed(bytes: &[u8], base_url: &str) -> Result<ParsedFeed> {
    let feed = match parse_raw(bytes, base_url) {
        Ok(feed) => feed,
        // 解析不了时，去掉 feed-rs 不认的空元素再试一次；还不行就报第一次的错
        Err(first) => match drop_empty_media_text(bytes) {
            Some(fixed) => parse_raw(&fixed, base_url).map_err(|_| first)?,
            None => return Err(first),
        },
    };

    let site_url = pick_link(&feed.links).filter(|u| u != base_url);
    let mut entries: Vec<ParsedEntry> = feed.entries.into_iter().map(convert_entry).collect();
    // feed-rs 只在有 media:content 时才保留缩略图。直接挂在条目下的 media:thumbnail
    // （新闻网站常这样写）从原文里补上；条目数对得上才用，免得张冠李戴
    let xml = String::from_utf8_lossy(bytes);
    let thumbs = standalone_thumbnails(&xml);
    if thumbs.len() == entries.len() {
        for (entry, thumb) in entries.iter_mut().zip(thumbs) {
            if entry.image.is_none() {
                entry.image = thumb;
            }
        }
    }
    // 播客单集的时长、章节、文字稿从原文补
    let blocks = entry_blocks(&xml);
    if blocks.len() == entries.len() {
        for (entry, block) in entries.iter_mut().zip(blocks) {
            if let Some(media) = entry.media.as_mut() {
                if media.duration.is_none() {
                    media.duration = itunes_duration(block);
                }
                media.chapters = podcast_chapters(block);
                media.transcript = podcast_transcript(block);
            }
        }
    }
    Ok(ParsedFeed {
        title: feed.title.as_ref().map(plain).unwrap_or_default(),
        site_url,
        icon_url: feed
            .icon
            .or(feed.logo)
            .map(|i| i.uri)
            .filter(|u| !u.is_empty()),
        description: feed
            .description
            .as_ref()
            .map(|d| truncate(&html_to_text(&as_html(d)), SUMMARY_CHARS))
            .filter(|d| !d.is_empty()),
        entries,
    })
}

fn parse_raw(bytes: &[u8], base_url: &str) -> Result<feed_rs::model::Feed> {
    let parser = feed_rs::parser::Builder::new()
        .base_uri(Some(base_url))
        .sanitize_content(false)
        // 源没给 guid 时用链接当标识。连链接都没有就留空，后面用内容算一个。
        // 不能用库默认的随机值，否则每次抓取都会被当成新文章。
        .id_generator(|links, _title, _uri| {
            links.first().map(|l| l.href.clone()).unwrap_or_default()
        })
        .build();
    parser
        .parse(bytes)
        .map_err(|e| Error::NotAFeed(e.to_string()))
}

/// feed-rs 遇到空的 `<media:title>` 或 `<media:description>` 会让整个源解析失败，
/// 比如 YouTube 频道里只要有一个视频没写简介，整个频道就订阅不了。把这些空元素去掉。
/// 没有可去的就返回 `None`。
fn drop_empty_media_text(bytes: &[u8]) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut changed = false;
    while let Some(at) = rest.find("<media:") {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        if let Some(len) = empty_media_text(tail) {
            changed = true;
            rest = &tail[len..];
        } else {
            out.push('<');
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    changed.then(|| out.into_bytes())
}

/// `tail` 以一个空的 `media:title` 或 `media:description` 开头时，返回这个元素的长度。
fn empty_media_text(tail: &str) -> Option<usize> {
    let name_end = tail[1..].find(|c: char| c.is_whitespace() || c == '>' || c == '/')? + 1;
    let name = &tail[1..name_end];
    if name != "media:title" && name != "media:description" {
        return None;
    }
    let open_end = tail.find('>')? + 1;
    if tail[..open_end].ends_with("/>") {
        return Some(open_end);
    }
    let close = format!("</{name}>");
    let inner_len = tail[open_end..].find(&close)?;
    let inner = tail[open_end..open_end + inner_len].trim();
    let empty = inner.is_empty()
        || inner
            .strip_prefix("<![CDATA[")
            .and_then(|s| s.strip_suffix("]]>"))
            .is_some_and(|s| s.trim().is_empty());
    empty.then_some(open_end + inner_len + close.len())
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

    let image = media_image(&entry);
    let media = media_attachment(&entry);
    ParsedEntry {
        guid,
        url,
        title,
        author,
        published_ms,
        summary,
        content,
        image,
        media,
    }
}

/// 按顺序列出每个条目（`<item>` 或 `<entry>`）里第一个 `media:thumbnail` 的地址。
fn standalone_thumbnails(xml: &str) -> Vec<Option<String>> {
    entry_blocks(xml)
        .into_iter()
        .map(|block| {
            block.find("<media:thumbnail").and_then(|i| {
                let tag = &block[i..];
                let tag = &tag[..tag.find('>').unwrap_or(tag.len())];
                let value = tag.split("url=").nth(1)?;
                let quote = value.chars().next().filter(|c| *c == '"' || *c == '\'')?;
                let url = value[1..].split(quote).next()?.replace("&amp;", "&");
                (url.starts_with("https://") || url.starts_with("http://")).then_some(url)
            })
        })
        .collect()
}

/// 原文里每个条目（`<item>` 或 `<entry>`）的内容，按顺序。
fn entry_blocks(xml: &str) -> Vec<&str> {
    let (open, close) = if xml.contains("<item") {
        ("<item", "</item>")
    } else {
        ("<entry", "</entry>")
    };
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(open) {
        let after = &rest[start + open.len()..];
        // `<items>`、`<entryLink>` 之类不算
        if !after.starts_with(['>', ' ', '\n', '\r', '\t']) {
            rest = after;
            continue;
        }
        let end = after.find(close).unwrap_or(after.len());
        out.push(&after[..end]);
        rest = &after[end..];
    }
    out
}

/// 条目的 `<itunes:duration>`，秒。feed-rs 在条目没有别的 iTunes 信息时会把它丢掉，从原文补。
fn itunes_duration(block: &str) -> Option<u64> {
    let start = block.find("<itunes:duration>")? + "<itunes:duration>".len();
    let text = &block[start..];
    parse_duration(&text[..text.find('<')?])
}

/// 一个标签里某个属性的值（`name="..."` 或 `name='...'`），去掉 XML 转义。
fn attr(tag: &str, name: &str) -> Option<String> {
    let mut rest = tag;
    loop {
        let at = rest.find(name)?;
        let before = rest[..at].chars().last();
        let after = &rest[at + name.len()..];
        rest = after;
        if !matches!(before, Some(c) if c.is_whitespace()) {
            continue;
        }
        let after = after.trim_start().strip_prefix('=')?.trim_start();
        let quote = after.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let value = after[1..].split(quote).next()?;
        return Some(
            value
                .replace("&amp;", "&")
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\"")
                .trim()
                .to_string(),
        );
    }
}

/// 条目里所有某个名字的标签（只要开头那一段）。
fn tags<'a>(block: &'a str, name: &str) -> Vec<&'a str> {
    let open = format!("<{name}");
    let mut out = Vec::new();
    let mut rest = block;
    while let Some(at) = rest.find(&open) {
        let tail = &rest[at..];
        let end = tail.find('>').unwrap_or(tail.len());
        if tail[open.len()..].starts_with([' ', '\n', '\r', '\t', '/', '>']) {
            out.push(&tail[..end]);
        }
        rest = &tail[end.min(tail.len())..];
        if rest.len() == tail.len() {
            break;
        }
    }
    out
}

fn web_url(url: String) -> Option<String> {
    (url.starts_with("https://") || url.starts_with("http://")).then_some(url)
}

/// `<podcast:chapters url="..." type="application/json+chapters"/>`
fn podcast_chapters(block: &str) -> Option<String> {
    tags(block, "podcast:chapters")
        .into_iter()
        .find_map(|tag| attr(tag, "url").and_then(web_url))
}

/// `<podcast:transcript url="..." type="text/vtt"/>`，有好几个时挑能按时间对上的格式。
fn podcast_transcript(block: &str) -> Option<(String, String)> {
    let rank = |mime: &str| match mime {
        "text/vtt" => 0,
        "application/x-subrip" | "application/srt" | "text/srt" => 1,
        "application/json" => 2,
        "text/html" => 3,
        "text/plain" => 4,
        _ => 9,
    };
    tags(block, "podcast:transcript")
        .into_iter()
        .filter_map(|tag| {
            let url = attr(tag, "url").and_then(web_url)?;
            let mime = attr(tag, "type").unwrap_or_default().to_ascii_lowercase();
            (rank(&mime) < 9).then_some((url, mime))
        })
        .min_by_key(|(_, mime)| rank(mime))
}

/// `1:02:03`、`62:03` 或者秒数。
fn parse_duration(text: &str) -> Option<u64> {
    let mut total = 0u64;
    let parts: Vec<&str> = text.trim().split(':').collect();
    if parts.len() > 3 {
        return None;
    }
    for part in parts {
        let n: f64 = part.trim().parse().ok()?;
        if !n.is_finite() || n < 0.0 {
            return None;
        }
        total = total * 60 + n as u64;
    }
    (total > 0).then_some(total)
}

/// 按文件扩展名猜音频、视频的 MIME 类型（有些源的附件不写 type）。
fn mime_from_url(url: &str) -> Option<&'static str> {
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase();
    let ext = path.rsplit('.').next()?;
    Some(match ext {
        "mp3" => "audio/mpeg",
        "m4a" | "m4b" => "audio/mp4",
        "aac" => "audio/aac",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        _ => return None,
    })
}

/// 音频或视频的 MIME 类型；别的类型（图片、网页、Flash）不算。
fn playable_mime(declared: Option<&str>, url: &str) -> Option<String> {
    let declared = declared
        .map(|t| t.trim().to_ascii_lowercase())
        .filter(|t| !t.is_empty());
    match declared {
        Some(t) if t.starts_with("audio/") || t.starts_with("video/") => Some(t),
        // 写成 application/octet-stream 之类的，按扩展名再看一眼
        Some(t) if t.starts_with("application/octet-stream") || t == "binary/octet-stream" => {
            mime_from_url(url).map(str::to_string)
        }
        Some(_) => None,
        None => mime_from_url(url).map(str::to_string),
    }
}

/// 条目的音频或视频附件：先看媒体内容（RSS 的 enclosure 也在这里），再看 Atom 的附件链接。
/// 时长取附件自己的，没有再取 iTunes 的 `<itunes:duration>`。
fn media_attachment(entry: &Entry) -> Option<ParsedMedia> {
    let web = |u: &str| u.starts_with("https://") || u.starts_with("http://");
    let any_duration = entry
        .media
        .iter()
        .find_map(|m| m.duration)
        .map(|d| d.as_secs());
    for media in &entry.media {
        for c in &media.content {
            let Some(url) = c.url.as_ref().map(|u| u.to_string()).filter(|u| web(u)) else {
                continue;
            };
            let declared = c.content_type.as_ref().map(|t| t.to_string());
            if let Some(mime) = playable_mime(declared.as_deref(), &url) {
                return Some(ParsedMedia {
                    url,
                    mime,
                    duration: c
                        .duration
                        .or(media.duration)
                        .map(|d| d.as_secs())
                        .or(any_duration)
                        .filter(|s| *s > 0),
                    size: c.size.filter(|s| *s > 0),
                    chapters: None,
                    transcript: None,
                });
            }
        }
    }
    entry.links.iter().find_map(|l| {
        if l.rel.as_deref() != Some("enclosure") || !web(&l.href) {
            return None;
        }
        let mime = playable_mime(l.media_type.as_deref(), &l.href)?;
        Some(ParsedMedia {
            url: l.href.clone(),
            mime,
            duration: any_duration.filter(|s| *s > 0),
            size: l.length.filter(|s| *s > 0),
            chapters: None,
            transcript: None,
        })
    })
}

/// 媒体缩略图、图片类型的媒体内容，或者图片附件。
fn media_image(entry: &Entry) -> Option<String> {
    let web = |u: &str| u.starts_with("https://") || u.starts_with("http://");
    for media in &entry.media {
        if let Some(thumb) = media.thumbnails.iter().find(|t| web(&t.image.uri)) {
            return Some(thumb.image.uri.clone());
        }
        let picture = media.content.iter().find(|c| {
            c.content_type
                .as_ref()
                .is_some_and(|t| t.to_string().starts_with("image/"))
        });
        if let Some(url) = picture.and_then(|c| c.url.as_ref()) {
            return Some(url.to_string());
        }
    }
    entry
        .links
        .iter()
        .find(|l| {
            l.rel.as_deref() == Some("enclosure")
                && l.media_type
                    .as_deref()
                    .is_some_and(|t| t.starts_with("image/"))
                && web(&l.href)
        })
        .map(|l| l.href.clone())
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

pub(crate) fn text_to_html(text: &str) -> String {
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

pub(crate) fn truncate(s: &str, max_chars: usize) -> String {
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
    fn cover_images_from_media_and_enclosures() {
        let rss = r#"<?xml version="1.0"?>
<rss version="2.0" xmlns:media="http://search.yahoo.com/mrss/">
<channel><title>Pics</title><link>https://pics.example/</link>
<item><guid>1</guid><title>Thumb</title>
  <media:thumbnail url="https://pics.example/thumb.jpg"/></item>
<item><guid>2</guid><title>Enclosure</title>
  <enclosure url="https://pics.example/photo.png" type="image/png" length="1"/></item>
<item><guid>3</guid><title>Podcast</title>
  <enclosure url="https://pics.example/episode.mp3" type="audio/mpeg" length="1"/></item>
</channel></rss>"#;
        let feed = parse_feed(rss.as_bytes(), "https://pics.example/feed").unwrap();
        let images: Vec<Option<&str>> = feed.entries.iter().map(|e| e.image.as_deref()).collect();
        assert_eq!(
            images,
            vec![
                Some("https://pics.example/thumb.jpg"),
                Some("https://pics.example/photo.png"),
                None
            ]
        );
    }

    #[test]
    fn video_without_a_description_does_not_break_the_feed() {
        // YouTube 频道源的样子：第一个视频没写简介，第二个的标题也是空的
        let atom = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns:yt="http://www.youtube.com/xml/schemas/2015" xmlns:media="http://search.yahoo.com/mrss/" xmlns="http://www.w3.org/2005/Atom">
 <title>A Channel</title>
 <link rel="alternate" href="https://www.youtube.com/channel/UCxyz"/>
 <entry>
  <id>yt:video:aaa</id>
  <title>First video</title>
  <link rel="alternate" href="https://www.youtube.com/watch?v=aaa"/>
  <published>2026-11-15T10:00:00+00:00</published>
  <media:group>
   <media:title>First video</media:title>
   <media:content url="https://www.youtube.com/v/aaa?version=3" type="application/x-shockwave-flash" width="640" height="390"/>
   <media:thumbnail url="https://i1.ytimg.com/vi/aaa/hqdefault.jpg" width="480" height="360"/>
   <media:description></media:description>
  </media:group>
 </entry>
 <entry>
  <id>yt:video:bbb</id>
  <title>Second video</title>
  <link rel="alternate" href="https://www.youtube.com/watch?v=bbb"/>
  <published>2026-11-14T10:00:00+00:00</published>
  <media:group>
   <media:title/>
   <media:content url="https://www.youtube.com/v/bbb?version=3" type="application/x-shockwave-flash" width="640" height="390"/>
   <media:thumbnail url="https://i1.ytimg.com/vi/bbb/hqdefault.jpg" width="480" height="360"/>
   <media:description>Has a description</media:description>
  </media:group>
 </entry>
</feed>"#;
        let feed = parse_feed(atom.as_bytes(), "https://www.youtube.com/feeds/videos.xml").unwrap();
        assert_eq!(feed.title, "A Channel");
        let titles: Vec<&str> = feed.entries.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, vec!["First video", "Second video"]);
        assert_eq!(
            feed.entries[0].image.as_deref(),
            Some("https://i1.ytimg.com/vi/aaa/hqdefault.jpg")
        );
        assert!(feed.entries[1].content.contains("Has a description"));
    }

    #[test]
    fn only_empty_media_text_is_dropped() {
        let xml = "<a><media:description> </media:description><media:title><![CDATA[]]></media:title>\
<media:descriptions>x</media:descriptions><media:description>keep</media:description><media:title/></a>";
        let fixed = String::from_utf8(drop_empty_media_text(xml.as_bytes()).unwrap()).unwrap();
        assert_eq!(
            fixed,
            "<a><media:descriptions>x</media:descriptions><media:description>keep</media:description></a>"
        );
        assert!(drop_empty_media_text(b"<a><media:title>x</media:title></a>").is_none());
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

    #[test]
    fn podcast_enclosures() {
        let rss = r#"<?xml version="1.0"?>
<rss version="2.0" xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd">
<channel><title>Show</title><link>https://show.example.com/</link>
<item><title>Ep 2</title><guid>ep2</guid>
  <enclosure url="https://cdn.example.com/ep2.mp3?x=1" length="31415926" type="audio/mpeg"/>
  <itunes:duration>1:02:03</itunes:duration></item>
<item><title>Ep 1</title><guid>ep1</guid>
  <enclosure url="https://cdn.example.com/ep1.m4a" length="0"/></item>
<item><title>Cover only</title><guid>img</guid>
  <enclosure url="https://cdn.example.com/cover.jpg" type="image/jpeg"/></item>
</channel></rss>"#;
        let feed = parse_feed(rss.as_bytes(), "https://show.example.com/feed").unwrap();
        let ep2 = feed.entries[0].media.clone().unwrap();
        assert_eq!(ep2.url, "https://cdn.example.com/ep2.mp3?x=1");
        assert_eq!(ep2.mime, "audio/mpeg");
        assert_eq!(ep2.duration, Some(3723));
        assert_eq!(ep2.size, Some(31415926));
        assert!(ep2.is_audio());
        // 没写类型的按扩展名猜，大小 0 当作不知道
        let ep1 = feed.entries[1].media.clone().unwrap();
        assert_eq!(ep1.mime, "audio/mp4");
        assert_eq!(ep1.size, None);
        // 图片附件不是音视频
        assert_eq!(feed.entries[2].media, None);
    }

    #[test]
    fn atom_video_enclosure() {
        let atom = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom"><title>Clips</title><id>c</id><updated>2026-01-01T00:00:00Z</updated>
<entry><title>Clip</title><id>c1</id><updated>2026-01-01T00:00:00Z</updated>
  <link rel="alternate" href="https://clips.example.com/1"/>
  <link rel="enclosure" type="video/mp4" length="1234" href="https://clips.example.com/1.mp4"/></entry>
</feed>"#;
        let feed = parse_feed(atom.as_bytes(), "https://clips.example.com/atom").unwrap();
        let media = feed.entries[0].media.clone().unwrap();
        assert_eq!(media.url, "https://clips.example.com/1.mp4");
        assert_eq!(media.mime, "video/mp4");
        assert!(!media.is_audio());
        assert_eq!(
            feed.entries[0].url.as_deref(),
            Some("https://clips.example.com/1")
        );
    }

    #[test]
    fn podcast_chapters_and_transcripts() {
        let rss = r#"<?xml version="1.0"?>
<rss version="2.0" xmlns:podcast="https://podcastindex.org/namespace/1.0">
<channel><title>Show</title><link>https://show.example.com/</link>
<item><title>Ep 1</title><guid>ep1</guid>
  <enclosure url="https://cdn.example.com/1.mp3" type="audio/mpeg" length="10"/>
  <podcast:chapters url="https://show.example.com/1/chapters.json" type="application/json+chapters"/>
  <podcast:transcript url="https://show.example.com/1.html" type="text/html"/>
  <podcast:transcript url="https://show.example.com/1.vtt?a=1&amp;b=2" type="text/vtt" language="en"/>
</item>
<item><title>Ep 0</title><guid>ep0</guid>
  <enclosure url="https://cdn.example.com/0.mp3" type="audio/mpeg" length="10"/>
</item>
</channel></rss>"#;
        let feed = parse_feed(rss.as_bytes(), "https://show.example.com/feed").unwrap();
        let ep1 = feed.entries[0].media.clone().unwrap();
        assert_eq!(
            ep1.chapters.as_deref(),
            Some("https://show.example.com/1/chapters.json")
        );
        assert_eq!(
            ep1.transcript,
            Some((
                "https://show.example.com/1.vtt?a=1&b=2".into(),
                "text/vtt".into()
            ))
        );
        let ep0 = feed.entries[1].media.clone().unwrap();
        assert_eq!((ep0.chapters, ep0.transcript), (None, None));
    }
}
