//! 自动规则要看的条目：标题、作者、来源、发件人、正文（邮件是开头的摘要）和当前状态。
//! 规则本身和匹配在核心里（crates/core/src/rules.rs）。

use rusqlite::params;
use serde::Serialize;

use crate::error::Result;
use crate::store::Store;

#[derive(Debug, Clone, Serialize)]
pub struct RuleItem {
    pub id: String,
    pub kind: String,
    pub source_id: String,
    /// 订阅源名或邮箱名。
    pub source: String,
    pub title: String,
    pub author: String,
    pub from_address: String,
    /// 收件人和抄送，逗号隔开。
    pub to: String,
    /// 文章是摘要加正文的 HTML，邮件是开头的摘要。
    pub body: String,
    pub fetched_at: i64,
}

impl Store {
    /// 抓到时间晚于 `since` 的条目，旧的在前；`since` 为 None 时是最新的 `limit` 条。草稿和已发送的不算。
    pub fn rule_items(&self, since: Option<i64>, limit: usize) -> Result<Vec<RuleItem>> {
        let inner = self.lock();
        let sql = format!(
            "SELECT i.id, i.kind, i.source_id, \
                 CASE WHEN i.kind = 'mail' THEN COALESCE(NULLIF(a.name, ''), a.email, '') ELSE COALESCE(f.title, '') END, \
                 i.title, COALESCE(i.author, ''), COALESCE(m.from_address, ''), COALESCE(m.recipients, ''), \
                 i.summary || ' ' || COALESCE(c.html, ''), i.fetched_at \
             FROM items i \
             LEFT JOIN feeds f ON i.kind = 'article' AND f.id = i.source_id \
             LEFT JOIN accounts a ON i.kind = 'mail' AND a.id = i.source_id \
             LEFT JOIN mail_meta m ON i.kind = 'mail' AND m.id = i.id \
             LEFT JOIN item_content c ON c.id = i.id \
             WHERE i.archived NOT IN (4, 5) {} \
             ORDER BY i.fetched_at {} LIMIT ?2",
            if since.is_some() {
                "AND i.fetched_at > ?1"
            } else {
                "AND ?1 IS NULL"
            },
            if since.is_some() { "ASC" } else { "DESC" },
        );
        let mut stmt = inner.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![since, limit as i64], |r| {
                Ok(RuleItem {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    source_id: r.get(2)?,
                    source: r.get(3)?,
                    title: r.get(4)?,
                    author: r.get(5)?,
                    from_address: r.get(6)?,
                    to: r.get(7)?,
                    body: r.get(8)?,
                    fetched_at: r.get(9)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}
