//! 邮箱账户和邮件的存储。
//!
//! 账户设置（不含密码）写进日志，同步给其他设备；邮件本身每台设备各自从邮件服务器同步，
//! 只存在本机索引里。已读、星标、归档以服务器为准，本机改了以后再写回服务器。

use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde_json::{Map, Value, json};

use crate::error::{Error, Result};
use crate::hlc::now_ms;
use crate::ids;
use crate::model::*;
use crate::store::{Store, lww_fields};

/// 同步时只看收件箱。
pub const INBOX: &str = "INBOX";

const ACCOUNT_SQL: &str = "SELECT a.id, a.email, a.name, a.imap_host, a.imap_port, a.imap_security, \
            a.username, a.added_at, \
            (SELECT COUNT(*) FROM items i \
              WHERE i.kind = 'mail' AND i.source_id = a.id AND i.read = 0 AND i.archived = 0), \
            s.last_sync_at, s.error, s.error_code \
     FROM accounts a LEFT JOIN mail_state s ON s.account_id = a.id";

fn account_from_row(r: &Row<'_>) -> rusqlite::Result<Account> {
    Ok(Account {
        id: r.get(0)?,
        email: r.get(1)?,
        name: r.get(2)?,
        imap_host: r.get(3)?,
        imap_port: r.get(4)?,
        imap_security: r.get(5)?,
        username: r.get(6)?,
        added_at: r.get(7)?,
        unread: r.get(8)?,
        last_sync_at: r.get(9)?,
        error: r.get(10)?,
        error_code: r.get(11)?,
    })
}

fn ref_from_row(r: &Row<'_>) -> rusqlite::Result<MailRef> {
    Ok(MailRef {
        id: r.get(0)?,
        account_id: r.get(1)?,
        mailbox: r.get(2)?,
        uid: r.get(3)?,
        message_id: r.get(4)?,
        size: r.get(5)?,
    })
}

const REF_SQL: &str =
    "SELECT id, account_id, mailbox, uid, message_id, size FROM mail_meta WHERE id = ?1";

/// 由字段表重新生成 accounts 表里的一行。
pub(crate) fn materialize_account(conn: &Connection, id: &str) -> Result<()> {
    let fields = lww_fields(conn, "account", id)?;
    let text = |key: &str| fields.get(key).and_then(Value::as_str).map(str::to_string);
    let Some(email) = text("email") else {
        return Ok(());
    };
    conn.execute(
        "INSERT OR REPLACE INTO accounts \
         (id, email, name, imap_host, imap_port, imap_security, username, deleted, added_at, position) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            id,
            email,
            text("name").unwrap_or_default(),
            text("imap_host").unwrap_or_default(),
            fields
                .get("imap_port")
                .and_then(Value::as_i64)
                .unwrap_or(993),
            text("imap_security").unwrap_or_else(|| "tls".to_string()),
            text("username").unwrap_or_default(),
            fields
                .get("deleted")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            fields.get("added_at").and_then(Value::as_i64).unwrap_or(0),
            fields.get("position").and_then(Value::as_f64),
        ],
    )?;
    // 别的设备移除了这个邮箱：本机同步下来的邮件也清掉
    if fields
        .get("deleted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        delete_account_mail(conn, id)?;
    }
    Ok(())
}

fn delete_account_mail(conn: &Connection, account_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM mail_body WHERE id IN (SELECT id FROM mail_meta WHERE account_id = ?1)",
        [account_id],
    )?;
    conn.execute(
        "DELETE FROM items WHERE kind = 'mail' AND source_id = ?1",
        [account_id],
    )?;
    conn.execute("DELETE FROM mail_meta WHERE account_id = ?1", [account_id])?;
    conn.execute("DELETE FROM mail_state WHERE account_id = ?1", [account_id])?;
    conn.execute(
        "DELETE FROM mail_folders WHERE account_id = ?1",
        [account_id],
    )?;
    Ok(())
}

