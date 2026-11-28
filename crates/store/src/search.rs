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

/// 索引规则变了就加一，旧索引清空重建。2：加了收件人一栏（`to:`）。
pub(crate) const SEARCH_VERSION: &str = "2";
/// 每条正文最多收录这么多字。
const BODY_CHARS: usize = 50_000;
/// 摘要片段在命中处前后各取多少字。
const SNIPPET_BEFORE: usize = 30;
const SNIPPET_LENGTH: usize = 140;

/// 只存词，不存原文（原文在 search_docs.body 和各自的表里）。
/// people 是作者和发件人，recipients 是收件人和抄送。
const FTS_TABLE: &str = "CREATE VIRTUAL TABLE IF NOT EXISTS search USING fts5(
    title, people, body, recipients,
    content = '', contentless_delete = 1,
    tokenize = 'unicode61 remove_diacritics 2'
);";

pub(crate) const SCHEMA: &str = r#"
-- 搜索索引里的每一篇：docid 是 FTS5 的行号，body 是收录的正文纯文字（显示搜索片段用）
CREATE TABLE IF NOT EXISTS search_docs (
    docid INTEGER PRIMARY KEY,
    id    TEXT NOT NULL UNIQUE,
    body  TEXT NOT NULL DEFAULT ''
);
-- 要重新收录的条目
CREATE TABLE IF NOT EXISTS search_dirty (id TEXT PRIMARY KEY) WITHOUT ROWID;
-- 邮件的附件，收录邮件时从原文里取出来，附件总览用
CREATE TABLE IF NOT EXISTS mail_attachments (
    item_id TEXT NOT NULL,
    idx     INTEGER NOT NULL,
    name    TEXT NOT NULL,
    mime    TEXT NOT NULL,
    size    INTEGER NOT NULL,
    PRIMARY KEY (item_id, idx)
) WITHOUT ROWID;

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
        // 栏目可能变了：整个表重建
        conn.execute_batch("DROP TABLE IF EXISTS search;")?;
        conn.execute_batch(FTS_TABLE)?;
        conn.execute_batch(
            "DELETE FROM search_docs; DELETE FROM search_dirty; DELETE FROM mail_attachments WHERE idx < 10000;
             INSERT OR IGNORE INTO search_dirty (id) SELECT id FROM items;",
        )?;
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('search', ?1)",
            [SEARCH_VERSION],
        )?;
    }
    conn.execute_batch(FTS_TABLE)?;
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
/// 只看普通的词，`from:` 这类写法见 [`parse`]。
pub fn match_query(input: &str) -> Option<String> {
    let terms: Vec<String> = input.split_whitespace().filter_map(term_query).collect();
    (!terms.is_empty()).then(|| terms.join(" AND "))
}

/// 邮件原文里给搜索用的文字：正文，以及收件人和抄送（名字和地址）。
#[derive(Debug, Clone, Default)]
pub struct MailText {
    pub body: String,
    pub recipients: String,
    /// 真正的附件（正文里内嵌的图片不算），附件总览用。
    pub attachments: Vec<AttachmentMeta>,
}

/// 邮件里的一个附件。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AttachmentMeta {
    /// 在邮件附件列表里的位置，下载时用。
    pub index: u32,
    pub name: String,
    pub mime: String,
    pub size: u64,
}

/// 附件总览里的一个附件，带着它所在的邮件。
#[derive(Debug, Clone, serde::Serialize)]
pub struct AttachmentRow {
    pub item_id: String,
    pub index: u32,
    pub name: String,
    pub mime: String,
    pub size: i64,
    pub title: String,
    pub from_name: Option<String>,
    pub from_address: Option<String>,
    pub published_at: i64,
    pub account_id: String,
    /// 正文已经下载了。没下载的（大邮件）是从邮件结构里认出来的，保存时先下载正文。
    pub downloaded: bool,
}

/// 从邮件结构认出来的附件，编号从这里开始，和收录正文时取出的真附件分开。
pub const STRUCTURE_INDEX: u32 = 10_000;

