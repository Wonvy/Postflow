//! 存进「稍后读」的网页：抓下网页，取出标题、作者、封面、摘要和正文。

use scraper::{Html, Selector};
use url::Url;

use crate::client::{Client, decode_html};
use crate::fulltext::extract_article;
use crate::sanitize::html_to_text;
use crate::{Error, Result, normalize_url};

/// 摘要最多多少字。
const EXCERPT_CHARS: usize = 240;

/// 网页里取出来的东西。正文是未净化的 HTML，取不出来时为空，`content_error` 说明原因。
#[derive(Debug, Clone, Default)]
pub struct SavedPage {
    /// 跳转之后的地址。
    pub url: String,
    pub title: String,
    pub author: Option<String>,
    pub image: Option<String>,
    /// 纯文本摘要。
    pub excerpt: String,
    pub content: String,
    pub content_error: Option<String>,
}

/// 跟踪用的查询参数，存下来的地址里去掉，同一篇从不同地方点进来是同一个地址。
const TRACKING: &[&str] = &[
    "fbclid", "gclid", "dclid", "msclkid", "mc_cid", "mc_eid", "igshid", "yclid", "_hsenc", "_hsmi",
];

/// 规范化要存的地址：补协议、去掉片段和跟踪参数。
pub fn clean_url(input: &str) -> Result<Url> {
    let mut url = normalize_url(input)?;
    let kept: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(k, _)| {
            let k = k.to_ascii_lowercase();
            !k.starts_with("utm_") && !TRACKING.contains(&k.as_str())
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if url.query().is_some() {
        if kept.is_empty() {
            url.set_query(None);
        } else {
            url.query_pairs_mut().clear().extend_pairs(kept);
        }
    }
    Ok(url)
}

impl Client {
    /// 抓一个网页存进「稍后读」。连不上、被拒绝时返回错误；网页打开了但认不出正文时照样返回，
    /// 正文留空。
    pub async fn save_page(&self, url: &str) -> Result<SavedPage> {
        let target = clean_url(url)?;
        let page = match self.article_page(target.as_str()).await {
            Err(Error::Http(_)) => self.article_page(target.as_str()).await,
            other => other,
        }?;
        let is_html = page
            .content_type
            .as_deref()
            .is_none_or(|t| t.contains("html") || t.contains("xml"));
        if !is_html {
            return Err(Error::Extract("not a web page".into()));
        }
        let html = decode_html(&page);
        Ok(page_parts(&html, &page.final_url))
    }
}

fn page_parts(html: &str, url: &Url) -> SavedPage {
    let doc = Html::parse_document(html);
    let meta = |names: &[&str]| -> Option<String> {
        names.iter().find_map(|name| {
            let selector =
                Selector::parse(&format!("meta[property=\"{name}\"], meta[name=\"{name}\"]"))
                    .ok()?;
            doc.select(&selector)
                .filter_map(|m| m.value().attr("content"))
                .map(collapse)
                .find(|c| !c.is_empty())
        })
    };
    let page_title = Selector::parse("title")
        .ok()
        .and_then(|s| {
            doc.select(&s)
                .next()
                .map(|t| collapse(&t.text().collect::<String>()))
        })
        .filter(|t| !t.is_empty());
    let title = meta(&["og:title", "twitter:title"])
        .or(page_title)
        .unwrap_or_else(|| url.as_str().to_string());
    let author = meta(&[
        "author",
        "article:author",
        "og:site_name",
        "application-name",
    ])
    .filter(|a| !a.starts_with("http"));
    let image = meta(&[
        "og:image",
        "og:image:url",
        "twitter:image",
        "twitter:image:src",
    ])
    .and_then(|src| url.join(&src).ok())
    .filter(|u| matches!(u.scheme(), "http" | "https"))
    .map(String::from);
    let (content, content_error) = match extract_article(html, url) {
        Ok(content) => (content, None),
        Err(err) => (String::new(), Some(err.to_string())),
    };
    let excerpt = meta(&["og:description", "description", "twitter:description"])
        .unwrap_or_else(|| html_to_text(&content));
    SavedPage {
        url: url.to_string(),
        title,
        author,
        image,
        excerpt: truncate(&excerpt, EXCERPT_CHARS),
        content,
        content_error,
    }
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate(s: &str, max: usize) -> String {
    let s = collapse(s);
    if s.chars().count() <= max {
        return s;
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracking_parameters_are_dropped() {
        assert_eq!(
            clean_url("example.com/a?utm_source=x&id=3&fbclid=y#top")
                .unwrap()
                .as_str(),
            "https://example.com/a?id=3"
        );
        assert_eq!(
            clean_url("https://example.com/a?utm_medium=mail")
                .unwrap()
                .as_str(),
            "https://example.com/a"
        );
        assert!(clean_url("ftp://example.com/").is_err());
    }

    #[test]
    fn reads_title_author_image_and_body() {
        let body =
            "<p>".to_string() + &"Calm software respects your attention. ".repeat(12) + "</p>";
        let html = format!(
            "<html><head><title>Fallback</title>\
             <meta property=\"og:title\" content=\" Calm  software \">\
             <meta name=\"author\" content=\"Mira\">\
             <meta property=\"og:image\" content=\"/cover.png\">\
             </head><body><nav>Menu</nav><article><h1>Calm software</h1>{body}{body}</article></body></html>"
        );
        let url = Url::parse("https://example.com/posts/calm").unwrap();
        let page = page_parts(&html, &url);
        assert_eq!(page.title, "Calm software");
        assert_eq!(page.author.as_deref(), Some("Mira"));
        assert_eq!(page.image.as_deref(), Some("https://example.com/cover.png"));
        assert!(
            page.content.contains("respects your attention"),
            "{}",
            page.content
        );
        assert!(page.content_error.is_none());
        assert!(
            page.excerpt.starts_with("Calm software respects"),
            "{}",
            page.excerpt
        );
    }

    #[test]
    fn page_without_article_keeps_its_title() {
        let url = Url::parse("https://example.com/").unwrap();
        let page = page_parts(
            "<html><head><title>Home</title></head><body>Hi</body></html>",
            &url,
        );
        assert_eq!(page.title, "Home");
        assert!(page.content.is_empty());
        assert!(page.content_error.is_some());
    }
}
