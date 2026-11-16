use crate::client::{Client, decode_html};
use crate::{Error, Result, normalize_url};

/// 提取出的正文少于这么多字时，认为没找到正文。
const MIN_TEXT_CHARS: usize = 120;

impl Client {
    /// 抓取文章原网页并提取正文，给只在源里放摘要的站点用。返回未净化的 HTML。
    pub async fn full_text(&self, url: &str) -> Result<String> {
        let target = normalize_url(url)?;
        let page = self
            .get(target.as_str(), None, None)
            .await?
            .ok_or_else(|| Error::Extract("empty response".into()))?;
        let html = decode_html(&page);
        extract(&html, page.final_url.as_str())
    }
}

fn extract(html: &str, url: &str) -> Result<String> {
    let mut readability = dom_smoothie::Readability::new(html, Some(url), None)
        .map_err(|e| Error::Extract(e.to_string()))?;
    let article = readability
        .parse()
        .map_err(|e| Error::Extract(e.to_string()))?;
    if article.text_content.trim().chars().count() < MIN_TEXT_CHARS {
        return Err(Error::Extract(
            "no article content found on the page".into(),
        ));
    }
    Ok(article.content.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_the_article_and_drops_page_chrome() {
        let paragraph =
            "Reading the whole article inside the reader is much nicer than a two line teaser. ";
        let html = format!(
            r#"<!doctype html><html><head><title>A post</title></head><body>
            <nav><a href="/">Home</a><a href="/about">About</a></nav>
            <article><h1>A post</h1>{}</article>
            <footer>Copyright notice that should not be in the result</footer>
            </body></html>"#,
            format!("<p>{}</p>", paragraph.repeat(4)).repeat(5)
        );
        let content = extract(&html, "https://example.com/post").unwrap();
        assert!(content.contains("Reading the whole article"));
        assert!(!content.contains("Copyright notice"));
    }

    #[test]
    fn pages_without_an_article_are_an_error() {
        let html = "<html><body><nav><a href='/'>Home</a></nav></body></html>";
        assert!(matches!(
            extract(html, "https://example.com/"),
            Err(Error::Extract(_))
        ));
    }
}