fn delete_mail(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM items WHERE id = ?1 AND kind = 'mail'", [id])?;
    conn.execute("DELETE FROM mail_meta WHERE id = ?1", [id])?;
    conn.execute("DELETE FROM mail_body WHERE id = ?1", [id])?;
    Ok(())
}

impl Store {
    // ---------------------------------------------------------------- 账户

    /// 添加邮箱。同一个地址再次添加会更新设置；移除过的会恢复。
    pub fn put_account(&self, cfg: AccountConfig) -> Result<Account> {
        let email = cfg.email.trim().to_string();
        if !email.contains('@') {
            return Err(Error::Invalid("email address is invalid".into()));
        }
        let id = ids::account_id(&email);
        let existing: Option<bool> = self
            .lock()
            .conn
            .query_row("SELECT deleted FROM accounts WHERE id = ?1", [&id], |r| {
                r.get(0)
            })
            .optional()?;

        let mut set = Map::new();
        set.insert("email".into(), json!(email));
        set.insert("name".into(), json!(cfg.name.trim()));
        set.insert("imap_host".into(), json!(cfg.imap_host.trim()));
        set.insert("imap_port".into(), json!(cfg.imap_port));
        set.insert("imap_security".into(), json!(cfg.imap_security));
        set.insert("username".into(), json!(cfg.username.trim()));
        set.insert("deleted".into(), json!(false));
        if existing.is_none() {
            set.insert("added_at".into(), json!(now_ms()));
        }
        self.set_fields("account", std::slice::from_ref(&id), set)?;
        self.account(&id)?
            .ok_or_else(|| Error::Invalid("account was not created".into()))
    }

    pub fn update_account(&self, id: &str, patch: AccountPatch) -> Result<()> {
        let mut set = Map::new();
        if let Some(name) = patch.name {
            set.insert("name".into(), json!(name.trim()));
        }
        if let Some(host) = patch.imap_host {
            set.insert("imap_host".into(), json!(host.trim()));
        }
        if let Some(port) = patch.imap_port {
            set.insert("imap_port".into(), json!(port));
        }
        if let Some(security) = patch.imap_security {
            set.insert("imap_security".into(), json!(security));
        }
        if let Some(username) = patch.username {
            set.insert("username".into(), json!(username.trim()));
        }
        self.set_fields("account", &[id.to_string()], set)
    }

    /// 移除邮箱：所有设备上都不再显示，本机同步下来的邮件一并删掉。服务器上的邮件不受影响。
    pub fn remove_account(&self, id: &str) -> Result<()> {
        let mut set = Map::new();
        set.insert("deleted".into(), json!(true));
        self.set_fields("account", &[id.to_string()], set)?;
        delete_account_mail(&self.lock().conn, id)
    }

