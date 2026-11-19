//! 写信和发信：回复、全部回复、转发、写新邮件，以及草稿和附件。
//!
//! 信由本机直接交给邮箱的发信服务器（SMTP），不经过任何别的服务器。发出去以后在服务器的
//! 「已发送」里存一份，其他设备和别的邮件程序也能看到；Gmail 和 Outlook 发信时自己会存，就不再存。
//!
//! 草稿存在邮箱自己的「草稿」文件夹里（IMAP），手机和网页邮箱里也看得到。每存一次是一封新的，
//! 存好以后删掉上一版。
//!
//! 附件先交给这里，记在内存里，写信窗口只拿着一个编号；自动存草稿时不用每次把文件再传一遍。
//! 写信窗口关掉时放掉。转发和接着写草稿时，原信里的附件直接从原文里取出来放进这里。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use postflow_mail::{Attachment, Auth, Connection, Mailbox, Outgoing, ServerConfig};
use postflow_store::ids;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::oneshot;

use crate::mail::{Command, security_str, server_config, smtp_config};
use crate::rpc::{parse, reply};
use crate::{Core, RpcError, Session};

/// 发完等一会儿再看「已发送」里有没有：有的服务器自己存一份，但不是马上就能看到。
const SENT_COPY_DELAY: Duration = Duration::from_secs(3);
/// 服务器自己存「已发送」时，发出后等多久再去同步。
const SENT_REFRESH_DELAY: Duration = Duration::from_secs(5);
/// 收件人一栏最多提示这么多联系人。
const MAX_CONTACTS: usize = 500;
/// 发信后会自己在「已发送」里存一份的服务器。
const SAVES_SENT_ITSELF: &[&str] = &[
    "smtp.gmail.com",
    "smtp-mail.outlook.com",
    "smtp.office365.com",
];
/// 存草稿最多等这么久。
const DRAFT_TIMEOUT: Duration = Duration::from_secs(60);
/// 内存里最多留这么多附件，超出时先放掉最早的。
const UPLOADS_LIMIT: usize = 256 * 1024 * 1024;

/// 写信窗口交来的附件，按编号存着。
#[derive(Default)]
pub(crate) struct Uploads {
    files: HashMap<String, (Instant, Arc<Attachment>)>,
    /// 编号只要在这次运行里不重复就行，附件不跨重启保留。
    next: u64,
}

impl Uploads {
    fn insert(&mut self, file: Attachment) -> String {
        self.next += 1;
        let token = format!("file-{}", self.next);
        self.files
            .insert(token.clone(), (Instant::now(), Arc::new(file)));
        // 占的内存太多了：放掉最早的。还在写的信再用到时会报错，界面让用户重新添加
        let mut total: usize = self.files.values().map(|(_, f)| f.data.len()).sum();
        while total > UPLOADS_LIMIT && self.files.len() > 1 {
            let Some(oldest) = self
                .files
                .iter()
                .filter(|(t, _)| **t != token)
                .min_by_key(|(_, (at, _))| *at)
                .map(|(t, _)| t.clone())
            else {
                break;
            };
            if let Some((_, f)) = self.files.remove(&oldest) {
                total -= f.data.len();
            }
        }
        token
    }
}

#[derive(Deserialize)]
struct Id {
    id: String,
}

#[derive(Deserialize)]
struct ContextParams {
    id: String,
    /// 转发：原信的附件也取出来，放进待发的附件里。
    #[serde(default)]
    forward: bool,
}

mod outbox;

/// 写信窗口里的一封信。发送和存草稿用同一个样子；定时发送时原样存在本机。
#[derive(Deserialize, Serialize, Clone)]
struct Letter {
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
    /// 带格式的正文。`text` 是它的纯文本版。
    #[serde(default)]
    html: Option<String>,
    /// 回复的那封信的 Message-ID 和讨论里之前的那些，接上同一个讨论。
    #[serde(default)]
    in_reply_to: Option<String>,
    #[serde(default)]
    references: Vec<String>,
    /// 附件的编号（`mail.attach` 给的）。
    #[serde(default)]
    attachments: Vec<String>,
    /// 这封信之前存过的草稿的 Message-ID：发出去或者再存时，把它删掉。
    #[serde(default)]
    draft: Option<String>,
    /// 「没回复就提醒」：到这个时间（毫秒）还没收到回复就提醒。
    #[serde(default)]
    follow_up: Option<i64>,
}

