//! 本机索引：把数据目录里所有设备的日志合并成一个可查询的 SQLite 数据库。
//!
//! 索引完全由日志推导而来，删掉它再重建不会丢任何同步数据。
//! 所有写入都走同一条路：先把一行追加到本设备的日志，再把这个文件增量导入索引。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension, Row, params, params_from_iter};
use serde_json::{Map, Value, json};

use crate::error::{Error, Result};
use crate::hlc::{Clock, now_ms};
use crate::ids;
use crate::logs::{DataDir, LogFile, LogKind, read_new};
use crate::model::*;

/// 索引表结构的版本。结构变了就加一，旧索引会被丢弃并从日志重建。
const SCHEMA_VERSION: i64 = 2;

/// 由日志推导出来的表，重建索引时清空。items 里的文章也是推导出来的，单独清理。
const DERIVED_TABLES: &[&str] = &["log_files", "lww", "feeds", "accounts", "item_content"];
/// 只存在本机的表：抓取状态、全文缓存，以及从邮件服务器同步下来的邮件。重建索引时保留。
const LOCAL_TABLES: &[&str] = &[
    "search",
    "search_docs",
    "search_dirty",
    "feed_fetch",
    "item_fulltext",
    "mail_meta",
    "mail_body",
    "mail_state",
    "ai_results",
    "ai_messages",
    "ai_usage",
    "item_labels",
];

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS log_files (
    path   TEXT PRIMARY KEY,
    offset INTEGER NOT NULL
) WITHOUT ROWID;

