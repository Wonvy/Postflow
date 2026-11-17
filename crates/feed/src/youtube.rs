//! YouTube：频道、播放列表的网页地址换成 YouTube 自带的订阅源地址。
//!
//! YouTube 给每个频道和播放列表都提供 Atom 源（`/feeds/videos.xml`），只是网页上不显眼。
//! 用户贴 `https://www.youtube.com/@名字` 这样的频道地址时，从频道页里找出频道 ID，
//! 顺便带上频道名和头像，订阅后侧栏里显示的就是博主自己的头像。

use scraper::{Html, Selector};
use url::Url;

use crate::Result;
use crate::client::{Client, decode_html};
use crate::sanitize::collapse_whitespace;

/// 不带这个 Cookie，欧洲等地的访问会先被转到 cookie 同意页。
const CONSENT_COOKIE: &str = "SOCS=CAI; CONSENT=YES+1";

/// 找到的频道或播放列表。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Found {
    pub feed_url: String,
    pub title: Option<String>,
    pub avatar: Option<String>,
}

fn is_youtube(url: &Url) -> bool {
    matches!(
        url.host_str()
            .map(|h| h.trim_start_matches("www.").trim_start_matches("m.")),
        Some("youtube.com" | "youtu.be" | "music.youtube.com")
    )
}

fn channel_feed(id: &str) -> String {
    format!("https://www.youtube.com/feeds/videos.xml?channel_id={id}")
}

