//! 稍后读：把任意网页存下来，和订阅的文章一样读。
//!
//! 存下的网页是内置「稍后读」订阅源（见 shibapost-store 的 `LATER_URL`）的文章，写进文章日志，
//! 所以会同步到别的设备。存的时候就把正文抓下来一起存，原网页以后删了改了也还在。
//! 从 Pocket、Instapaper 和浏览器书签导入的只存地址和标题，打开时再去抓全文。

use std::sync::Arc;

use regex::Regex;
use serde::Serialize;
use shibapost_feed::{clean_url, html_to_text};
use shibapost_store::{Article, ItemPatch, hlc, ids};

use crate::rpc::RpcError;
use crate::{Core, Scope, Session};

/// 一次最多导入多少条。
const MAX_IMPORT: usize = 50_000;

#[derive(Debug, Serialize)]
pub(crate) struct Saved {
    pub id: String,
    pub title: String,
    /// 以前就存过（这次把它从归档里拿回来）。
    pub existed: bool,
    /// 网页打不开或者认不出正文时的原因。条目照样存了，打开时会再试一次。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Imported {
    /// 文件里认出了多少条链接。
    pub found: usize,
    /// 真正新增的条数（以前存过的不算）。
    pub added: usize,
}

/// 导入文件里的一条。
#[derive(Debug, Clone, PartialEq)]
struct Entry {
    url: String,
    title: String,
    /// 存进去的时间，毫秒。
    added: Option<i64>,
    archived: bool,
    starred: bool,
}

fn now() -> i64 {
    hlc::now_ms() as i64
}

impl Core {
    /// 存一个网页。
    pub(crate) async fn save_later(
        self: &Arc<Self>,
        session: &Arc<Session>,
        url: &str,
    ) -> Result<Saved, RpcError> {
        let clean = clean_url(url)?;
        let guid = clean.to_string();
        let feed = session.db(|s| s.ensure_later_feed()).await?;
        let id = ids::item_id(&feed, &guid);
        let now = now();
        let mut article = Article {
            id: id.clone(),
            feed: feed.clone(),
            guid: guid.clone(),
            url: Some(guid.clone()),
            title: guid.clone(),
            author: None,
            published: now,
            fetched: now,
            summary: String::new(),
            content: String::new(),
            image: None,
            media: None,
        };
        let warning = match self.client.save_page(&guid).await {
            Ok(page) => {
                // 跳转后的地址也去掉跟踪参数
                article.url = Some(clean_url(&page.url).map_or(page.url, String::from));
                article.title = page.title;
                article.author = page.author;
                article.image = page.image;
                article.summary = page.excerpt;
                article.content = page.content;
                page.content_error
            }
            Err(err) => {
                tracing::info!("read later: could not fetch {guid}: {err}");
                Some(err.to_string())
            }
        };
        let title = article.title.clone();
        let target = id.clone();
        let added = session
            .db(move |s| {
                let added = s.add_articles(vec![article])?;
                if added == 0 {
                    // 存过的：从归档里拿回来，标为未读
                    s.set_item_state(
                        std::slice::from_ref(&target),
                        &ItemPatch {
                            read: Some(false),
                            archived: Some(false),
                            ..Default::default()
                        },
                    )?;
                }
                Ok(added)
            })
            .await?;
        self.emit_changed(&[Scope::Feeds, Scope::Items], None);
        Ok(Saved {
            id,
            title,
            existed: added == 0,
            warning,
        })
    }

