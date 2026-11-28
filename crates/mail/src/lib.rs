//! 邮件相关的一切：服务商预设、IMAP 连接与操作、邮件解析与安全渲染。
//!
//! 和 `shibapost-feed` 一样，这个 crate 不碰存储，只负责和邮件服务器打交道、把邮件变成结构化数据。

pub mod calendar;
mod code;
pub use code::verification_code;
mod compose;
mod imap;
mod message;
pub mod oauth;
mod presets;
pub mod smtp;

pub use compose::{
    Attachment, Built, MAX_ATTACHMENTS, Outgoing, build, build_draft, parse_address, thread_headers,
};
pub use imap::{
    Auth, Connection, FlagChange, FolderInfo, IdleOutcome, Listing, MailboxInfo, Security,
    ServerConfig, SpecialMailbox,
};
pub use message::{
    AttachmentInfo, BodyKind, DraftContents, Mailbox, RenderOptions, Rendered, ReplyContext,
    Summary, Unsubscribe, attachment, base64, base64_decode, clean_compose_html, draft_contents,
    plain_text, render, reply_context, search_text, summarize, unsubscribe, unsubscribe_one_click,
};
pub use presets::{Preset, detect, guess, presets, provider_for_mx, smtp_for};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("authentication failed: {0}")]
    Auth(String),
    #[error("could not connect: {0}")]
    Connect(String),
    #[error("secure connection failed: {0}")]
    Tls(String),
    #[error("timed out")]
    Timeout,
    #[error("server rejected the request: {0}")]
    Server(String),
    #[error("connection lost: {0}")]
    Io(String),
    #[error("{0}")]
    Config(String),
    /// 信太大，服务器（或者我们自己）不收。带着上限，字节数。
    #[error("the message is too large (limit {0} bytes)")]
    TooLarge(usize),
}

impl Error {
    /// 给界面用的稳定错误码。
    pub fn code(&self) -> &'static str {
        match self {
            Error::Auth(_) => "mail_auth",
            Error::Connect(_) => "mail_connect",
            Error::Tls(_) => "mail_tls",
            Error::Timeout => "mail_timeout",
            Error::Server(_) => "mail_server",
            Error::Io(_) => "mail_connect",
            Error::Config(_) => "bad_params",
            Error::TooLarge(_) => "mail_too_large",
        }
    }

    /// 去掉类型前缀的原因，多半是服务器的原话，给用户看。
    pub fn detail(&self) -> &str {
        match self {
            Error::Auth(s)
            | Error::Connect(s)
            | Error::Tls(s)
            | Error::Server(s)
            | Error::Io(s)
            | Error::Config(s) => s,
            Error::Timeout | Error::TooLarge(_) => "",
        }
    }

    /// 重试也不会好的错误：要等用户改了配置或密码才有意义再连。
    pub fn needs_user(&self) -> bool {
        matches!(self, Error::Auth(_) | Error::Config(_))
    }
}

impl From<async_imap::error::Error> for Error {
    fn from(err: async_imap::error::Error) -> Self {
        use async_imap::error::Error as E;
        match err {
            E::Io(e) => Error::Io(e.to_string()),
            E::ConnectionLost => Error::Io("connection closed by server".into()),
            E::No(msg) | E::Bad(msg) => Error::Server(server_text(&msg)),
            other => Error::Server(other.to_string()),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// async-imap 把服务器的拒绝原因包成 `code: …, info: Some("…")`，取出里面给人看的那句话。
pub(crate) fn server_text(raw: &str) -> String {
    const OPEN: &str = "info: Some(\"";
    let Some(start) = raw.find(OPEN) else {
        return raw.trim().to_string();
    };
    let inner = &raw[start + OPEN.len()..];
    let inner = inner.strip_suffix("\")").unwrap_or(inner);
    let text = inner.replace("\\\"", "\"").replace("\\\\", "\\");
    // 去掉开头像 [AUTHENTICATIONFAILED] 这样的机器码，保留后面的说明
    match text.strip_prefix('[').and_then(|t| t.split_once(']')) {
        Some((_, rest)) if !rest.trim().is_empty() => rest.trim().to_string(),
        _ => text.trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_text_is_extracted() {
        assert_eq!(
            server_text(
                r#"code: None, info: Some("[AUTHENTICATIONFAILED] Invalid credentials (Failure)")"#
            ),
            "Invalid credentials (Failure)"
        );
        assert_eq!(
            server_text(r#"code: None, info: Some("Basic authentication is disabled.")"#),
            "Basic authentication is disabled."
        );
        assert_eq!(
            server_text(r#"code: None, info: Some("say \"hi\"")"#),
            r#"say "hi""#
        );
        assert_eq!(server_text("  plain reason "), "plain reason");
    }
}
