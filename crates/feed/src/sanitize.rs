use std::borrow::Cow;
use std::collections::HashSet;

use ammonia::{Builder, UrlRelative};

/// 可以嵌在文章里的播放器：主机名，以及地址要以什么路径开头。订阅源里常带着这些网站的播放器
/// （比如 RSSHub 的 B 站视频），别的内嵌框架一律去掉。
const PLAYERS: &[(&str, &str)] = &[
    ("player.bilibili.com", "/"),
    ("www.bilibili.com", "/blackboard/"),
    ("www.youtube.com", "/embed/"),
    ("www.youtube-nocookie.com", "/embed/"),
    ("player.vimeo.com", "/video/"),
    ("v.qq.com", "/txp/iframe/"),
    ("player.youku.com", "/embed/"),
    ("www.ixigua.com", "/iframe/"),
    ("open.spotify.com", "/embed/"),
    ("w.soundcloud.com", "/player/"),
    ("embed.music.apple.com", "/"),
    ("embed.podcasts.apple.com", "/"),
    ("music.163.com", "/outchain/player"),
];

/// `src` 是不是允许的播放器。返回补全了协议的地址（`//player…` 这种写法补成 https）。
fn player_src(src: &str) -> Option<String> {
    let src = src.trim();
    let full = if src.starts_with("//") {
        format!("https:{src}")
    } else {
        src.to_string()
    };
    let url = ammonia::Url::parse(&full).ok()?;
    if !matches!(url.scheme(), "https" | "http") {
        return None;
    }
    let host = url.host_str()?.to_string();
    PLAYERS
        .iter()
        .any(|(h, path)| *h == host && url.path().starts_with(path))
        .then(|| {
            // 播放器一律用 https，免得在应用里被当成不安全内容拦下
            let mut url = url;
            let _ = url.set_scheme("https");
            // B 站的播放器默认一打开就播放，打开文章时不该自己出声
            if host == "player.bilibili.com" && !url.query_pairs().any(|(k, _)| k == "autoplay") {
                url.query_pairs_mut().append_pair("autoplay", "0");
            }
            url.to_string()
        })
}