#[derive(Deserialize)]
struct Upload {
    name: String,
    #[serde(default)]
    mime: String,
    /// Base64。
    data: String,
}

#[derive(Deserialize)]
struct AttachFrom {
    id: String,
    index: u32,
}

#[derive(Deserialize)]
struct Tokens {
    tokens: Vec<String>,
}

/// 要删的草稿：写信窗口知道邮箱和 Message-ID；列表里的草稿只给 ID。
#[derive(Deserialize)]
struct DeleteDraft {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    message_id: Option<String>,
}

/// 交给界面的一个附件：编号、名字、类型和大小。
#[derive(Serialize)]
struct AttachedFile {
    token: String,
    name: String,
    mime: String,
    size: usize,
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
        E::TooLarge(_) => "send_too_large",
    };
    with_detail(err, code)
}

/// 错误码加上给人看的原因。信太大时 message 是上限的 MB 数，界面拿来说「最多 N MB」。
fn with_detail(err: &postflow_mail::Error, code: &str) -> RpcError {
    if let postflow_mail::Error::TooLarge(limit) = err {
        return RpcError::new(code, format!("{}", limit.div_ceil(1024 * 1024)));
    }
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

/// 存草稿时写错的收件人不挡着，存不进去就算了。
fn loose_recipients(field: &[String]) -> Vec<Mailbox> {
    field
        .iter()
        .filter_map(|s| postflow_mail::parse_address(s))
        .collect()
}

/// 一个收件人写成界面上一格里的样子。
fn mailbox_text(m: &Mailbox) -> String {
    let address = m.address.clone().unwrap_or_default();
    match m.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        Some(name) if name.contains([',', ';', '<', '>', '"', '@']) => {
            format!("\"{}\" <{address}>", name.replace('"', ""))
        }
        Some(name) => format!("{name} <{address}>"),
        None => address,
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
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
            "mail.save_draft" => self.save_draft(params, session).await,
            "mail.delete_draft" => self.delete_draft(params, session).await,
            "mail.draft" => self.open_draft(params, session).await,
            "mail.attach" => self.attach(params),
            "mail.attach_from" => self.attach_from(params, session).await,
            "mail.attach_drop" => match parse::<Tokens>(params) {
                Ok(p) => {
                    let mut uploads = self.uploads();
                    for t in p.tokens {
                        uploads.files.remove(&t);
                    }
                    reply(json!({}))
                }
                Err(e) => Err(e),
            },
            "mail.contacts" => match session.db(|s| s.mail_contacts(MAX_CONTACTS)).await {
                Ok(contacts) => reply(json!({ "contacts": contacts })),
                Err(e) => Err(e),
            },
            "accounts.smtp" => self.smtp_settings(params, session).await,
            m if m == "mail.schedule"
                || m == "mail.outbox"
                || m == "mail.outbox_send"
                || m == "mail.unschedule" =>
            {
                return self.outbox_rpc(m, params, session).await;
            }
            _ => return None,
        };
        Some(result)
    }

    fn uploads(&self) -> std::sync::MutexGuard<'_, Uploads> {
        self.uploads.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn store_upload(&self, file: Attachment) -> AttachedFile {
        let (name, mime, size) = (file.name.clone(), file.mime.clone(), file.data.len());
        let token = self.uploads().insert(file);
        AttachedFile {
            token,
            name,
            mime,
            size,
        }
    }

    /// 写信窗口加了一个文件。
    fn attach(&self, params: Value) -> Result<Value, RpcError> {
        let p: Upload = parse(params)?;
        let data = postflow_mail::base64_decode(&p.data)
            .ok_or_else(|| RpcError::new("bad_params", "the file could not be read"))?;
        if data.len() > postflow_mail::MAX_ATTACHMENTS {
            return Err(with_detail(
                &postflow_mail::Error::TooLarge(postflow_mail::MAX_ATTACHMENTS),
                "send_too_large",
            ));
        }
        reply(self.store_upload(Attachment {
            name: p.name,
            mime: p.mime,
            data,
        }))
    }

    /// 把一封邮件里的附件放进待发的附件里。
    async fn attach_from(&self, params: Value, session: &Arc<Session>) -> Result<Value, RpcError> {
        let p: AttachFrom = parse(params)?;
        let raw = self.raw_mail(session, &p.id).await?;
        reply(self.extract_attachment(&raw, p.index)?)
    }

    fn extract_attachment(&self, raw: &[u8], index: u32) -> Result<AttachedFile, RpcError> {
        let (name, mime, data) = postflow_mail::attachment(raw, index)
            .ok_or_else(|| RpcError::new("not_found", "no such attachment"))?;
        Ok(self.store_upload(Attachment { name, mime, data }))
    }

    /// 从编号取回附件。找不到的（内存不够被放掉了）报错，让用户重新添加。
    fn take_attachments(&self, tokens: &[String]) -> Result<Vec<Attachment>, RpcError> {
        let uploads = self.uploads();
        tokens
            .iter()
            .map(|t| {
                uploads
                    .files
                    .get(t)
                    .map(|(_, f)| (**f).clone())
                    .ok_or_else(|| {
                        RpcError::new("attachment_gone", "an attachment needs to be added again")
                    })
            })
            .collect()
    }

    /// 回复、转发一封信要用到的原信内容：发件人、收件人、主题、时间、正文的文字，
    /// 以及接上讨论用的 In-Reply-To 和 References。转发时还有原信的附件。
    async fn compose_context(
        &self,
        params: Value,
        session: &Arc<Session>,
    ) -> Result<Value, RpcError> {
        let p: ContextParams = parse(params)?;
        let id = p.id.clone();
        let r = session
            .db(move |s| s.mail_ref(&id))
            .await?
            .ok_or_else(|| RpcError::new("not_found", "no such message"))?;
        let raw = self.raw_mail(session, &p.id).await?;
        let ctx = postflow_mail::reply_context(&raw)
            .ok_or_else(|| RpcError::new("mail_parse", "this message could not be read"))?;
        let (in_reply_to, references) =
            postflow_mail::thread_headers(ctx.message_id.as_deref(), &ctx.references);
        let attachments: Vec<AttachedFile> = if p.forward {
            ctx.attachments
                .iter()
                .filter_map(|a| self.extract_attachment(&raw, a.index).ok())
                .collect()
        } else {
            Vec::new()
        };
        let mut value = reply(&ctx)?;
        value["account_id"] = json!(r.account_id);
        value["thread"] = json!({ "in_reply_to": in_reply_to, "references": references });
        value["attachments"] = json!(attachments);
        Ok(value)
    }

    /// 接着写一封草稿：草稿里写了什么，附件也放进待发的附件里。
    async fn open_draft(&self, params: Value, session: &Arc<Session>) -> Result<Value, RpcError> {
        let p: Id = parse(params)?;
        let id = p.id.clone();
        let r = session
            .db(move |s| s.mail_ref(&id))
            .await?
            .ok_or_else(|| RpcError::new("not_found", "no such draft"))?;
        let raw = self.raw_mail(session, &p.id).await?;
        let draft = postflow_mail::draft_contents(&raw)
            .ok_or_else(|| RpcError::new("mail_parse", "this draft could not be read"))?;
        let attachments: Vec<AttachedFile> = draft
            .attachments
            .iter()
            .filter_map(|a| self.extract_attachment(&raw, a.index).ok())
            .collect();
        let list = |ms: &[Mailbox]| ms.iter().map(mailbox_text).collect::<Vec<_>>();
        reply(json!({
            "account_id": r.account_id,
            "message_id": draft.message_id,
            "to": list(&draft.to),
            "cc": list(&draft.cc),
            "bcc": list(&draft.bcc),
            "subject": draft.subject,
            "text": draft.text,
            "html": draft.html,
            "thread": { "in_reply_to": draft.in_reply_to, "references": draft.references },
            "attachments": attachments,
        }))
    }

    /// 账户现在用的发信服务器，设置里显示。`custom` 表示是自己填的，不是自动推断的。
    async fn smtp_settings(
        &self,
        params: Value,
        session: &Arc<Session>,
    ) -> Result<Value, RpcError> {
        let p: Id = parse(params)?;
        let account = self.account(session, &p.id).await?;
        let cfg = smtp_config(&account)?;
        reply(json!({
            "host": cfg.host,
            "port": cfg.port,
            "security": security_str(cfg.security),
            "custom": account.smtp_host.as_deref().is_some_and(|h| !h.trim().is_empty()),
        }))
    }

    async fn account(
        &self,
        session: &Arc<Session>,
        id: &str,
    ) -> Result<postflow_store::Account, RpcError> {
        let id = id.to_string();
        session
            .db(move |s| s.account(&id))
            .await?
            .ok_or_else(|| RpcError::new("not_found", "no such account"))
    }

    async fn send(&self, params: Value, session: &Arc<Session>) -> Result<Value, RpcError> {
        let p: Letter = parse(params)?;
        let account = self.account(session, &p.account_id).await?;
        let outgoing = letter(
            &account,
            &p,
            [recipients(&p.to)?, recipients(&p.cc)?, recipients(&p.bcc)?],
            self.take_attachments(&p.attachments)?,
        );
        let built =
            postflow_mail::build(&outgoing, now_secs()).map_err(|e| send_error(&e, false))?;

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
        // 没回复就提醒：记进同步日志，到时间由提醒检查去看有没有回复（见 reminders.rs）
        if let Some(until) = p.follow_up.filter(|u| *u > 0) {
            let to: Vec<String> = [&p.to, &p.cc]
                .into_iter()
                .flat_map(|list| list.iter())
                .filter_map(|s| postflow_mail::parse_address(s))
                .filter_map(|m| m.address.map(|a| a.to_lowercase()))
                .collect();
            let follow = postflow_store::FollowUp {
                message_id: built.message_id.clone(),
                account_id: account.id.clone(),
                subject: p.subject.clone(),
                to,
                sent_at: now_secs() * 1000,
                until,
            };
            if let Err(e) = session.db(move |s| s.set_follow_up(&follow)).await {
                tracing::warn!("could not save the follow-up reminder: {}", e.message);
            }
        }
        // 只记人数，不记地址和文件名
        tracing::info!(
            account = %account.id,
            recipients = built.recipients.len(),
            attachments = p.attachments.len(),
            "mail sent"
        );

        // 发出去了：草稿不要了，附件也放掉
        if let Some(message_id) = p.draft.clone().filter(|d| !d.trim().is_empty()) {
            self.send_mail(session, &account.id, Command::DeleteDraft { message_id });
        }
        {
            let mut uploads = self.uploads();
            for t in &p.attachments {
                uploads.files.remove(t);
            }
        }

        let saves_itself = SAVES_SENT_ITSELF
            .iter()
            .any(|h| smtp.host.eq_ignore_ascii_case(h));
        // 发出去以后，把「已发送」同步一次，刚发的信马上出现在「已发送」里，不用等下一次收信
        let session = session.clone();
        let account_id = account.id.clone();
        if !saves_itself && let Ok(imap) = server_config(&account) {
            let message_id = built.message_id.clone();
            let raw = built.raw;
            tokio::spawn(async move {
                tokio::time::sleep(SENT_COPY_DELAY).await;
                if let Err(e) = save_sent_copy(&imap, &auth, &message_id, &raw).await {
                    tracing::warn!(account = %account_id, code = e.code(), "saving a copy to Sent failed: {e}");
                }
                Core::nudge_mail(&session, &account_id);
            });
        } else {
            // 服务器自己存「已发送」的（Gmail、Outlook），给它几秒钟存好
            tokio::spawn(async move {
                tokio::time::sleep(SENT_REFRESH_DELAY).await;
                Core::nudge_mail(&session, &account_id);
            });
        }
        reply(json!({ "message_id": built.message_id }))
    }

    /// 存草稿。返回新草稿的 Message-ID 和它在列表里的 ID，下次存时用来替换它。
    async fn save_draft(&self, params: Value, session: &Arc<Session>) -> Result<Value, RpcError> {
        let p: Letter = parse(params)?;
        let account = self.account(session, &p.account_id).await?;
        let outgoing = letter(
            &account,
            &p,
            [
                loose_recipients(&p.to),
                loose_recipients(&p.cc),
                loose_recipients(&p.bcc),
            ],
            self.take_attachments(&p.attachments)?,
        );
        let built = postflow_mail::build_draft(&outgoing, now_secs())
            .map_err(|e| with_detail(&e, "draft_failed"))?;
        let (tx, rx) = oneshot::channel();
        let command = Command::SaveDraft {
            raw: built.raw,
            replace: p.draft.clone().filter(|d| !d.trim().is_empty()),
            reply: tx,
        };
        let unavailable = || RpcError::new("mail_unavailable", "this account is not connected");
        if !self.send_mail(session, &account.id, command) {
            return Err(unavailable());
        }
        match tokio::time::timeout(DRAFT_TIMEOUT, rx).await {
            Ok(Ok(Ok(()))) => {}
            Ok(Ok(Err(e))) => return Err(e),
            Ok(Err(_)) => return Err(unavailable()),
            Err(_) => return Err(RpcError::new("mail_timeout", "timed out")),
        }
        tracing::debug!(account = %account.id, "draft saved");
        let item_id = ids::mail_id(&account.id, &format!("draft:{}", built.message_id));
        reply(json!({ "message_id": built.message_id, "item_id": item_id }))
    }

    /// 丢掉一封草稿。交给后台任务去删，不用等。
    async fn delete_draft(&self, params: Value, session: &Arc<Session>) -> Result<Value, RpcError> {
        let p: DeleteDraft = parse(params)?;
        let (account_id, message_id) = match (p.id, p.account_id, p.message_id) {
            (Some(id), _, _) => {
                let r = session
                    .db(move |s| s.mail_ref(&id))
                    .await?
                    .ok_or_else(|| RpcError::new("not_found", "no such draft"))?;
                (r.account_id, r.message_id.unwrap_or_default())
            }
            (None, Some(account), Some(message_id)) => (account, message_id),
            _ => return Err(RpcError::new("bad_params", "which draft?")),
        };
        let message_id = message_id.trim().to_string();
        if message_id.is_empty() {
            return Err(RpcError::new("bad_params", "message id is empty"));
        }
        if !self.send_mail(session, &account_id, Command::DeleteDraft { message_id }) {
            return Err(RpcError::new(
                "mail_unavailable",
                "this account is not connected",
            ));
        }
        reply(json!({}))
    }
}

/// 把写信窗口里的一封信变成要生成的邮件。`lists` 是收件人、抄送、密送。
fn letter(
    account: &postflow_store::Account,
    p: &Letter,
    lists: [Vec<Mailbox>; 3],
    attachments: Vec<Attachment>,
) -> Outgoing {
    let [to, cc, bcc] = lists;
    Outgoing {
        from: Mailbox {
            // 侧栏里的邮箱名（比如「工作」）不是人名，只用专门填的发件人名字
            name: Some(account.sender_name.trim().to_string()).filter(|n| !n.is_empty()),
            address: Some(account.email.trim().to_string()),
        },
        to,
        cc,
        bcc,
        subject: p.subject.clone(),
        text: p.text.clone(),
        html: p.html.clone(),
        in_reply_to: p.in_reply_to.clone(),
        references: p.references.clone(),
        attachments,
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