    /// 导入 Pocket、Instapaper 的导出文件或者浏览器书签。
    pub(crate) async fn import_later(
        self: &Arc<Self>,
        session: &Arc<Session>,
        text: &str,
    ) -> Result<Imported, RpcError> {
        let mut entries = parse_import(text);
        if entries.is_empty() {
            return Err(RpcError::new(
                "nothing_to_import",
                "no links found in this file",
            ));
        }
        entries.truncate(MAX_IMPORT);
        let found = entries.len();
        let added = session
            .db(move |s| {
                let feed = s.ensure_later_feed()?;
                let now = now();
                let mut archived = Vec::new();
                let mut starred = Vec::new();
                let count = entries.len() as i64;
                let articles: Vec<Article> = entries
                    .into_iter()
                    .enumerate()
                    .map(|(i, e)| {
                        let id = ids::item_id(&feed, &e.url);
                        if e.archived {
                            archived.push(id.clone());
                        }
                        if e.starred {
                            starred.push(id.clone());
                        }
                        Article {
                            id,
                            feed: feed.clone(),
                            guid: e.url.clone(),
                            url: Some(e.url),
                            title: e.title,
                            author: None,
                            // 没有时间的按文件里的顺序排（前面的算新）
                            published: e.added.unwrap_or(now - i as i64).min(now),
                            fetched: now - (count - i as i64),
                            summary: String::new(),
                            content: String::new(),
                            image: None,
                            media: None,
                        }
                    })
                    .collect();
                let added = s.add_articles(articles)?;
                if !archived.is_empty() {
                    s.set_item_state(
                        &archived,
                        &ItemPatch {
                            read: Some(true),
                            archived: Some(true),
                            ..Default::default()
                        },
                    )?;
                }
                if !starred.is_empty() {
                    s.set_item_state(
                        &starred,
                        &ItemPatch {
                            starred: Some(true),
                            ..Default::default()
                        },
                    )?;
                }
                Ok(added)
            })
            .await?;
        self.emit_changed(&[Scope::Feeds, Scope::Items], None);
        Ok(Imported { found, added })
    }
}

/// 认出文件的格式并取出链接：HTML（Pocket 的旧导出、浏览器书签）、CSV（Pocket 的新导出、
/// Instapaper），都不是时把文本里的每个网址当一条。重复的只留第一条。
fn parse_import(text: &str) -> Vec<Entry> {
    let text = text.trim_start_matches('\u{feff}');
    let raw = if Regex::new(r"(?i)<a\s[^>]*href=").unwrap().is_match(text) {
        parse_html(text)
    } else if text.lines().next().is_some_and(|l| {
        l.to_ascii_lowercase()
            .split(',')
            .any(|c| c.trim().trim_matches('"') == "url")
    }) {
        parse_csv(text)
    } else {
        parse_plain(text)
    };
    let mut seen = std::collections::HashSet::new();
    raw.into_iter()
        .filter_map(|mut e| {
            let url = clean_url(&e.url).ok()?.to_string();
            if !seen.insert(url.clone()) {
                return None;
            }
            if e.title.trim().is_empty() {
                e.title = url.clone();
            }
            e.url = url;
            Some(e)
        })
        .collect()
}

/// 秒、毫秒、微秒都有人用，统一成毫秒。
fn timestamp(value: &str) -> Option<i64> {
    let n: i64 = value.trim().parse().ok().filter(|n| *n > 0)?;
    Some(match n {
        n if n > 100_000_000_000_000 => n / 1000,
        n if n > 100_000_000_000 => n,
        n => n * 1000,
    })
}

