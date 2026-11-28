//! 苹果播客目录里的节目：有的播客托管平台只给白名单里的客户端发订阅源（比如小宇宙的
//! `feed.xyzfm.space`，对 ShibaPost 回「403 访问受限」的网页），这些节目改从苹果播客目录的
//! 公开查询接口（iTunes Search API 的 lookup，免费、不要密钥）读单集列表。
//!
//! 订阅地址就是查询接口的地址，所以抓取、预览、刷新都和普通订阅源一样走，只在解析时换成这里。
//! 音频文件还是节目自己托管的地址，直接放。不碰被拦的订阅源，也不伪装成别的客户端。
//!
//! 和直接读 RSS 比：只有最近 200 期；单集介绍是纯文本；苹果目录更新节目有一点延迟。

use serde::Deserialize;
use url::Url;

use crate::parse::{ParsedEntry, ParsedFeed, ParsedMedia, mime_from_url, text_to_html, truncate};
use crate::sanitize::html_to_text;
use crate::telegram::parse_time;
use crate::{Error, Result};

/// 一次最多取多少期（接口的上限）。
const LIMIT: u32 = 200;
const SUMMARY_CHARS: usize = 280;

/// 只给白名单客户端发订阅源的托管平台。这些平台上的节目订阅时改走苹果目录。
const BLOCKING_HOSTS: [&str; 1] = ["feed.xyzfm.space"];

/// 节目在苹果目录里的编号对应的订阅地址。
pub fn lookup_url(id: u64) -> String {
    format!("https://itunes.apple.com/lookup?id={id}&entity=podcastEpisode&limit={LIMIT}")
}

/// 这个订阅地址所在的平台只给白名单客户端发订阅源（直接订阅会被拒绝）。
pub fn feed_is_blocked(feed_url: &str) -> bool {
    Url::parse(feed_url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
        .is_some_and(|h| BLOCKING_HOSTS.contains(&h.as_str()))
}

/// 这个地址是不是苹果目录的单集查询（订阅地址），是的话解析时用这里。
pub(crate) fn is_lookup(url: &str) -> bool {
    let Ok(u) = Url::parse(url) else {
        return false;
    };
    if u.host_str() != Some("itunes.apple.com") || u.path() != "/lookup" {
        return false;
    }
    let mut id = false;
    let mut episodes = false;
    for (k, v) in u.query_pairs() {
        match k.as_ref() {
            "id" => id = !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()),
            "entity" => episodes = v == "podcastEpisode",
            _ => {}
        }
    }
    id && episodes
}

