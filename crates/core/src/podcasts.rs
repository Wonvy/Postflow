//! 搜索播客：用苹果播客目录的公开搜索接口（iTunes Search API，免费、不要密钥），按节目名找到订阅地址。
//! 只用它来找地址，订阅以后照常直接抓节目自己的 RSS。只给白名单客户端发订阅源的平台（小宇宙）上的节目
//! 例外：订阅地址换成苹果目录的单集查询（见 postflow-feed 的 apple 模块）。

use serde::{Deserialize, Serialize};

use crate::Core;
use crate::rpc::RpcError;

const SEARCH_URL: &str = "https://itunes.apple.com/search";
const LIMIT: usize = 30;

/// 搜到的一个节目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Podcast {
    pub title: String,
    pub author: String,
    /// 节目的 RSS 地址。
    pub feed_url: String,
    /// 封面。
    pub artwork: Option<String>,
    pub genre: Option<String>,
    pub episodes: Option<u32>,
}

#[derive(Deserialize)]
struct Response {
    #[serde(default)]
    results: Vec<Entry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    collection_id: Option<u64>,
    collection_name: Option<String>,
    artist_name: Option<String>,
    feed_url: Option<String>,
    artwork_url600: Option<String>,
    artwork_url100: Option<String>,
    primary_genre_name: Option<String>,
    track_count: Option<u32>,
}

fn encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// 搜索地址。`country` 是两个字母的地区代码，中文界面用 cn，搜到的中文节目多一些。
pub(crate) fn search_url(term: &str, country: &str) -> String {
    let country: String = country
        .chars()
        .filter(char::is_ascii_alphabetic)
        .take(2)
        .collect::<String>()
        .to_ascii_lowercase();
    let country = if country.len() == 2 {
        country
    } else {
        "us".into()
    };
    format!(
        "{SEARCH_URL}?media=podcast&entity=podcast&limit={LIMIT}&country={country}&term={}",
        encode(term.trim())
    )
}

/// 读搜索结果。没有订阅地址的（只在苹果平台上的节目）去掉，同一个地址只留一个。
pub(crate) fn parse_results(json: &str) -> Result<Vec<Podcast>, serde_json::Error> {
    let response: Response = serde_json::from_str(json)?;
    let mut seen = std::collections::HashSet::new();
    Ok(response
        .results
        .into_iter()
        .filter_map(|e| {
            let feed_url = e
                .feed_url
                .map(|u| u.trim().to_string())
                .filter(|u| u.starts_with("https://") || u.starts_with("http://"))?;
            // 直接订阅会被拒绝的，改从苹果目录读
            let feed_url = match e.collection_id {
                Some(id) if postflow_feed::apple::feed_is_blocked(&feed_url) => {
                    postflow_feed::apple::lookup_url(id)
                }
                _ => feed_url,
            };
            if !seen.insert(feed_url.clone()) {
                return None;
            }
            Some(Podcast {
                title: e.collection_name.unwrap_or_default().trim().to_string(),
                author: e.artist_name.unwrap_or_default().trim().to_string(),
                feed_url,
                artwork: e
                    .artwork_url600
                    .or(e.artwork_url100)
                    .filter(|u| u.starts_with("https://")),
                genre: e.primary_genre_name,
                episodes: e.track_count,
            })
        })
        .collect())
}

/// 搜索结果里订阅地址正好是 `feed_url` 的节目在苹果目录里的编号。
pub(crate) fn collection_for(json: &str, feed_url: &str) -> Option<u64> {
    let response: Response = serde_json::from_str(json).ok()?;
    let want = feed_url.trim().trim_end_matches('/');
    response.results.into_iter().find_map(|e| {
        let url = e.feed_url?;
        (url.trim().trim_end_matches('/') == want).then_some(e.collection_id?)
    })
}

impl Core {
    /// 只给白名单客户端发订阅源的节目（小宇宙）改从苹果目录读：按节目名在目录里搜，订阅地址和
    /// 这个一样的才算同一个节目。返回苹果目录的单集查询地址；找不到是 `None`。
    pub(crate) async fn apple_lookup_for(&self, title: &str, feed_url: &str) -> Option<String> {
        if title.trim().is_empty() {
            return None;
        }
        let text = self
            .client
            .fetch_text(&search_url(title, "cn"))
            .await
            .ok()?;
        collection_for(&text, feed_url).map(postflow_feed::apple::lookup_url)
    }

    pub(crate) async fn search_podcasts(
        &self,
        term: &str,
        country: &str,
    ) -> Result<Vec<Podcast>, RpcError> {
        if term.trim().is_empty() {
            return Ok(Vec::new());
        }
        let text = self.client.fetch_text(&search_url(term, country)).await?;
        parse_results(&text).map_err(|e| RpcError::new("bad_response", e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_search_urls() {
        assert_eq!(
            search_url(" 忽左忽右 & more ", "CN"),
            "https://itunes.apple.com/search?media=podcast&entity=podcast&limit=30&country=cn&term=%E5%BF%BD%E5%B7%A6%E5%BF%BD%E5%8F%B3+%26+more"
        );
        assert!(search_url("x", "zz-ZZ!").contains("country=zz"));
        assert!(search_url("x", "").contains("country=us"));
    }

    #[test]
    fn keeps_shows_with_a_feed() {
        let json = r#"{"resultCount":3,"results":[
          {"collectionName":"Show A","artistName":"Ann","feedUrl":"https://a.example.com/rss",
           "artworkUrl100":"https://img.example.com/100.jpg","artworkUrl600":"https://img.example.com/600.jpg",
           "primaryGenreName":"Technology","trackCount":120},
          {"collectionName":"Apple only","artistName":"B"},
          {"collectionName":"Show A again","feedUrl":"https://a.example.com/rss"}
        ]}"#;
        let shows = parse_results(json).unwrap();
        assert_eq!(shows.len(), 1);
        assert_eq!(shows[0].title, "Show A");
        assert_eq!(shows[0].feed_url, "https://a.example.com/rss");
        assert_eq!(
            shows[0].artwork.as_deref(),
            Some("https://img.example.com/600.jpg")
        );
        assert_eq!(shows[0].episodes, Some(120));
        assert!(parse_results("not json").is_err());
    }

    #[test]
    fn finds_the_show_by_its_feed() {
        let json = r#"{"results":[
          {"collectionId":1,"feedUrl":"https://feed.xyzfm.space/other"},
          {"collectionId":1834069371,"feedUrl":"https://feed.xyzfm.space/wmnkvmrpwuww"}]}"#;
        assert_eq!(
            collection_for(json, "https://feed.xyzfm.space/wmnkvmrpwuww/"),
            Some(1834069371)
        );
        assert_eq!(collection_for(json, "https://feed.xyzfm.space/nope"), None);
        assert_eq!(
            collection_for("oops", "https://feed.xyzfm.space/nope"),
            None
        );
    }

    #[test]
    fn blocked_hosts_go_through_apple() {
        let json = r#"{"results":[{"collectionId":1834069371,"collectionName":"罗永浩的十字路口",
          "feedUrl":"https://feed.xyzfm.space/wmnkvmrpwuww"}]}"#;
        let shows = parse_results(json).unwrap();
        assert_eq!(
            shows[0].feed_url,
            "https://itunes.apple.com/lookup?id=1834069371&entity=podcastEpisode&limit=200"
        );
    }
}
