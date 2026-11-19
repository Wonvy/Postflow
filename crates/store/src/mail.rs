//! 邮箱账户和邮件的存储。
//!
//! 账户设置（不含密码）写进日志，同步给其他设备；邮件本身每台设备各自从邮件服务器同步，
//! 只存在本机索引里。已读、星标、归档以服务器为准，本机改了以后再写回服务器。

use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde_json::{Map, Value, json};

use crate::error::{Error, Result};
use crate::hlc::now_ms;
use crate::ids;
use crate::model::*;
use crate::store::{Store, lww_fields};

/// 同步时只看收件箱。
pub const INBOX: &str = "INBOX";

// 未读数不算正稍后提醒着的
const ACCOUNT_SQL: &str = concat!(
    "SELECT a.id, a.email, a.name, a.imap_host, a.imap_port, a.imap_security, \
            a.username, a.added_at, \
            (SELECT COUNT(*) FROM items i \
              WHERE i.kind = 'mail' AND i.source_id = a.id AND i.read = 0 AND i.archived = 0 AND NOT ",
    crate::snoozed_now_sql!(),
    " AND NOT ",
    crate::newsletter_sql!(),
    "), \
            s.last_sync_at, s.error, s.error_code, a.smtp_host, a.smtp_port, a.smtp_security, \
            a.sender_name, a.signature \
     FROM accounts a LEFT JOIN mail_state s ON s.account_id = a.id"
);