/// 频道 ID：UC 开头，后面 22 个字母、数字、`-`、`_`。
fn is_channel_id(id: &str) -> bool {
    id.len() == 24
        && id.starts_with("UC")
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// 不用请求网页就能直接换算的地址：`/channel/UC…` 和带 `list=` 的播放列表。
fn direct(url: &Url) -> Option<String> {
    if let Some(list) = url
        .query_pairs()
        .find(|(k, _)| k == "list")
        .map(|(_, v)| v.into_owned())
        .filter(|v| {
            !v.is_empty()
                && v.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
    {
        return Some(format!(
            "https://www.youtube.com/feeds/videos.xml?playlist_id={list}"
        ));
    }
    let mut segments = url.path_segments()?;
    if segments.next() == Some("channel") {
        let id = segments.next()?;
        return is_channel_id(id).then(|| channel_feed(id));
    }
    None
}

fn meta(doc: &Html, property: &str) -> Option<String> {
    let selector = Selector::parse(&format!(
        r#"meta[property="{property}"], meta[name="{property}"]"#
    ))
    .ok()?;
    doc.select(&selector)
        .find_map(|m| m.attr("content"))
        .map(collapse_whitespace)
        .filter(|v| !v.is_empty())
}

/// 从频道页里读出频道 ID、名字和头像。
pub(crate) fn channel_in_html(html: &str) -> Option<Found> {
    let doc = Html::parse_document(html);
    let from_links = || {
        let selector = Selector::parse(r#"link[rel="alternate"][href*="channel_id="], link[rel="canonical"][href*="/channel/"], meta[itemprop="identifier"], meta[itemprop="channelId"]"#).ok()?;
        doc.select(&selector).find_map(|el| {
            let value = el.attr("href").or_else(|| el.attr("content"))?;
            let id = value
                .rsplit(['=', '/'])
                .next()
                .unwrap_or(value)
                .split(['?', '&', '#'])
                .next()
                .unwrap_or_default();
            is_channel_id(id).then(|| id.to_string())
        })
    };
    // 页面里的数据：`"externalId":"UC…"`，频道页一定有
    let from_data = || {
        let at = html.find(r#""externalId":""#)? + r#""externalId":""#.len();
        let id = html[at..].split('"').next()?;
        is_channel_id(id).then(|| id.to_string())
    };
    let id = from_links().or_else(from_data)?;
    Some(Found {
        feed_url: channel_feed(&id),
        title: meta(&doc, "og:title"),
        avatar: meta(&doc, "og:image").filter(|u| u.starts_with("https://")),
    })
}

impl Client {
    /// 是 YouTube 的频道或播放列表地址时，返回它的订阅源。不是 YouTube 返回 `None`。
    /// 视频页之类找不出频道时也返回 `None`，交给通用的发现流程。
    pub(crate) async fn youtube(&self, url: &Url) -> Result<Option<Found>> {
        if !is_youtube(url) {
            return Ok(None);
        }
        // 已经是订阅源地址了
        if url.path() == "/feeds/videos.xml" {
            return Ok(None);
        }
        let direct = direct(url);
        // 频道地址直接换算；页面还是要请求一次，拿频道名和头像，拿不到也不要紧
        let page = self
            .get_with_cookie(url.as_str(), None, None, Some(CONSENT_COOKIE))
            .await;
        let found = match page {
            Ok(Some(page)) => channel_in_html(&decode_html(&page)),
            _ => None,
        };
        // 视频页上的分享图是视频封面，不是频道头像
        let video = url.host_str() == Some("youtu.be")
            || url.path().starts_with("/watch")
            || url.path().starts_with("/shorts");
        let found = found.map(|f| Found {
            avatar: f.avatar.filter(|_| !video),
            ..f
        });
        Ok(match (direct, found) {
            // 播放列表：用列表的地址，频道页上的名字和头像不对，只留标题
            (Some(feed_url), found) if feed_url.contains("playlist_id=") => Some(Found {
                feed_url,
                title: found.and_then(|f| f.title),
                avatar: None,
            }),
            (Some(feed_url), Some(found)) => Some(Found { feed_url, ..found }),
            (Some(feed_url), None) => Some(Found {
                feed_url,
                title: None,
                avatar: None,
            }),
            (None, found) => found,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "UCez62GvvsH05IunJJsRuvuQ";

    #[test]
    fn recognizes_youtube_hosts() {
        for u in [
            "https://www.youtube.com/@someone",
            "https://m.youtube.com/@someone",
            "https://youtube.com/channel/x",
            "https://youtu.be/abc",
        ] {
            assert!(is_youtube(&Url::parse(u).unwrap()), "{u}");
        }
        assert!(!is_youtube(&Url::parse("https://notyoutube.com/").unwrap()));
    }

    #[test]
    fn channel_and_playlist_urls_need_no_page() {
        let channel = Url::parse(&format!("https://www.youtube.com/channel/{ID}/videos")).unwrap();
        assert_eq!(direct(&channel), Some(channel_feed(ID)));
        let list = Url::parse("https://www.youtube.com/playlist?list=PLabc-_123").unwrap();
        assert_eq!(
            direct(&list).as_deref(),
            Some("https://www.youtube.com/feeds/videos.xml?playlist_id=PLabc-_123")
        );
        assert_eq!(
            direct(&Url::parse("https://www.youtube.com/@someone").unwrap()),
            None
        );
        assert_eq!(
            direct(&Url::parse("https://www.youtube.com/channel/nope").unwrap()),
            None
        );
    }

    #[test]
    fn reads_channel_page() {
        let html = format!(
            r#"<html><head>
            <meta property="og:title" content="JSK-koubou">
            <meta property="og:image" content="https://yt3.example.com/avatar=s900">
            <link rel="alternate" type="application/rss+xml" title="RSS" href="https://www.youtube.com/feeds/videos.xml?channel_id={ID}">
            <link rel="canonical" href="https://www.youtube.com/channel/{ID}">
            </head><body></body></html>"#
        );
        assert_eq!(
            channel_in_html(&html),
            Some(Found {
                feed_url: channel_feed(ID),
                title: Some("JSK-koubou".into()),
                avatar: Some("https://yt3.example.com/avatar=s900".into()),
            })
        );
    }

    #[test]
    fn falls_back_to_page_data() {
        let html = format!(
            r#"<html><body><script>var x = {{"externalId":"{ID}","title":"x"}};</script></body></html>"#
        );
        assert_eq!(
            channel_in_html(&html).map(|f| f.feed_url),
            Some(channel_feed(ID))
        );
        assert_eq!(
            channel_in_html("<html><body>no channel</body></html>"),
            None
        );
    }
}
