//! 已知网站：页面上没有声明订阅源、但订阅地址有固定规律的网站。用户贴的是主页或详情页地址时，
//! 不用请求网页，直接换算成订阅地址。和界面「常用网站」（src/lib/quickSources.ts）里的拼法一致。

use url::Url;

use crate::telegram;

fn host(url: &Url) -> &str {
    url.host_str()
        .unwrap_or_default()
        .trim_start_matches("www.")
}

fn segments(url: &Url) -> Vec<&str> {
    url.path_segments()
        .map(|s| s.filter(|p| !p.is_empty()).collect())
        .unwrap_or_default()
}

fn word(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}

/// 网页地址对应的订阅地址；不认识返回 `None`。
pub(crate) fn feed_for(url: &Url) -> Option<String> {
    if let Some(page) = telegram::channel_page(url) {
        return Some(page);
    }
    let h = host(url);
    let parts = segments(url);
    match h {
        // pypi.org/project/requests/
        "pypi.org" => match parts.as_slice() {
            ["project", name, ..] if word(name) => Some(format!(
                "https://pypi.org/rss/project/{}/releases.xml",
                name.to_ascii_lowercase()
            )),
            _ => None,
        },
        // store.steampowered.com/app/570/Dota_2/
        "store.steampowered.com" | "steamcommunity.com" => match parts.as_slice() {
            ["app", id, ..] if id.bytes().all(|b| b.is_ascii_digit()) => Some(format!(
                "https://store.steampowered.com/feeds/news/app/{id}/"
            )),
            _ => None,
        },
        // letterboxd.com/dave/、letterboxd.com/dave/films/；film/… 是电影页，不是用户
        "letterboxd.com" => match parts.as_slice() {
            [user, ..]
                if word(user)
                    && !matches!(
                        *user,
                        "film" | "films" | "lists" | "members" | "journal" | "search"
                    ) =>
            {
                Some(format!("https://letterboxd.com/{user}/rss/"))
            }
            _ => None,
        },
        // www.tumblr.com/staff → staff.tumblr.com/rss
        "tumblr.com" => match parts.as_slice() {
            [blog, ..]
                if word(blog)
                    && !matches!(*blog, "dashboard" | "explore" | "tagged" | "search") =>
            {
                Some(format!(
                    "https://{}.tumblr.com/rss",
                    blog.to_ascii_lowercase()
                ))
            }
            _ => None,
        },
        _ if h.ends_with(".tumblr.com") && !h.starts_with("assets.") => {
            Some(format!("https://{h}/rss"))
        }
        // pinterest.com/用户/ 或 /用户/画板/，各国域名都一样
        _ if h.starts_with("pinterest.") || h.contains(".pinterest.") => match parts.as_slice() {
            [user] if word(user) && !matches!(*user, "pin" | "search" | "ideas" | "today") => {
                Some(format!("https://www.pinterest.com/{user}/feed.rss"))
            }
            [user, board, ..]
                if word(user)
                    && word(board)
                    && !matches!(*user, "pin" | "search" | "ideas" | "today") =>
            {
                Some(format!("https://www.pinterest.com/{user}/{board}.rss"))
            }
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(s: &str) -> Option<String> {
        feed_for(&Url::parse(s).unwrap())
    }

    #[test]
    fn known_pages_map_to_their_feeds() {
        assert_eq!(
            feed("https://t.me/durov").as_deref(),
            Some("https://t.me/s/durov")
        );
        assert_eq!(
            feed("https://pypi.org/project/Requests/").as_deref(),
            Some("https://pypi.org/rss/project/requests/releases.xml")
        );
        assert_eq!(
            feed("https://store.steampowered.com/app/570/Dota_2/").as_deref(),
            Some("https://store.steampowered.com/feeds/news/app/570/")
        );
        assert_eq!(
            feed("https://letterboxd.com/dave/films/").as_deref(),
            Some("https://letterboxd.com/dave/rss/")
        );
        assert_eq!(feed("https://letterboxd.com/film/dune-part-two/"), None);
        assert_eq!(
            feed("https://staff.tumblr.com/post/123").as_deref(),
            Some("https://staff.tumblr.com/rss")
        );
        assert_eq!(
            feed("https://www.tumblr.com/staff").as_deref(),
            Some("https://staff.tumblr.com/rss")
        );
        assert_eq!(
            feed("https://www.pinterest.com/someone/recipes/").as_deref(),
            Some("https://www.pinterest.com/someone/recipes.rss")
        );
        assert_eq!(
            feed("https://jp.pinterest.com/someone/").as_deref(),
            Some("https://www.pinterest.com/someone/feed.rss")
        );
        assert_eq!(feed("https://www.pinterest.com/pin/12345/"), None);
        assert_eq!(feed("https://example.com/project/x"), None);
    }
}
