//! 稍后提醒到时间了：标为未读（邮件写回服务器），通知界面弹提醒。
//!
//! 提醒时间本身存在同步日志里（见 store 的 `set_snooze`）。每台设备在本机目录的
//! `reminders-state.json` 里记着处理到了哪个时刻，所以应用关着时到期的，下次打开会补上，
//! 也不会每次启动都把同一条再标一遍未读。第一次运行从现在开始算，不翻旧账。

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use shibapost_store::ItemPatch;

use crate::refresh::now;
use crate::{Core, Event, Scope, Session};

const STATE_FILE: &str = "reminders-state.json";
/// 提醒里最多列几条，其余只给个数。
const SHOWN: usize = 5;

#[derive(Serialize, Deserialize)]
struct State {
    /// 处理到的时刻（毫秒）。
    up_to: i64,
}

/// 提醒里的一条。
#[derive(Debug, Clone, Serialize)]
pub struct Reminder {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub source_title: String,
}

fn read(path: &Path) -> Option<i64> {
    let state: State = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    Some(state.up_to)
}

fn write(path: &Path, up_to: i64) {
    if let Ok(bytes) = serde_json::to_vec(&State { up_to })
        && let Err(err) = std::fs::write(path, bytes)
    {
        tracing::warn!("reminders: could not save state: {err}");
    }
}

/// 讨论的主题：去掉回复、转发的前缀（各种语言，可能好几层），忽略大小写和多余空白。
/// 和界面的 grouping.ts 里 `threadSubject` 一样。
pub(crate) fn thread_subject(title: &str) -> String {
    static PREFIX: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = PREFIX.get_or_init(|| {
        regex::Regex::new(
            r"(?i)^\s*(re|fw|fwd|aw|sv|antw|tr|rif|回复|答复|回覆|转发|轉寄|转寄)\s*(\[\d+\]|\(\d+\))?\s*[:：]\s*",
        )
        .expect("valid regex")
    });
    let mut subject = title.trim().to_string();
    loop {
        let next = re.replace(&subject, "").into_owned();
        if next == subject {
            break;
        }
        subject = next;
    }
    subject
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

impl Core {
    /// 「没回复就提醒」到时间的：收件人里有人在发信之后回了同一个主题的信就算回过了，不提醒；
    /// 没回的，把「已发送」里那封标成未读，它就出现在收件箱顶上的「提醒」里。
    async fn due_follow_ups(
        self: &Arc<Self>,
        session: &Arc<Session>,
        after: i64,
        now: i64,
    ) -> Vec<shibapost_store::ItemSummary> {
        let due = match session.db(move |s| s.due_follow_ups(after, now)).await {
            Ok(due) => due,
            Err(e) => {
                tracing::warn!("follow-ups: {}", e.message);
                return Vec::new();
            }
        };
        let mut out = Vec::new();
        for f in due {
            let (to, since) = (f.to.clone(), f.sent_at);
            let replies = session
                .db(move |s| s.titles_from_since(&to, since))
                .await
                .unwrap_or_default();
            let topic = thread_subject(&f.subject);
            if replies.iter().any(|t| thread_subject(t) == topic) {
                tracing::info!("follow-up: already answered");
                continue;
            }
            let id =
                shibapost_store::ids::mail_id(&f.account_id, &format!("sent:{}", f.message_id));
            let lookup = id.clone();
            let Ok(Some(detail)) = session.db(move |s| s.item(&lookup)).await else {
                continue;
            };
            let patch = ItemPatch {
                read: Some(false),
                ..Default::default()
            };
            let p = patch.clone();
            let ids = vec![id];
            match session.db(move |s| s.set_item_state(&ids, &p)).await {
                Ok(refs) => self.dispatch(session, refs, &patch),
                Err(e) => tracing::warn!("follow-up: could not mark unread: {}", e.message),
            }
            out.push(detail.item);
        }
        out
    }

    /// 看看有没有到时间的稍后提醒。调度器每分钟调用一次。
    pub(crate) async fn check_reminders(self: &Arc<Self>, session: &Arc<Session>) {
        let path = self.local_dir.join(STATE_FILE);
        let now = now();
        let Some(after) = read(&path) else {
            write(&path, now);
            return;
        };
        if after >= now {
            return;
        }
        let due = match session.db(move |s| s.due_reminders(after, now)).await {
            Ok(due) => due,
            Err(e) => {
                tracing::warn!("reminders: {}", e.message);
                return;
            }
        };
        let mut due = due;
        due.extend(self.due_follow_ups(session, after, now).await);
        write(&path, now);
        if due.is_empty() {
            return;
        }
        // 回到收件箱时标成未读，「提醒」一栏和未读数都看得到它
        let ids: Vec<String> = due.iter().map(|i| i.id.clone()).collect();
        let patch = ItemPatch {
            read: Some(false),
            ..Default::default()
        };
        let p = patch.clone();
        match session.db(move |s| s.set_item_state(&ids, &p)).await {
            Ok(refs) => self.dispatch(session, refs, &patch),
            Err(e) => tracing::warn!("reminders: could not mark unread: {}", e.message),
        }
        tracing::info!(count = due.len(), "reminders due");
        self.emit(Event::Reminders {
            total: due.len(),
            items: due
                .into_iter()
                .take(SHOWN)
                .map(|i| Reminder {
                    id: i.id,
                    kind: i.kind,
                    title: i.title,
                    source_title: i.source_title,
                })
                .collect(),
        });
        self.emit_changed(&[Scope::Feeds, Scope::Items], None);
    }
}

#[cfg(test)]
mod tests {
    use super::thread_subject;

    #[test]
    fn reply_prefixes_are_ignored() {
        assert_eq!(thread_subject("Re: Fwd: 回复： Q4  Budget"), "q4 budget");
        assert_eq!(thread_subject("AW[2]: Plan"), "plan");
        assert_eq!(thread_subject("Repo update"), "repo update");
    }
}
