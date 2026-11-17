//! 接入 AI 服务：总结、关键词、摘要、翻译、对话和分类。
//!
//! 只用各家都支持的 OpenAI 兼容接口。用户自己的 API Key，从这台电脑直接请求，没有中间服务器。

pub mod client;
pub mod presets;
pub mod prompts;
pub mod text;

pub use client::{Client, Error, Message, Provider, Reply, Request, Role, Usage};
pub use presets::{PRESETS, Preset, preset};
pub use prompts::{Category, Doc};
