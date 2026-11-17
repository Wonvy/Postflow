//! AI 生成的内容：总结、关键词、摘要、对话记录，以及用量。只存在本机，不进数据目录。

use std::collections::HashMap;

use rusqlite::{OptionalExtension, params, params_from_iter};
use serde::Serialize;

use crate::error::Result;
use crate::hlc::now_ms;
use crate::store::Store;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AiResult {
    pub task: String,
    /// 结果用的语言，同一篇可以有中文和英文两份总结。
    pub lang: String,
    pub provider: String,
    pub model: String,
    pub content: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AiMessage {
    pub id: i64,
    /// user 或 assistant。
    pub role: String,
    pub content: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AiUsage {
    pub provider: String,
    pub requests: i64,
    pub input: i64,
    pub output: i64,
}

const DAY_MS: i64 = 86_400_000;

/// 一个分类里有多少条、多少未读（收件箱里的）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LabelCount {
    pub label: String,
    pub unread: i64,
    pub total: i64,
}

/// 从 1970-01-01 起的第几天（UTC）。
pub fn day_of(ms: i64) -> i64 {
    ms.div_euclid(DAY_MS)
}

impl Store {
    pub fn ai_result(&self, item_id: &str, task: &str, lang: &str) -> Result<Option<AiResult>> {
        Ok(self
            .lock()
            .conn
            .query_row(
                "SELECT task, lang, provider, model, content, created_at FROM ai_results \
                 WHERE item_id = ?1 AND task = ?2 AND lang = ?3",
                params![item_id, task, lang],
                |r| {
                    Ok(AiResult {
                        task: r.get(0)?,
                        lang: r.get(1)?,
                        provider: r.get(2)?,
                        model: r.get(3)?,
                        content: r.get(4)?,
                        created_at: r.get(5)?,
                    })
                },
            )
            .optional()?)
    }

    /// 一批条目某个任务的结果，例如列表里看得到的那些条目的摘要。
    pub fn ai_results_for(
        &self,
        ids: &[String],
        task: &str,
        lang: &str,
    ) -> Result<HashMap<String, String>> {
        let mut out = HashMap::new();
        if ids.is_empty() {
            return Ok(out);
        }
        let guard = self.lock();
        for chunk in ids.chunks(400) {
            let marks = (0..chunk.len())
                .map(|i| format!("?{}", i + 3))
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT item_id, content FROM ai_results WHERE task = ?1 AND lang = ?2 AND item_id IN ({marks})"
            );
            let mut args: Vec<String> = vec![task.to_string(), lang.to_string()];
            args.extend(chunk.iter().cloned());
            let mut stmt = guard.conn.prepare(&sql)?;
            let rows = stmt.query_map(params_from_iter(args), |r| Ok((r.get(0)?, r.get(1)?)))?;
            for row in rows {
                let (id, content): (String, String) = row?;
                out.insert(id, content);
            }
        }
        Ok(out)
    }

    pub fn put_ai_result(&self, item_id: &str, result: &AiResult) -> Result<()> {
        self.lock().conn.execute(
            "INSERT OR REPLACE INTO ai_results (item_id, task, lang, provider, model, content, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                item_id,
                result.task,
                result.lang,
                result.provider,
                result.model,
                result.content,
                result.created_at
            ],
        )?;
        Ok(())
    }

    pub fn ai_messages(&self, item_id: &str) -> Result<Vec<AiMessage>> {
        let guard = self.lock();
        let mut stmt = guard.conn.prepare(
            "SELECT id, role, content, created_at FROM ai_messages WHERE item_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map([item_id], |r| {
            Ok(AiMessage {
                id: r.get(0)?,
                role: r.get(1)?,
                content: r.get(2)?,
                created_at: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn add_ai_message(&self, item_id: &str, role: &str, content: &str) -> Result<AiMessage> {
        let guard = self.lock();
        let at = now_ms() as i64;
        guard.conn.execute(
            "INSERT INTO ai_messages (item_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![item_id, role, content, at],
        )?;
        Ok(AiMessage {
            id: guard.conn.last_insert_rowid(),
            role: role.to_string(),
            content: content.to_string(),
            created_at: at,
        })
    }

    pub fn clear_ai_messages(&self, item_id: &str) -> Result<()> {
        self.lock()
            .conn
            .execute("DELETE FROM ai_messages WHERE item_id = ?1", [item_id])?;
        Ok(())
    }

    /// 记一次请求的用量。
    pub fn add_ai_usage(&self, provider: &str, input: u64, output: u64) -> Result<()> {
        self.lock().conn.execute(
            "INSERT INTO ai_usage (day, provider, requests, input, output) VALUES (?1, ?2, 1, ?3, ?4) \
             ON CONFLICT (day, provider) DO UPDATE SET requests = requests + 1, \
                 input = input + excluded.input, output = output + excluded.output",
            params![day_of(now_ms() as i64), provider, input as i64, output as i64],
        )?;
        Ok(())
    }

    /// 从某一天起每个服务的用量合计。
    pub fn ai_usage(&self, since_day: i64) -> Result<Vec<AiUsage>> {
        let guard = self.lock();
        let mut stmt = guard.conn.prepare(
            "SELECT provider, SUM(requests), SUM(input), SUM(output) FROM ai_usage \
             WHERE day >= ?1 GROUP BY provider ORDER BY provider",
        )?;
        let rows = stmt.query_map([since_day], |r| {
            Ok(AiUsage {
                provider: r.get(0)?,
                requests: r.get(1)?,
                input: r.get(2)?,
                output: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// 记下分类。AI 分的不会覆盖用户定的；用户定的总是生效。`None` 表示哪一类都不是。
    pub fn set_labels(&self, labels: &[(String, Option<String>)], by_user: bool) -> Result<()> {
        let mut guard = self.lock();
        let tx = guard.conn.transaction()?;
        {
            let sql = if by_user {
                "INSERT INTO item_labels (item_id, label, by_user, at) VALUES (?1, ?2, 1, ?3) \
                 ON CONFLICT (item_id) DO UPDATE SET label = excluded.label, by_user = 1, at = excluded.at"
            } else {
                "INSERT INTO item_labels (item_id, label, by_user, at) VALUES (?1, ?2, 0, ?3) \
                 ON CONFLICT (item_id) DO UPDATE SET label = excluded.label, at = excluded.at \
                 WHERE item_labels.by_user = 0"
            };
            let mut stmt = tx.prepare(sql)?;
            let at = now_ms() as i64;
            for (id, label) in labels {
                stmt.execute(params![id, label.as_deref().unwrap_or(""), at])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// 收件箱里每个分类的条数和未读数。
    pub fn label_counts(&self) -> Result<Vec<LabelCount>> {
        let guard = self.lock();
        let sql = format!(
            "SELECT l.label, COALESCE(SUM(i.read = 0), 0), COUNT(*) FROM {} \
             WHERE l.label <> '' AND i.archived = 0 AND {} GROUP BY l.label ORDER BY l.label",
            crate::store::ITEM_FROM,
            crate::store::ALIVE
        );
        let mut stmt = guard.conn.prepare(&sql)?;
        let rows = stmt.query_map([], |r| {
            Ok(LabelCount {
                label: r.get(0)?,
                unread: r.get(1)?,
                total: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// 还没分类的条目，新的在前。只看收件箱里 `since` 之后的；`mail` 为假时只要文章。
    pub fn unlabeled(&self, since: i64, mail: bool, limit: u32) -> Result<Vec<crate::ItemSummary>> {
        let guard = self.lock();
        let sql = format!(
            "SELECT {} FROM {} WHERE l.item_id IS NULL AND i.archived = 0 AND {} \
             AND i.published_at >= ?1 AND (?2 OR i.kind = 'article') \
             ORDER BY i.published_at DESC LIMIT ?3",
            crate::store::ITEM_COLUMNS,
            crate::store::ITEM_FROM,
            crate::store::ALIVE
        );
        let mut stmt = guard.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![since, mail, limit], crate::store::item_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}
