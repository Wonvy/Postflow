//! 订阅统计：每个订阅源更新得多勤、内容完不完整、我读了多少。
//!
//! 阅读记录（打开一篇停留了多久、往下看到了哪里）只存在本机的索引里，不进数据目录、不同步。

use std::collections::HashMap;

use rusqlite::{OptionalExtension, params};
use serde::Serialize;

use crate::error::Result;
use crate::store::Store;

const DAY_MS: i64 = 86_400_000;
const WEEK_MS: i64 = 7 * DAY_MS;
/// 算内容完整度时看每个源最近多少篇。
const RECENT_ITEMS: i64 = 30;
/// 正文 HTML 至少这么长才算全文，否则算只给了摘要。
const FULL_HTML_BYTES: i64 = 1500;
/// 更新频率的小图最多画多少周。
const MAX_WEEKS: i64 = 13;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FeedStats {
    pub id: String,
    pub title: String,
    pub url: String,
    pub icon_url: Option<String>,
    pub folder: String,
    pub added_at: i64,
    /// 最近一次抓取出的错。
    pub error: Option<String>,
    /// 存着的文章总数。
    pub total: i64,
    pub last_published: Option<i64>,
    /// 统计期内发布的文章数。
    pub published: i64,
    /// 每周发布几篇，最早的一周在前，最后一周截至现在。
    pub weekly: Vec<i64>,
    /// 最近几篇里带全文的比例。一篇都没有时为空。
    pub full_share: Option<f64>,
    /// 统计期内发布、已经标为已读的（包括没打开就标的）。
    pub read: i64,
    /// 统计期内发布、我打开看过的。
    pub opened: i64,
    /// 其中看到了结尾的。
    pub finished: i64,
    /// 打开过的文章平均往下看到了哪里（0 到 1）。
    pub avg_depth: Option<f64>,
    /// 统计期内在这个源的文章上花的时间。
    pub dwell_ms: i64,
}

/// 某一天（本地时间）：所有订阅源发了几篇，我打开了几篇、读了多久。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReadingDay {
    /// 从 1970-01-01 起的第几天，按调用方给的时区算。
    pub day: i64,
    pub published: i64,
    pub opened: i64,
    pub dwell_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Stats {
    pub since: i64,
    pub now: i64,
    /// 最早的一条阅读记录。之前的阅读没有记录。
    pub tracking_since: Option<i64>,
    pub feeds: Vec<FeedStats>,
    pub days: Vec<ReadingDay>,
}

