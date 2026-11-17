//! 全文搜索：标题、发件人、作者和正文。
//!
//! 用 SQLite 的 FTS5。它默认的分词器把一串连着的汉字当成一个词，「订阅」就搜不到「添加订阅时」，
//! 所以写进索引之前先把汉字（以及假名、韩文）拆成相邻两个字一组：「添加订阅」→「添加 加订 订阅 阅」，
//! 末尾多留一个单字，搜单个字时用前缀也能找到。搜索词按同样的规则拆开，作为短语去匹配。
//!
//! 哪些条目要重新收录由触发器记在 `search_dirty` 里：条目、文章正文、抓到的全文、邮件原文
//! 有任何写入都会记一笔，不管是哪条代码路径写的。核心在后台调用 `index_search` 一批批地处理。

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::Result;
use crate::store::Store;

/// 索引规则变了就加一，旧索引清空重建。
pub(crate) const SEARCH_VERSION: &str = "1";
/// 每条正文最多收录这么多字。
const BODY_CHARS: usize = 50_000;
/// 摘要片段在命中处前后各取多少字。
const SNIPPET_BEFORE: usize = 30;
const SNIPPET_LENGTH: usize = 140;

pub(crate) const SCHEMA: &str = r#"
-- 搜索索引里的每一篇：docid 是 FTS5 的行号，body 是收录的正文纯文字（显示搜索片段用）
CREATE TABLE IF NOT EXISTS search_docs (
    docid INTEGER PRIMARY KEY,
    id    TEXT NOT NULL UNIQUE,
    body  TEXT NOT NULL DEFAULT ''
);
-- 只存词，不存原文（原文在 search_docs.body 和各自的表里）
CREATE VIRTUAL TABLE IF NOT EXISTS search USING fts5(
    title, people, body,
    content = '', contentless_delete = 1,
    tokenize = 'unicode61 remove_diacritics 2'
);
-- 要重新收录的条目
CREATE TABLE IF NOT EXISTS search_dirty (id TEXT PRIMARY KEY) WITHOUT ROWID;

CREATE TRIGGER IF NOT EXISTS search_items_insert AFTER INSERT ON items
BEGIN INSERT OR IGNORE INTO search_dirty (id) VALUES (new.id); END;
CREATE TRIGGER IF NOT EXISTS search_items_update AFTER UPDATE OF title, author, summary ON items
BEGIN INSERT OR IGNORE INTO search_dirty (id) VALUES (new.id); END;
CREATE TRIGGER IF NOT EXISTS search_items_delete AFTER DELETE ON items
BEGIN INSERT OR IGNORE INTO search_dirty (id) VALUES (old.id); END;
CREATE TRIGGER IF NOT EXISTS search_content AFTER INSERT ON item_content
BEGIN INSERT OR IGNORE INTO search_dirty (id) VALUES (new.id); END;
CREATE TRIGGER IF NOT EXISTS search_fulltext AFTER INSERT ON item_fulltext
BEGIN INSERT OR IGNORE INTO search_dirty (id) VALUES (new.id); END;
CREATE TRIGGER IF NOT EXISTS search_mail_body AFTER INSERT ON mail_body
BEGIN INSERT OR IGNORE INTO search_dirty (id) VALUES (new.id); END;
"#;

/// 索引规则变了（或者第一次建）：清空，所有条目都等着重新收录。
pub(crate) fn init(conn: &Connection) -> Result<()> {
    let version: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = 'search'", [], |r| {
            r.get(0)
        })
        .optional()?;
    if version.as_deref() != Some(SEARCH_VERSION) {
        conn.execute_batch(
            "DELETE FROM search; DELETE FROM search_docs; DELETE FROM search_dirty;
             INSERT OR IGNORE INTO search_dirty (id) SELECT id FROM items;",
        )?;
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('search', ?1)",
            [SEARCH_VERSION],
        )?;
    }
    Ok(())
}

/// 汉字、假名、韩文这类不用空格分词的文字。
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF   // 平假名、片假名
        | 0x3400..=0x4DBF // 扩展 A
        | 0x4E00..=0x9FFF // 常用汉字
        | 0xAC00..=0xD7AF // 韩文
        | 0xF900..=0xFAFF // 兼容汉字
        | 0x20000..=0x2FFFF)
}

/// 一段文字里的词：西文按字母数字连成词，CJK 文字拆成相邻两字一组。
/// `trailing` 时一串 CJK 文字末尾再留一个单字（收录时要，这样搜最后那个字也能找到；查询时不要）。
fn tokens(text: &str, trailing: bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut run: Vec<char> = Vec::new();
    let flush_run = |run: &mut Vec<char>, out: &mut Vec<String>| {
        if run.len() == 1 {
            out.push(run[0].to_string());
        } else if run.len() > 1 {
            for pair in run.windows(2) {
                out.push(pair.iter().collect());
            }
            if trailing && let Some(last) = run.last() {
                out.push(last.to_string());
            }
        }
        run.clear();
    };
    for c in text.chars() {
        if is_cjk(c) {
            if !word.is_empty() {
                out.push(std::mem::take(&mut word));
            }
            run.push(c);
        } else if c.is_alphanumeric() {
            flush_run(&mut run, &mut out);
            word.push(c);
        } else {
            flush_run(&mut run, &mut out);
            if !word.is_empty() {
                out.push(std::mem::take(&mut word));
            }
        }
    }
    flush_run(&mut run, &mut out);
    if !word.is_empty() {
        out.push(word);
    }
    out
}

