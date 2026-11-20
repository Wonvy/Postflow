//! Telegram 公开频道：频道没有订阅源，但 `https://t.me/s/频道名` 是一个不用登录就能看的网页，
//! 列着最近的二十来条消息。这里把那个网页读成订阅源：每条消息一篇，文字、图片、视频、链接预览都带上。
//!
//! 订阅地址就是这个网页的地址，所以抓取、预览、刷新都和普通订阅源一样走，只在解析时换成这里。
//! 只读公开网页，不登录、不伪装浏览器。私有频道和群组看不到。

use scraper::{ElementRef, Html, Selector};
use url::Url;

use crate::parse::{ParsedEntry, ParsedFeed, truncate};
use crate::sanitize::{collapse_whitespace, html_to_text};
use crate::{Error, Result};

const SUMMARY_CHARS: usize = 280;
/// 标题取消息的第一行，太长就截断。
const TITLE_CHARS: usize = 80;

fn is_telegram_host(url: &Url) -> bool {
    matches!(
        url.host_str().map(|h| h.trim_start_matches("www.")),
        Some("t.me" | "telegram.me")
    )
}

/// 频道名：5 到 32 个字母、数字、下划线（老频道有 4 个字符的，也放过）。
fn is_channel_name(name: &str) -> bool {
    (4..=32).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// 用户贴的频道地址（`t.me/频道`、`t.me/频道/123`、`t.me/s/频道`）换成公开网页的地址。
/// 不是频道地址返回 `None`。
pub(crate) fn channel_page(url: &Url) -> Option<String> {
    if !is_telegram_host(url) {
        return None;
    }
    let mut parts = url.path_segments()?.filter(|s| !s.is_empty());
    let first = parts.next()?;
    let name = if first == "s" { parts.next()? } else { first };
    // t.me/joinchat/…、t.me/+邀请码 这些是私有的，没有公开网页
    if matches!(
        name,
        "joinchat" | "addstickers" | "share" | "proxy" | "socks"
    ) || !is_channel_name(name)
    {
        return None;
    }
    Some(format!("https://t.me/s/{name}"))
}

/// 这个地址是不是频道的公开网页（订阅地址），是的话解析时用这里。
pub(crate) fn is_channel_page(url: &str) -> bool {
    Url::parse(url).ok().is_some_and(|u| {
        is_telegram_host(&u)
            && channel_page(&u).is_some_and(|page| page.trim_end_matches('/') == strip_query(&u))
    })
}

fn strip_query(url: &Url) -> String {
    let mut u = url.clone();
    u.set_query(None);
    u.set_fragment(None);
    let s = u.to_string();
    s.trim_end_matches('/')
        .replacen("://www.", "://", 1)
        .replacen("telegram.me", "t.me", 1)
}

fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("valid selector")
}

