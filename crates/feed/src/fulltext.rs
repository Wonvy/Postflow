use scraper::{ElementRef, Html, Node, Selector};
use serde_json::Value;
use url::Url;

use crate::client::{Client, Page, decode_html};
use crate::parse::text_to_html;
use crate::{Error, Result, normalize_url};

/// 提取出的正文少于这么多字（不算空白）时，认为没找到正文。
const MIN_TEXT_CHARS: usize = 120;
/// 网页里能看到的字少于这么多、又有脚本时，认为正文是脚本加载的。
const SCRIPT_PAGE_CHARS: usize = 200;

impl Client {
    /// 抓取文章原网页并提取正文，给只在源里放摘要的站点用。返回未净化的 HTML。
    pub async fn full_text(&self, url: &str) -> Result<String> {
        let target = normalize_url(url)?;
        let page = match self.article_page(target.as_str()).await {
            // 超时、连不上：常常是一时的，再试一次
            Err(Error::Http(_)) => self.article_page(target.as_str()).await,
            other => other,
        }?;
        let html = decode_html(&page);
        extract_article(&html, &page.final_url)
    }

    pub(crate) async fn article_page(&self, url: &str) -> Result<Page> {
        match self.get(url, None, None).await {
            // 被转到了人机验证页，比如微信公众号的「环境异常」。不去绕，照实告诉用户。
            Ok(Some(page)) if page.final_url.path().contains("captcha") => {
                Err(Error::Blocked("verification page".into()))
            }
            Ok(Some(page)) => Ok(page),
            Ok(None) => Err(Error::Extract("empty response".into())),
            Err(Error::Status(code @ (401 | 402))) => Err(Error::NeedsLogin(code)),
            Err(Error::Status(code @ (403 | 429 | 451))) => {
                Err(Error::Blocked(format!("HTTP {code}")))
            }
            Err(e) => Err(e),
        }
    }
}

/// 从网页里取出正文：先看有没有专门的站点规则，再用 Readability，
/// 最后用网页给搜索引擎看的结构化数据（JSON-LD 的 articleBody）兜底。
pub(crate) fn extract_article(html: &str, url: &Url) -> Result<String> {
    if let Some(content) = site_rule(html, url) {
        return Ok(content);
    }
    let structured = json_ld_body(html);
    match readability(html, url.as_str()) {
        // 结构化数据里的正文长得多，说明 Readability 挑错了地方
        Ok((_, chars)) if structured.as_deref().is_some_and(|b| count(b) > chars * 2) => {
            Ok(text_to_html(&structured.unwrap_or_default()))
        }
        Ok((content, _)) => Ok(content),
        Err(err) => match structured {
            Some(body) => Ok(text_to_html(&body)),
            None if built_by_script(html) => Err(Error::ScriptPage),
            None => Err(err),
        },
    }
}

/// 用 Readability 提取正文，返回 HTML 和正文的字数。
fn readability(html: &str, url: &str) -> Result<(String, usize)> {
    let mut readability = dom_smoothie::Readability::new(html, Some(url), None)
        .map_err(|e| Error::Extract(e.to_string()))?;
    let article = readability
        .parse()
        .map_err(|e| Error::Extract(e.to_string()))?;
    let chars = count(&article.text_content);
    if chars < MIN_TEXT_CHARS {
        return Err(Error::Extract(
            "no article content found on the page".into(),
        ));
    }
    Ok((article.content.to_string(), chars))
}

/// 不算空白的字数。
fn count(text: &str) -> usize {
    text.chars().filter(|c| !c.is_whitespace()).count()
}

/// 有些网站的正文 Readability 认不出来，按网站单独处理。
fn site_rule(html: &str, url: &Url) -> Option<String> {
    match url.host_str()? {
        // 微信公众号：正文在 #js_content 里，先用样式藏起来、等脚本再显示，
        // Readability 会当成隐藏内容丢掉。图片的真实地址在 data-src 里。
        "mp.weixin.qq.com" => {
            let doc = Html::parse_document(html);
            let selector = Selector::parse("#js_content").ok()?;
            let content = doc.select(&selector).next()?;
            let inner = content.inner_html();
            (count(&visible_text(content)) > 0 || inner.contains("<img"))
                .then(|| promote_data_src(&inner))
        }
        _ => None,
    }
}

/// 懒加载的图片：真正的地址在 data-src 里，src 是占位图或者干脆没有。把 data-src 换成 src。
fn promote_data_src(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = rest.find("<img") {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let end = tail.find('>').map_or(tail.len(), |i| i + 1);
        let tag = &tail[..end];
        if tag.contains(" data-src=") {
            out.push_str(&drop_attr(tag, "src").replacen(" data-src=", " src=", 1));
        } else {
            out.push_str(tag);
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// 从一个标签里去掉某个属性。
fn drop_attr(tag: &str, name: &str) -> String {
    let needle = format!(" {name}=");
    let Some(at) = tag.find(&needle) else {
        return tag.to_string();
    };
    let value = &tag[at + needle.len()..];
    let len = match value.chars().next() {
        Some(quote @ ('"' | '\'')) => value[1..].find(quote).map_or(value.len(), |i| i + 2),
        _ => value
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .unwrap_or(value.len()),
    };
    format!("{}{}", &tag[..at], &tag[at + needle.len() + len..])
}

/// 网页给搜索引擎看的结构化数据（JSON-LD）里的 articleBody，有好几段时取最长的。
fn json_ld_body(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let selector = Selector::parse(r#"script[type="application/ld+json"]"#).ok()?;
    let mut best: Option<String> = None;
    for script in doc.select(&selector) {
        let text: String = script.text().collect();
        let Ok(value) = serde_json::from_str::<Value>(text.trim()) else {
            continue;
        };
        find_bodies(&value, &mut |body| {
            if count(body) >= MIN_TEXT_CHARS && best.as_ref().is_none_or(|b| b.len() < body.len()) {
                best = Some(body.trim().to_string());
            }
        });
    }
    best
}

fn find_bodies(value: &Value, found: &mut impl FnMut(&str)) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(body)) = map.get("articleBody") {
                found(body);
            }
            for child in map.values() {
                find_bodies(child, found);
            }
        }
        Value::Array(list) => {
            for child in list {
                find_bodies(child, found);
            }
        }
        _ => {}
    }
}

