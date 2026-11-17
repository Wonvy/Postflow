//! 订阅源相关的一切：抓取、解析、从网页地址发现订阅源、OPML、正文净化与全文提取。
//!
//! 这个 crate 不碰存储，只负责把网络上的内容变成结构化数据。

mod client;
mod discover;
mod frame;
mod fulltext;
pub mod opml;
mod parse;
mod preview;
pub mod rsshub;
mod sanitize;
mod site_icon;
mod youtube;

pub use client::{Client, Conditional, FetchResult, Fetched};
pub use discover::Candidate;
pub use frame::FramePolicy;
pub use parse::{ParsedEntry, ParsedFeed, parse_feed};
pub use preview::{FeedPreview, Preview, PreviewEntry};
pub use sanitize::{html_to_paragraphs, html_to_text, sanitize_html};
pub use site_icon::valid_domain;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid url: {0}")]
    InvalidUrl(String),
    #[error("request failed: {0}")]
    Http(String),
    #[error("server returned HTTP {0}")]
    Status(u16),
    #[error("response is larger than {0} MB")]
    TooLarge(usize),
    #[error("not a feed: {0}")]
    NotAFeed(String),
    #[error("no feed found at this address")]
    NoFeedFound,
    #[error("invalid OPML: {0}")]
    Opml(String),
    #[error("could not extract the article: {0}")]
    Extract(String),
    #[error("the site blocks automatic fetching ({0})")]
    Blocked(String),
    #[error("the page needs a login or subscription (HTTP {0})")]
    NeedsLogin(u16),
    #[error("the page loads its text with scripts")]
    ScriptPage,
}

impl Error {
    /// 给界面用的稳定错误码。
    pub fn code(&self) -> &'static str {
        match self {
            Error::InvalidUrl(_) => "invalid_url",
            Error::Http(_) => "network",
            Error::Status(_) => "http_status",
            Error::TooLarge(_) => "too_large",
            Error::NotAFeed(_) => "not_a_feed",
            Error::NoFeedFound => "no_feed_found",
            Error::Opml(_) => "invalid_opml",
            Error::Extract(_) => "extract_failed",
            Error::Blocked(_) => "fulltext_blocked",
            Error::NeedsLogin(_) => "fulltext_login",
            Error::ScriptPage => "fulltext_script",
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// 把用户输入的地址规范化：补上协议，去掉片段，只接受 http 和 https。
pub fn normalize_url(input: &str) -> Result<url::Url> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(Error::InvalidUrl(input.to_string()));
    }
    let lower = trimmed.to_ascii_lowercase();
    let with_scheme = if lower.starts_with("http://") || lower.starts_with("https://") {
        trimmed.to_string()
    } else if let Some(rest) = lower.strip_prefix("feed://") {
        format!("https://{}", &trimmed[trimmed.len() - rest.len()..])
    } else if trimmed.contains("://") {
        return Err(Error::InvalidUrl(input.to_string()));
    } else {
        format!("https://{trimmed}")
    };
    let mut url =
        url::Url::parse(&with_scheme).map_err(|_| Error::InvalidUrl(input.to_string()))?;
    if url.host_str().is_none_or(|h| h.is_empty()) {
        return Err(Error::InvalidUrl(input.to_string()));
    }
    url.set_fragment(None);
    Ok(url)
}

/// 源没有自带图标时，猜测站点根目录下的 favicon。界面在图片加载失败时会退回到默认图标。
pub fn favicon_url(site_url: Option<&str>, feed_url: &str) -> Option<String> {
    let base = site_url
        .and_then(|u| url::Url::parse(u).ok())
        .or_else(|| url::Url::parse(feed_url).ok())?;
    base.join("/favicon.ico").ok().map(String::from)
}

/// 取地址里的主机名，用作没有标题的源的显示名。
pub fn host_of(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()?
        .host_str()
        .map(|h| h.trim_start_matches("www.").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favicon_and_host() {
        assert_eq!(
            favicon_url(
                Some("https://blog.example.com/posts/"),
                "https://feeds.example.net/x"
            ),
            Some("https://blog.example.com/favicon.ico".to_string())
        );
        assert_eq!(
            favicon_url(None, "https://feeds.example.net/x.xml"),
            Some("https://feeds.example.net/favicon.ico".to_string())
        );
        assert_eq!(
            host_of("https://www.example.com/feed").as_deref(),
            Some("example.com")
        );
    }

    #[test]
    fn normalize() {
        assert_eq!(
            normalize_url("example.com/feed").unwrap().as_str(),
            "https://example.com/feed"
        );
        assert_eq!(
            normalize_url(" HTTP://Example.com ").unwrap().as_str(),
            "http://example.com/"
        );
        assert_eq!(
            normalize_url("feed://example.com/rss#top")
                .unwrap()
                .as_str(),
            "https://example.com/rss"
        );
        assert!(normalize_url("").is_err());
        assert!(normalize_url("ftp://example.com").is_err());
        assert!(normalize_url("javascript:alert(1)").is_err());
    }
}
