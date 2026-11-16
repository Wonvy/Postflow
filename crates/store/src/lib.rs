//! Postflow 的数据层。
//!
//! 数据分两处存放：
//!
//! - **数据目录**：只追加的 JSONL 日志，是唯一的事实来源，可以交给任何文件同步工具。
//! - **本机索引**：由日志推导出的 SQLite 数据库，只在本机，随时可以删除重建。

mod error;
pub mod hlc;
pub mod ids;
pub mod logs;
mod mail;
mod model;
mod store;

pub use error::{Error, Result};
pub use model::*;
pub use store::Store;
