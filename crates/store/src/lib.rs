//! Postflow 的数据层。
//!
//! 数据分两处存放：
//!
//! - **数据目录**：只追加的 JSONL 日志，是唯一的事实来源，可以交给任何文件同步工具。
//! - **本机索引**：由日志推导出的 SQLite 数据库，只在本机，随时可以删除重建。

mod ai;
mod error;
pub mod hlc;
pub mod ids;
pub mod image;
pub mod logs;
mod mail;
mod model;
mod rules;
mod search;
mod stats;
mod store;
mod usage;

pub use ai::{AiMessage, AiResult, AiUsage, LabelCount, day_of};
pub use error::{Error, Result};
pub use model::*;
pub use rules::RuleItem;
pub use search::{AttachmentMeta, AttachmentOverview, AttachmentRow, MailText, match_query};
pub use stats::{FeedStats, ReadingDay, Stats};
pub use store::{LATER_URL, Store, later_feed_id};
pub use usage::{IndexUsage, SourceUsage};