/// 写进 FTS5 的文字：词之间用空格隔开，交给 unicode61 分词器（它负责大小写和变音符号）。
pub(crate) fn index_text(text: &str) -> String {
    tokens(text, true).join(" ")
}

/// 用户输入的一个词在查询里的写法：拆出来的几个词连成一个短语，最后一个按前缀匹配，
/// 打字打到一半、或者只输了一个汉字时也能搜到。
fn term_query(term: &str) -> Option<String> {
    let parts = tokens(term, false);
    if parts.is_empty() {
        return None;
    }
    let phrase = parts.join(" ").replace('"', "\"\"");
    // 最后一个词按前缀匹配：西文打到一半、或者只输了一个汉字时都能找到
    Some(format!("\"{phrase}\"*"))
}

/// 搜索框里的文字变成 FTS5 的查询：空格分开的每个词都要出现。没有能搜的词时返回 `None`。
pub fn match_query(input: &str) -> Option<String> {
    let terms: Vec<String> = input.split_whitespace().filter_map(term_query).collect();
    (!terms.is_empty()).then(|| terms.join(" AND "))
}

/// 搜索框里的词，给片段和高亮用。
fn terms(input: &str) -> Vec<String> {
    input
        .split_whitespace()
        .map(|t| t.trim_matches('"').to_lowercase())
        .filter(|t| !t.is_empty())
        .collect()
}

/// 正文里第一次出现搜索词的地方，前后取一段。找不到时返回 `None`。
pub(crate) fn snippet(body: &str, input: &str) -> Option<String> {
    let lower = body.to_lowercase();
    // 转小写可能改变长度，按字符位置对齐
    let lower_chars: Vec<char> = lower.chars().collect();
    let chars: Vec<char> = body.chars().collect();
    if lower_chars.len() != chars.len() {
        return None;
    }
    let at = terms(input)
        .iter()
        .filter_map(|t| {
            let needle: Vec<char> = t.chars().collect();
            lower_chars
                .windows(needle.len())
                .position(|w| w == needle.as_slice())
        })
        .min()?;
    // 命中处前面留一点上下文；靠近结尾时往前多取，尽量把片段填满
    let mut start = at
        .saturating_sub(SNIPPET_BEFORE)
        .min(chars.len().saturating_sub(SNIPPET_LENGTH));
    // 西文别从半个词开始：往后挪到下一个空格（不越过命中处）
    if start > 0
        && chars[start - 1].is_alphanumeric()
        && !is_cjk(chars[start - 1])
        && let Some(space) = chars[start..at].iter().position(|c| c.is_whitespace())
    {
        start += space + 1;
    }
    let end = (start + SNIPPET_LENGTH).min(chars.len());
    let mut out: String = chars[start..end].iter().collect();
    out = out.trim().to_string();
    if start > 0 {
        out.insert(0, '…');
    }
    if end < chars.len() {
        out.push('…');
    }
    Some(out)
}