impl Store {
    /// 记下一次阅读。条目不存在时不记，返回 false。
    pub fn record_reading(
        &self,
        item_id: &str,
        at: i64,
        dwell_ms: i64,
        depth: f64,
        finished: bool,
    ) -> Result<bool> {
        let guard = self.lock();
        let source: Option<String> = guard
            .conn
            .query_row(
                "SELECT source_id FROM items WHERE id = ?1",
                [item_id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(source) = source else {
            return Ok(false);
        };
        let depth = if depth.is_finite() {
            depth.clamp(0.0, 1.0)
        } else {
            0.0
        };
        guard.conn.execute(
            "INSERT INTO reading (item_id, source_id, at, dwell_ms, depth, finished) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![item_id, source, at, dwell_ms.max(0), depth, finished],
        )?;
        Ok(true)
    }

    /// 清掉所有阅读记录。
    pub fn clear_reading(&self) -> Result<()> {
        self.lock().conn.execute("DELETE FROM reading", [])?;
        Ok(())
    }

    /// 最近 `days` 天的订阅统计。`tz_offset_ms` 是本地时间比 UTC 快多少，用来按本地的日子分天。
    pub fn feed_stats(&self, now: i64, days: i64, tz_offset_ms: i64) -> Result<Stats> {
        let days = days.clamp(1, 366);
        let since = now - days * DAY_MS;
        let weeks = ((days + 6) / 7).clamp(1, MAX_WEEKS);
        let guard = self.lock();
        let conn = &guard.conn;

        let mut feeds: Vec<FeedStats> = {
            let mut stmt = conn.prepare(
                "SELECT f.id, f.title, f.url, f.icon_url, f.folder, f.added_at, ff.error \
                 FROM feeds f LEFT JOIN feed_fetch ff ON ff.feed_id = f.id \
                 WHERE f.deleted = 0 AND f.url <> 'postflow:later' ORDER BY f.title COLLATE NOCASE",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(FeedStats {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    url: r.get(2)?,
                    icon_url: r.get(3)?,
                    folder: r.get(4)?,
                    added_at: r.get(5)?,
                    error: r.get(6)?,
                    total: 0,
                    last_published: None,
                    published: 0,
                    weekly: vec![0; weeks as usize],
                    full_share: None,
                    read: 0,
                    opened: 0,
                    finished: 0,
                    avg_depth: None,
                    dwell_ms: 0,
                })
            })?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let index: HashMap<String, usize> = feeds
            .iter()
            .enumerate()
            .map(|(i, f)| (f.id.clone(), i))
            .collect();
        // 对每个查到的 (源, 值) 调一下 `apply`
        let each =
            |sql: &str,
             args: &[&dyn rusqlite::ToSql],
             apply: &mut dyn FnMut(&mut FeedStats, &rusqlite::Row) -> rusqlite::Result<()>,
             feeds: &mut Vec<FeedStats>|
             -> Result<()> {
                let mut stmt = conn.prepare(sql)?;
                let mut rows = stmt.query(args)?;
                while let Some(row) = rows.next()? {
                    let id: String = row.get(0)?;
                    if let Some(&i) = index.get(&id) {
                        apply(&mut feeds[i], row)?;
                    }
                }
                Ok(())
            };

        each(
            "SELECT source_id, COUNT(*), MAX(published_at) FROM items \
             WHERE kind = 'article' GROUP BY source_id",
            &[],
            &mut |f, r| {
                f.total = r.get(1)?;
                f.last_published = r.get(2)?;
                Ok(())
            },
            &mut feeds,
        )?;
        each(
            "SELECT source_id, COUNT(*), SUM(read) FROM items \
             WHERE kind = 'article' AND published_at >= ?1 GROUP BY source_id",
            &[&since],
            &mut |f, r| {
                f.published = r.get(1)?;
                f.read = r.get(2)?;
                Ok(())
            },
            &mut feeds,
        )?;
        each(
            "SELECT source_id, (?1 - published_at) / ?2, COUNT(*) FROM items \
             WHERE kind = 'article' AND published_at > ?1 - ?2 * ?3 AND published_at <= ?1 \
             GROUP BY source_id, 2",
            &[&now, &WEEK_MS, &weeks],
            &mut |f, r| {
                let ago: i64 = r.get(1)?;
                let slot = weeks - 1 - ago;
                if (0..weeks).contains(&slot) {
                    f.weekly[slot as usize] = r.get(2)?;
                }
                Ok(())
            },
            &mut feeds,
        )?;
        each(
            "SELECT t.source_id, AVG(LENGTH(c.html) >= ?2) FROM ( \
               SELECT id, source_id, ROW_NUMBER() OVER ( \
                 PARTITION BY source_id ORDER BY published_at DESC) AS n \
               FROM items WHERE kind = 'article') t \
             JOIN item_content c ON c.id = t.id WHERE t.n <= ?1 GROUP BY t.source_id",
            &[&RECENT_ITEMS, &FULL_HTML_BYTES],
            &mut |f, r| {
                f.full_share = r.get(1)?;
                Ok(())
            },
            &mut feeds,
        )?;
        each(
            "SELECT source_id, COUNT(*), SUM(done), AVG(depth) FROM ( \
               SELECT i.source_id, r.item_id, MAX(r.finished) AS done, MAX(r.depth) AS depth \
               FROM reading r JOIN items i ON i.id = r.item_id \
               WHERE i.published_at >= ?1 GROUP BY r.item_id) \
             GROUP BY source_id",
            &[&since],
            &mut |f, r| {
                f.opened = r.get(1)?;
                f.finished = r.get(2)?;
                f.avg_depth = r.get(3)?;
                Ok(())
            },
            &mut feeds,
        )?;
        each(
            "SELECT source_id, SUM(dwell_ms) FROM reading WHERE at >= ?1 GROUP BY source_id",
            &[&since],
            &mut |f, r| {
                f.dwell_ms = r.get(1)?;
                Ok(())
            },
            &mut feeds,
        )?;

        // 按天：先把统计期里的每一天都列出来，再填数
        let first = (since + tz_offset_ms).div_euclid(DAY_MS);
        let last = (now + tz_offset_ms).div_euclid(DAY_MS);
        let mut days_out: Vec<ReadingDay> = (first..=last)
            .map(|day| ReadingDay {
                day,
                published: 0,
                opened: 0,
                dwell_ms: 0,
            })
            .collect();
        let mut fill = |sql: &str, apply: &dyn Fn(&mut ReadingDay, i64, i64)| -> Result<()> {
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt.query_map(params![since, tz_offset_ms, DAY_MS, now], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?;
            for row in rows {
                let (day, a, b) = row?;
                if let Some(slot) = usize::try_from(day - first)
                    .ok()
                    .and_then(|i| days_out.get_mut(i))
                {
                    apply(slot, a, b);
                }
            }
            Ok(())
        };
        fill(
            "SELECT (published_at + ?2) / ?3, COUNT(*), 0 FROM items i \
             WHERE kind = 'article' AND published_at >= ?1 AND published_at <= ?4 \
               AND EXISTS (SELECT 1 FROM feeds f WHERE f.id = i.source_id AND f.deleted = 0) \
             GROUP BY 1",
            &|d, n, _| d.published = n,
        )?;
        fill(
            "SELECT (at + ?2) / ?3, COUNT(DISTINCT item_id), SUM(dwell_ms) FROM reading r \
             WHERE at >= ?1 AND at <= ?4 \
               AND EXISTS (SELECT 1 FROM feeds f WHERE f.id = r.source_id) \
             GROUP BY 1",
            &|d, n, ms| {
                d.opened = n;
                d.dwell_ms = ms;
            },
        )?;

        let tracking_since: Option<i64> =
            conn.query_row("SELECT MIN(at) FROM reading", [], |r| r.get(0))?;
        drop(guard);
        Ok(Stats {
            since,
            now,
            tracking_since,
            feeds,
            days: days_out,
        })
    }
}