fn meta(doc: &Html, property: &str) -> Option<String> {
    let s = Selector::parse(&format!(r#"meta[property="{property}"]"#)).ok()?;
    doc.select(&s)
        .find_map(|m| m.value().attr("content"))
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

/// `style="background-image:url('…')"` 里的地址。
fn background_url(el: ElementRef) -> Option<String> {
    let style = el.value().attr("style")?;
    let start = style.find("url(")? + 4;
    let rest = &style[start..];
    let end = rest.find(')')?;
    let url = rest[..end].trim().trim_matches(['\'', '"']);
    (url.starts_with("https://") || url.starts_with("http://")).then(|| url.to_string())
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// `2026-05-14T16:08:31+00:00` 换成毫秒时间戳。
pub(crate) fn parse_time(s: &str) -> Option<i64> {
    let s = s.trim();
    let num = |a: usize, b: usize| s.get(a..b)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi, sec) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    // 时区：Z 或 ±hh:mm，在秒后面（可能先有小数）
    let tail = &s[19..];
    let tz = tail
        .find(['Z', '+', '-'])
        .map(|i| &tail[i..])
        .unwrap_or("Z");
    let offset = if tz.starts_with('Z') {
        0
    } else {
        let sign = if tz.starts_with('-') { -1 } else { 1 };
        let oh = tz.get(1..3)?.parse::<i64>().ok()?;
        let om = tz
            .get(4..6)
            .and_then(|m| m.parse::<i64>().ok())
            .unwrap_or(0);
        sign * (oh * 3600 + om * 60)
    };
    // 公历日期换成 1970-01-01 以来的天数（Howard Hinnant 的算法）
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some((days * 86_400 + h * 3600 + mi * 60 + sec - offset) * 1000)
}

/// 一条消息。
fn entry(msg: ElementRef) -> Option<ParsedEntry> {
    let post = msg.value().attr("data-post")?; // 频道/编号
    let link = format!("https://t.me/{post}");

    let text_el = msg
        .select(&sel(".tgme_widget_message_text.js-message_text"))
        .next();
    let text_html = text_el.map(|e| e.inner_html()).unwrap_or_default();
    let text = text_el
        .map(|e| html_to_text(&e.inner_html()))
        .unwrap_or_default();

    let mut content = String::new();
    let mut image = None;
    // 图片（一组图片时有好几张）
    for photo in msg.select(&sel(".tgme_widget_message_photo_wrap")) {
        if let Some(src) = background_url(photo) {
            image.get_or_insert_with(|| src.clone());
            content.push_str(&format!(
                r#"<p><img src="{}" alt=""></p>"#,
                escape_attr(&src)
            ));
        }
    }
    // 视频：Telegram 的视频地址带有时效的口令，过一阵就失效，所以不当附件，只放封面，点封面看原消息
    if msg.select(&sel("video")).next().is_some()
        && let Some(poster) = msg
            .select(&sel(".tgme_widget_message_video_thumb"))
            .next()
            .and_then(background_url)
    {
        image.get_or_insert_with(|| poster.clone());
        content.push_str(&format!(
            r#"<p><a href="{}"><img src="{}" alt=""></a></p>"#,
            escape_attr(&link),
            escape_attr(&poster)
        ));
    }
    if !text_html.is_empty() {
        content.push_str(&format!("<p>{text_html}</p>"));
    }
    // 链接预览
    if let Some(preview) = msg
        .select(&sel("a.tgme_widget_message_link_preview"))
        .next()
    {
        let href = preview.value().attr("href").unwrap_or_default();
        let title = preview
            .select(&sel(".link_preview_title"))
            .next()
            .map(|e| collapse_whitespace(&e.text().collect::<String>()))
            .unwrap_or_default();
        let desc = preview
            .select(&sel(".link_preview_description"))
            .next()
            .map(|e| html_to_text(&e.inner_html()))
            .unwrap_or_default();
        let pic = preview
            .select(&sel(".link_preview_image, .link_preview_right_image"))
            .next()
            .and_then(background_url);
        if href.starts_with("http") && (!title.is_empty() || !desc.is_empty()) {
            content.push_str("<blockquote>");
            if let Some(p) = &pic {
                image.get_or_insert_with(|| p.clone());
                content.push_str(&format!(r#"<p><img src="{}" alt=""></p>"#, escape_attr(p)));
            }
            content.push_str(&format!(
                r#"<p><a href="{}">{}</a></p>"#,
                escape_attr(href),
                escape_text(if title.is_empty() { href } else { &title })
            ));
            if !desc.is_empty() {
                content.push_str(&format!("<p>{}</p>", escape_text(&desc)));
            }
            content.push_str("</blockquote>");
        }
    }

    let published_ms = msg
        .select(&sel(".tgme_widget_message_date time"))
        .next()
        .and_then(|t| t.value().attr("datetime"))
        .and_then(parse_time);
    let author = msg
        .select(&sel(".tgme_widget_message_from_author"))
        .next()
        .map(|e| collapse_whitespace(&e.text().collect::<String>()))
        .filter(|a| !a.is_empty());

    // 标题：第一行文字；只有图片或视频的消息没有标题，列表里显示「无标题」
    let first_line = text_el
        .map(|e| {
            let html = e.inner_html();
            let line = html.split("<br").next().unwrap_or_default().to_string();
            html_to_text(&line)
        })
        .unwrap_or_default();
    let title = truncate(&first_line, TITLE_CHARS);
    let summary = truncate(&text, SUMMARY_CHARS);
    // 什么内容都读不出来的（投票、贴纸这些网页上不显示的）：放一个指向原消息的链接
    if content.is_empty() {
        content = format!(
            r#"<p><a href="{}">{}</a></p>"#,
            escape_attr(&link),
            escape_text(&link)
        );
    }
    Some(ParsedEntry {
        guid: link.clone(),
        url: Some(link),
        title,
        author,
        published_ms,
        summary,
        content,
        image,
        media: None,
    })
}

/// 把频道公开网页读成订阅源。
pub(crate) fn parse(html: &str, page_url: &str) -> Result<ParsedFeed> {
    let doc = Html::parse_document(html);
    let messages: Vec<ElementRef> = doc
        .select(&sel(".tgme_widget_message[data-post]"))
        .collect();
    let name = Url::parse(page_url)
        .ok()
        .and_then(|u| {
            u.path_segments()?
                .find(|s| !s.is_empty() && *s != "s")
                .map(str::to_string)
        })
        .unwrap_or_default();
    // 没有消息列表：频道不存在、是私有的，或者打开的是普通个人主页
    if messages.is_empty() {
        return Err(Error::NotAFeed(format!(
            "t.me/{name} is not a public channel"
        )));
    }
    let channel_url = format!("https://t.me/{name}");
    let title = meta(&doc, "og:title").unwrap_or_else(|| name.clone());
    let entries = messages
        .into_iter()
        .rev() // 网页上最新的在最下面
        .filter_map(entry)
        .collect();
    Ok(ParsedFeed {
        title,
        site_url: Some(channel_url),
        icon_url: meta(&doc, "og:image"),
        description: meta(&doc, "og:description"),
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"<!DOCTYPE html><html><head>
<meta property="og:title" content="Telegram News">
<meta property="og:image" content="https://cdn1.telesco.pe/file/avatar.jpg">
<meta property="og:description" content="The official Telegram on Telegram.">
</head><body>
<div class="tgme_widget_message_wrap js-widget_message_wrap"><div class="tgme_widget_message js-widget_message" data-post="telegram/440">
<div class="tgme_widget_message_author"><a class="tgme_widget_message_owner_name" href="https://t.me/telegram"><span>Telegram News</span></a></div>
<a class="tgme_widget_message_photo_wrap" href="https://t.me/telegram/440" style="width:800px;background-image:url('https://cdn1.telesco.pe/file/photo.jpg')"></a>
<div class="tgme_widget_message_text js-message_text" dir="auto"><b>Stories for channels.</b> Now available.<br/><br/>Read <a href="https://telegram.org/blog" onclick="return confirm('x')">the blog</a>.</div>
<a class="tgme_widget_message_link_preview" href="https://telegram.org/blog/stories"><i class="link_preview_right_image" style="background-image:url('https://cdn1.telesco.pe/file/preview.jpg')"></i><div class="link_preview_title">Stories</div><div class="link_preview_description">All about stories.</div></a>
<span class="tgme_widget_message_meta"><a class="tgme_widget_message_date" href="https://t.me/telegram/440"><time datetime="2026-05-14T16:08:31+00:00" class="time">16:08</time></a></span>
</div></div>
<div class="tgme_widget_message_wrap js-widget_message_wrap"><div class="tgme_widget_message js-widget_message" data-post="telegram/441">
<a class="tgme_widget_message_video_player" href="https://t.me/telegram/441"><i class="tgme_widget_message_video_thumb" style="background-image:url('https://cdn1.telesco.pe/file/thumb.jpg')"></i>
<div class="tgme_widget_message_video_wrap"><video src="https://cdn1.telesco.pe/file/v.mp4?token=abc" class="tgme_widget_message_video"></video></div></a>
<span class="tgme_widget_message_meta"><a class="tgme_widget_message_date" href="https://t.me/telegram/441"><time datetime="2026-05-15T08:00:00+08:00" class="time">08:00</time></a></span>
</div></div>
</body></html>"#;

    #[test]
    fn channel_addresses_become_the_public_page() {
        let page = |s: &str| channel_page(&Url::parse(s).unwrap());
        assert_eq!(
            page("https://t.me/telegram").as_deref(),
            Some("https://t.me/s/telegram")
        );
        assert_eq!(
            page("https://t.me/s/telegram").as_deref(),
            Some("https://t.me/s/telegram")
        );
        assert_eq!(
            page("https://t.me/durov/123").as_deref(),
            Some("https://t.me/s/durov")
        );
        assert_eq!(
            page("https://telegram.me/telegram").as_deref(),
            Some("https://t.me/s/telegram")
        );
        assert_eq!(page("https://t.me/joinchat/AAAA"), None);
        assert_eq!(page("https://t.me/+AbCdEf"), None);
        assert_eq!(page("https://example.com/telegram"), None);
        assert!(is_channel_page("https://t.me/s/telegram"));
        assert!(is_channel_page("https://t.me/s/telegram/"));
        assert!(!is_channel_page("https://t.me/telegram"));
        assert!(!is_channel_page("https://example.com/s/telegram"));
    }

    #[test]
    fn reads_messages_newest_first() {
        let feed = parse(PAGE, "https://t.me/s/telegram").unwrap();
        assert_eq!(feed.title, "Telegram News");
        assert_eq!(feed.site_url.as_deref(), Some("https://t.me/telegram"));
        assert_eq!(
            feed.icon_url.as_deref(),
            Some("https://cdn1.telesco.pe/file/avatar.jpg")
        );
        assert_eq!(feed.entries.len(), 2);

        let video = &feed.entries[0];
        assert_eq!(video.guid, "https://t.me/telegram/441");
        assert_eq!(video.title, "");
        assert_eq!(
            video.image.as_deref(),
            Some("https://cdn1.telesco.pe/file/thumb.jpg")
        );
        assert!(video.media.is_none());
        assert!(video.content.contains(r#"<a href="https://t.me/telegram/441"><img src="https://cdn1.telesco.pe/file/thumb.jpg""#));
        // 2026-05-15 08:00 +08:00 = 2026-05-15 00:00 UTC
        assert_eq!(video.published_ms, Some(1_778_803_200_000));

        let post = &feed.entries[1];
        assert_eq!(post.url.as_deref(), Some("https://t.me/telegram/440"));
        assert_eq!(post.title, "Stories for channels. Now available.");
        assert!(
            post.summary
                .starts_with("Stories for channels. Now available.")
        );
        assert!(post.summary.contains("Read the blog."));
        assert_eq!(
            post.image.as_deref(),
            Some("https://cdn1.telesco.pe/file/photo.jpg")
        );
        assert!(
            post.content
                .contains(r#"<img src="https://cdn1.telesco.pe/file/photo.jpg""#)
        );
        assert!(
            post.content
                .contains(r#"<a href="https://telegram.org/blog/stories">Stories</a>"#)
        );
        assert!(post.content.contains("All about stories."));
        assert_eq!(post.published_ms, Some(1_778_774_911_000));
    }

    #[test]
    fn a_page_without_messages_is_not_a_channel() {
        let html = r#"<html><head><meta property="og:title" content="Someone"></head><body></body></html>"#;
        assert!(parse(html, "https://t.me/s/someone").is_err());
    }
}