/// Pocket 的 ril_export.html（「Unread」「Read Archive」两段）和浏览器导出的书签（Netscape 格式）。
fn parse_html(text: &str) -> Vec<Entry> {
    let token = Regex::new(r#"(?is)<h[1-6][^>]*>(.*?)</h[1-6]>|<a\s([^>]*)>(.*?)</a>"#).unwrap();
    let attr = |attrs: &str, name: &str| -> Option<String> {
        Regex::new(&format!(
            r#"(?is)\b{name}\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#
        ))
        .unwrap()
        .captures(attrs)
        .and_then(|c| c.get(1).or(c.get(2)).or(c.get(3)))
        .map(|m| html_to_text(m.as_str()))
    };
    let mut archived = false;
    let mut out = Vec::new();
    for cap in token.captures_iter(text) {
        if let Some(heading) = cap.get(1) {
            let heading = html_to_text(heading.as_str()).to_ascii_lowercase();
            archived = heading.contains("archive");
            continue;
        }
        let attrs = cap.get(2).map_or("", |m| m.as_str());
        let Some(url) = attr(attrs, "href") else {
            continue;
        };
        let added = attr(attrs, "time_added")
            .or_else(|| attr(attrs, "add_date"))
            .and_then(|t| timestamp(&t));
        out.push(Entry {
            url,
            title: html_to_text(cap.get(3).map_or("", |m| m.as_str())),
            added,
            archived,
            starred: false,
        });
    }
    out
}

/// Pocket 的 CSV（title,url,time_added,tags,status）和 Instapaper 的 CSV（URL,Title,Selection,Folder,Timestamp）。
fn parse_csv(text: &str) -> Vec<Entry> {
    let mut rows = csv_rows(text).into_iter();
    let Some(header) = rows.next() else {
        return Vec::new();
    };
    let col = |names: &[&str]| {
        header
            .iter()
            .position(|h| names.contains(&h.trim().to_ascii_lowercase().as_str()))
    };
    let (Some(url), title, time, status, folder) = (
        col(&["url"]),
        col(&["title"]),
        col(&["time_added", "timestamp", "added", "date"]),
        col(&["status"]),
        col(&["folder"]),
    ) else {
        return Vec::new();
    };
    let get = |row: &[String], i: Option<usize>| -> String {
        i.and_then(|i| row.get(i)).cloned().unwrap_or_default()
    };
    rows.filter(|r| r.iter().any(|c| !c.trim().is_empty()))
        .map(|row| {
            let status = get(&row, status).to_ascii_lowercase();
            let folder = get(&row, folder).to_ascii_lowercase();
            Entry {
                url: get(&row, Some(url)).trim().to_string(),
                title: get(&row, title).trim().to_string(),
                added: timestamp(&get(&row, time)),
                archived: status.contains("archive") || folder == "archive",
                starred: folder == "starred" || status.contains("favorite"),
            }
        })
        .collect()
}

/// 按 RFC 4180 切 CSV：引号里的逗号、换行和两个连着的引号都照原样保留。
fn csv_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', true) => quoted = false,
            ('"', false) if field.is_empty() => quoted = true,
            (',', false) => row.push(std::mem::take(&mut field)),
            ('\r', false) => {}
            ('\n', false) => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            (c, _) => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

/// 随便一段文字：每个网址算一条。
fn parse_plain(text: &str) -> Vec<Entry> {
    Regex::new(r#"https?://[^\s<>"']+"#)
        .unwrap()
        .find_iter(text)
        .map(|m| Entry {
            url: m
                .as_str()
                .trim_end_matches(['.', ',', ')', ';'])
                .to_string(),
            title: String::new(),
            added: None,
            archived: false,
            starred: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pocket_html_export() {
        let html = r#"<!DOCTYPE html><html><body>
            <h1>Unread</h1><ul>
            <li><a href="https://example.com/a?utm_source=pocket" time_added="1700000000" tags="">A &amp; B</a></li>
            </ul><h1>Read Archive</h1><ul>
            <li><a href="https://example.com/b" time_added="1600000000" tags="x">Old one</a></li>
            </ul></body></html>"#;
        let got = parse_import(html);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].url, "https://example.com/a");
        assert_eq!(got[0].title, "A & B");
        assert_eq!(got[0].added, Some(1_700_000_000_000));
        assert!(!got[0].archived);
        assert!(got[1].archived);
    }

    #[test]
    fn browser_bookmarks() {
        let html = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1>
            <DL><p><DT><H3>Reading</H3><DL><p>
            <DT><A HREF="https://example.org/x" ADD_DATE="1650000000">X</A>
            <DT><A HREF="javascript:void(0)">nope</A>
            </DL></DL>"#;
        let got = parse_import(html);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].title, "X");
        assert!(!got[0].archived);
    }

    #[test]
    fn pocket_and_instapaper_csv() {
        let pocket = "title,url,time_added,tags,status\n\
                      \"Hello, world\",https://example.com/1,1700000000,,unread\n\
                      Done,https://example.com/2,1690000000,a|b,archive\n";
        let got = parse_import(pocket);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].title, "Hello, world");
        assert!(got[1].archived);

        let insta = "URL,Title,Selection,Folder,Timestamp\r\n\
                     https://example.com/3,\"Say \"\"hi\"\"\",,Unread,1700000001\r\n\
                     https://example.com/4,Fav,,Starred,1700000002\r\n\
                     https://example.com/5,Gone,,Archive,1700000003\r\n";
        let got = parse_import(insta);
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].title, "Say \"hi\"");
        assert!(got[1].starred && !got[1].archived);
        assert!(got[2].archived);
    }

    #[test]
    fn plain_list_of_links() {
        let got = parse_import(
            "read https://a.example/x, and\nhttps://b.example/y.\nhttps://a.example/x",
        );
        let urls: Vec<_> = got.iter().map(|e| e.url.as_str()).collect();
        assert_eq!(urls, ["https://a.example/x", "https://b.example/y"]);
        assert_eq!(got[0].title, "https://a.example/x");
    }

    #[test]
    fn timestamps_in_any_unit() {
        assert_eq!(timestamp("1700000000"), Some(1_700_000_000_000));
        assert_eq!(timestamp("1700000000000"), Some(1_700_000_000_000));
        assert_eq!(timestamp("1700000000000000"), Some(1_700_000_000_000));
        assert_eq!(timestamp(""), None);
    }
}
