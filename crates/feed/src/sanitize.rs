use std::collections::HashSet;

use ammonia::{Builder, UrlRelative};

/// 把不可信的 HTML 净化成可以直接放进界面的 HTML。
///
/// 脚本、样式、事件属性、表单、内嵌框架一律去掉，只留排版用的标签。
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
        ])
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
    builder.clean(html).to_string()
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
}
