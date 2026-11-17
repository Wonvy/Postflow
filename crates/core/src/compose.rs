//! 写信和发信：回复、全部回复、转发和写新邮件都走这里。
//!
//! 信由本机直接交给邮箱的发信服务器（SMTP），不经过任何别的服务器。发出去以后在服务器的
//! 「已发送」里存一份，其他设备和别的邮件程序也能看到；Gmail 和 Outlook 发信时自己会存，就不再存。

use std::sync::Arc;
use std::time::Duration;

use postflow_mail::{Auth, Connection, Mailbox, Outgoing, ServerConfig};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::mail::{mail_error, security_str, server_config, smtp_config};
use crate::rpc::{parse, reply};
use crate::{Core, RpcError, Session};

/// 发完等一会儿再看「已发送」里有没有：有的服务器自己存一份，但不是马上就能看到。
const SENT_COPY_DELAY: Duration = Duration::from_secs(3);
/// 收件人一栏最多提示这么多联系人。
const MAX_CONTACTS: usize = 500;
/// 发信后会自己在「已发送」里存一份的服务器。
const SAVES_SENT_ITSELF: &[&str] = &[
    "smtp.gmail.com",
    "smtp-mail.outlook.com",
    "smtp.office365.com",
];

#[derive(Deserialize)]
struct Id {
    id: String,
}

#[derive(Deserialize)]
struct Send {
    account_id: String,
    #[serde(default)]
    to: Vec<String>,
    #[serde(default)]
    cc: Vec<String>,
    #[serde(default)]
    bcc: Vec<String>,
    #[serde(default)]
    subject: String,
    #[serde(default)]
    text: String,
    /// 回复的那封信。用它的 Message-ID 接上同一个讨论。
    #[serde(default)]
    in_reply_to: Option<String>,
}

/// 发信出错时给界面的错误码。和收信的分开，界面才能说清楚是发信那边出的问题。
pub(crate) fn send_error(err: &postflow_mail::Error, oauth: bool) -> RpcError {
    use postflow_mail::Error as E;
    let code = match err {
        // 微软账户：多半是登录时还没有发信权限，或者管理员关了 SMTP 发信
        E::Auth(_) if oauth => "send_sign_in",
        E::Auth(_) => "send_auth",
        E::Connect(_) | E::Io(_) => "send_connect",
        E::Tls(_) => "send_tls",
        E::Timeout => "send_timeout",
        E::Server(_) => "send_rejected",
        E::Config(_) => "bad_params",
    };
    let detail = err.detail();
    RpcError::new(
        code,
        if detail.is_empty() {
            err.to_string()
        } else {
            detail.to_string()
        },
    )
}

fn recipients(field: &[String]) -> Result<Vec<Mailbox>, RpcError> {
    field
        .iter()
        .filter(|s| !s.trim().is_empty())
        .map(|s| {
            postflow_mail::parse_address(s)
                .ok_or_else(|| RpcError::new("send_bad_address", s.trim().to_string()))
        })
        .collect()
}

impl Core {
    pub(crate) async fn compose_rpc(
        self: &Arc<Self>,
        method: &str,
        params: Value,
        session: &Arc<Session>,
    ) -> Option<Result<Value, RpcError>> {
        let result = match method {
            "mail.compose_context" => self.compose_context(params, session).await,
            "mail.send" => self.send(params, session).await,
            "mail.contacts" => match session.db(|s| s.mail_contacts(MAX_CONTACTS)).await {
                Ok(contacts) => reply(json!({ "contacts": contacts })),
                Err(e) => Err(e),
            },
            "accounts.smtp" => self.smtp_settings(params, session).await,
            _ => return None,
        };
        Some(result)
    }

    /// 回复、转发一封信要用到的原信内容：发件人、收件人、主题、时间和正文的文字。
    async fn compose_context(
        &self,
        params: Value,
        session: &Arc<Session>,
    ) -> Result<Value, RpcError> {
        let p: Id = parse(params)?;
        let id = p.id.clone();
        let r = session
            .db(move |s| s.mail_ref(&id))
            .await?
            .ok_or_else(|| RpcError::new("not_found", "no such message"))?;
        let raw = self.raw_mail(session, &p.id).await?;
        let ctx = postflow_mail::reply_context(&raw)
            .ok_or_else(|| RpcError::new("mail_parse", "this message could not be read"))?;
        let mut value = reply(ctx)?;
        value["account_id"] = json!(r.account_id);
        Ok(value)
    }