#[derive(Debug, Clone, serde::Serialize)]
pub struct AttachmentOverview {
    pub files: Vec<AttachmentRow>,
    /// 带附件、但原文还没下载的邮件（大邮件同步时只取信头）。
    pub not_downloaded: Vec<crate::model::ItemSummary>,
}

/// 搜索框里的一个条件（除了普通的词）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Filter {
    HasAttachment,
    Unread,
    Read,
    Starred,
    /// 日期，按当地时间那一天的零点：`before:` 之前、`after:` 当天起。毫秒。
    Before(i64),
    After(i64),
    /// `in:`：只在这个文件夹里找（items.archived 的值：0 收件箱、1 归档、2 垃圾邮件、3 已删除、
    /// 4 草稿、5 已发送）。不受当前列表的范围限制。
    In(i64),
}

/// 拆开以后的搜索：要出现的、不能出现的（FTS5 查询）和其他条件（`negated` 为真表示前面有 `-`）。
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Parsed {
    pub include: Option<String>,
    pub exclude: Vec<String>,
    pub filters: Vec<(Filter, bool)>,
    /// 普通的词（不含条件），给片段和高亮用。
    pub words: Vec<String>,
    /// 有写了但搜不了的东西（比如只有标点，或者看不懂的日期）：什么都找不到。
    pub impossible: bool,
}