    pub fn accounts(&self) -> Result<Vec<Account>> {
        let guard = self.lock();
        let mut stmt = guard.conn.prepare(&format!(
            "{ACCOUNT_SQL} WHERE a.deleted = 0 \
             ORDER BY a.position IS NULL, a.position, a.added_at, a.email"
        ))?;
        let rows = stmt.query_map([], account_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// 按给定的顺序排列邮箱。只写顺序变了的那些。
    pub fn reorder_accounts(&self, ids: &[String]) -> Result<()> {
        for (index, id) in ids.iter().enumerate() {
            let current: Option<Option<f64>> = self
                .lock()
                .conn
                .query_row(
                    "SELECT position FROM accounts WHERE id = ?1 AND deleted = 0",
                    [id],
                    |r| r.get(0),
                )
                .optional()?;
            let Some(old) = current else {
                continue;
            };
            let position = index as f64;
            if old == Some(position) {
                continue;
            }
            let mut set = Map::new();
            set.insert("position".into(), json!(position));
            self.set_fields("account", std::slice::from_ref(id), set)?;
        }
        Ok(())
    }

    pub fn account(&self, id: &str) -> Result<Option<Account>> {
        let guard = self.lock();
        let mut stmt = guard
            .conn
            .prepare(&format!("{ACCOUNT_SQL} WHERE a.deleted = 0 AND a.id = ?1"))?;
        Ok(stmt.query_row([id], account_from_row).optional()?)
    }

    // ---------------------------------------------------------------- 同步

    /// 上次同步时收件箱的 UIDVALIDITY。
    pub fn mail_uidvalidity(&self, account_id: &str) -> Result<Option<u32>> {
        Ok(self
            .lock()
            .conn
            .query_row(
                "SELECT uidvalidity FROM mail_state WHERE account_id = ?1",
                [account_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten())
    }

    /// 上次同步时，这个文件夹在服务器上叫什么、UIDVALIDITY 是多少。
    pub fn mail_folder(
        &self,
        account_id: &str,
        folder: MailFolder,
    ) -> Result<Option<(String, u32)>> {
        if folder == MailFolder::Inbox {
            return Ok(self
                .mail_uidvalidity(account_id)?
                .map(|v| (INBOX.to_string(), v)));
        }
        Ok(self
            .lock()
            .conn
            .query_row(
                "SELECT name, uidvalidity FROM mail_folders WHERE account_id = ?1 AND folder = ?2",
                params![account_id, folder.code()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    /// 本机已有的、上次同步时还在 `mailbox` 里的邮件。
    pub fn known_mail(&self, account_id: &str, mailbox: &str) -> Result<Vec<KnownMail>> {
        let guard = self.lock();
        let mut stmt = guard.conn.prepare(
            "SELECT m.id, m.uid, i.read, i.starred, i.archived, m.touched_at \
             FROM mail_meta m JOIN items i ON i.id = m.id \
             WHERE m.account_id = ?1 AND m.mailbox = ?2 AND m.uid IS NOT NULL",
        )?;
        let rows = stmt.query_map(params![account_id, mailbox], |r| {
            Ok(KnownMail {
                id: r.get(0)?,
                uid: r.get(1)?,
                read: r.get(2)?,
                starred: r.get(3)?,
                archived: r.get(4)?,
                touched_at: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// 文件夹被服务器重建了（UIDVALIDITY 变了），或者换了一个文件夹：本机记的 UID 全部作废。
    /// 收件箱里本机归档、还没写回服务器的那些保留。
    pub fn reset_mail_folder(&self, account_id: &str, folder: MailFolder) -> Result<()> {
        let guard = self.lock();
        let ids: Vec<String> = {
            let mut stmt = guard.conn.prepare(
                "SELECT m.id FROM mail_meta m JOIN items i ON i.id = m.id \
                 WHERE m.account_id = ?1 AND i.archived = ?2 \
                   AND (?2 != 0 OR m.mailbox = ?3)",
            )?;
            let rows = stmt.query_map(params![account_id, folder.code(), INBOX], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for id in ids {
            delete_mail(&guard.conn, &id)?;
        }
        Ok(())
    }

    /// 把一次收件箱同步的结果写进去。
    pub fn apply_mail_sync(&self, account_id: &str, sync: MailSync) -> Result<MailSyncStats> {
        self.apply_folder_sync(account_id, MailFolder::Inbox, INBOX, sync)
    }

    /// 把一次同步的结果写进去。`mailbox` 是这个文件夹在服务器上的名字。
    pub fn apply_folder_sync(
        &self,
        account_id: &str,
        folder: MailFolder,
        mailbox: &str,
        sync: MailSync,
    ) -> Result<MailSyncStats> {
        let mut guard = self.lock();
        let tx = guard.conn.transaction()?;
        let mut stats = MailSyncStats::default();
        let now = now_ms() as i64;

        if folder == MailFolder::Inbox {
            tx.execute(
                "INSERT INTO mail_state (account_id, uidvalidity) VALUES (?1, ?2) \
                 ON CONFLICT (account_id) DO UPDATE SET uidvalidity = excluded.uidvalidity",
                params![account_id, sync.uidvalidity],
            )?;
        } else {
            tx.execute(
                "INSERT OR REPLACE INTO mail_folders (account_id, folder, name, uidvalidity) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![account_id, folder.code(), mailbox, sync.uidvalidity],
            )?;
        }

        for mail in sync.new {
            // 同一封邮件可能因为被移回收件箱而换了 UID，按 ID 合并
            tx.execute(
                "INSERT INTO items \
                 (id, kind, source_id, guid, url, title, author, summary, published_at, fetched_at, \
                  read, starred, archived) \
                 VALUES (?1, 'mail', ?2, ?3, NULL, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) \
                 ON CONFLICT (id) DO UPDATE SET read = excluded.read, starred = excluded.starred, \
                     archived = excluded.archived",
                params![
                    mail.id,
                    account_id,
                    mail.message_id.clone().unwrap_or_default(),
                    mail.subject,
                    mail.from_name.clone().or_else(|| mail.from_address.clone()),
                    mail.snippet,
                    mail.date,
                    now,
                    mail.read,
                    mail.starred,
                    folder.code(),
                ],
            )?;
            tx.execute(
                "INSERT INTO mail_meta \
                 (id, account_id, mailbox, uidvalidity, uid, message_id, from_name, from_address, \
                  has_attachments, list_unsubscribe, size) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) \
                 ON CONFLICT (id) DO UPDATE SET mailbox = excluded.mailbox, \
                     uidvalidity = excluded.uidvalidity, uid = excluded.uid",
                params![
                    mail.id,
                    account_id,
                    mailbox,
                    sync.uidvalidity,
                    mail.uid,
                    mail.message_id,
                    mail.from_name,
                    mail.from_address,
                    mail.has_attachments,
                    mail.list_unsubscribe,
                    mail.size,
                ],
            )?;
            if let Some(raw) = &mail.raw {
                tx.execute(
                    "INSERT OR REPLACE INTO mail_body (id, raw) VALUES (?1, ?2)",
                    params![mail.id, raw],
                )?;
            }
            stats.added += 1;
        }

        for (uid, read, starred) in sync.flags {
            let row: Option<(String, i64)> = tx
                .query_row(
                    "SELECT id, touched_at FROM mail_meta \
                     WHERE account_id = ?1 AND mailbox = ?2 AND uid = ?3",
                    params![account_id, mailbox, uid],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let Some((id, touched_at)) = row else {
                continue;
            };
            if touched_at >= sync.started_at {
                continue;
            }
            stats.updated += tx.execute(
                "UPDATE items SET read = ?2, starred = ?3 \
                 WHERE id = ?1 AND (read != ?2 OR starred != ?3)",
                params![id, read, starred],
            )?;
        }

        // 垃圾邮件和已删除以服务器为准：本机点了移回收件箱、却没移成的，放回原处
        if folder != MailFolder::Inbox {
            stats.updated += tx.execute(
                "UPDATE items SET archived = ?3 WHERE archived != ?3 AND id IN \
                 (SELECT id FROM mail_meta WHERE account_id = ?1 AND mailbox = ?2 AND touched_at < ?4)",
                params![account_id, mailbox, folder.code(), sync.started_at],
            )?;
        }

        for uid in sync.vanished {
            let row: Option<(String, i64, i64)> = tx
                .query_row(
                    "SELECT m.id, m.touched_at, i.archived FROM mail_meta m JOIN items i ON i.id = m.id \
                     WHERE m.account_id = ?1 AND m.mailbox = ?2 AND m.uid = ?3",
                    params![account_id, mailbox, uid],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let Some((id, touched_at, archived)) = row else {
                continue;
            };
            // 本机刚归档、正等着写回服务器的，不算消失
            if archived == 1 || touched_at >= sync.started_at {
                continue;
            }
            delete_mail(&tx, &id)?;
            stats.removed += 1;
        }

        tx.commit()?;
        Ok(stats)
    }

    pub fn record_mail_sync(&self, account_id: &str, error: Option<(&str, &str)>) -> Result<()> {
        let now = now_ms() as i64;
        let (code, message) = match error {
            Some((code, message)) => (Some(code), Some(message)),
            None => (None, None),
        };
        self.lock().conn.execute(
            "INSERT INTO mail_state (account_id, last_sync_at, error, error_code) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT (account_id) DO UPDATE SET \
                 last_sync_at = CASE WHEN excluded.error IS NULL THEN excluded.last_sync_at ELSE mail_state.last_sync_at END, \
                 error = excluded.error, error_code = excluded.error_code",
            params![account_id, if error.is_none() { Some(now) } else { None }, message, code],
        )?;
        Ok(())
    }

    // ---------------------------------------------------------------- 状态

    pub(crate) fn mail_ids_among(&self, ids: &[String]) -> Result<HashSet<String>> {
        let guard = self.lock();
        let mut stmt = guard
            .conn
            .prepare_cached("SELECT 1 FROM items WHERE id = ?1 AND kind = 'mail'")?;
        let mut out = HashSet::new();
        for id in ids {
            if stmt.exists([id])? {
                out.insert(id.clone());
            }
        }
        Ok(out)
    }

    /// 在本机改邮件的状态，返回它们在服务器上的位置。
    pub(crate) fn set_mail_state_local(
        &self,
        ids: &[String],
        patch: &ItemPatch,
    ) -> Result<Vec<MailRef>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let now = now_ms() as i64;
        let mut guard = self.lock();
        let tx = guard.conn.transaction()?;
        let mut refs = Vec::new();
        for id in ids {
            if let Some(read) = patch.read {
                tx.execute(
                    "UPDATE items SET read = ?2 WHERE id = ?1",
                    params![id, read],
                )?;
            }
            if let Some(starred) = patch.starred {
                tx.execute(
                    "UPDATE items SET starred = ?2 WHERE id = ?1",
                    params![id, starred],
                )?;
            }
            if let Some(archived) = patch.archived {
                // 垃圾邮件和已删除的只能移回收件箱，不能归档
                tx.execute(
                    "UPDATE items SET archived = ?2 WHERE id = ?1 AND (?2 = 0 OR archived < 2)",
                    params![id, archived],
                )?;
            }
            tx.execute(
                "UPDATE mail_meta SET touched_at = ?2 WHERE id = ?1",
                params![id, now],
            )?;
            if let Some(r) = tx.query_row(REF_SQL, [id], ref_from_row).optional()? {
                refs.push(r);
            }
        }
        tx.commit()?;
        Ok(refs)
    }

    pub fn mail_ref(&self, id: &str) -> Result<Option<MailRef>> {
        Ok(self
            .lock()
            .conn
            .query_row(REF_SQL, [id], ref_from_row)
            .optional()?)
    }

    /// 写回服务器之后，记下邮件现在在哪个邮箱、UID 是多少。
    ///
    /// 从垃圾邮件或已删除移回收件箱的，本机这条直接删掉：它在收件箱里是另一个 ID，
    /// 下次同步收件箱时会重新取回来。
    pub fn mail_moved(&self, id: &str, mailbox: &str, uid: Option<u32>) -> Result<()> {
        let guard = self.lock();
        let from_other: bool = guard.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM mail_meta m JOIN mail_folders f \
                 ON f.account_id = m.account_id AND f.name = m.mailbox \
             WHERE m.id = ?1 AND m.mailbox != ?2)",
            params![id, mailbox],
            |r| r.get(0),
        )?;
        if from_other {
            return delete_mail(&guard.conn, id);
        }
        guard.conn.execute(
            "UPDATE mail_meta SET mailbox = ?2, uid = ?3 WHERE id = ?1",
            params![id, mailbox, uid],
        )?;
        Ok(())
    }

    pub fn mail_body(&self, id: &str) -> Result<Option<Vec<u8>>> {
        Ok(self
            .lock()
            .conn
            .query_row("SELECT raw FROM mail_body WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub fn set_mail_body(&self, id: &str, raw: &[u8]) -> Result<()> {
        self.lock().conn.execute(
            "INSERT OR REPLACE INTO mail_body (id, raw) VALUES (?1, ?2)",
            params![id, raw],
        )?;
        Ok(())
    }
}
