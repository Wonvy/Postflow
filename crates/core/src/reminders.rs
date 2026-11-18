//! 稍后提醒到时间了：标为未读（邮件写回服务器），通知界面弹提醒。
//!
//! 提醒时间本身存在同步日志里（见 store 的 `set_snooze`）。每台设备在本机目录的
//! `reminders-state.json` 里记着处理到了哪个时刻，所以应用关着时到期的，下次打开会补上，
//! 也不会每次启动都把同一条再标一遍未读。第一次运行从现在开始算，不翻旧账。

use std::path::Path;
use std::sync::Arc;

use postflow_store::ItemPatch;
use serde::{Deserialize, Serialize};

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

impl Core {
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
