//! 从文章正文里找一张能当封面的图，给卡片视图用。
//!
//! 只做简单的扫描，不解析整个 HTML：找第一个看起来像正经配图的 `<img>`。
//! 跳过统计像素、表情符号这类小图。相对地址按文章链接补全。

/// 这些地址几乎都是统计像素或表情，不当封面。
const SKIP: &[&str] = &[
    "feeds.feedburner.com",
    "feedsportal.com",
    "pixel.",
    "/pixel",
    "stats.wordpress.com",
    "s.w.org/images/core/emoji",
    "gravatar.com/avatar",
    "doubleclick.net",
    "/1x1.",
];

/// 最多看正文的前这么多字节。封面一般在开头。
const SCAN_BYTES: usize = 200_000;

/// 正文里第一张能当封面的图片地址。`base` 是文章链接，用来补全相对地址。
pub fn first_image(html: &str, base: Option<&str>) -> Option<String> {
    let mut end = html.len().min(SCAN_BYTES);
    while !html.is_char_boundary(end) {
        end -= 1;
    }
    let html = &html[..end];
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(pos) = lower[from..].find("<img") {
        let start = from + pos;
        let tag_end = lower[start..].find('>').map_or(lower.len(), |e| start + e);
        let tag = &html[start..tag_end];
        from = tag_end;
        if let Some(src) = attr(tag, "src")
            && !tiny(tag)
            && let Some(url) = absolute(&decode(&src), base)
            && !SKIP.iter().any(|s| url.contains(s))
        {
            return Some(url);
        }
    }
    None
}

/// 取标签里某个属性的值。
fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(pos) = lower[from..].find(name) {
        let at = from + pos;
        from = at + name.len();
        // 必须是独立的属性名：`data-src` 不算 `src`
        let before = lower[..at].chars().next_back();
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let rest = tag[from..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start();
        let value = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => rest[1..].split(q).next().unwrap_or(""),
            _ => rest
                .split(|c: char| c.is_whitespace() || c == '>')
                .next()
                .unwrap_or(""),
        };
        let value = value.trim();
        return (!value.is_empty()).then(|| value.to_string());
    }
    None
}

/// 宽或高写明了很小的，是像素或图标。
fn tiny(tag: &str) -> bool {
    ["width", "height"].iter().any(|name| {
        attr(tag, name)
            .and_then(|v| v.trim_end_matches("px").parse::<u32>().ok())
            .is_some_and(|n| n < 48)
    })
}

fn decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&#38;", "&")
        .replace("&quot;", "\"")
}

fn absolute(src: &str, base: Option<&str>) -> Option<String> {
    if src.starts_with("https://") || src.starts_with("http://") {
        return Some(src.to_string());
    }
    if let Some(rest) = src.strip_prefix("//") {
        return Some(format!("https://{rest}"));
    }
    if src.starts_with("data:") || src.contains(':') {
        return None;
    }
    // 相对地址：按文章链接补全
    let base = base?;
    let scheme_end = base.find("://")? + 3;
    let host_end = base[scheme_end..]
        .find('/')
        .map_or(base.len(), |i| scheme_end + i);
    if src.starts_with('/') {
        return Some(format!("{}{src}", &base[..host_end]));
    }
    let path = &base[..base[host_end..]
        .rfind('/')
        .map_or(base.len(), |i| host_end + i)];
    Some(format!("{path}/{src}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_first_real_image() {
        let html = r#"<p><img src="https://feeds.feedburner.com/~r/x.gif" width="1" height="1">
            <img data-src="lazy.jpg" src='https://cdn.example.org/a.jpg?w=800&amp;h=400' alt="cover"></p>"#;
        assert_eq!(
            first_image(html, None).as_deref(),
            Some("https://cdn.example.org/a.jpg?w=800&h=400")
        );
    }

    #[test]
    fn skips_tiny_images_and_emoji() {
        let html = r#"<IMG SRC="https://example.org/icon.png" WIDTH=16>
            <img src="https://s.w.org/images/core/emoji/14.0.0/72x72/1f600.png">"#;
        assert_eq!(first_image(html, None), None);
    }

    #[test]
    fn resolves_relative_addresses() {
        let base = Some("https://blog.example.org/2026/11/post.html");
        assert_eq!(
            first_image(r#"<img src="/img/a.png">"#, base).as_deref(),
            Some("https://blog.example.org/img/a.png")
        );
        assert_eq!(
            first_image(r#"<img src="b.png">"#, base).as_deref(),
            Some("https://blog.example.org/2026/11/b.png")
        );
        assert_eq!(
            first_image(r#"<img src="//cdn.example.org/c.png">"#, None).as_deref(),
            Some("https://cdn.example.org/c.png")
        );
        assert_eq!(first_image(r#"<img src="b.png">"#, None), None);
        assert_eq!(
            first_image(r#"<img src="data:image/png;base64,AAAA">"#, base),
            None
        );
    }

    #[test]
    fn no_images() {
        assert_eq!(first_image("<p>just text</p>", None), None);
        assert_eq!(first_image("<img>", None), None);
    }
}