fn account_from_row(r: &Row<'_>) -> rusqlite::Result<Account> {
    Ok(Account {
        id: r.get(0)?,
        email: r.get(1)?,
        name: r.get(2)?,
        imap_host: r.get(3)?,
        imap_port: r.get(4)?,
        imap_security: r.get(5)?,
        username: r.get(6)?,
        smtp_host: r.get(12)?,
        smtp_port: r.get(13)?,
        smtp_security: r.get(14)?,
        sender_name: r.get(15)?,
        signature: r.get(16)?,
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
    // 发信服务器没填就是自动，端口和加密方式跟着它
    let smtp_host = text("smtp_host").filter(|h| !h.trim().is_empty());
    conn.execute(
        "INSERT OR REPLACE INTO accounts \
         (id, email, name, imap_host, imap_port, imap_security, username, deleted, added_at, position, \
          smtp_host, smtp_port, smtp_security, sender_name, signature) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
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
            smtp_host.as_deref(),
            smtp_host
                .as_ref()
                .and_then(|_| fields.get("smtp_port").and_then(Value::as_i64)),
            smtp_host.as_ref().and_then(|_| text("smtp_security")),
            text("sender_name").unwrap_or_default(),
            text("signature").unwrap_or_default(),
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
    conn.execute(
        "DELETE FROM mail_mailboxes WHERE account_id = ?1",
        [account_id],
    )?;
    Ok(())
}

/// 自动发的信用的地址：不回信，也不算联系人。
fn automated(address: &str) -> bool {
    let local = address.split('@').next().unwrap_or("").to_ascii_lowercase();
    [
        "noreply",
        "no-reply",
        "no_reply",
        "donotreply",
        "do-not-reply",
        "mailer-daemon",
        "postmaster",
        "notifications",
        "notification",
        "bounce",
    ]
    .iter()
    .any(|bad| {
        local == *bad
            || local.starts_with(&format!("{bad}+"))
            || local.starts_with(&format!("{bad}-"))
            || local.ends_with(&format!("-{bad}"))
            || local.ends_with(&format!(".{bad}"))
    })
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
        if let Some(host) = cfg
            .smtp_host
            .as_deref()
            .map(str::trim)
            .filter(|h| !h.is_empty())
        {
            set.insert("smtp_host".into(), json!(host));
            set.insert("smtp_port".into(), json!(cfg.smtp_port.unwrap_or(465)));
            set.insert(
                "smtp_security".into(),
                json!(cfg.smtp_security.as_deref().unwrap_or("tls")),
            );
        }
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
        if let Some(host) = patch.smtp_host {
            set.insert("smtp_host".into(), json!(host.trim()));
        }
        if let Some(port) = patch.smtp_port {
            set.insert("smtp_port".into(), json!(port));
        }
        if let Some(security) = patch.smtp_security {
            set.insert("smtp_security".into(), json!(security));
        }
        if let Some(name) = patch.sender_name {
            set.insert("sender_name".into(), json!(name.trim()));
        }
        if let Some(signature) = patch.signature {
            // 只去掉末尾的空白，开头的空行和缩进是用户自己排的
            set.insert("signature".into(), json!(signature.trim_end()));
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
    /// 用户自己的文件夹按名字（`mailbox`）找，其他的按用途找。
    pub fn mail_folder(
        &self,
        account_id: &str,
        folder: MailFolder,
        mailbox: &str,
    ) -> Result<Option<(String, u32)>> {
        if folder == MailFolder::Inbox {
            return Ok(self
                .mail_uidvalidity(account_id)?
                .map(|v| (INBOX.to_string(), v)));
        }
        if folder == MailFolder::Other {
            return Ok(self
                .lock()
                .conn
                .query_row(
                    "SELECT name, uidvalidity FROM mail_mailboxes WHERE account_id = ?1 AND name = ?2",
                    params![account_id, mailbox],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?);
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
    /// 收件箱里本机归档、还没写回服务器的那些保留。`mailbox` 是收件箱和用户自己的文件夹的名字，
    /// 只清这一个文件夹里的。
    pub fn reset_mail_folder(
        &self,
        account_id: &str,
        folder: MailFolder,
        mailbox: &str,
    ) -> Result<()> {
        let guard = self.lock();
        let by_name = matches!(folder, MailFolder::Inbox | MailFolder::Other);
        let ids: Vec<String> = {
            let mut stmt = guard.conn.prepare(
                "SELECT m.id FROM mail_meta m JOIN items i ON i.id = m.id \
                 WHERE m.account_id = ?1 AND i.archived = ?2 \
                   AND (?4 = 0 OR m.mailbox = ?3)",
            )?;
            let rows = stmt
                .query_map(params![account_id, folder.code(), mailbox, by_name], |r| {
                    r.get(0)
                })?;
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
        } else if folder == MailFolder::Other {
            tx.execute(
                "INSERT OR REPLACE INTO mail_mailboxes (account_id, name, uidvalidity) \
                 VALUES (?1, ?2, ?3)",
                params![account_id, mailbox, sync.uidvalidity],
            )?;
        } else {
            tx.execute(
                "INSERT OR REPLACE INTO mail_folders (account_id, folder, name, uidvalidity) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![account_id, folder.code(), mailbox, sync.uidvalidity],
            )?;
        }

        for mail in sync.new {
            let known: bool = tx
                .query_row("SELECT 1 FROM items WHERE id = ?1", [&mail.id], |_| Ok(()))
                .optional()?
                .is_some();
            if !known && folder == MailFolder::Inbox && !mail.read {
                stats.fresh.push(FreshMail {
                    id: mail.id.clone(),
                    from: mail
                        .from_name
                        .clone()
                        .or_else(|| mail.from_address.clone())
                        .unwrap_or_default(),
                    address: mail.from_address.as_ref().map(|a| a.trim().to_lowercase()),
                    subject: mail.subject.clone(),
                    date: mail.date,
                    code: mail.code.clone(),
                });
            }
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
                  has_attachments, list_unsubscribe, size, recipients) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12) \
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
                    (!mail.recipients.is_empty())
                        .then(|| serde_json::to_string(&mail.recipients).unwrap_or_default()),
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

    /// 草稿和已发送的信：不能归档或移回收件箱。
    pub(crate) fn pinned_ids_among(&self, ids: &HashSet<String>) -> Result<HashSet<String>> {
        let guard = self.lock();
        let mut stmt = guard.conn.prepare_cached(
            "SELECT 1 FROM items WHERE id = ?1 AND kind = 'mail' AND archived IN (4, 5)",
        )?;
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

    /// 给你来过信的人和你写过信的人，来往多的在前。写过信的分量重一些。
    /// 电子报、通知一类自动发的信不算，垃圾邮件、已删除和草稿里的也不算。
    pub fn mail_contacts(&self, limit: usize) -> Result<Vec<Contact>> {
        /// 写过一封信抵得上收到几封。
        const SENT_WEIGHT: i64 = 3;
        let guard = self.lock();
        // 地址（小写）→ 名字、显示的地址、分数
        let mut people: HashMap<String, (Option<String>, String, i64)> = HashMap::new();
        let mut add = |name: Option<String>, address: String, n: i64| {
            if !address.contains('@') || automated(&address) {
                return;
            }
            let name = name.filter(|n| !n.trim().is_empty());
            let entry = people
                .entry(address.to_lowercase())
                .or_insert_with(|| (None, address.clone(), 0));
            if entry.0.is_none() {
                entry.0 = name;
            }
            entry.2 += n;
        };
        let mut stmt = guard.conn.prepare(
            "SELECT MAX(from_name), MIN(from_address), COUNT(*) FROM mail_meta \
             WHERE from_address IS NOT NULL AND list_unsubscribe IS NULL \
               AND id IN (SELECT id FROM items WHERE kind = 'mail' AND archived IN (0, 1, 6)) \
               AND account_id IN (SELECT id FROM accounts WHERE deleted = 0) \
             GROUP BY lower(from_address)",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        for row in rows {
            let (name, address, n) = row?;
            add(name, address, n);
        }
        let mut stmt = guard.conn.prepare(
            "SELECT recipients FROM mail_meta \
             WHERE recipients IS NOT NULL \
               AND id IN (SELECT id FROM items WHERE kind = 'mail' AND archived = 5) \
               AND account_id IN (SELECT id FROM accounts WHERE deleted = 0)",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        for row in rows {
            let list: Vec<(Option<String>, String)> =
                serde_json::from_str(&row?).unwrap_or_default();
            for (name, address) in list {
                add(name, address, SENT_WEIGHT);
            }
        }
        let mut out: Vec<Contact> = people
            .into_values()
            .map(|(name, address, count)| Contact {
                name,
                address,
                count,
            })
            .collect();
        out.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| a.address.to_lowercase().cmp(&b.address.to_lowercase()))
        });
        out.truncate(limit);
        Ok(out)
    }

    /// 新加或者整段改写一段划线。
    pub fn put_highlight(&self, h: &Highlight) -> Result<()> {
        let mut set = Map::new();
        set.insert("item".into(), json!(h.item));
        set.insert("quote".into(), json!(h.quote));
        set.insert("prefix".into(), json!(h.prefix));
        set.insert("suffix".into(), json!(h.suffix));
        set.insert("note".into(), json!(h.note));
        set.insert("created_at".into(), json!(h.created_at));
        set.insert("deleted".into(), json!(false));
        self.set_fields("highlight", std::slice::from_ref(&h.id), set)
    }

    pub fn set_highlight_note(&self, id: &str, note: &str) -> Result<()> {
        let mut set = Map::new();
        set.insert("note".into(), json!(note));
        self.set_fields("highlight", &[id.to_string()], set)
    }

    pub fn remove_highlight(&self, id: &str) -> Result<()> {
        let mut set = Map::new();
        set.insert("deleted".into(), json!(true));
        self.set_fields("highlight", &[id.to_string()], set)
    }

    /// 一篇文章的划线（`item` 为 `None` 时是全部），按加上的先后。
    pub fn highlights(&self, item: Option<&str>) -> Result<Vec<Highlight>> {
        let guard = self.lock();
        let ids: Vec<String> = match item {
            Some(item) => {
                let mut stmt = guard.conn.prepare_cached(
                    "SELECT id FROM lww WHERE kind = 'highlight' AND field = 'item' AND value = ?1",
                )?;
                stmt.query_map([json!(item).to_string()], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?
            }
            None => {
                let mut stmt = guard.conn.prepare_cached(
                    "SELECT id FROM lww WHERE kind = 'highlight' AND field = 'item'",
                )?;
                stmt.query_map([], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?
            }
        };
        let mut out = Vec::new();
        for id in ids {
            let f = lww_fields(&guard.conn, "highlight", &id)?;
            if f.get("deleted").and_then(Value::as_bool) == Some(true) {
                continue;
            }
            let text = |k: &str| {
                f.get(k)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            out.push(Highlight {
                id: id.clone(),
                item: text("item"),
                quote: text("quote"),
                prefix: text("prefix"),
                suffix: text("suffix"),
                note: text("note"),
                created_at: f
                    .get("created_at")
                    .and_then(Value::as_i64)
                    .unwrap_or_default(),
            });
        }
        out.sort_by_key(|h| (h.created_at, h.id.clone()));
        Ok(out)
    }

    /// 全部划线，带着文章的标题、地址和来源，导出笔记用。按文章分在一起，新的文章在前。
    pub fn all_highlights(&self) -> Result<Vec<HighlightWithItem>> {
        let list = self.highlights(None)?;
        let mut out = Vec::new();
        for h in list {
            let Some(detail) = self.item(&h.item)? else {
                continue;
            };
            out.push(HighlightWithItem {
                title: detail.item.title,
                url: detail.item.url,
                source_title: detail.item.source_title,
                highlight: h,
            });
        }
        out.sort_by(|a, b| {
            b.highlight
                .created_at
                .cmp(&a.highlight.created_at)
                .then_with(|| a.highlight.item.cmp(&b.highlight.item))
        });
        Ok(out)
    }

    /// 对一封会议邀请的回复（ACCEPTED、TENTATIVE、DECLINED），记在同步日志里，别的设备也看得到。
    pub fn set_rsvp(&self, item_id: &str, status: &str) -> Result<()> {
        let mut set = Map::new();
        set.insert("status".into(), json!(status));
        self.set_fields("rsvp", &[item_id.to_string()], set)
    }

    pub fn rsvp(&self, item_id: &str) -> Result<Option<String>> {
        let guard = self.lock();
        let f = lww_fields(&guard.conn, "rsvp", item_id)?;
        Ok(f.get("status").and_then(Value::as_str).map(str::to_string))
    }

    /// 记下「没回复就提醒」。
    pub fn set_follow_up(&self, f: &FollowUp) -> Result<()> {
        let mut set = Map::new();
        set.insert("account".into(), json!(f.account_id));
        set.insert("subject".into(), json!(f.subject));
        set.insert("to".into(), json!(f.to));
        set.insert("sent_at".into(), json!(f.sent_at));
        set.insert("until".into(), json!(f.until));
        self.set_fields("followup", std::slice::from_ref(&f.message_id), set)
    }

    /// 这封信（按 Message-ID）的「没回复就提醒」。取消了的（时间是 0）算没有。
    pub fn follow_up(&self, message_id: &str) -> Result<Option<FollowUp>> {
        let guard = self.lock();
        let f = lww_fields(&guard.conn, "followup", message_id)?;
        let until = f.get("until").and_then(Value::as_i64).unwrap_or_default();
        if until <= 0 {
            return Ok(None);
        }
        let text = |k: &str| {
            f.get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        Ok(Some(FollowUp {
            message_id: message_id.to_string(),
            account_id: text("account"),
            subject: text("subject"),
            to: f
                .get("to")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            sent_at: f.get("sent_at").and_then(Value::as_i64).unwrap_or_default(),
            until,
        }))
    }

    /// 取消「没回复就提醒」。
    pub fn cancel_follow_up(&self, message_id: &str) -> Result<()> {
        let mut set = Map::new();
        set.insert("until".into(), json!(0));
        self.set_fields("followup", &[message_id.to_string()], set)
    }

    /// 提醒时间在 (`after`, `until`] 之间的「没回复就提醒」。
    pub fn due_follow_ups(&self, after: i64, until: i64) -> Result<Vec<FollowUp>> {
        let guard = self.lock();
        let mut stmt = guard.conn.prepare(
            "SELECT id FROM lww WHERE kind = 'followup' AND field = 'until' \
               AND CAST(value AS INTEGER) > ?1 AND CAST(value AS INTEGER) <= ?2",
        )?;
        let ids = stmt
            .query_map(params![after, until], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut out = Vec::new();
        for id in ids {
            let f = lww_fields(&guard.conn, "followup", &id)?;
            let text = |k: &str| {
                f.get(k)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            let num = |k: &str| f.get(k).and_then(Value::as_i64).unwrap_or_default();
            out.push(FollowUp {
                message_id: id.clone(),
                account_id: text("account"),
                subject: text("subject"),
                to: f
                    .get("to")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(str::to_lowercase))
                            .collect()
                    })
                    .unwrap_or_default(),
                sent_at: num("sent_at"),
                until: num("until"),
            });
        }
        Ok(out)
    }

    /// 这些地址在 `since` 之后发来的信的标题（不算垃圾邮件和已删除）。判断有没有回复用。
    pub fn titles_from_since(&self, addresses: &[String], since: i64) -> Result<Vec<String>> {
        if addresses.is_empty() {
            return Ok(Vec::new());
        }
        let guard = self.lock();
        let marks = vec!["?"; addresses.len()].join(", ");
        let sql = format!(
            "SELECT i.title FROM items i JOIN mail_meta m ON m.id = i.id \
             WHERE i.kind = 'mail' AND i.published_at > ?1 AND i.archived NOT IN (2, 3, 4, 5) \
               AND lower(m.from_address) IN ({marks})"
        );
        let mut args: Vec<rusqlite::types::Value> = vec![since.into()];
        args.extend(addresses.iter().map(|a| a.to_lowercase().into()));
        let mut stmt = guard.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args), |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// 发件人卡片：和这个地址的往来。垃圾邮件和已删除的不算。
    pub fn sender_profile(&self, address: &str) -> Result<SenderProfile> {
        use crate::store::{ALIVE, ITEM_COLUMNS, ITEM_FROM, item_from_row};
        let address = address.trim().to_lowercase();
        let guard = self.lock();
        let (received, unread, first_at, last_at, name): (i64, i64, Option<i64>, Option<i64>, Option<String>) =
            guard.conn.query_row(
                &format!(
                    "SELECT COUNT(*), COALESCE(SUM(i.read = 0), 0), MIN(i.published_at), MAX(i.published_at), \
                            (SELECT m2.from_name FROM mail_meta m2 JOIN items i2 ON i2.id = m2.id \
                              WHERE lower(m2.from_address) = ?1 AND COALESCE(m2.from_name, '') <> '' \
                              ORDER BY i2.published_at DESC LIMIT 1) \
                     FROM {ITEM_FROM} WHERE i.kind = 'mail' AND lower(m.from_address) = ?1 \
                       AND i.archived NOT IN (2, 3, 5) AND {ALIVE}"
                ),
                [&address],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )?;
        let sent: i64 = guard.conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM {ITEM_FROM} WHERE i.kind = 'mail' AND i.archived = 5 AND {ALIVE} \
                   AND m.id IN (SELECT id FROM mail_meta WHERE instr(lower(recipients), ?1) > 0)"
            ),
            [format!("\"{}\"", address.replace('"', ""))],
            |r| r.get(0),
        )?;
        let mut stmt = guard.conn.prepare(&format!(
            "SELECT {ITEM_COLUMNS} FROM {ITEM_FROM} WHERE i.kind = 'mail' AND lower(m.from_address) = ?1 \
               AND i.archived NOT IN (2, 3, 5) AND {ALIVE} \
             ORDER BY i.published_at DESC, i.id DESC LIMIT 6"
        ))?;
        let recent = stmt
            .query_map([&address], item_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(SenderProfile {
            address,
            name,
            received,
            unread,
            sent,
            first_at,
            last_at,
            recent,
        })
    }

    /// 邮件要移到别的文件夹：先从本机拿掉。移成以后，目标文件夹同步时会以那边的身份带回来；
    /// 没移成的，下次同步原来的文件夹时也会回来。返回它们在服务器上的位置。
    pub fn take_mail_for_move(&self, ids: &[String]) -> Result<Vec<MailRef>> {
        let guard = self.lock();
        let mut refs = Vec::new();
        for id in ids {
            if let Some(r) = guard
                .conn
                .query_row(REF_SQL, [id], ref_from_row)
                .optional()?
            {
                refs.push(r);
                delete_mail(&guard.conn, id)?;
            }
        }
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
            "SELECT EXISTS (SELECT 1 FROM mail_meta m WHERE m.id = ?1 AND m.mailbox != ?2 AND ( \
                 EXISTS (SELECT 1 FROM mail_folders f \
                         WHERE f.account_id = m.account_id AND f.name = m.mailbox) \
                 OR EXISTS (SELECT 1 FROM mail_mailboxes b \
                         WHERE b.account_id = m.account_id AND b.name = m.mailbox)))",
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

    /// 列表里解码失败（带替换字符）的邮件，用本机存着的原文重新算标题、摘要和发件人。
    /// `summarize` 给出这三样。每份索引只做一次，返回改了几封。
    ///
    /// 0.1.8 之前不认识 GBK、Big5 这些字符集，那时同步下来的中文邮件在列表里是乱码。
    pub fn repair_mail_text(
        &self,
        summarize: impl Fn(&[u8]) -> (String, String, Option<String>),
    ) -> Result<usize> {
        const KEY: &str = "mail_text_repaired";
        let mut guard = self.lock();
        let tx = guard.conn.transaction()?;
        let done: Option<String> = tx
            .query_row("SELECT value FROM meta WHERE key = ?1", [KEY], |r| r.get(0))
            .optional()?;
        if done.is_some() {
            return Ok(0);
        }
        let garbled: Vec<(String, Vec<u8>)> = {
            let mut stmt = tx.prepare(
                "SELECT i.id, b.raw FROM items i JOIN mail_body b ON b.id = i.id \
                 WHERE i.kind = 'mail' AND (instr(i.title, ?1) > 0 OR instr(i.summary, ?1) > 0 \
                     OR instr(COALESCE(i.author, ''), ?1) > 0)",
            )?;
            stmt.query_map(["\u{fffd}"], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?
        };
        for (id, raw) in &garbled {
            let (subject, snippet, from_name) = summarize(raw);
            tx.execute(
                "UPDATE items SET title = ?2, summary = ?3, \
                     author = COALESCE(?4, (SELECT from_address FROM mail_meta WHERE id = ?1), author) \
                 WHERE id = ?1",
                params![id, subject, snippet, from_name],
            )?;
            tx.execute(
                "UPDATE mail_meta SET from_name = ?2 WHERE id = ?1",
                params![id, from_name],
            )?;
        }
        // 之前离线翻译的列表译文也作废了，换了缓存名，旧的删掉
        tx.execute(
            "DELETE FROM ai_results WHERE task = 'list_translation_offline'",
            [],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, '1')",
            [KEY],
        )?;
        tx.commit()?;
        Ok(garbled.len())
    }
}