-- 每个实体每个字段的当前值，以及写入它的时间戳
CREATE TABLE IF NOT EXISTS lww (
    kind  TEXT NOT NULL,
    id    TEXT NOT NULL,
    field TEXT NOT NULL,
    value TEXT NOT NULL,
    ts    TEXT NOT NULL,
    PRIMARY KEY (kind, id, field)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS feeds (
    id           TEXT PRIMARY KEY,
    url          TEXT NOT NULL,
    title        TEXT NOT NULL DEFAULT '',
    site_url     TEXT,
    icon_url     TEXT,
    folder       TEXT NOT NULL DEFAULT '',
    interval_min INTEGER,
    deleted      INTEGER NOT NULL DEFAULT 0,
    added_at     INTEGER NOT NULL DEFAULT 0,
    -- 在侧栏里的顺序。没拖动过的为空，排在后面按名称排
    position     REAL,
    -- 用户选的类型：article、video、podcast。为空表示按内容自动判断
    kind         TEXT,
    -- 打开这个源的文章时自动取全文
    full_text    INTEGER NOT NULL DEFAULT 0
) WITHOUT ROWID;

-- 邮箱账户的配置。密码不在这里，在系统钥匙串里。
CREATE TABLE IF NOT EXISTS accounts (
    id            TEXT PRIMARY KEY,
    email         TEXT NOT NULL,
    name          TEXT NOT NULL DEFAULT '',
    imap_host     TEXT NOT NULL DEFAULT '',
    imap_port     INTEGER NOT NULL DEFAULT 993,
    imap_security TEXT NOT NULL DEFAULT 'tls',
    username      TEXT NOT NULL DEFAULT '',
    deleted       INTEGER NOT NULL DEFAULT 0,
    added_at      INTEGER NOT NULL DEFAULT 0,
    position      REAL,
    -- 自己填的发信服务器。为空表示按服务商预设自动选
    smtp_host     TEXT,
    smtp_port     INTEGER,
    smtp_security TEXT,
    -- 发信时收件人看到的名字
    sender_name   TEXT NOT NULL DEFAULT '',
    -- 签名
    signature     TEXT NOT NULL DEFAULT ''
) WITHOUT ROWID;

-- 信息流里的每一条：订阅文章（kind = article）或邮件（kind = mail）。
-- 文章由日志推导；邮件由本机从邮件服务器同步，状态以服务器为准。
CREATE TABLE IF NOT EXISTS items (
    id           TEXT PRIMARY KEY,
    kind         TEXT NOT NULL DEFAULT 'article',
    source_id    TEXT NOT NULL,
    guid         TEXT NOT NULL DEFAULT '',
    url          TEXT,
    title        TEXT NOT NULL DEFAULT '',
    author       TEXT,
    summary      TEXT NOT NULL DEFAULT '',
    published_at INTEGER NOT NULL,
    fetched_at   INTEGER NOT NULL,
    read         INTEGER NOT NULL DEFAULT 0,
    starred      INTEGER NOT NULL DEFAULT 0,
    archived     INTEGER NOT NULL DEFAULT 0,
    image        TEXT,
    -- 播客、视频的附件（JSON）和它是 audio 还是 video
    media        TEXT,
    media_kind   TEXT
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS items_by_time ON items (published_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS items_by_source ON items (source_id, published_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS items_unread ON items (source_id) WHERE read = 0 AND archived = 0;
CREATE INDEX IF NOT EXISTS items_starred ON items (published_at DESC) WHERE starred = 1;

CREATE TABLE IF NOT EXISTS item_content (
    id   TEXT PRIMARY KEY,
    html TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS item_fulltext (
    id   TEXT PRIMARY KEY,
    html TEXT NOT NULL
);

-- 邮件在服务器上的位置和信头信息
CREATE TABLE IF NOT EXISTS mail_meta (
    id               TEXT PRIMARY KEY,
    account_id       TEXT NOT NULL,
    mailbox          TEXT NOT NULL,
    uidvalidity      INTEGER NOT NULL DEFAULT 0,
    uid              INTEGER,
    message_id       TEXT,
    from_name        TEXT,
    from_address     TEXT,
    has_attachments  INTEGER NOT NULL DEFAULT 0,
    list_unsubscribe TEXT,
    size             INTEGER NOT NULL DEFAULT 0,
    -- 本机最后一次改它状态的时间。比这更早开始的同步不能覆盖它
    touched_at       INTEGER NOT NULL DEFAULT 0
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS mail_by_uid ON mail_meta (account_id, mailbox, uid);

CREATE TABLE IF NOT EXISTS mail_body (
    id  TEXT PRIMARY KEY,
    raw BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS mail_state (
    account_id   TEXT PRIMARY KEY,
    uidvalidity  INTEGER,
    last_sync_at INTEGER,
    error        TEXT,
    error_code   TEXT
) WITHOUT ROWID;

-- 收件箱以外另外同步的文件夹（垃圾邮件、已删除）在服务器上叫什么、UIDVALIDITY 是多少
CREATE TABLE IF NOT EXISTS mail_folders (
    account_id  TEXT NOT NULL,
    folder      INTEGER NOT NULL,
    name        TEXT NOT NULL,
    uidvalidity INTEGER NOT NULL,
    PRIMARY KEY (account_id, folder)
) WITHOUT ROWID;

-- 用户自己建的文件夹，打开过才同步，按名字记
CREATE TABLE IF NOT EXISTS mail_mailboxes (
    account_id  TEXT NOT NULL,
    name        TEXT NOT NULL,
    uidvalidity INTEGER NOT NULL,
    PRIMARY KEY (account_id, name)
) WITHOUT ROWID;

-- AI 生成的内容和对话，只在本机
CREATE TABLE IF NOT EXISTS ai_results (
    item_id    TEXT NOT NULL,
    task       TEXT NOT NULL,
    lang       TEXT NOT NULL DEFAULT '',
    provider   TEXT NOT NULL DEFAULT '',
    model      TEXT NOT NULL DEFAULT '',
    content    TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (item_id, task, lang)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS ai_messages (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    item_id    TEXT NOT NULL,
    role       TEXT NOT NULL,
    content    TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS ai_messages_by_item ON ai_messages (item_id, id);

CREATE TABLE IF NOT EXISTS ai_usage (
    day      INTEGER NOT NULL,
    provider TEXT NOT NULL,
    requests INTEGER NOT NULL DEFAULT 0,
    input    INTEGER NOT NULL DEFAULT 0,
    output   INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (day, provider)
) WITHOUT ROWID;

-- 分类。AI 分的可以被用户改掉；用户定的（by_user = 1）AI 不再覆盖。
-- label 为空字符串表示「哪一类都不是」，也算分过了
CREATE TABLE IF NOT EXISTS item_labels (
    item_id TEXT PRIMARY KEY,
    label   TEXT NOT NULL,
    by_user INTEGER NOT NULL DEFAULT 0,
    at      INTEGER NOT NULL
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS item_labels_by_label ON item_labels (label);

-- 阅读记录：每次打开一篇停留了多久、往下看到了哪里。只在本机，用来做订阅统计
CREATE TABLE IF NOT EXISTS reading (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    item_id   TEXT NOT NULL,
    source_id TEXT NOT NULL,
    at        INTEGER NOT NULL,
    dwell_ms  INTEGER NOT NULL,
    depth     REAL NOT NULL,
    finished  INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS reading_by_at ON reading (at);
CREATE INDEX IF NOT EXISTS reading_by_item ON reading (item_id);

CREATE TABLE IF NOT EXISTS feed_fetch (
    feed_id       TEXT PRIMARY KEY,
    etag          TEXT,
    last_modified TEXT,
    last_fetch_at INTEGER,
    next_fetch_at INTEGER NOT NULL DEFAULT 0,
    error         TEXT,
    error_code    TEXT,
    error_count   INTEGER NOT NULL DEFAULT 0
) WITHOUT ROWID;
"#;

/// 列表查询的表连接：文章连订阅源，邮件连账户和信头。
pub(crate) const ITEM_FROM: &str = "items i \
     LEFT JOIN feeds f ON i.kind = 'article' AND f.id = i.source_id \
     LEFT JOIN accounts a ON i.kind = 'mail' AND a.id = i.source_id \
     LEFT JOIN mail_meta m ON i.kind = 'mail' AND m.id = i.id \
     LEFT JOIN item_labels l ON l.item_id = i.id";

pub(crate) const ITEM_COLUMNS: &str = "i.id, i.kind, i.source_id, \
     CASE WHEN i.kind = 'mail' THEN COALESCE(NULLIF(a.name, ''), a.email, '') ELSE COALESCE(f.title, '') END, \
     f.icon_url, i.url, i.title, i.author, i.summary, i.published_at, i.read, i.starred, i.archived, \
     m.from_address, COALESCE(m.has_attachments, 0), i.image, NULLIF(l.label, ''), \
     NULLIF((SELECT CAST(s.value AS INTEGER) FROM lww s \
             WHERE s.kind = 'snooze' AND s.id = i.id AND s.field = 'until'), 0), \
     i.media";

/// `ITEM_COLUMNS` 有几列。在它后面再选别的列时，从这个序号开始读。
pub(crate) const ITEM_COLUMN_COUNT: usize = 19;

/// 这一条正稍后提醒着、还没到时间：收件箱、今天、未读、各个来源和计数里都先不算它。
/// 提醒时间存在同步日志里（kind = snooze），邮件和文章都一样，所以其他设备也知道。
/// 写成宏，固定的 SQL 字符串（比如邮箱列表）也能用 `concat!` 拼进去。
#[macro_export]
#[doc(hidden)]
macro_rules! snoozed_now_sql {
    () => {
        "EXISTS (SELECT 1 FROM lww s WHERE s.kind = 'snooze' AND s.id = i.id AND s.field = 'until' \
         AND CAST(s.value AS INTEGER) > (CAST(strftime('%s', 'now') AS INTEGER) * 1000))"
    };
}

/// 这一条是放到订阅里看的订阅邮件（Newsletter）：发件人在设置 `newsletters` 的地址列表里。
/// 这些邮件在订阅那边和文章一起看，不算在邮件那边的视图和计数里。
#[macro_export]
#[doc(hidden)]
macro_rules! newsletter_sql {
    () => {
        "i.id IN (SELECT nm.id FROM mail_meta nm WHERE lower(nm.from_address) IN            (SELECT lower(nj.value) FROM lww ns, json_each(ns.value) nj              WHERE ns.kind = 'setting' AND ns.id = 'newsletters' AND ns.field = 'value'))"
    };
}

/// 设置里放到订阅里看的发件人地址（小写）。
pub const NEWSLETTERS_SETTING: &str = "newsletters";

/// 现在的时间（毫秒），在 SQL 里用。
const NOW_MS: &str = "(CAST(strftime('%s', 'now') AS INTEGER) * 1000)";

fn snoozed_now() -> String {
    snoozed_now_sql!().to_string()
}

/// 提醒时间在 `after`（不含）到现在之间的条目。
const REMINDER_WINDOW_MS: i64 = 3 * 86_400_000;

/// 来源还在：订阅没退订，邮箱没移除。
pub(crate) const ALIVE: &str = "COALESCE(f.deleted, a.deleted, 1) = 0";

/// 「稍后读」：用户自己存下来的网页。它是一个地址固定的内置订阅源，存下的网页是它的文章，
/// 所以阅读、翻译、星标、搜索、同步都和别的文章一样。它不刷新、不出现在订阅源列表和 OPML 里，
/// 文章也不混进「全部文章」「今天」「未读」，只在侧栏的「稍后读」里看。
pub const LATER_URL: &str = "postflow:later";

/// 「稍后读」订阅源的 ID。地址固定，所以每台设备算出来都一样。
pub fn later_feed_id() -> &'static str {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(|| ids::feed_id(LATER_URL))
}

/// 已发送的信 `i`（连着 `m`）设了「没回复就提醒」，还没到时间。
fn waiting_sql() -> String {
    format!(
        "EXISTS (SELECT 1 FROM lww s WHERE s.kind = 'followup' AND s.id = m.message_id \
         AND s.field = 'until' AND CAST(s.value AS INTEGER) > {NOW_MS})"
    )
}

/// 条目 `i`（连着订阅源 `f`）不是「稍后读」里的。
const NOT_LATER: &str = "COALESCE(f.url, '') <> 'postflow:later'";

/// 一条操作里最多带多少个 ID，超过就拆成多行，避免出现特别长的行。
const MAX_IDS_PER_OP: usize = 500;

pub(crate) struct Inner {
    pub(crate) conn: Connection,
    clock: Clock,
    data: DataDir,
    device_id: String,
}

pub struct Store {
    inner: Mutex<Inner>,
}

impl Store {
    /// 打开数据目录和本机索引，并把还没导入的日志导入。
    pub fn open(data_dir: &Path, index_path: &Path, device_id: &str) -> Result<Self> {
        if device_id.is_empty() || device_id.contains(['/', '\\', '.']) {
            return Err(Error::Invalid(format!("invalid device id: {device_id:?}")));
        }
        let data = DataDir::open(data_dir)?;
        let canonical = fs::canonicalize(data_dir).unwrap_or_else(|_| data_dir.to_path_buf());
        if let Some(parent) = index_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(index_path)?;
        init_schema(&conn, &canonical)?;

        let mut clock = Clock::new(device_id);
        let max_ts: Option<String> = conn.query_row("SELECT MAX(ts) FROM lww", [], |r| r.get(0))?;
        if let Some(ts) = max_ts.and_then(|s| s.parse().ok()) {
            clock.observe(&ts);
        }

        let store = Store {
            inner: Mutex::new(Inner {
                conn,
                clock,
                data,
                device_id: device_id.to_string(),
            }),
        };
        store.import_all()?;
        Ok(store)
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn data_dir(&self) -> PathBuf {
        self.lock().data.root().to_path_buf()
    }

    pub fn device_id(&self) -> String {
        self.lock().device_id.clone()
    }

    // ---------------------------------------------------------------- 导入

    /// 增量导入所有设备的日志。启动时、以及发现数据目录有变化时调用。
    pub fn import_all(&self) -> Result<ImportStats> {
        let mut guard = self.lock();
        let Inner {
            conn, clock, data, ..
        } = &mut *guard;
        let mut stats = ImportStats::default();
        for kind in [LogKind::Articles, LogKind::Ops] {
            for file in data.list(kind)? {
                // 单个文件读不了（权限、云盘还没下载完）不应该挡住其他文件
                if import_file(conn, clock, &file, &mut stats).is_err() {
                    stats.unreadable += 1;
                }
            }
        }
        Ok(stats)
    }

    /// 丢弃索引里所有推导出来的内容，从日志重新导入。
    pub fn rebuild_index(&self) -> Result<ImportStats> {
        {
            let guard = self.lock();
            for table in DERIVED_TABLES {
                guard.conn.execute(&format!("DELETE FROM {table}"), [])?;
            }
            // 邮件来自邮件服务器，不在日志里，保留
            guard
                .conn
                .execute("DELETE FROM items WHERE kind = 'article'", [])?;
        }
        self.import_all()
    }

    // ---------------------------------------------------------------- 写入

    /// 把若干实体的若干字段设为新值。所有状态变更最终都走这里。
    pub fn set_fields(&self, kind: &str, ids: &[String], set: Map<String, Value>) -> Result<()> {
        if ids.is_empty() || set.is_empty() {
            return Ok(());
        }
        let mut guard = self.lock();
        let inner = &mut *guard;
        let mut lines = Vec::new();
        for chunk in ids.chunks(MAX_IDS_PER_OP) {
            let op = Op {
                ts: inner.clock.tick(),
                k: kind.to_string(),
                id: (chunk.len() == 1).then(|| chunk[0].clone()),
                ids: if chunk.len() == 1 {
                    Vec::new()
                } else {
                    chunk.to_vec()
                },
                set: set.clone(),
            };
            lines.push(serde_json::to_string(&op)?);
        }
        let file = inner.data.append(LogKind::Ops, &inner.device_id, &lines)?;
        import_file(
            &mut inner.conn,
            &mut inner.clock,
            &file,
            &mut ImportStats::default(),
        )
    }

    /// 订阅一个源。已订阅过则直接返回；退订过的会恢复。
    /// 「稍后读」订阅源，没有就建一个（退订过的也恢复）。返回它的 ID。
    pub fn ensure_later_feed(&self) -> Result<String> {
        let id = later_feed_id();
        if self.feed(id)?.is_none() {
            self.put_feed(NewFeed {
                url: LATER_URL.into(),
                title: "Read later".into(),
                ..Default::default()
            })?;
        }
        Ok(id.to_string())
    }

    pub fn put_feed(&self, new: NewFeed) -> Result<Feed> {
        let url = new.url.trim().to_string();
        if url.is_empty() {
            return Err(Error::Invalid("feed url is empty".into()));
        }
        let id = ids::feed_id(&url);
        let existing: Option<bool> = self
            .lock()
            .conn
            .query_row("SELECT deleted FROM feeds WHERE id = ?1", [&id], |r| {
                r.get(0)
            })
            .optional()?;

        let mut set = Map::new();
        match existing {
            Some(false) => {}
            Some(true) => {
                set.insert("deleted".into(), json!(false));
                set.insert("folder".into(), json!(new.folder));
            }
            None => {
                set.insert("url".into(), json!(url));
                set.insert("title".into(), json!(new.title));
                set.insert("site_url".into(), json!(new.site_url));
                set.insert("icon_url".into(), json!(new.icon_url));
                set.insert("folder".into(), json!(new.folder));
                set.insert("deleted".into(), json!(false));
                set.insert("added_at".into(), json!(now_ms()));
            }
        }
        self.set_fields("feed", std::slice::from_ref(&id), set)?;
        self.feed(&id)?
            .ok_or_else(|| Error::Invalid("feed was not created".into()))
    }

    pub fn update_feed(&self, id: &str, patch: FeedPatch) -> Result<()> {
        let mut set = Map::new();
        if let Some(title) = patch.title {
            set.insert("title".into(), json!(title));
        }
        if let Some(folder) = patch.folder {
            set.insert("folder".into(), json!(folder));
        }
        if let Some(url) = patch
            .url
            .map(|u| u.trim().to_string())
            .filter(|u| !u.is_empty())
        {
            set.insert("url".into(), json!(url));
        }
        if let Some(site_url) = patch.site_url {
            set.insert("site_url".into(), json!(site_url));
        }
        if let Some(icon_url) = patch.icon_url {
            set.insert("icon_url".into(), json!(icon_url));
        }
        if let Some(interval) = patch.interval_min {
            set.insert("interval_min".into(), json!(interval));
        }
        if let Some(full_text) = patch.full_text {
            set.insert("full_text".into(), json!(full_text));
        }
        if let Some(kind) = patch.kind {
            let kind = kind.filter(|k| FEED_KINDS.contains(&k.as_str()));
            set.insert("kind".into(), json!(kind));
        }
        self.set_fields("feed", &[id.to_string()], set)
    }

    /// 把一组订阅源按给定的顺序放进一个文件夹（空字符串是不在任何文件夹里）。
    /// 只写真正变了的字段，拖动一次通常只改一两个源。
    pub fn reorder_feeds(&self, folder: &str, ids: &[String]) -> Result<()> {
        for (index, id) in ids.iter().enumerate() {
            let current: Option<(String, Option<f64>)> = self
                .lock()
                .conn
                .query_row(
                    "SELECT folder, position FROM feeds WHERE id = ?1 AND deleted = 0",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let Some((old_folder, old_position)) = current else {
                continue;
            };
            let position = index as f64;
            let mut set = Map::new();
            if old_folder != folder {
                set.insert("folder".into(), json!(folder));
            }
            if old_position != Some(position) {
                set.insert("position".into(), json!(position));
            }
            self.set_fields("feed", std::slice::from_ref(id), set)?;
        }
        Ok(())
    }

    /// 退订。文章存档保留，重新订阅后还在。
    pub fn remove_feed(&self, id: &str) -> Result<()> {
        let mut set = Map::new();
        set.insert("deleted".into(), json!(true));
        self.set_fields("feed", &[id.to_string()], set)
    }

    /// 改条目状态。文章的状态写进日志同步给其他设备；邮件的状态先在本机生效，
    /// 返回这些邮件在服务器上的位置，由调用方写回邮件服务器。
    pub fn set_item_state(&self, ids: &[String], patch: &ItemPatch) -> Result<Vec<MailRef>> {
        let mut mail: HashSet<String> = self.mail_ids_among(ids)?;
        // 草稿和已发送的信只在各自的文件夹里，不能归档或移回收件箱
        if patch.archived.is_some() {
            let pinned = self.pinned_ids_among(&mail)?;
            mail.retain(|id| !pinned.contains(id));
        }
        let all_mail = self.mail_ids_among(ids)?;
        let articles: Vec<String> = ids
            .iter()
            .filter(|id| !all_mail.contains(*id))
            .cloned()
            .collect();
        let mail: Vec<String> = ids
            .iter()
            .filter(|id| mail.contains(*id))
            .cloned()
            .collect();
        let refs = self.set_mail_state_local(&mail, patch)?;

        let mut set = Map::new();
        if let Some(read) = patch.read {
            set.insert("read".into(), json!(read));
        }
        if let Some(starred) = patch.starred {
            set.insert("starred".into(), json!(starred));
        }
        if let Some(archived) = patch.archived {
            set.insert("archived".into(), json!(archived));
        }
        self.set_fields("item", &articles, set)?;
        Ok(refs)
    }

    /// 稍后提醒：`until` 之前这些条目不出现在收件箱里，到时间再回来。`None` 取消。
    /// 写进同步日志，邮件和文章一样，其他设备也会藏起来。
    pub fn set_snooze(&self, ids: &[String], until: Option<i64>) -> Result<()> {
        let mut set = Map::new();
        set.insert("until".into(), until.map_or(Value::Null, |v| json!(v)));
        self.set_fields("snooze", ids, set)
    }

    /// 提醒时间落在 (`after`, `until`] 里的条目（不含已删除、垃圾邮件），用来到时间时提醒。
    pub fn due_reminders(&self, after: i64, until: i64) -> Result<Vec<ItemSummary>> {
        let guard = self.lock();
        let sql = format!(
            "SELECT {ITEM_COLUMNS} FROM {ITEM_FROM} \
             JOIN lww s ON s.kind = 'snooze' AND s.id = i.id AND s.field = 'until' \
             WHERE i.archived < 2 AND {ALIVE} \
               AND CAST(s.value AS INTEGER) > ?1 AND CAST(s.value AS INTEGER) <= ?2 \
             ORDER BY CAST(s.value AS INTEGER)"
        );
        let mut stmt = guard.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![after, until], item_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// 把筛选范围内的未读条目全部标为已读。返回条目数，以及需要写回服务器的邮件。
    pub fn mark_all_read(&self, filter: &ItemFilter) -> Result<(usize, Vec<MailRef>)> {
        let ids: Vec<String> = {
            let guard = self.lock();
            let mut args = Vec::new();
            let conds = filter_sql(filter, &mut args);
            let sql = format!("SELECT i.id FROM {ITEM_FROM} WHERE {conds} AND i.read = 0");
            let mut stmt = guard.conn.prepare(&sql)?;
            let rows = stmt.query_map(params_from_iter(args), |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let refs = self.set_item_state(
            &ids,
            &ItemPatch {
                read: Some(true),
                ..Default::default()
            },
        )?;
        Ok((ids.len(), refs))
    }

    /// 存入新抓到的文章，已经有的跳过。返回真正新增的篇数。
    pub fn add_articles(&self, articles: Vec<Article>) -> Result<usize> {
        if articles.is_empty() {
            return Ok(0);
        }
        let mut guard = self.lock();
        let inner = &mut *guard;
        let mut seen = HashSet::new();
        let mut lines = Vec::new();
        {
            // 已经有的跳过；但以前入库时附件没存全（没有附件、没有章节或文字稿），这次带来了的，再记一次好补上
            let mut stored_media = inner
                .conn
                .prepare_cached("SELECT media FROM items WHERE id = ?1")?;
            for article in &articles {
                if !seen.insert(article.id.as_str()) {
                    continue;
                }
                let existing: Option<Option<String>> = stored_media
                    .query_row([&article.id], |r| r.get(0))
                    .optional()?;
                if let Some(stored) = existing {
                    let stored: Option<Media> =
                        stored.and_then(|json| serde_json::from_str(&json).ok());
                    if !article
                        .media
                        .as_ref()
                        .is_some_and(|m| m.adds_to(stored.as_ref()))
                    {
                        continue;
                    }
                }
                lines.push(serde_json::to_string(article)?);
            }
        }
        if lines.is_empty() {
            return Ok(0);
        }
        let file = inner
            .data
            .append(LogKind::Articles, &inner.device_id, &lines)?;
        let mut stats = ImportStats::default();
        import_file(&mut inner.conn, &mut inner.clock, &file, &mut stats)?;
        Ok(stats.articles)
    }

    pub fn set_setting(&self, key: &str, value: Value) -> Result<()> {
        let mut set = Map::new();
        set.insert("value".into(), value);
        self.set_fields("setting", &[key.to_string()], set)
    }

    pub fn set_device_name(&self, name: &str) -> Result<()> {
        let id = self.device_id();
        let mut set = Map::new();
        set.insert("name".into(), json!(name));
        self.set_fields("device", &[id], set)
    }

    // ---------------------------------------------------------------- 查询

    pub fn feeds(&self) -> Result<Vec<Feed>> {
        let guard = self.lock();
        let mut stmt = guard
            .conn
            .prepare(&feed_sql("f.deleted = 0 AND f.url <> 'postflow:later'"))?;
        let rows = stmt.query_map([], feed_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn feed(&self, id: &str) -> Result<Option<Feed>> {
        let guard = self.lock();
        let mut stmt = guard
            .conn
            .prepare(&feed_sql("f.deleted = 0 AND f.id = ?1"))?;
        Ok(stmt.query_row([id], feed_from_row).optional()?)
    }

    pub fn items(
        &self,
        filter: &ItemFilter,
        before: Option<&Cursor>,
        limit: u32,
    ) -> Result<Vec<ItemSummary>> {
        let guard = self.lock();
        let mut args = Vec::new();
        let mut conds = filter_sql(filter, &mut args);
        if let Some(cursor) = before {
            args.push(SqlValue::Integer(cursor.published_at));
            let p = args.len();
            args.push(SqlValue::Text(cursor.id.clone()));
            let i = args.len();
            conds.push_str(&format!(
                " AND (i.published_at < ?{p} OR (i.published_at = ?{p} AND i.id < ?{i}))"
            ));
        }
        args.push(SqlValue::Integer(i64::from(limit.clamp(1, 500))));
        let sql = format!(
            "SELECT {ITEM_COLUMNS} FROM {ITEM_FROM} \
             WHERE {conds} ORDER BY i.published_at DESC, i.id DESC LIMIT ?{}",
            args.len()
        );
        let mut stmt = guard.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args), item_from_row)?;
        let mut items: Vec<ItemSummary> = rows.collect::<rusqlite::Result<_>>()?;
        // 搜索时，标题里没有搜索词的，给出正文里命中的那一段
        if let Some(query) = filter.query.as_deref().filter(|q| !q.trim().is_empty()) {
            let mut body = guard
                .conn
                .prepare_cached("SELECT body FROM search_docs WHERE id = ?1")?;
            for item in &mut items {
                if crate::search::snippet(&item.title, query).is_some() {
                    continue;
                }
                let text: Option<String> = body.query_row([&item.id], |r| r.get(0)).optional()?;
                item.snippet = text.and_then(|t| crate::search::snippet(&t, query));
            }
        }
        Ok(items)
    }

    /// 条目在订阅源里的 guid（Atom 的 id），「查看源代码」时用它在源里找回这一篇。
    pub fn item_guid(&self, id: &str) -> Result<Option<String>> {
        let guard = self.lock();
        Ok(guard
            .conn
            .query_row("SELECT guid FROM items WHERE id = ?1", [id], |r| r.get(0))
            .optional()?)
    }

    pub fn item(&self, id: &str) -> Result<Option<ItemDetail>> {
        let guard = self.lock();
        let sql = format!(
            "SELECT {ITEM_COLUMNS}, COALESCE(c.html, ''), t.html \
             FROM {ITEM_FROM} \
             LEFT JOIN item_content c ON c.id = i.id \
             LEFT JOIN item_fulltext t ON t.id = i.id \
             WHERE i.id = ?1"
        );
        let mut stmt = guard.conn.prepare(&sql)?;
        let detail = stmt
            .query_row([id], |r| {
                Ok(ItemDetail {
                    item: item_from_row(r)?,
                    // ITEM_COLUMNS 之后的两列（ITEM_COLUMN_COUNT 是它的列数）
                    content: r.get(ITEM_COLUMN_COUNT)?,
                    full_text: r.get(ITEM_COLUMN_COUNT + 1)?,
                })
            })
            .optional()?;
        Ok(detail)
    }

    pub fn counts(&self, today_since: i64) -> Result<Counts> {
        let guard = self.lock();
        // 放到订阅里看的订阅邮件算在订阅那边
        let sql = format!(
            "SELECT COALESCE(SUM(i.read = 0), 0), \
                    COALESCE(SUM(i.read = 0 AND i.published_at >= ?1), 0), \
                    COALESCE(SUM(i.read = 0 AND i.kind = 'mail' AND NOT {nl}), 0), \
                    COALESCE(SUM(i.read = 0 AND i.kind = 'mail' AND NOT {nl} AND i.published_at >= ?1), 0) \
             FROM {ITEM_FROM} WHERE i.archived = 0 AND {ALIVE} AND {NOT_LATER} AND NOT {}",
            snoozed_now(),
            nl = newsletter_sql!(),
        );
        let (unread, today, unread_mail, today_mail): (i64, i64, i64, i64) =
            guard.conn.query_row(&sql, [today_since], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?;
        let starred =
            guard
                .conn
                .query_row("SELECT COUNT(*) FROM items WHERE starred = 1", [], |r| {
                    r.get(0)
                })?;
        let later = guard.conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM {ITEM_FROM} WHERE i.read = 0 AND i.archived = 0 AND {ALIVE} \
                 AND f.url = 'postflow:later' AND NOT {}",
                snoozed_now()
            ),
            [],
            |r| r.get(0),
        )?;
        let waiting = guard.conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM {ITEM_FROM} WHERE i.archived = 5 AND {ALIVE} AND {}",
                waiting_sql()
            ),
            [],
            |r| r.get(0),
        )?;
        Ok(Counts {
            later,
            waiting,
            unread,
            today,
            starred,
            unread_mail,
            today_mail,
            unread_article: unread - unread_mail,
            today_article: today - today_mail,
        })
    }

    /// 放到订阅里看的订阅邮件发件人，按名字排序。还没收到过信的也列出来（数字是 0）。
    pub fn newsletters(&self) -> Result<Vec<Newsletter>> {
        let addresses: Vec<String> = self
            .settings()?
            .get(NEWSLETTERS_SETTING)
            .and_then(Value::as_array)
            .map(|list| {
                let mut out: Vec<String> = list
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|a| a.trim().to_lowercase())
                    .filter(|a| a.contains('@'))
                    .collect();
                out.sort();
                out.dedup();
                out
            })
            .unwrap_or_default();
        if addresses.is_empty() {
            return Ok(Vec::new());
        }
        let guard = self.lock();
        let sql = format!(
            "SELECT lower(m.from_address), \
                    (SELECT COALESCE(m2.from_name, '') FROM mail_meta m2 JOIN items i2 ON i2.id = m2.id \
                      WHERE lower(m2.from_address) = lower(m.from_address) \
                      ORDER BY i2.published_at DESC LIMIT 1), \
                    SUM(i.read = 0 AND i.archived = 0 AND NOT {snoozed}), COUNT(*) \
             FROM items i JOIN mail_meta m ON m.id = i.id \
                  JOIN accounts a ON a.id = i.source_id AND a.deleted = 0 \
             WHERE i.kind = 'mail' AND i.archived < 2 AND {nl} \
             GROUP BY lower(m.from_address)",
            snoozed = snoozed_now(),
            nl = newsletter_sql!(),
        );
        let mut stmt = guard.conn.prepare(&sql)?;
        let found: HashMap<String, Newsletter> = stmt
            .query_map([], |r| {
                Ok(Newsletter {
                    address: r.get(0)?,
                    name: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    unread: r.get(2)?,
                    total: r.get(3)?,
                })
            })?
            .map(|row| row.map(|n| (n.address.clone(), n)))
            .collect::<rusqlite::Result<_>>()?;
        let mut out: Vec<Newsletter> = addresses
            .into_iter()
            .map(|address| {
                found.get(&address).cloned().unwrap_or(Newsletter {
                    address,
                    name: String::new(),
                    unread: 0,
                    total: 0,
                })
            })
            .collect();
        out.sort_by_key(|n| {
            if n.name.is_empty() {
                n.address.clone()
            } else {
                n.name.to_lowercase()
            }
        });
        Ok(out)
    }

    pub fn settings(&self) -> Result<Map<String, Value>> {
        let guard = self.lock();
        let mut stmt = guard
            .conn
            .prepare("SELECT id, value FROM lww WHERE kind = 'setting' AND field = 'value'")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = Map::new();
        for row in rows {
            let (key, raw) = row?;
            out.insert(key, serde_json::from_str(&raw).unwrap_or(Value::Null));
        }
        Ok(out)
    }

    pub fn devices(&self) -> Result<Vec<Device>> {
        let guard = self.lock();
        let mut stmt = guard.conn.prepare(
            "SELECT id, value FROM lww WHERE kind = 'device' AND field = 'name' ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (id, raw) = row?;
            let name = serde_json::from_str::<String>(&raw).unwrap_or_default();
            out.push(Device {
                current: id == guard.device_id,
                id,
                name,
            });
        }
        Ok(out)
    }

    // ---------------------------------------------------------------- 抓取状态（仅本机）

    /// 到了该刷新时间的订阅源。
    pub fn due_feeds(&self, now: i64) -> Result<Vec<FetchTarget>> {
        self.fetch_targets(
            "COALESCE(ff.next_fetch_at, 0) <= ?1",
            [SqlValue::Integer(now)],
        )
    }

    pub fn all_fetch_targets(&self) -> Result<Vec<FetchTarget>> {
        self.fetch_targets("1 = 1", [])
    }

    pub fn fetch_target(&self, feed_id: &str) -> Result<Option<FetchTarget>> {
        let targets = self.fetch_targets("f.id = ?1", [SqlValue::Text(feed_id.to_string())])?;
        Ok(targets.into_iter().next())
    }

    fn fetch_targets<const N: usize>(
        &self,
        cond: &str,
        args: [SqlValue; N],
    ) -> Result<Vec<FetchTarget>> {
        let guard = self.lock();
        let sql = format!(
            "SELECT f.id, f.url, f.interval_min, ff.etag, ff.last_modified, \
                    COALESCE(ff.error_count, 0) \
             FROM feeds f LEFT JOIN feed_fetch ff ON ff.feed_id = f.id \
             WHERE f.deleted = 0 AND f.url NOT LIKE 'postflow:%' AND {cond}"
        );
        let mut stmt = guard.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args), |r| {
            Ok(FetchTarget {
                feed_id: r.get(0)?,
                url: r.get(1)?,
                interval_min: r.get(2)?,
                etag: r.get(3)?,
                last_modified: r.get(4)?,
                error_count: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// 忘掉上次抓取的校验值和错误，换了订阅地址后用。
    pub fn reset_fetch(&self, feed_id: &str) -> Result<()> {
        self.lock()
            .conn
            .execute("DELETE FROM feed_fetch WHERE feed_id = ?1", [feed_id])?;
        Ok(())
    }

    pub fn record_fetch_ok(
        &self,
        feed_id: &str,
        etag: Option<&str>,
        last_modified: Option<&str>,
        now: i64,
        next_at: i64,
    ) -> Result<()> {
        self.lock().conn.execute(
            "INSERT INTO feed_fetch (feed_id, etag, last_modified, last_fetch_at, next_fetch_at, error, error_count) \
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, 0) \
             ON CONFLICT (feed_id) DO UPDATE SET etag = excluded.etag, \
                 last_modified = excluded.last_modified, last_fetch_at = excluded.last_fetch_at, \
                 next_fetch_at = excluded.next_fetch_at, error = NULL, error_code = NULL, error_count = 0",
            params![feed_id, etag, last_modified, now, next_at],
        )?;
        Ok(())
    }

    /// 记下一次失败的抓取。`code` 是错误的种类（界面按它说明），`error` 是原始的错误信息。
    pub fn record_fetch_err(
        &self,
        feed_id: &str,
        code: &str,
        error: &str,
        now: i64,
        next_at: i64,
    ) -> Result<()> {
        self.lock().conn.execute(
            "INSERT INTO feed_fetch (feed_id, last_fetch_at, next_fetch_at, error, error_code, error_count) \
             VALUES (?1, ?2, ?3, ?4, ?5, 1) \
             ON CONFLICT (feed_id) DO UPDATE SET last_fetch_at = excluded.last_fetch_at, \
                 next_fetch_at = excluded.next_fetch_at, error = excluded.error, \
                 error_code = excluded.error_code, error_count = feed_fetch.error_count + 1",
            params![feed_id, now, next_at, error, code],
        )?;
        Ok(())
    }

    /// 缓存抓取到的全文。只存本机，别的设备需要时自己再抓。
    pub fn set_full_text(&self, item_id: &str, html: &str) -> Result<()> {
        self.lock().conn.execute(
            "INSERT OR REPLACE INTO item_fulltext (id, html) VALUES (?1, ?2)",
            params![item_id, html],
        )?;
        Ok(())
    }
}

// -------------------------------------------------------------------- 内部实现

fn init_schema(conn: &Connection, data_dir: &Path) -> Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;",
    )?;
    let get = |key: &str| -> Result<Option<String>> {
        Ok(conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    };
    let version = SCHEMA_VERSION.to_string();
    let dir = data_dir.to_string_lossy().into_owned();

    // 表结构变了，或者换了数据目录（旧索引和邮件属于另一份数据里的账户），整个重建
    if get("schema")?.as_deref() != Some(version.as_str())
        || get("data_dir")?.as_deref() != Some(dir.as_str())
    {
        for table in DERIVED_TABLES.iter().chain(LOCAL_TABLES).chain(&["items"]) {
            conn.execute(&format!("DROP TABLE IF EXISTS {table}"), [])?;
        }
    }
    conn.execute_batch(SCHEMA)?;
    migrate(conn)?;
    conn.execute_batch(crate::search::SCHEMA)?;
    crate::search::init(conn)?;
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema', ?1), ('data_dir', ?2)",
        params![version, dir],
    )?;
    Ok(())
}

/// 给旧索引补上后来加的列。改版本号会连本机同步下来的邮件一起丢掉，加列不会。
fn migrate(conn: &Connection) -> Result<()> {
    add_column(conn, "feeds", "position", "REAL")?;
    add_column(conn, "accounts", "position", "REAL")?;
    add_column(conn, "accounts", "smtp_host", "TEXT")?;
    add_column(conn, "accounts", "smtp_port", "INTEGER")?;
    add_column(conn, "accounts", "smtp_security", "TEXT")?;
    add_column(conn, "accounts", "sender_name", "TEXT NOT NULL DEFAULT ''")?;
    add_column(conn, "accounts", "signature", "TEXT NOT NULL DEFAULT ''")?;
    add_column(conn, "mail_meta", "recipients", "TEXT")?;
    if add_column(conn, "items", "image", "TEXT")? {
        backfill_images(conn)?;
    }
    // 以前入库的播客单集没有附件，下次抓到同一条时补上（见 add_articles）
    add_column(conn, "items", "media", "TEXT")?;
    add_column(conn, "items", "media_kind", "TEXT")?;
    add_column(conn, "feeds", "kind", "TEXT")?;
    add_column(conn, "feeds", "full_text", "INTEGER NOT NULL DEFAULT 0")?;
    // 上次抓取出错的种类（postflow_feed::Error::code），界面按它给出说明；以前的错误没有，下次抓取时补上
    add_column(conn, "feed_fetch", "error_code", "TEXT")?;
    Ok(())
}

/// 表里没有这一列就加上。返回是否新加了。
fn add_column(conn: &Connection, table: &str, column: &str, decl: &str) -> Result<bool> {
    let exists: bool = conn.query_row(
        &format!("SELECT COUNT(*) > 0 FROM pragma_table_info('{table}') WHERE name = ?1"),
        [column],
        |r| r.get(0),
    )?;
    if !exists {
        conn.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {decl}"),
            [],
        )?;
    }
    Ok(!exists)
}

/// 刚加上封面列时，从已经入库的文章正文里找一遍。
fn backfill_images(conn: &Connection) -> Result<()> {
    let found: Vec<(String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT i.id, i.url, c.html FROM items i JOIN item_content c ON c.id = i.id \
             WHERE i.kind = 'article'",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut found = Vec::new();
        for row in rows {
            let (id, url, html) = row?;
            if let Some(image) = crate::image::first_image(&html, url.as_deref()) {
                found.push((id, image));
            }
        }
        found
    };
    let mut update = conn.prepare("UPDATE items SET image = ?2 WHERE id = ?1")?;
    for (id, image) in found {
        update.execute(params![id, image])?;
    }
    Ok(())
}

fn import_file(
    conn: &mut Connection,
    clock: &mut Clock,
    file: &LogFile,
    stats: &mut ImportStats,
) -> Result<()> {
    let offset: u64 = conn
        .query_row(
            "SELECT offset FROM log_files WHERE path = ?1",
            [&file.rel],
            |r| r.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(0) as u64;
    if offset == file.len {
        return Ok(());
    }
    let chunk = read_new(&file.path, offset)?;
    if chunk.lines.is_empty() && chunk.new_offset == offset {
        return Ok(());
    }

    let tx = conn.transaction()?;
    for line in &chunk.lines {
        match file.kind {
            LogKind::Ops => match serde_json::from_str::<Op>(line) {
                Ok(op) => {
                    clock.observe(&op.ts);
                    if apply_op(&tx, &op)? {
                        stats.ops += 1;
                    }
                }
                Err(_) => stats.skipped += 1,
            },
            LogKind::Articles => match serde_json::from_str::<Article>(line) {
                Ok(article) => {
                    if insert_article(&tx, &article)? {
                        stats.articles += 1;
                    }
                }
                Err(_) => stats.skipped += 1,
            },
        }
    }
    tx.execute(
        "INSERT INTO log_files (path, offset) VALUES (?1, ?2) \
         ON CONFLICT (path) DO UPDATE SET offset = excluded.offset",
        params![file.rel, chunk.new_offset as i64],
    )?;
    tx.commit()?;
    stats.files += 1;
    Ok(())
}

/// 应用一条操作：每个字段只有在时间戳更新时才覆盖。重复应用同一条操作没有副作用。
fn apply_op(conn: &Connection, op: &Op) -> Result<bool> {
    let ts = op.ts.to_string();
    let mut changed = false;
    for id in op.targets() {
        let mut touched = false;
        {
            let mut upsert = conn.prepare_cached(
                "INSERT INTO lww (kind, id, field, value, ts) VALUES (?1, ?2, ?3, ?4, ?5) \
                 ON CONFLICT (kind, id, field) DO UPDATE \
                 SET value = excluded.value, ts = excluded.ts WHERE excluded.ts > lww.ts",
            )?;
            for (field, value) in &op.set {
                touched |= upsert.execute(params![op.k, id, field, value.to_string(), ts])? > 0;
            }
        }
        if touched {
            match op.k.as_str() {
                "feed" => materialize_feed(conn, id)?,
                "item" => materialize_item(conn, id)?,
                "account" => crate::mail::materialize_account(conn, id)?,
                _ => {}
            }
            changed = true;
        }
    }
    Ok(changed)
}

pub(crate) fn lww_fields(
    conn: &Connection,
    kind: &str,
    id: &str,
) -> Result<HashMap<String, Value>> {
    let mut stmt =
        conn.prepare_cached("SELECT field, value FROM lww WHERE kind = ?1 AND id = ?2")?;
    let rows = stmt.query_map([kind, id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    let mut out = HashMap::new();
    for row in rows {
        let (field, raw) = row?;
        out.insert(field, serde_json::from_str(&raw).unwrap_or(Value::Null));
    }
    Ok(out)
}

/// 由字段表重新生成 feeds 表里的一行。字段到达的顺序不影响结果。
fn materialize_feed(conn: &Connection, id: &str) -> Result<()> {
    let fields = lww_fields(conn, "feed", id)?;
    let text = |key: &str| fields.get(key).and_then(Value::as_str).map(str::to_string);
    // 地址还没同步过来时先不建行，等它到了自然会补上
    let Some(url) = text("url") else {
        return Ok(());
    };
    conn.execute(
        "INSERT OR REPLACE INTO feeds \
         (id, url, title, site_url, icon_url, folder, interval_min, deleted, added_at, position, kind, full_text) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            id,
            url,
            text("title").unwrap_or_default(),
            text("site_url"),
            text("icon_url"),
            text("folder").unwrap_or_default(),
            fields.get("interval_min").and_then(Value::as_i64),
            fields
                .get("deleted")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            fields.get("added_at").and_then(Value::as_i64).unwrap_or(0),
            fields.get("position").and_then(Value::as_f64),
            text("kind").filter(|k| FEED_KINDS.contains(&k.as_str())),
            fields
                .get("full_text")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ],
    )?;
    Ok(())
}

/// 把条目状态写到 items 表。文章本身还没同步到时什么都不做，等文章入库时会再调用一次。
fn materialize_item(conn: &Connection, id: &str) -> Result<()> {
    let fields = lww_fields(conn, "item", id)?;
    if fields.is_empty() {
        return Ok(());
    }
    let flag = |key: &str| fields.get(key).and_then(Value::as_bool).unwrap_or(false);
    conn.prepare_cached(
        "UPDATE items SET read = ?2, starred = ?3, archived = ?4 WHERE id = ?1 AND kind = 'article'",
    )?
        .execute(params![id, flag("read"), flag("starred"), flag("archived")])?;
    Ok(())
}

fn insert_article(conn: &Connection, a: &Article) -> Result<bool> {
    let inserted = conn
        .prepare_cached(
            "INSERT OR IGNORE INTO items \
             (id, kind, source_id, guid, url, title, author, summary, published_at, fetched_at, image, \
              media, media_kind) \
             VALUES (?1, 'article', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )?
        .execute(params![
            a.id,
            a.feed,
            a.guid,
            a.url,
            a.title,
            a.author,
            a.summary,
            a.published,
            a.fetched,
            article_image(a),
            media_json(a),
            a.media.as_ref().map(Media::kind),
        ])?;
    if inserted == 0 {
        // 早先入库时附件没存全的单集：这次带着来了，补上
        if let Some(media) = &a.media {
            let stored: Option<Media> = conn
                .prepare_cached("SELECT media FROM items WHERE id = ?1 AND kind = 'article'")?
                .query_row([&a.id], |r| r.get::<_, Option<String>>(0))
                .optional()?
                .flatten()
                .and_then(|json| serde_json::from_str(&json).ok());
            if media.adds_to(stored.as_ref()) {
                conn.prepare_cached("UPDATE items SET media = ?2, media_kind = ?3 WHERE id = ?1 AND kind = 'article'")?
                    .execute(params![a.id, media_json(a), media.kind()])?;
            }
        }
        return Ok(false);
    }
    conn.prepare_cached("INSERT OR REPLACE INTO item_content (id, html) VALUES (?1, ?2)")?
        .execute(params![a.id, a.content])?;
    materialize_item(conn, &a.id)?;
    Ok(true)
}

fn media_json(a: &Article) -> Option<String> {
    a.media.as_ref().and_then(|m| serde_json::to_string(m).ok())
}

/// 文章的封面：源里给了就用，否则从正文里找。
fn article_image(a: &Article) -> Option<String> {
    a.image
        .clone()
        .filter(|u| u.starts_with("https://") || u.starts_with("http://"))
        .or_else(|| crate::image::first_image(&a.content, a.url.as_deref()))
}

fn filter_sql(filter: &ItemFilter, args: &mut Vec<SqlValue>) -> String {
    use crate::search::Filter;
    let parsed = filter
        .query
        .as_deref()
        .filter(|q| !q.trim().is_empty())
        .map(|q| crate::search::parse(q, filter.tz_offset.unwrap_or(0)));
    // 搜索里写了 in:文件夹：只在那个文件夹里找，不管现在看的是哪个列表
    let folder = parsed.as_ref().and_then(|p| {
        p.filters.iter().find_map(|(f, negated)| match f {
            Filter::In(code) if !negated => Some(*code),
            _ => None,
        })
    });
    let mut conds = if let Some(code) = folder {
        format!("i.archived = {code} AND {ALIVE}")
    } else {
        match filter.view {
            View::Inbox => format!("i.archived = 0 AND {ALIVE} AND NOT {}", snoozed_now()),
            View::All => format!("i.archived < 2 AND {ALIVE}"),
            View::Today => {
                args.push(SqlValue::Integer(filter.since.unwrap_or(0)));
                format!(
                    "i.archived = 0 AND {ALIVE} AND i.published_at >= ?{} AND NOT {}",
                    args.len(),
                    snoozed_now()
                )
            }
            View::Unread => format!(
                "i.read = 0 AND i.archived = 0 AND {ALIVE} AND NOT {}",
                snoozed_now()
            ),
            // 星标的文章即使退订了也保留。垃圾邮件和已删除的不算
            View::Starred => "i.starred = 1 AND i.archived < 2".to_string(),
            View::Archived => format!("i.archived = 1 AND {ALIVE}"),
            View::Junk => format!("i.archived = 2 AND {ALIVE}"),
            View::Trash => format!("i.archived = 3 AND {ALIVE}"),
            View::Drafts => format!("i.archived = 4 AND {ALIVE}"),
            View::Sent => format!("i.archived = 5 AND {ALIVE}"),
            View::Folder => format!("i.archived = 6 AND {ALIVE}"),
            View::Snoozed => format!("i.archived < 2 AND {ALIVE} AND {}", snoozed_now()),
            View::Waiting => format!("i.archived = 5 AND {ALIVE} AND {}", waiting_sql()),
            View::Later => format!(
                "i.archived = 0 AND {ALIVE} AND f.url = 'postflow:later' AND NOT {}",
                snoozed_now()
            ),
            // 稍后提醒刚到时间的；以及发出去的信「没回复就提醒」到了时间、还没回复的（提醒时标成了未读）
            View::Reminders => format!(
                "i.read = 0 AND {ALIVE} AND ((i.archived = 0 AND EXISTS (SELECT 1 FROM lww s \
             WHERE s.kind = 'snooze' AND s.id = i.id AND s.field = 'until' \
             AND CAST(s.value AS INTEGER) BETWEEN {NOW_MS} - {REMINDER_WINDOW_MS} AND {NOW_MS})) \
             OR (i.archived = 5 AND EXISTS (SELECT 1 FROM lww s \
             WHERE s.kind = 'followup' AND s.id = m.message_id AND s.field = 'until' \
             AND CAST(s.value AS INTEGER) BETWEEN {NOW_MS} - {REMINDER_WINDOW_MS} AND {NOW_MS})))"
            ),
        }
    };
    if let Some(source_id) = &filter.source_id {
        args.push(SqlValue::Text(source_id.clone()));
        conds.push_str(&format!(" AND i.source_id = ?{}", args.len()));
    } else if matches!(filter.view, View::Inbox | View::Today | View::Unread)
        && filter.label.is_none()
    {
        // 存下的网页只在「稍后读」里，不混进订阅的文章流
        conds.push_str(&format!(" AND {NOT_LATER}"));
    }
    if let Some(mailbox) = &filter.mailbox {
        args.push(SqlValue::Text(mailbox.clone()));
        conds.push_str(&format!(
            " AND i.id IN (SELECT id FROM mail_meta WHERE mailbox = ?{})",
            args.len()
        ));
    }
    if let Some(folder) = &filter.folder {
        args.push(SqlValue::Text(folder.clone()));
        conds.push_str(&format!(" AND f.folder = ?{}", args.len()));
    }
    // 放到订阅里看的订阅邮件：在订阅（文章）那边，不在邮件那边
    if let Some(kind) = filter.kind {
        conds.push_str(match kind {
            ItemKind::Article => concat!(" AND (i.kind = 'article' OR ", newsletter_sql!(), ")"),
            ItemKind::Mail => concat!(" AND i.kind = 'mail' AND NOT ", newsletter_sql!()),
        });
    }
    if let Some(sender) = filter.sender.as_deref().filter(|s| !s.trim().is_empty()) {
        args.push(SqlValue::Text(sender.trim().to_lowercase()));
        conds.push_str(&format!(" AND lower(m.from_address) = ?{}", args.len()));
    }
    if let Some(label) = &filter.label {
        args.push(SqlValue::Text(label.clone()));
        conds.push_str(&format!(" AND l.label = ?{}", args.len()));
    }
    if filter.feed_kind.as_deref() == Some("newsletter") {
        conds.push_str(concat!(" AND ", newsletter_sql!()));
    } else if let Some(feed_kind) = &filter.feed_kind {
        args.push(SqlValue::Text(feed_kind.clone()));
        conds.push_str(&format!(
            " AND i.kind = 'article' AND i.source_id IN \
               (SELECT fk.id FROM feeds fk WHERE {} = ?{})",
            feed_kind_sql("fk"),
            args.len()
        ));
    }
    if let Some(parsed) = parsed {
        let matching = |args: &mut Vec<SqlValue>, expr: String| {
            args.push(SqlValue::Text(expr));
            format!(
                "i.id IN (SELECT d.id FROM search_docs d WHERE d.docid IN \
                   (SELECT rowid FROM search WHERE search MATCH ?{}))",
                args.len()
            )
        };
        if let Some(expr) = parsed.include {
            let cond = matching(args, expr);
            conds.push_str(&format!(" AND {cond}"));
        }
        for expr in parsed.exclude {
            let cond = matching(args, expr);
            conds.push_str(&format!(" AND NOT {cond}"));
        }
        for (f, negated) in parsed.filters {
            let cond = match f {
                Filter::HasAttachment => "COALESCE(m.has_attachments, 0) = 1".to_string(),
                Filter::Unread => "i.read = 0".to_string(),
                Filter::Read => "i.read = 1".to_string(),
                Filter::Starred => "i.starred = 1".to_string(),
                Filter::Before(at) => format!("i.published_at < {at}"),
                Filter::After(at) => format!("i.published_at >= {at}"),
                Filter::In(code) => format!("i.archived = {code}"),
            };
            conds.push_str(&format!(
                " AND {}({cond})",
                if negated { "NOT " } else { "" }
            ));
        }
        // 只有标点之类搜不了的字，或者看不懂的日期
        if parsed.impossible {
            conds.push_str(" AND 0");
        }
    }
    conds
}

/// 订阅源 `f` 的类型：用户选的；没选时，一半以上的条目带音频算播客，带视频或者是视频网站的源算视频，
/// 其他是文章。YouTube 的条目附件不是视频文件，按地址认。
pub(crate) fn feed_kind_sql(f: &str) -> String {
    format!(
        "COALESCE({f}.kind, CASE \
           WHEN (SELECT COUNT(*) FROM items k WHERE k.source_id = {f}.id AND k.media_kind = 'audio') * 2 \
                > (SELECT COUNT(*) FROM items k WHERE k.source_id = {f}.id) THEN 'podcast' \
           WHEN {f}.url LIKE '%youtube.com/%' OR {f}.url LIKE '%bilibili%' OR {f}.url LIKE '%vimeo.com/%' \
             OR (SELECT COUNT(*) FROM items k WHERE k.source_id = {f}.id AND k.media_kind = 'video') * 2 \
                > (SELECT COUNT(*) FROM items k WHERE k.source_id = {f}.id) THEN 'video' \
           ELSE 'article' END)"
    )
}

fn feed_sql(cond: &str) -> String {
    format!(
        "SELECT f.id, f.url, f.title, f.site_url, f.icon_url, f.folder, f.interval_min, f.added_at, \
                (SELECT COUNT(*) FROM items i \
                  WHERE i.source_id = f.id AND i.read = 0 AND i.archived = 0 AND NOT {snoozed}), \
                ff.last_fetch_at, ff.error, {kind}, f.kind, f.full_text, ff.error_code \
         FROM feeds f LEFT JOIN feed_fetch ff ON ff.feed_id = f.id \
         WHERE {cond} \
         ORDER BY f.folder COLLATE NOCASE, f.position IS NULL, f.position, f.title COLLATE NOCASE",
        snoozed = snoozed_now(),
        kind = feed_kind_sql("f"),
    )
}

fn feed_from_row(r: &Row<'_>) -> rusqlite::Result<Feed> {
    Ok(Feed {
        id: r.get(0)?,
        url: r.get(1)?,
        title: r.get(2)?,
        site_url: r.get(3)?,
        icon_url: r.get(4)?,
        folder: r.get(5)?,
        interval_min: r.get(6)?,
        added_at: r.get(7)?,
        unread: r.get(8)?,
        last_fetch_at: r.get(9)?,
        error: r.get(10)?,
        kind: r.get(11)?,
        kind_set: r.get(12)?,
        full_text: r.get(13)?,
        error_code: r.get(14)?,
    })
}

pub(crate) fn item_from_row(r: &Row<'_>) -> rusqlite::Result<ItemSummary> {
    let mut item = raw_item_from_row(r)?;
    // 存下的网页各来自不同的网站：来源显示网站的域名，图标由界面按域名去找
    if item.kind == "article" && item.source_id == later_feed_id() {
        item.source_title = item.url.as_deref().map(site_of).unwrap_or_default();
        item.source_icon = None;
    }
    Ok(item)
}

/// 网址的域名，去掉开头的 `www.`。
fn site_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    let host = host.split(':').next().unwrap_or_default();
    host.trim_start_matches("www.").to_ascii_lowercase()
}

fn raw_item_from_row(r: &Row<'_>) -> rusqlite::Result<ItemSummary> {
    Ok(ItemSummary {
        id: r.get(0)?,
        kind: r.get(1)?,
        source_id: r.get(2)?,
        source_title: r.get(3)?,
        source_icon: r.get(4)?,
        url: r.get(5)?,
        title: r.get(6)?,
        author: r.get(7)?,
        summary: r.get(8)?,
        published_at: r.get(9)?,
        read: r.get(10)?,
        starred: r.get(11)?,
        // 0 收件箱，1 归档，2 垃圾邮件，3 已删除，4 草稿
        archived: r.get::<_, i64>(12)? != 0,
        draft: r.get::<_, i64>(12)? == 4,
        sent: r.get::<_, i64>(12)? == 5,
        sender_address: r.get(13)?,
        has_attachments: r.get(14)?,
        image: r.get(15)?,
        label: r.get(16)?,
        snoozed_until: r.get(17)?,
        snippet: None,
        media: r
            .get::<_, Option<String>>(18)?
            .and_then(|json| serde_json::from_str(&json).ok()),
    })
}
