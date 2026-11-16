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
    "feed_fetch",
    "item_fulltext",
    "mail_meta",
    "mail_body",
    "mail_state",
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
    position     REAL
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
    position      REAL
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
    image        TEXT
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

CREATE TABLE IF NOT EXISTS feed_fetch (
    feed_id       TEXT PRIMARY KEY,
    etag          TEXT,
    last_modified TEXT,
    last_fetch_at INTEGER,
    next_fetch_at INTEGER NOT NULL DEFAULT 0,
    error         TEXT,
    error_count   INTEGER NOT NULL DEFAULT 0
) WITHOUT ROWID;
"#;

/// 列表查询的表连接：文章连订阅源，邮件连账户和信头。
pub(crate) const ITEM_FROM: &str = "items i \
     LEFT JOIN feeds f ON i.kind = 'article' AND f.id = i.source_id \
     LEFT JOIN accounts a ON i.kind = 'mail' AND a.id = i.source_id \
     LEFT JOIN mail_meta m ON i.kind = 'mail' AND m.id = i.id";

const ITEM_COLUMNS: &str = "i.id, i.kind, i.source_id, \
     CASE WHEN i.kind = 'mail' THEN COALESCE(NULLIF(a.name, ''), a.email, '') ELSE COALESCE(f.title, '') END, \
     f.icon_url, i.url, i.title, i.author, i.summary, i.published_at, i.read, i.starred, i.archived, \
     m.from_address, COALESCE(m.has_attachments, 0), i.image";

/// 来源还在：订阅没退订，邮箱没移除。
pub(crate) const ALIVE: &str = "COALESCE(f.deleted, a.deleted, 1) = 0";

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
        if let Some(interval) = patch.interval_min {
            set.insert("interval_min".into(), json!(interval));
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
        let mail: HashSet<String> = self.mail_ids_among(ids)?;
        let articles: Vec<String> = ids
            .iter()
            .filter(|id| !mail.contains(*id))
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
            let mut exists = inner
                .conn
                .prepare_cached("SELECT 1 FROM items WHERE id = ?1")?;
            for article in &articles {
                if !seen.insert(article.id.as_str()) || exists.exists([&article.id])? {
                    continue;
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
        let mut stmt = guard.conn.prepare(&feed_sql("f.deleted = 0"))?;
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
        Ok(rows.collect::<rusqlite::Result<_>>()?)
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
                    content: r.get(16)?,
                    full_text: r.get(17)?,
                })
            })
            .optional()?;
        Ok(detail)
    }

    pub fn counts(&self, today_since: i64) -> Result<Counts> {
        let guard = self.lock();
        let sql = format!(
            "SELECT COALESCE(SUM(i.read = 0), 0), \
                    COALESCE(SUM(i.read = 0 AND i.published_at >= ?1), 0) \
             FROM {ITEM_FROM} WHERE i.archived = 0 AND {ALIVE}"
        );
        let (unread, today) = guard
            .conn
            .query_row(&sql, [today_since], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let starred =
            guard
                .conn
                .query_row("SELECT COUNT(*) FROM items WHERE starred = 1", [], |r| {
                    r.get(0)
                })?;
        Ok(Counts {
            unread,
            today,
            starred,
        })
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
             WHERE f.deleted = 0 AND {cond}"
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
                 next_fetch_at = excluded.next_fetch_at, error = NULL, error_count = 0",
            params![feed_id, etag, last_modified, now, next_at],
        )?;
        Ok(())
    }

    pub fn record_fetch_err(
        &self,
        feed_id: &str,
        error: &str,
        now: i64,
        next_at: i64,
    ) -> Result<()> {
        self.lock().conn.execute(
            "INSERT INTO feed_fetch (feed_id, last_fetch_at, next_fetch_at, error, error_count) \
             VALUES (?1, ?2, ?3, ?4, 1) \
             ON CONFLICT (feed_id) DO UPDATE SET last_fetch_at = excluded.last_fetch_at, \
                 next_fetch_at = excluded.next_fetch_at, error = excluded.error, \
                 error_count = feed_fetch.error_count + 1",
            params![feed_id, now, next_at, error],
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
    if add_column(conn, "items", "image", "TEXT")? {
        backfill_images(conn)?;
    }
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
         (id, url, title, site_url, icon_url, folder, interval_min, deleted, added_at, position) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
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
             (id, kind, source_id, guid, url, title, author, summary, published_at, fetched_at, image) \
             VALUES (?1, 'article', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
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
        ])?;
    if inserted == 0 {
        return Ok(false);
    }
    conn.prepare_cached("INSERT OR REPLACE INTO item_content (id, html) VALUES (?1, ?2)")?
        .execute(params![a.id, a.content])?;
    materialize_item(conn, &a.id)?;
    Ok(true)
}

/// 文章的封面：源里给了就用，否则从正文里找。
fn article_image(a: &Article) -> Option<String> {
    a.image
        .clone()
        .filter(|u| u.starts_with("https://") || u.starts_with("http://"))
        .or_else(|| crate::image::first_image(&a.content, a.url.as_deref()))
}

fn filter_sql(filter: &ItemFilter, args: &mut Vec<SqlValue>) -> String {
    let mut conds = match filter.view {
        View::Inbox => format!("i.archived = 0 AND {ALIVE}"),
        View::Today => {
            args.push(SqlValue::Integer(filter.since.unwrap_or(0)));
            format!(
                "i.archived = 0 AND {ALIVE} AND i.published_at >= ?{}",
                args.len()
            )
        }
        View::Unread => format!("i.read = 0 AND i.archived = 0 AND {ALIVE}"),
        // 星标的文章即使退订了也保留
        View::Starred => "i.starred = 1".to_string(),
        View::Archived => format!("i.archived = 1 AND {ALIVE}"),
    };
    if let Some(source_id) = &filter.source_id {
        args.push(SqlValue::Text(source_id.clone()));
        conds.push_str(&format!(" AND i.source_id = ?{}", args.len()));
    }
    if let Some(folder) = &filter.folder {
        args.push(SqlValue::Text(folder.clone()));
        conds.push_str(&format!(" AND f.folder = ?{}", args.len()));
    }
    if let Some(kind) = filter.kind {
        args.push(SqlValue::Text(kind.as_str().to_string()));
        conds.push_str(&format!(" AND i.kind = ?{}", args.len()));
    }
    conds
}

fn feed_sql(cond: &str) -> String {
    format!(
        "SELECT f.id, f.url, f.title, f.site_url, f.icon_url, f.folder, f.interval_min, f.added_at, \
                (SELECT COUNT(*) FROM items i \
                  WHERE i.source_id = f.id AND i.read = 0 AND i.archived = 0), \
                ff.last_fetch_at, ff.error \
         FROM feeds f LEFT JOIN feed_fetch ff ON ff.feed_id = f.id \
         WHERE {cond} \
         ORDER BY f.folder COLLATE NOCASE, f.position IS NULL, f.position, f.title COLLATE NOCASE"
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
    })
}

fn item_from_row(r: &Row<'_>) -> rusqlite::Result<ItemSummary> {
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
        archived: r.get(12)?,
        sender_address: r.get(13)?,
        has_attachments: r.get(14)?,
        image: r.get(15)?,
    })
}