/// 把搜索框里的一段文字切成词：空格分开，引号里的空格不算（`from:"Mira Chen"`）。
fn split_terms(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in input.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// 按当地时间解析日期：`2026-11-19`、`2026/11/19`、`2026-11`、`2026`。返回那一段的起点和下一段的起点（毫秒）。
fn date_range(text: &str, tz_offset_min: i64) -> Option<(i64, i64)> {
    let parts: Vec<&str> = text.split(['-', '/', '.']).collect();
    let num = |i: usize| parts.get(i).and_then(|p| p.parse::<i64>().ok());
    let year = num(0).filter(|y| (1970..=9999).contains(y))?;
    let (start, end) = match parts.len() {
        1 => ((year, 1, 1), (year + 1, 1, 1)),
        2 => {
            let m = num(1).filter(|m| (1..=12).contains(m))?;
            let next = if m == 12 {
                (year + 1, 1, 1)
            } else {
                (year, m + 1, 1)
            };
            ((year, m, 1), next)
        }
        3 => {
            let m = num(1).filter(|m| (1..=12).contains(m))?;
            let d = num(2).filter(|d| (1..=31).contains(d))?;
            ((year, m, d), (year, m, d + 1))
        }
        _ => return None,
    };
    let ms = |(y, m, d): (i64, i64, i64)| {
        (days_from_civil(y, m, 1) + d - 1) * 86_400_000 - tz_offset_min * 60_000
    };
    Some((ms(start), ms(end)))
}

/// 公历日期到 1970-01-01 的天数（Howard Hinnant 的算法）。
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// 拆开搜索框里的文字。认得的写法：
/// - `from:` 发件人或作者，`to:` 收件人和抄送，`subject:`（或 `title:`）标题
/// - `has:attachment` 带附件，`is:unread` / `is:read` / `is:starred`
/// - `before:2026-01-01`、`after:2025-12`、`after:2025`（按当地时间，`tz_offset_min` 是当地比 UTC 快几分钟）
/// - 前面加 `-` 表示排除，比如 `-from:newsletter`、`-广告`
/// - `in:sent`、`in:archive` 之类：只在那个文件夹里找
/// - 两个词之间写 `OR`（或者 `或`）：有其中一个就行
///
/// 中文也行：`发件人:`、`收件人:`、`标题:`、`有:附件`、`是:未读`、`早于:`、`晚于:`、`在:已发送`。
/// 不认得的写法（比如网址里的冒号）当普通的词搜。
pub(crate) fn parse(input: &str, tz_offset_min: i64) -> Parsed {
    let mut out = Parsed::default();
    let mut include: Vec<String> = Vec::new();
    // 上一个是 OR：下一个要出现的词和前一个合成「其中一个」
    let mut or_next = false;
    for raw in split_terms(input) {
        if raw == "OR" || raw == "或" || raw == "|" {
            or_next = !include.is_empty();
            continue;
        }
        let (negated, term) = match raw.strip_prefix('-') {
            Some(rest) if !rest.is_empty() => (true, rest),
            _ => (false, raw.as_str()),
        };
        // 「OR 后面跟一个排除」说不通，当它没写
        or_next &= !negated;
        let (key, value) = match term.split_once([':', '：']) {
            Some((k, v)) if !v.is_empty() => (k.to_lowercase(), v),
            _ => (String::new(), term),
        };
        let column = match key.as_str() {
            "from" | "发件人" | "作者" => Some("people"),
            "to" | "cc" | "收件人" => Some("recipients"),
            "subject" | "title" | "标题" | "主题" => Some("title"),
            _ => None,
        };
        let mut push_fts = |expr: String| {
            if negated {
                out.exclude.push(expr);
            } else if std::mem::take(&mut or_next)
                && let Some(prev) = include.pop()
            {
                include.push(format!("({prev} OR {expr})"));
            } else {
                include.push(expr);
            }
        };
        if let Some(column) = column {
            match term_query(value) {
                Some(q) => push_fts(format!("{column} : {q}")),
                None => out.impossible |= !negated,
            }
            continue;
        }
        let value_lower = value.to_lowercase();
        let filter = match (key.as_str(), value_lower.as_str()) {
            ("has" | "有", "attachment" | "attachments" | "附件") => Some(Filter::HasAttachment),
            ("is" | "是", "unread" | "未读") => Some(Filter::Unread),
            ("is" | "是", "read" | "已读") => Some(Filter::Read),
            ("is" | "是", "starred" | "星标") => Some(Filter::Starred),
            ("before" | "早于", d) => match date_range(d, tz_offset_min) {
                Some((start, _)) => Some(Filter::Before(start)),
                None => {
                    out.impossible = true;
                    continue;
                }
            },
            ("in" | "在", folder) => match folder {
                "inbox" | "收件箱" => Some(Filter::In(0)),
                "archive" | "archived" | "归档" | "已归档" => Some(Filter::In(1)),
                "junk" | "spam" | "垃圾邮件" | "垃圾" => Some(Filter::In(2)),
                "trash" | "deleted" | "已删除" | "废纸篓" => Some(Filter::In(3)),
                "drafts" | "draft" | "草稿" => Some(Filter::In(4)),
                "sent" | "已发送" => Some(Filter::In(5)),
                _ => {
                    out.impossible = true;
                    continue;
                }
            },
            ("after" | "晚于", d) => match date_range(d, tz_offset_min) {
                Some((start, _)) => Some(Filter::After(start)),
                None => {
                    out.impossible = true;
                    continue;
                }
            },
            _ => None,
        };
        if let Some(filter) = filter {
            out.filters.push((filter, negated));
            continue;
        }
        // 普通的词
        match term_query(term) {
            Some(q) => {
                if !negated {
                    out.words.push(term.to_lowercase());
                }
                push_fts(q);
            }
            None => out.impossible |= !negated,
        }
    }
    out.include = (!include.is_empty()).then(|| include.join(" AND "));
    out
}

/// 搜索框里的普通词，给片段和高亮用。
fn terms(input: &str) -> Vec<String> {
    parse(input, 0).words
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
    /// 收录一批等着的条目。`mail_text` 把邮件原文变成纯文字（解析邮件在 shibapost-mail 里）。
    /// 返回这次处理了几条，0 表示都收录完了。
    pub fn index_search(
        &self,
        batch: usize,
        mail_text: &dyn Fn(&[u8]) -> MailText,
    ) -> Result<usize> {
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
                    tx.execute("DELETE FROM mail_attachments WHERE item_id = ?1", [id])?;
                }
                Some((title, author, summary, from_name, from_address, html, raw)) => {
                    let (body, recipients) = match (raw, html) {
                        (Some(raw), _) => {
                            let text = mail_text(&raw);
                            tx.execute("DELETE FROM mail_attachments WHERE item_id = ?1", [id])?;
                            for a in &text.attachments {
                                tx.execute(
                                    "INSERT OR REPLACE INTO mail_attachments (item_id, idx, name, mime, size) \
                                     VALUES (?1, ?2, ?3, ?4, ?5)",
                                    params![id, a.index, a.name, a.mime, a.size as i64],
                                )?;
                            }
                            (text.body, text.recipients)
                        }
                        (None, Some(html)) => (html_text(&html), String::new()),
                        // 大邮件只取了信头，正文要等打开时才下载：先收录列表里的摘要
                        (None, None) => (summary.clone(), String::new()),
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
                        "INSERT INTO search (rowid, title, people, body, recipients) \
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![
                            docid,
                            index_text(&title),
                            index_text(&people),
                            index_text(&body),
                            index_text(&recipients)
                        ],
                    )?;
                }
            }
            tx.execute("DELETE FROM search_dirty WHERE id = ?1", [id])?;
        }
        tx.commit()?;
        Ok(ids.len())
    }

    /// 附件总览：最近邮件里的附件，新的在前。`query` 按文件名、邮件标题和发件人筛。
    /// 垃圾邮件和已删除里的不算。另外给出带附件、但还没下载原文的大邮件（附件要打开邮件时才知道）。
    pub fn attachments(&self, query: Option<&str>, limit: u32) -> Result<AttachmentOverview> {
        use crate::store::{ALIVE, ITEM_COLUMNS, ITEM_FROM, item_from_row};
        let guard = self.lock();
        let like = query.map(str::trim).filter(|q| !q.is_empty()).map(|q| {
            format!(
                "%{}%",
                q.replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            )
        });
        let cond = if like.is_some() {
            "AND (x.name LIKE ?2 ESCAPE '\\' OR i.title LIKE ?2 ESCAPE '\\' \
                  OR m.from_name LIKE ?2 ESCAPE '\\' OR m.from_address LIKE ?2 ESCAPE '\\')"
        } else {
            ""
        };
        let sql = format!(
            "SELECT x.item_id, x.idx, x.name, x.mime, x.size, i.title, m.from_name, m.from_address, \
                    i.published_at, i.source_id, x.idx < 10000 \
             FROM {ITEM_FROM} JOIN mail_attachments x ON x.item_id = i.id \
             WHERE i.archived NOT IN (2, 3) AND {ALIVE} {cond} \
             ORDER BY i.published_at DESC, x.item_id, x.idx LIMIT ?1"
        );
        let mut stmt = guard.conn.prepare(&sql)?;
        let map = |r: &rusqlite::Row<'_>| {
            Ok(AttachmentRow {
                item_id: r.get(0)?,
                index: r.get(1)?,
                name: r.get(2)?,
                mime: r.get(3)?,
                size: r.get(4)?,
                title: r.get(5)?,
                from_name: r.get(6)?,
                from_address: r.get(7)?,
                published_at: r.get(8)?,
                account_id: r.get(9)?,
                downloaded: r.get(10)?,
            })
        };
        let files = match &like {
            Some(like) => stmt.query_map(params![limit, like], map)?,
            None => stmt.query_map(params![limit], map)?,
        }
        .collect::<rusqlite::Result<Vec<_>>>()?;
        let pending_sql = format!(
            "SELECT {ITEM_COLUMNS} FROM {ITEM_FROM} \
             WHERE i.kind = 'mail' AND m.has_attachments = 1 AND i.archived NOT IN (2, 3) AND {ALIVE} \
               AND NOT EXISTS (SELECT 1 FROM mail_body b WHERE b.id = i.id) \
               AND NOT EXISTS (SELECT 1 FROM mail_attachments x WHERE x.item_id = i.id) {} \
             ORDER BY i.published_at DESC LIMIT 50",
            if like.is_some() {
                "AND (i.title LIKE ?1 ESCAPE '\\' OR m.from_name LIKE ?1 ESCAPE '\\' \
                      OR m.from_address LIKE ?1 ESCAPE '\\')"
            } else {
                ""
            }
        );
        let mut stmt = guard.conn.prepare(&pending_sql)?;
        let not_downloaded = match &like {
            Some(like) => stmt.query_map(params![like], item_from_row)?,
            None => stmt.query_map([], item_from_row)?,
        }
        .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(AttachmentOverview {
            files,
            not_downloaded,
        })
    }

    /// 只取了信头的大邮件：记下从邮件结构里认出来的附件。正文已经下载了的不管（收录时会换成真的）。
    pub fn set_structure_attachments(&self, item_id: &str, list: &[AttachmentMeta]) -> Result<()> {
        let guard = self.lock();
        let has_body: bool = guard
            .conn
            .query_row("SELECT 1 FROM mail_body WHERE id = ?1", [item_id], |_| {
                Ok(true)
            })
            .optional()?
            .unwrap_or(false);
        if has_body {
            return Ok(());
        }
        guard.conn.execute(
            "DELETE FROM mail_attachments WHERE item_id = ?1 AND idx >= 10000",
            [item_id],
        )?;
        for (i, a) in list.iter().enumerate() {
            guard.conn.execute(
                "INSERT OR REPLACE INTO mail_attachments (item_id, idx, name, mime, size) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![item_id, STRUCTURE_INDEX + i as u32, a.name, a.mime, a.size as i64],
            )?;
        }
        Ok(())
    }

    /// 从邮件结构认出来的那个附件叫什么（保存时用名字在下载下来的正文里找）。
    pub fn structure_attachment_name(&self, item_id: &str, index: u32) -> Result<Option<String>> {
        Ok(self
            .lock()
            .conn
            .query_row(
                "SELECT name FROM mail_attachments WHERE item_id = ?1 AND idx = ?2",
                params![item_id, index],
                |r| r.get(0),
            )
            .optional()?)
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
    fn search_syntax() {
        let p = parse(
            r#"from:"Mira Chen" -to:bob has:attachment is:未读 预算 -广告"#,
            0,
        );
        assert_eq!(
            p.include.as_deref(),
            Some(r#"people : "Mira Chen"* AND "预算"*"#)
        );
        assert_eq!(
            p.exclude,
            vec![
                r#"recipients : "bob"*"#.to_string(),
                r#""广告"*"#.to_string()
            ]
        );
        assert_eq!(
            p.filters,
            vec![(Filter::HasAttachment, false), (Filter::Unread, false)]
        );
        assert_eq!(p.words, vec!["预算"]);
        assert!(!p.impossible);
        // 日期按当地时间：东八区的 2026-01-01 零点是 UTC 前一天 16 点
        let p = parse("after:2026-01-01 before:2026-02", 8 * 60);
        assert_eq!(
            p.filters,
            vec![
                (Filter::After(1_767_196_800_000), false),
                (Filter::Before(1_769_875_200_000), false)
            ]
        );
        assert!(parse("before:yesterday", 0).impossible);
        // 不认得的写法当普通的词
        assert_eq!(parse("note:1", 0).words, vec!["note:1"]);
        // OR 和文件夹
        let p = parse("budget OR 预算 from:mei in:sent", 0);
        assert_eq!(
            p.include.as_deref(),
            Some(r#"("budget"* OR "预算"*) AND people : "mei"*"#)
        );
        assert_eq!(p.filters, vec![(Filter::In(5), false)]);
        assert_eq!(
            parse("OR budget", 0).include.as_deref(),
            Some(r#""budget"*"#)
        );
        assert!(parse("in:nowhere", 0).impossible);
    }

    #[test]
    fn chinese_is_split_into_pairs() {
        assert_eq!(index_text("添加订阅时"), "添加 加订 订阅 阅时 时");
        assert_eq!(index_text("李"), "李");
        assert_eq!(index_text("ShibaPost 的iPhone版"), "ShibaPost 的 iPhone 版");
        assert_eq!(index_text("Re: Q4 budget, v2.0!"), "Re Q4 budget v2 0");
        assert_eq!(index_text("こんにちは"), "こん んに にち ちは は");
    }

    #[test]
    fn queries_are_phrases_with_prefix() {
        assert_eq!(match_query("订阅").as_deref(), Some("\"订阅\"*"));
        assert_eq!(match_query("订阅时").as_deref(), Some("\"订阅 阅时\"*"));
        assert_eq!(
            match_query("ShibaPost的").as_deref(),
            Some("\"ShibaPost 的\"*")
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
