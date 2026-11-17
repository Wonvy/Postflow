//! 存储空间：本机索引里各部分占了多少，哪些订阅源、邮箱占得最多，以及清理本机缓存。
//!
//! 能清的只有本机缓存：抓取的全文、从服务器下载的邮件原文。清掉后用到时会重新下载。
//! 数据目录里的内容是同步的事实来源，这里不动。

use rusqlite::params;
use serde::Serialize;

use crate::error::Result;
use crate::store::Store;

/// 一个订阅源或邮箱在本机索引里占的空间。
#[derive(Debug, Clone, Serialize)]
pub struct SourceUsage {
    pub source_id: String,
    pub kind: String,
    pub items: i64,
    pub bytes: i64,
}

/// 本机索引里几类内容各占多少（按内容长度估算，不含索引结构本身）。
#[derive(Debug, Clone, Default, Serialize)]
pub struct IndexUsage {
    /// 订阅文章的正文。
    pub articles: i64,
    /// 抓取的全文缓存。
    pub fulltext: i64,
    /// 邮件原文缓存（含附件）。
    pub mail: i64,
    /// 能清掉的邮件原文：早于 `keep_since`、没加星标、不是草稿。
    pub mail_clearable: i64,
    pub sources: Vec<SourceUsage>,
}

/// 清缓存时保留的条件：星标邮件和草稿（archived = 4）一直留着。
const MAIL_CLEARABLE: &str = "SELECT b.id FROM mail_body b JOIN items i ON i.id = b.id \
     WHERE i.published_at < ?1 AND i.starred = 0 AND i.archived <> 4";

impl Store {
    /// `keep_since`：这之后的邮件原文不算在可清理里（毫秒）。
    pub fn index_usage(&self, keep_since: i64) -> Result<IndexUsage> {
        let inner = self.lock();
        let conn = &inner.conn;
        let sum = |sql: &str| -> Result<i64> {
            Ok(conn
                .query_row(sql, [], |r| r.get::<_, Option<i64>>(0))?
                .unwrap_or(0))
        };
        let articles = sum("SELECT SUM(LENGTH(CAST(html AS BLOB))) FROM item_content")?;
        let fulltext = sum("SELECT SUM(LENGTH(CAST(html AS BLOB))) FROM item_fulltext")?;
        let mail = sum("SELECT SUM(LENGTH(raw)) FROM mail_body")?;
        let mail_clearable = conn
            .query_row(
                &format!("SELECT SUM(LENGTH(raw)) FROM mail_body WHERE id IN ({MAIL_CLEARABLE})"),
                [keep_since],
                |r| r.get::<_, Option<i64>>(0),
            )?
            .unwrap_or(0);

        let mut stmt = conn.prepare(
            "SELECT i.source_id, i.kind, COUNT(*), \
                 SUM(LENGTH(CAST(i.title AS BLOB)) + LENGTH(CAST(i.summary AS BLOB)) \
                     + COALESCE(LENGTH(CAST(c.html AS BLOB)), 0) \
                     + COALESCE(LENGTH(CAST(t.html AS BLOB)), 0) \
                     + COALESCE(LENGTH(b.raw), 0)) AS bytes \
             FROM items i \
             LEFT JOIN item_content c ON c.id = i.id \
             LEFT JOIN item_fulltext t ON t.id = i.id \
             LEFT JOIN mail_body b ON b.id = i.id \
             GROUP BY i.source_id, i.kind \
             ORDER BY bytes DESC",
        )?;
        let sources = stmt
            .query_map([], |r| {
                Ok(SourceUsage {
                    source_id: r.get(0)?,
                    kind: r.get(1)?,
                    items: r.get(2)?,
                    bytes: r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(IndexUsage {
            articles,
            fulltext,
            mail,
            mail_clearable,
            sources,
        })
    }

    /// 清掉抓取的全文缓存，返回清了几篇。
    pub fn clear_full_text(&self) -> Result<usize> {
        Ok(self.lock().conn.execute("DELETE FROM item_fulltext", [])?)
    }

    /// 清掉 `before` 之前的邮件原文缓存（星标和草稿除外），返回清了几封。
    pub fn clear_mail_bodies(&self, before: i64) -> Result<usize> {
        Ok(self.lock().conn.execute(
            &format!("DELETE FROM mail_body WHERE id IN ({MAIL_CLEARABLE})"),
            params![before],
        )?)
    }

    /// 把删掉内容后空出来的地方还给磁盘，并把预写日志并回数据库。
    pub fn compact(&self) -> Result<()> {
        let inner = self.lock();
        inner
            .conn
            .execute_batch("VACUUM; PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::Store;

    #[test]
    fn empty_index_reports_zero_and_compacts() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(
            &dir.path().join("data"),
            &dir.path().join("index.db"),
            "dev1",
        )
        .unwrap();
        let usage = store.index_usage(0).unwrap();
        assert_eq!(usage.articles, 0);
        assert_eq!(usage.mail, 0);
        assert!(usage.sources.is_empty());
        assert_eq!(store.clear_full_text().unwrap(), 0);
        assert_eq!(store.clear_mail_bodies(i64::MAX).unwrap(), 0);
        store.compact().unwrap();
    }
}