/// 网页本身几乎没有能看到的字、全靠脚本加载（单页应用的空壳）。
fn built_by_script(html: &str) -> bool {
    let doc = Html::parse_document(html);
    let (Ok(body), Ok(script)) = (Selector::parse("body"), Selector::parse("script")) else {
        return false;
    };
    let visible = doc
        .select(&body)
        .next()
        .map_or(0, |b| count(&visible_text(b)));
    visible < SCRIPT_PAGE_CHARS && doc.select(&script).next().is_some()
}

/// 元素里能看到的文字：不算脚本、样式和模板里的。
fn visible_text(element: ElementRef) -> String {
    let mut out = String::new();
    for node in element.descendants() {
        let Node::Text(text) = node.value() else {
            continue;
        };
        let hidden = node.ancestors().any(|a| {
            a.value()
                .as_element()
                .is_some_and(|e| matches!(e.name(), "script" | "style" | "noscript" | "template"))
        });
        if !hidden {
            out.push_str(text);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARAGRAPH: &str =
        "Reading the whole article inside the reader is much nicer than a two line teaser. ";

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn extracts_the_article_and_drops_page_chrome() {
        let html = format!(
            r#"<!doctype html><html><head><title>A post</title></head><body>
            <nav><a href="/">Home</a><a href="/about">About</a></nav>
            <article><h1>A post</h1>{}</article>
            <footer>Copyright notice that should not be in the result</footer>
            </body></html>"#,
            format!("<p>{}</p>", PARAGRAPH.repeat(4)).repeat(5)
        );
        let content = extract_article(&html, &url("https://example.com/post")).unwrap();
        assert!(content.contains("Reading the whole article"));
        assert!(!content.contains("Copyright notice"));
    }

    #[test]
    fn pages_without_an_article_are_an_error() {
        let html = "<html><body><nav><a href='/'>Home</a></nav></body></html>";
        assert!(matches!(
            extract_article(html, &url("https://example.com/")),
            Err(Error::Extract(_))
        ));
    }

    #[test]
    fn wechat_articles_are_read_from_the_hidden_content() {
        let html = format!(
            r#"<html><body><div id="js_article">
            <h1 class="rich_media_title">标题</h1>
            <div class="rich_media_content" id="js_content" style="visibility: hidden; opacity: 0; ">
              <p>{}</p>
              <p><img class="rich_pages" src="data:image/gif;base64,AAAA" data-src="https://mmbiz.qpic.cn/a/640?wx_fmt=png" data-type="png"></p>
            </div></div></body></html>"#,
            "公众号的正文写在这里，打开以后要能看到完整的内容。".repeat(4)
        );
        let content = extract_article(&html, &url("https://mp.weixin.qq.com/s/abc")).unwrap();
        assert!(content.contains("公众号的正文"));
        assert!(content.contains(r#"src="https://mmbiz.qpic.cn/a/640?wx_fmt=png""#));
        assert!(!content.contains("data:image/gif"));
    }

    #[test]
    fn structured_data_fills_in_when_the_page_has_no_readable_article() {
        let body = PARAGRAPH.repeat(4);
        let html = format!(
            r#"<html><head><script type="application/ld+json">
            {{"@context":"https://schema.org","@graph":[{{"@type":"WebPage"}},
              {{"@type":"NewsArticle","headline":"Hi","articleBody":"{body}\n\nSecond paragraph."}}]}}
            </script></head><body><div id="app"></div><script src="/app.js"></script></body></html>"#
        );
        let content = extract_article(&html, &url("https://news.example/a")).unwrap();
        assert!(content.starts_with("<p>Reading the whole article"));
        assert!(content.contains("<p>Second paragraph.</p>"));
    }

    #[test]
    fn script_built_pages_say_so() {
        let html = r#"<html><head><title>App</title></head><body><div id="app"></div>
            <script>window.__INITIAL_STATE__ = {"article": "loaded later"}</script></body></html>"#;
        assert!(matches!(
            extract_article(html, &url("https://spa.example/p/1")),
            Err(Error::ScriptPage)
        ));
    }

    #[test]
    fn lazy_image_attributes() {
        assert_eq!(
            promote_data_src(r#"<p><img src="x.gif" data-src="https://a/b.png" alt="b"></p>"#),
            r#"<p><img data-src="https://a/b.png" alt="b"></p>"#.replace(" data-src=", " src=")
        );
        assert_eq!(
            promote_data_src("<img src=\"a.png\">"),
            "<img src=\"a.png\">"
        );
    }
}