#[derive(Deserialize)]
struct Response {
    #[serde(default)]
    results: Vec<Entry>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Entry {
    wrapper_type: Option<String>,
    kind: Option<String>,
    collection_name: Option<String>,
    artist_name: Option<String>,
    collection_view_url: Option<String>,
    artwork_url600: Option<String>,
    artwork_url160: Option<String>,
    // 单集
    track_id: Option<u64>,
    track_name: Option<String>,
    track_view_url: Option<String>,
    episode_guid: Option<String>,
    episode_url: Option<String>,
    episode_content_type: Option<String>,
    episode_file_extension: Option<String>,
    track_time_millis: Option<u64>,
    release_date: Option<String>,
    description: Option<String>,
    short_description: Option<String>,
}

fn web(url: Option<String>) -> Option<String> {
    url.map(|u| u.trim().to_string())
        .filter(|u| u.starts_with("https://") || u.starts_with("http://"))
}

fn text(s: Option<String>) -> String {
    s.unwrap_or_default().trim().to_string()
}

/// 单集的 MIME 类型：先按文件扩展名，再按接口给的大类（audio / video）。
fn mime_of(entry: &Entry, url: &str) -> String {
    let ext = entry
        .episode_file_extension
        .as_deref()
        .map(|e| format!("x.{}", e.trim().trim_start_matches('.')));
    if let Some(m) = ext
        .as_deref()
        .and_then(mime_from_url)
        .or_else(|| mime_from_url(url))
    {
        return m.to_string();
    }
    match entry.episode_content_type.as_deref() {
        Some("video") => "video/mp4".into(),
        _ => "audio/mpeg".into(),
    }
}

/// 把查询结果读成订阅源。节目本身是 `wrapperType: track, kind: podcast` 那一条，其余是单集。
pub(crate) fn parse(bytes: &[u8], url: &str) -> Result<ParsedFeed> {
    let response: Response =
        serde_json::from_slice(bytes).map_err(|e| Error::NotAFeed(e.to_string()))?;
    let mut show: Option<Entry> = None;
    let mut episodes = Vec::new();
    for entry in response.results {
        match entry.wrapper_type.as_deref() {
            Some("podcastEpisode") => episodes.push(entry),
            Some("track") if entry.kind.as_deref() == Some("podcast") && show.is_none() => {
                show = Some(entry);
            }
            _ => {}
        }
    }
    let Some(show) = show else {
        // 节目从苹果目录下架了，或者编号不对
        return Err(Error::NotAFeed(format!(
            "no podcast in Apple Podcasts for {url}"
        )));
    };
    let artwork = web(show.artwork_url600.clone()).or_else(|| web(show.artwork_url160.clone()));

    let entries = episodes
        .into_iter()
        .filter_map(|e| {
            let audio = web(e.episode_url.clone())?;
            let guid = e
                .episode_guid
                .as_deref()
                .map(str::trim)
                .filter(|g| !g.is_empty())
                .map(str::to_string)
                .or_else(|| e.track_id.map(|id| format!("apple:{id}")))
                .unwrap_or_else(|| audio.clone());
            let body = e
                .description
                .as_deref()
                .or(e.short_description.as_deref())
                .map(str::trim)
                .unwrap_or_default();
            // 接口给的多半是纯文本，偶尔是网页片段
            let content = if body.contains("</") || body.contains("<br") {
                body.to_string()
            } else if body.is_empty() {
                String::new()
            } else {
                text_to_html(body)
            };
            let summary_source = e
                .short_description
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(body);
            // 单集封面和节目封面一样时不重复存
            let image = web(e.artwork_url600.clone()).filter(|i| Some(i) != artwork.as_ref());
            Some(ParsedEntry {
                guid,
                url: web(e.track_view_url.clone()),
                title: text(e.track_name.clone()),
                author: show.artist_name.clone().map(|a| a.trim().to_string()),
                published_ms: e.release_date.as_deref().and_then(parse_time),
                summary: truncate(&html_to_text(&text_to_html(summary_source)), SUMMARY_CHARS),
                content,
                image,
                media: Some(ParsedMedia {
                    mime: mime_of(&e, &audio),
                    url: audio,
                    duration: e.track_time_millis.map(|ms| ms / 1000).filter(|s| *s > 0),
                    size: None,
                    chapters: None,
                    transcript: None,
                }),
            })
        })
        .collect();

    Ok(ParsedFeed {
        title: text(show.collection_name),
        site_url: web(show.collection_view_url),
        icon_url: artwork,
        description: show
            .artist_name
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty()),
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOOKUP: &str = r#"{"resultCount":3,"results":[
{"wrapperType":"track","kind":"podcast","collectionId":1834069371,"trackId":1834069371,
 "artistName":"罗永浩","collectionName":"罗永浩的十字路口","trackName":"罗永浩的十字路口",
 "collectionViewUrl":"https://podcasts.apple.com/us/podcast/id1834069371?uo=4",
 "feedUrl":"https://feed.xyzfm.space/wmnkvmrpwuww",
 "artworkUrl600":"https://is1-ssl.mzstatic.com/image/thumb/show/600x600bb.jpg"},
{"wrapperType":"podcastEpisode","kind":"podcast-episode","trackId":1000736,
 "trackName":"纪录片导演陈晓卿×罗永浩","episodeGuid":"6afbb377fe62f8f52c4c4f3a",
 "episodeUrl":"https://dts-api.xiaoyuzhoufm.com/track/x/y/media.xyzcdn.net/x/a.m4a",
 "episodeContentType":"audio","episodeFileExtension":"m4a","trackTimeMillis":15554000,
 "releaseDate":"2026-11-17T04:00:10Z",
 "trackViewUrl":"https://podcasts.apple.com/us/podcast/x/id1834069371?i=1000736",
 "description":"第一段 <好>\n第二行\n\n第二段",
 "artworkUrl600":"https://is1-ssl.mzstatic.com/image/thumb/show/600x600bb.jpg"},
{"wrapperType":"podcastEpisode","kind":"podcast-episode","trackId":1000737,
 "trackName":"没有音频的一期"}
]}"#;

    #[test]
    fn reads_the_show_and_its_episodes() {
        let url = lookup_url(1834069371);
        assert!(is_lookup(&url));
        let feed = parse(LOOKUP.as_bytes(), &url).unwrap();
        assert_eq!(feed.title, "罗永浩的十字路口");
        assert_eq!(
            feed.icon_url.as_deref(),
            Some("https://is1-ssl.mzstatic.com/image/thumb/show/600x600bb.jpg")
        );
        // 没有音频地址的一期不要
        assert_eq!(feed.entries.len(), 1);
        let ep = &feed.entries[0];
        assert_eq!(ep.guid, "6afbb377fe62f8f52c4c4f3a");
        assert_eq!(ep.title, "纪录片导演陈晓卿×罗永浩");
        assert_eq!(ep.published_ms, Some(1_794_888_010_000));
        assert_eq!(
            ep.content,
            "<p>第一段 &lt;好&gt;<br>第二行</p><p>第二段</p>"
        );
        assert!(ep.summary.starts_with("第一段 <好>"));
        // 和节目封面一样，不重复存
        assert_eq!(ep.image, None);
        let media = ep.media.as_ref().unwrap();
        assert_eq!(media.mime, "audio/mp4");
        assert_eq!(media.duration, Some(15_554));
    }

    #[test]
    fn missing_show_is_not_a_feed() {
        let err = parse(br#"{"resultCount":0,"results":[]}"#, &lookup_url(1)).unwrap_err();
        assert_eq!(err.code(), "not_a_feed");
    }

    #[test]
    fn recognizes_addresses() {
        assert!(is_lookup(
            "https://itunes.apple.com/lookup?id=123&entity=podcastEpisode&limit=200"
        ));
        assert!(!is_lookup("https://itunes.apple.com/lookup?id=123"));
        assert!(!is_lookup(
            "https://itunes.apple.com/search?term=x&entity=podcastEpisode"
        ));
        assert!(!is_lookup(
            "https://example.com/lookup?id=123&entity=podcastEpisode"
        ));
        assert!(feed_is_blocked("https://feed.xyzfm.space/wmnkvmrpwuww"));
        assert!(!feed_is_blocked("https://feeds.simplecast.com/abc"));
    }
}