    /// 账户现在用的发信服务器，设置里显示。`custom` 表示是自己填的，不是自动推断的。
    async fn smtp_settings(
        &self,
        params: Value,
        session: &Arc<Session>,
    ) -> Result<Value, RpcError> {
        let p: Id = parse(params)?;
        let id = p.id.clone();
        let account = session
            .db(move |s| s.account(&id))
            .await?
            .ok_or_else(|| RpcError::new("not_found", "no such account"))?;
        let cfg = smtp_config(&account)?;
        reply(json!({
            "host": cfg.host,
            "port": cfg.port,
            "security": security_str(cfg.security),
            "custom": account.smtp_host.as_deref().is_some_and(|h| !h.trim().is_empty()),
        }))
    }

    async fn send(&self, params: Value, session: &Arc<Session>) -> Result<Value, RpcError> {
        let p: Send = parse(params)?;
        let id = p.account_id.clone();
        let account = session
            .db(move |s| s.account(&id))
            .await?
            .ok_or_else(|| RpcError::new("not_found", "no such account"))?;

        // 回复时接上原信的讨论
        let (in_reply_to, references) = match p.in_reply_to.as_deref() {
            Some(item) => {
                let raw = self.raw_mail(session, item).await?;
                let ctx = postflow_mail::reply_context(&raw).unwrap_or_default();
                postflow_mail::thread_headers(ctx.message_id.as_deref(), &ctx.references)
            }
            None => (None, Vec::new()),
        };
        let outgoing = Outgoing {
            from: Mailbox {
                // 侧栏里的邮箱名（比如「工作」）不是人名，只用专门填的发件人名字
                name: Some(account.sender_name.trim().to_string()).filter(|n| !n.is_empty()),
                address: Some(account.email.trim().to_string()),
            },
            to: recipients(&p.to)?,
            cc: recipients(&p.cc)?,
            bcc: recipients(&p.bcc)?,
            subject: p.subject,
            text: p.text,
            in_reply_to,
            references,
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let built = postflow_mail::build(&outgoing, now).map_err(|e| mail_error(&e))?;

        let smtp = smtp_config(&account)?;
        let auth = self.mail_auth(&account.id).await?.ok_or_else(|| {
            RpcError::new("mail_no_password", "enter the password for this account")
        })?;
        let oauth = matches!(auth, Auth::OAuth2(_));
        postflow_mail::smtp::send(&smtp, &auth, &built.sender, &built.recipients, &built.raw)
            .await
            .map_err(|e| {
                tracing::warn!(account = %account.id, host = %smtp.host, code = e.code(), "sending mail failed: {e}");
                send_error(&e, oauth)
            })?;
        // 只记人数，不记地址
        tracing::info!(account = %account.id, recipients = built.recipients.len(), "mail sent");

        let saves_itself = SAVES_SENT_ITSELF
            .iter()
            .any(|h| smtp.host.eq_ignore_ascii_case(h));
        if !saves_itself && let Ok(imap) = server_config(&account) {
            let account_id = account.id.clone();
            let message_id = built.message_id.clone();
            let raw = built.raw;
            tokio::spawn(async move {
                tokio::time::sleep(SENT_COPY_DELAY).await;
                if let Err(e) = save_sent_copy(&imap, &auth, &message_id, &raw).await {
                    tracing::warn!(account = %account_id, code = e.code(), "saving a copy to Sent failed: {e}");
                }
            });
        }
        reply(json!({ "message_id": built.message_id }))
    }
}

/// 在服务器的「已发送」里存一份。服务器自己已经存了的（按 Message-ID 找得到）就不再存。
async fn save_sent_copy(
    imap: &ServerConfig,
    auth: &Auth,
    message_id: &str,
    raw: &[u8],
) -> Result<(), postflow_mail::Error> {
    let mut conn = Connection::connect(imap, auth).await?;
    let result = async {
        let sent = conn.sent_mailbox().await?;
        if !conn.contains_message_id(&sent, message_id).await? {
            conn.append(&sent, raw).await?;
        }
        Ok(())
    }
    .await;
    conn.logout().await;
    result
}