/// 去掉没有地址的内嵌框架（不是允许的播放器，地址已经被去掉了）。
fn drop_empty_iframes(html: String) -> String {
    if !html.contains("<iframe") {
        return html;
    }
    let mut out = String::with_capacity(html.len());
    let mut rest = html.as_str();
    while let Some(start) = rest.find("<iframe") {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let open_end = tail.find('>').map_or(tail.len(), |i| i + 1);
        let close = tail.find("</iframe>").map(|i| i + "</iframe>".len());
        let end = close.unwrap_or(open_end).max(open_end);
        let element = &tail[..end];
        if tail[..open_end].contains(" src=") {
            out.push_str(element);
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// 视频网页（YouTube、B 站）对应的嵌入播放器地址。
fn page_player(link: &str) -> Option<String> {
    let url = ammonia::Url::parse(link.trim()).ok()?;
    let host = url
        .host_str()?
        .trim_start_matches("www.")
        .trim_start_matches("m.");
    let segments: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    let valid = |id: &str| {
        id.len() == 11
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    };
    match host {
        "youtube.com" | "music.youtube.com" => {
            let id = match segments.as_slice() {
                ["watch"] => url
                    .query_pairs()
                    .find(|(k, _)| k == "v")
                    .map(|(_, v)| v.into_owned()),
                ["shorts" | "live" | "embed", id, ..] => Some((*id).to_string()),
                _ => None,
            }?;
            valid(&id).then(|| format!("https://www.youtube-nocookie.com/embed/{id}"))
        }
        "youtu.be" => {
            let id = segments.first()?;
            valid(id).then(|| format!("https://www.youtube-nocookie.com/embed/{id}"))
        }
        "bilibili.com" => match segments.as_slice() {
            ["video", id, ..]
                if id.starts_with("BV") && id.bytes().all(|b| b.is_ascii_alphanumeric()) =>
            {
                Some(format!(
                    "https://player.bilibili.com/player.html?bvid={id}&autoplay=0"
                ))
            }
            ["video", id, ..]
                if id.starts_with("av")
                    && id[2..].bytes().all(|b| b.is_ascii_digit())
                    && id.len() > 2 =>
            {
                Some(format!(
                    "https://player.bilibili.com/player.html?aid={}&autoplay=0",
                    &id[2..]
                ))
            }
            _ => None,
        },
        _ => None,
    }
}

/// 链接是 YouTube、B 站的视频页、正文里又没有播放器时（比如 YouTube 自己的订阅源），在正文最前面放上播放器，
/// 净化时和源里自带的播放器一样处理。不是视频页、或者已经有播放器时返回 `None`。
pub fn with_player(content: &str, link: Option<&str>) -> Option<String> {
    let player = page_player(link?)?;
    if [
        "youtube.com/embed",
        "youtube-nocookie.com/embed",
        "player.bilibili.com",
    ]
    .iter()
    .any(|p| content.contains(p))
    {
        return None;
    }
    let mut body = content;
    // YouTube 源里的缩略图就是播放器的封面，不用再显示一遍
    if player.contains("youtube") {
        let trimmed = body.trim_start();
        let head = trimmed.get(..trimmed.len().min(200)).unwrap_or(trimmed);
        if trimmed.starts_with("<p><img src=\"https://i")
            && head.contains("ytimg.com/")
            && let Some(end) = trimmed.find("</p>")
        {
            body = &trimmed[end + "</p>".len()..];
        }
    }
    Some(format!(
        "<iframe src=\"{}\" width=\"640\" height=\"360\"></iframe>{body}",
        player.replace('&', "&amp;")
    ))
}

/// 把不可信的 HTML 净化成可以直接放进界面的 HTML。
///
/// 脚本、样式、事件属性、表单一律去掉，只留排版用的标签。内嵌框架只留已知网站的视频、音频播放器
/// （`PLAYERS`），放在沙箱里，不能弹窗、不能改动阅读区。
/// 存档里保存的是原文，每次展示前才净化，所以净化规则以后收紧或放宽都会对旧文章生效。
pub fn sanitize_html(html: &str, base_url: Option<&str>) -> String {
    let mut builder = Builder::default();
    builder
        .add_tags([
            "figure",
            "figcaption",
            "picture",
            "source",
            "video",
            "audio",
            "iframe",
        ])
        .add_tag_attributes("iframe", ["src", "width", "height"])
        // 播放器要跑自己的脚本；同源指的是播放器自己的网站，碰不到阅读区
        .set_tag_attribute_value(
            "iframe",
            "sandbox",
            "allow-scripts allow-same-origin allow-presentation allow-popups",
        )
        .set_tag_attribute_value(
            "iframe",
            "allow",
            "fullscreen; picture-in-picture; encrypted-media",
        )
        .set_tag_attribute_value("iframe", "allowfullscreen", "")
        .set_tag_attribute_value("iframe", "loading", "lazy")
        .set_tag_attribute_value(
            "iframe",
            "referrerpolicy",
            "strict-origin-when-cross-origin",
        )
        .attribute_filter(|element, attribute, value| match (element, attribute) {
            ("iframe", "src") => player_src(value).map(Cow::Owned),
            _ => Some(Cow::Borrowed(value)),
        })
        .add_tag_attributes("video", ["src", "poster", "controls", "width", "height"])
        .add_tag_attributes("audio", ["src", "controls"])
        .add_tag_attributes("source", ["src", "type"])
        .url_schemes(HashSet::from(["http", "https", "mailto"]))
        .link_rel(Some("noopener noreferrer"))
        .set_tag_attribute_value("a", "target", "_blank")
        .set_tag_attribute_value("img", "loading", "lazy")
        // 不带来源页地址去请求图片：既少泄露一点阅读行为，也能绕开多数防盗链
        .set_tag_attribute_value("img", "referrerpolicy", "no-referrer");

    match base_url.and_then(|u| ammonia::Url::parse(u).ok()) {
        Some(base) => builder.url_relative(UrlRelative::RewriteWithBase(base)),
        None => builder.url_relative(UrlRelative::Deny),
    };
    // 不带协议的地址（`//player.bilibili.com/…`）没有基准地址时会被当成相对地址丢掉，先补上 https
    let html = html
        .replace("src=\"//", "src=\"https://")
        .replace("src='//", "src='https://");
    drop_empty_iframes(builder.clean(&html).to_string())
}

/// 去掉所有标签，得到折叠了空白的纯文本。
pub fn html_to_text(html: &str) -> String {
    if !html.contains(['<', '&']) {
        return collapse_whitespace(html);
    }
    let fragment = scraper::Html::parse_fragment(html);
    let mut out = String::new();
    for node in fragment.root_element().descendants() {
        match node.value() {
            scraper::Node::Text(text) => {
                let hidden = node
                    .parent()
                    .and_then(|p| p.value().as_element())
                    .is_some_and(|e| matches!(e.name(), "script" | "style" | "template"));
                if !hidden {
                    out.push_str(text);
                }
            }
            // 块级元素之间补一个空格，否则相邻段落的字会粘在一起
            scraper::Node::Element(el) if is_block(el.name()) => out.push(' '),
            _ => {}
        }
    }
    collapse_whitespace(&out)
}

/// 去掉标签，但保留段落：块级元素和换行之间换行，行内的空白折叠。交给 AI 读的正文用它。
pub fn html_to_paragraphs(html: &str) -> String {
    let fragment = scraper::Html::parse_fragment(html);
    let mut out = String::new();
    for node in fragment.root_element().descendants() {
        match node.value() {
            scraper::Node::Text(text) => {
                let hidden = node
                    .parent()
                    .and_then(|p| p.value().as_element())
                    .is_some_and(|e| {
                        matches!(e.name(), "script" | "style" | "template" | "head" | "title")
                    });
                if !hidden {
                    out.push_str(text);
                }
            }
            scraper::Node::Element(el) if is_block(el.name()) => out.push('\n'),
            _ => {}
        }
    }
    let mut lines: Vec<String> = Vec::new();
    for line in out.lines() {
        let line = collapse_whitespace(line);
        // 连续的空行只留一个
        if line.is_empty() && lines.last().is_none_or(|l| l.is_empty()) {
            continue;
        }
        lines.push(line);
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

fn is_block(tag: &str) -> bool {
    matches!(
        tag,
        "p" | "br"
            | "div"
            | "li"
            | "ul"
            | "ol"
            | "tr"
            | "td"
            | "th"
            | "blockquote"
            | "pre"
            | "figure"
            | "figcaption"
            | "section"
            | "article"
            | "hr"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
    )
}

pub(crate) fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_paragraphs_for_reading() {
        let html = "<style>p{}</style><h1>Title</h1><p>First   line<br>second</p><ul><li>a</li><li>b</li></ul>";
        assert_eq!(
            html_to_paragraphs(html),
            "Title\nFirst line\nsecond\n\na\nb"
        );
    }

    #[test]
    fn strips_active_content() {
        let dirty = r#"<p onclick="evil()">Hi<script>alert(1)</script>
            <img src="x.png" onerror="evil()">
            <a href="javascript:alert(1)">bad</a>
            <iframe src="https://evil.test"></iframe>
            <style>p{display:none}</style>
            <form action="/steal"><input name="q"></form></p>"#;
        let clean = sanitize_html(dirty, Some("https://example.com/post/1"));
        for needle in [
            "script",
            "onclick",
            "onerror",
            "javascript:",
            "iframe",
            "<style",
            "<form",
            "<input",
        ] {
            assert!(!clean.contains(needle), "{needle} survived: {clean}");
        }
        assert!(clean.contains("Hi"));
    }

    #[test]
    fn keeps_known_video_players_only() {
        let html = r#"<p>before</p><iframe width="640" height="360" src="//player.bilibili.com/player.html?aid=1&amp;bvid=BV1" frameborder="0" allowfullscreen="" onload="evil()"></iframe><iframe src="https://evil.test/embed/x"></iframe><iframe src="https://www.youtube.com/watch?v=x"></iframe><p>after</p>"#;
        let clean = sanitize_html(html, None);
        assert!(
            clean.contains(
                r#"src="https://player.bilibili.com/player.html?aid=1&amp;bvid=BV1&amp;autoplay=0""#
            ),
            "{clean}"
        );
        assert!(clean.contains("sandbox="), "{clean}");
        assert!(!clean.contains("evil"), "{clean}");
        assert!(!clean.contains("watch"), "{clean}");
        assert_eq!(clean.matches("<iframe").count(), 1, "{clean}");
        assert!(clean.contains("before") && clean.contains("after"));
    }

    #[test]
    fn rewrites_relative_urls_and_hardens_links() {
        let clean = sanitize_html(
            r#"<a href="/about">about</a><img src="img/a.png">"#,
            Some("https://example.com/post/1"),
        );
        assert!(
            clean.contains(r#"href="https://example.com/about""#),
            "{clean}"
        );
        assert!(
            clean.contains(r#"src="https://example.com/post/img/a.png""#),
            "{clean}"
        );
        assert!(clean.contains(r#"target="_blank""#));
        assert!(clean.contains("noopener"));
        assert!(clean.contains(r#"referrerpolicy="no-referrer""#));
        assert!(clean.contains(r#"loading="lazy""#));
    }

    #[test]
    fn relative_urls_are_dropped_without_a_base() {
        let clean = sanitize_html(r#"<a href="/about">about</a>"#, None);
        assert!(!clean.contains("href"), "{clean}");
        assert!(clean.contains("about"));
    }

    #[test]
    fn text_extraction() {
        assert_eq!(html_to_text("plain  text\n here"), "plain text here");
        assert_eq!(
            html_to_text("First <b>post</b> &amp; more"),
            "First post & more"
        );
        assert_eq!(
            html_to_text("<p>one</p><p>two</p><ul><li>three</li></ul>"),
            "one two three"
        );
        assert_eq!(
            html_to_text("<p>keep</p><script>drop()</script><style>.x{}</style>"),
            "keep"
        );
    }

    #[test]
    fn adds_a_player_for_video_pages() {
        let yt = with_player(
            r#"<p><img src="https://i2.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg"></p><p>About</p>"#,
            Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ"),
        )
        .unwrap();
        assert!(
            yt.starts_with(r#"<iframe src="https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ""#),
            "{yt}"
        );
        assert!(!yt.contains("ytimg"), "{yt}");
        assert!(yt.ends_with("<p>About</p>"));
        let clean = sanitize_html(&yt, None);
        assert!(
            clean.contains("youtube-nocookie.com/embed/dQw4w9WgXcQ"),
            "{clean}"
        );

        for link in [
            "https://youtu.be/dQw4w9WgXcQ",
            "https://www.youtube.com/shorts/dQw4w9WgXcQ",
        ] {
            assert!(
                with_player("", Some(link))
                    .unwrap()
                    .contains("embed/dQw4w9WgXcQ"),
                "{link}"
            );
        }
        let bili = with_player(
            "<p>hi</p>",
            Some("https://www.bilibili.com/video/BV17osu6pEP9"),
        )
        .unwrap();
        assert!(
            bili.contains("player.bilibili.com/player.html?bvid=BV17osu6pEP9&amp;autoplay=0"),
            "{bili}"
        );

        // 已经有播放器、不是视频页、地址不对的都不加
        assert_eq!(
            with_player(
                r#"<iframe src="https://player.bilibili.com/x"></iframe>"#,
                Some("https://www.bilibili.com/video/BV1")
            ),
            None
        );
        assert_eq!(
            with_player("", Some("https://www.youtube.com/@someone")),
            None
        );
        assert_eq!(
            with_player("", Some("https://www.youtube.com/watch?v=short")),
            None
        );
        assert_eq!(
            with_player("", Some("https://example.com/watch?v=dQw4w9WgXcQ")),
            None
        );
        assert_eq!(with_player("", None), None);
    }
}