/// 网页正文去掉标签，变成纯文字。只为搜索：不求排版，script 和 style 里的东西不要。
fn html_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        out.push(' ');
        let tail = &rest[lt..];
        let lower: String = tail
            .chars()
            .take(8)
            .collect::<String>()
            .to_ascii_lowercase();
        let skip_to = if lower.starts_with("<script") {
            tail.to_ascii_lowercase()
                .find("</script>")
                .map(|i| i + "</script>".len())
        } else if lower.starts_with("<style") {
            tail.to_ascii_lowercase()
                .find("</style>")
                .map(|i| i + "</style>".len())
        } else {
            tail.find('>').map(|i| i + 1)
        };
        match skip_to {
            Some(n) => rest = &tail[n..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clip(text: &str) -> String {
    if text.chars().count() <= BODY_CHARS {
        text.to_string()
    } else {
        text.chars().take(BODY_CHARS).collect()
    }
}

impl Store {
    /// 收录一批等着的条目。`mail_text` 把邮件原文变成纯文字（解析邮件在 postflow-mail 里）。
    /// 返回这次处理了几条，0 表示都收录完了。
    pub fn index_search(&self, batch: usize, mail_text: &dyn Fn(&[u8]) -> String) -> Result<usize> {
        let mut guard = self.lock();
        let tx = guard.conn.transaction()?;
        let ids: Vec<String> = {
            let mut stmt = tx.prepare("SELECT id FROM search_dirty LIMIT ?1")?;
            stmt.query_map([batch as i64], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        for id in &ids {
            let old: Option<i64> = tx
                .query_row("SELECT docid FROM search_docs WHERE id = ?1", [id], |r| {
                    r.get(0)
                })
                .optional()?;
            if let Some(docid) = old {
                tx.execute("DELETE FROM search WHERE rowid = ?1", [docid])?;
            }
            type Doc = (
                String,
                Option<String>,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<Vec<u8>>,
            );
            let doc: Option<Doc> = tx
                .query_row(
                    "SELECT i.title, i.author, i.summary, m.from_name, m.from_address, \
                            COALESCE(t.html, c.html), b.raw \
                     FROM items i \
                     LEFT JOIN mail_meta m ON m.id = i.id \
                     LEFT JOIN item_content c ON c.id = i.id \
                     LEFT JOIN item_fulltext t ON t.id = i.id \
                     LEFT JOIN mail_body b ON b.id = i.id \
                     WHERE i.id = ?1",
                    [id],
                    |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                        ))
                    },
                )
                .optional()?;
            match doc {
                // 条目已经没了
                None => {
                    tx.execute("DELETE FROM search_docs WHERE id = ?1", [id])?;
                }
                Some((title, author, summary, from_name, from_address, html, raw)) => {
                    let body = match (raw, html) {
                        (Some(raw), _) => mail_text(&raw),
                        (None, Some(html)) => html_text(&html),
                        // 大邮件只取了信头，正文要等打开时才下载：先收录列表里的摘要
                        (None, None) => summary.clone(),
                    };
                    let body = clip(&body);
                    let people = [author, from_name, from_address]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" ");
                    tx.execute(
                        "INSERT INTO search_docs (id, body) VALUES (?1, ?2) \
                         ON CONFLICT (id) DO UPDATE SET body = excluded.body",
                        params![id, body],
                    )?;
                    let docid: i64 =
                        tx.query_row("SELECT docid FROM search_docs WHERE id = ?1", [id], |r| {
                            r.get(0)
                        })?;
                    tx.execute(
                        "INSERT INTO search (rowid, title, people, body) VALUES (?1, ?2, ?3, ?4)",
                        params![
                            docid,
                            index_text(&title),
                            index_text(&people),
                            index_text(&body)
                        ],
                    )?;
                }
            }
            tx.execute("DELETE FROM search_dirty WHERE id = ?1", [id])?;
        }
        tx.commit()?;
        Ok(ids.len())
    }

    /// 还有多少条等着收录。
    pub fn search_pending(&self) -> Result<i64> {
        Ok(self
            .lock()
            .conn
            .query_row("SELECT COUNT(*) FROM search_dirty", [], |r| r.get(0))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_is_split_into_pairs() {
        assert_eq!(index_text("添加订阅时"), "添加 加订 订阅 阅时 时");
        assert_eq!(index_text("李"), "李");
        assert_eq!(index_text("Postflow 的iPhone版"), "Postflow 的 iPhone 版");
        assert_eq!(index_text("Re: Q4 budget, v2.0!"), "Re Q4 budget v2 0");
        assert_eq!(index_text("こんにちは"), "こん んに にち ちは は");
    }

    #[test]
    fn queries_are_phrases_with_prefix() {
        assert_eq!(match_query("订阅").as_deref(), Some("\"订阅\"*"));
        assert_eq!(match_query("订阅时").as_deref(), Some("\"订阅 阅时\"*"));
        assert_eq!(
            match_query("Postflow的").as_deref(),
            Some("\"Postflow 的\"*")
        );
        assert_eq!(match_query("雷").as_deref(), Some("\"雷\"*"));
        assert_eq!(
            match_query("budg q4").as_deref(),
            Some("\"budg\"* AND \"q4\"*")
        );
        assert_eq!(match_query("  \"  !! "), None);
        // 引号不会破坏查询语法
        assert_eq!(match_query("a\"b").as_deref(), Some("\"a b\"*"));
    }

    #[test]
    fn html_becomes_text() {
        let html = "<p>Hello&nbsp;<b>world</b></p><script>var x = '<p>';</script><style>p{}</style>&lt;3 &amp; more";
        assert_eq!(html_text(html), "Hello world <3 & more");
    }

    #[test]
    fn snippet_around_the_first_match() {
        let body = format!(
            "{}The quarterly BUDGET is attached.{}",
            "x ".repeat(40),
            " y".repeat(100)
        );
        let s = snippet(&body, "budget").unwrap();
        assert!(s.starts_with('…') && s.ends_with('…'));
        assert!(
            s.starts_with("…The") || s.starts_with("…x"),
            "starts at a word: {s}"
        );
        assert!(s.contains("BUDGET is attached"));
        assert_eq!(snippet("没有这个词", "预算"), None);
        assert_eq!(
            snippet("关于预算的说明", "预算").as_deref(),
            Some("关于预算的说明")
        );
    }
}
