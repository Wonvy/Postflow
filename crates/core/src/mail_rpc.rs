//! 和邮箱有关的接口方法。

use std::sync::Arc;
use std::time::Duration;

use postflow_mail::{Auth, RenderOptions, ServerConfig};
use postflow_store::{AccountConfig, AccountPatch, ItemSummary};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::oneshot;

use crate::mail::{Command, Write, mail_error, parse_security, security_str, verify, verify_login};
use crate::oauth::imap_server;
use crate::rpc::{parse, reply};
use crate::{Core, RpcError, Scope, Session};

/// 打开一封还没取回原文的邮件时，最多等这么久。
const FETCH_BODY_TIMEOUT: Duration = Duration::from_secs(60);
/// 设置里记着「总是显示这些发件人的远程图片」。
const TRUSTED_SENDERS: &str = "mail.trusted_senders";

#[derive(Deserialize)]
struct Email {
    email: String,
}

#[derive(Deserialize)]
struct AddAccount {
    email: String,
    /// 用 OAuth 登录（Outlook、Microsoft 365）时为空，改给 `oauth`。
    #[serde(default)]
    password: String,
    /// `accounts.oauth_start` 开始、`accounts.oauth_wait` 等完的那次登录。
    #[serde(default)]
    oauth: Option<String>,
    #[serde(default)]
    name: String,
    imap_host: Option<String>,
    imap_port: Option<u16>,
    imap_security: Option<String>,
    username: Option<String>,
}

#[derive(Deserialize)]
struct UpdateAccount {
    id: String,
    name: Option<String>,
    password: Option<String>,
    /// 重新用 OAuth 登录。
    #[serde(default)]
    oauth: Option<String>,
    imap_host: Option<String>,
    imap_port: Option<u16>,
    imap_security: Option<String>,
    username: Option<String>,
    /// 发信服务器。空字符串表示改回自动。
    smtp_host: Option<String>,
    smtp_port: Option<u16>,
    smtp_security: Option<String>,
    /// 发信时收件人看到的名字。
    sender_name: Option<String>,
    signature: Option<String>,
}

#[derive(Deserialize)]
struct Id {
    id: String,
}

#[derive(Deserialize)]
struct AccountId {
    account_id: String,
}

/// 「加载更早的邮件」：哪个邮箱（不给就是全部邮箱）、哪个文件夹。
#[derive(Deserialize)]
struct LoadOlder {
    #[serde(default)]
    account_id: Option<String>,
    /// inbox、sent、drafts、junk、trash，或者 folder（这时要给 `mailbox`）。
    folder: String,
    #[serde(default)]
    mailbox: Option<String>,
}

/// 取更早的邮件最多等这么久。
const OLDER_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Deserialize)]
struct MoveMail {
    ids: Vec<String>,
    /// 目标文件夹在服务器上的名字，收件箱是 INBOX。
    mailbox: String,
}

#[derive(Deserialize)]
struct OpenFolder {
    account_id: String,
    /// 服务器上的文件夹名。
    mailbox: String,
}

/// 列文件夹最多等这么久。
const FOLDERS_TIMEOUT: Duration = Duration::from_secs(30);

/// 收信。`auto` 为真时是应用自己发起的（窗口回到前台），不是用户点的刷新。
#[derive(Deserialize)]
struct SyncParams {
    id: Option<String>,
    #[serde(default)]
    auto: bool,
}

#[derive(Deserialize)]
struct Attachment {
    id: String,
    index: u32,
    /// 给了路径就直接写文件，桌面端用；不给就把内容返回，网页端自己触发下载。
    path: Option<String>,
}

#[derive(Deserialize)]
struct Address {
    address: String,
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// 侧栏里拖动后的新顺序。
#[derive(Deserialize)]
struct Reorder {
    ids: Vec<String>,
}

impl Core {
    pub(crate) async fn mail_rpc(
        self: &Arc<Self>,
        method: &str,
        params: Value,
        session: &Arc<Session>,
    ) -> Option<Result<Value, RpcError>> {
        let result = match method {
            "mail.presets" => reply(json!({ "presets": postflow_mail::presets() })),
            "accounts.guess" => self.guess(params).await,
            "accounts.add" => self.add_account(params, session).await,
            "accounts.update" => self.update_account(params, session).await,
            "accounts.remove" => self.remove_account(params, session).await,
            "accounts.reorder" => match parse::<Reorder>(params) {
                Ok(p) => match session.db(move |s| s.reorder_accounts(&p.ids)).await {
                    Ok(()) => {
                        self.emit_changed(&[Scope::Feeds], None);
                        reply(json!({}))
                    }
                    Err(e) => Err(e),
                },
                Err(e) => Err(e),
            },
            "accounts.sync" => match parse::<SyncParams>(params) {
                Ok(SyncParams { id: Some(id), auto }) => {
                    self.sync_account(session, &id, !auto);
                    reply(json!({}))
                }
                Ok(SyncParams { id: None, auto }) => {
                    self.sync_all_mail(session, !auto).await;
                    reply(json!({}))
                }
                Err(e) => Err(e),
            },
            "mail.attachment" => self.attachment(params, session).await,
            // 取消一封已发送邮件的「没回复就提醒」
            "mail.follow_up_cancel" => self.cancel_follow_up(params, session).await,
            "mail.folders" => self.folders(params, session).await,
            "mail.move" => self.move_mail(params, session).await,
            "mail.load_older" => self.load_older(params, session).await,
            "mail.open_folder" => match parse::<OpenFolder>(params) {
                Ok(p) => {
                    let name = p.mailbox.trim().to_string();
                    if name.is_empty() || name.eq_ignore_ascii_case("INBOX") {
                        Err(RpcError::new("bad_params", "which folder?"))
                    } else if self.send_mail(session, &p.account_id, Command::WatchFolder(name)) {
                        reply(json!({}))
                    } else {
                        Err(RpcError::new(
                            "mail_unavailable",
                            "this account is not connected",
                        ))
                    }
                }
                Err(e) => Err(e),
            },
            "mail.trust_sender" => self.trust_sender(params, session).await,
            "mail.unsubscribe" => self.unsubscribe(params, session).await,
            _ => match self.compose_rpc(method, params.clone(), session).await {
                Some(result) => result,
                None => return self.oauth_rpc(method, params, session).await,
            },
        };
        Some(result)
    }

    async fn guess(&self, params: Value) -> Result<Value, RpcError> {
        let p: Email = parse(params)?;
        // 企业邮箱和自定义域名要查 MX 记录才认得出来
        let (cfg, preset) = postflow_mail::detect(&p.email)
            .await
            .ok_or_else(|| RpcError::new("invalid_email", "email address is invalid"))?;
        reply(json!({
            "imap_host": cfg.host,
            "imap_port": cfg.port,
            "imap_security": security_str(cfg.security),
            "username": cfg.username,
            "preset": preset,
        }))
    }

    async fn add_account(
        self: &Arc<Self>,
        params: Value,
        session: &Arc<Session>,
    ) -> Result<Value, RpcError> {
        let p: AddAccount = parse(params)?;
        let email = p.email.trim().to_string();
        if let Some(flow) = p.oauth.clone() {
            return self.add_oauth_account(p, &email, &flow, session).await;
        }
        let (guess, _) = postflow_mail::detect(&email)
            .await
            .ok_or_else(|| RpcError::new("invalid_email", "email address is invalid"))?;
        if p.password.is_empty() {
            return Err(RpcError::new("bad_params", "password is empty"));
        }
        let cfg = ServerConfig {
            host: non_empty(p.imap_host).unwrap_or(guess.host),
            port: p.imap_port.unwrap_or(guess.port),
            security: match p.imap_security {
                Some(s) => parse_security(&s)?,
                None => guess.security,
            },
            username: non_empty(p.username).unwrap_or(guess.username),
        };
        cfg.validate().map_err(|e| mail_error(&e))?;
        // 先确认能登录，再保存，免得留下一个连不上的账户
        let cfg = verify_login(cfg, &email, &p.password).await?;

        let config = AccountConfig {
            email,
            name: p.name.trim().to_string(),
            imap_host: cfg.host.clone(),
            imap_port: cfg.port,
            imap_security: security_str(cfg.security).to_string(),
            username: cfg.username.clone(),
            ..Default::default()
        };
        let account = session.db(move |s| s.put_account(config)).await?;
        tracing::info!(account = %account.id, host = %account.imap_host, "mail account added");
        self.save_password(&account.id, p.password).await?;
        self.start_worker(session, &account.id);
        self.emit_changed(&[Scope::Feeds, Scope::Items], None);
        reply(account)
    }

    async fn update_account(
        self: &Arc<Self>,
        params: Value,
        session: &Arc<Session>,
    ) -> Result<Value, RpcError> {
        let p: UpdateAccount = parse(params)?;
        let id = p.id.clone();
        let account = session
            .db(move |s| s.account(&id))
            .await?
            .ok_or_else(|| RpcError::new("not_found", "no such account"))?;

        // 重新用微软账户登录：用新令牌试一次，成功就换掉原来的密码或令牌
        if let Some(flow) = &p.oauth {
            let (tokens, client) = self.take_oauth_tokens(flow)?;
            let cfg = crate::mail::server_config(&account)?;
            verify(&cfg, &Auth::OAuth2(tokens.access_token.clone())).await?;
            self.save_oauth(&account.id, &client, &tokens).await?;
            if let Some(name) = p.name.filter(|n| *n != account.name) {
                let id = account.id.clone();
                let patch = AccountPatch {
                    name: Some(name),
                    ..Default::default()
                };
                session.db(move |s| s.update_account(&id, patch)).await?;
            }
            tracing::info!(account = %account.id, "mail account signed in again");
            self.start_worker(session, &account.id);
            self.emit_changed(&[Scope::Feeds, Scope::Items], None);
            let id = account.id.clone();
            return reply(session.db(move |s| s.account(&id)).await?);
        }

        let host = non_empty(p.imap_host);
        let mut username = non_empty(p.username);
        let password = p.password.filter(|pw| !pw.is_empty());
        let server_changed = host.as_ref().is_some_and(|h| *h != account.imap_host)
            || p.imap_port.is_some_and(|port| port != account.imap_port)
            || p.imap_security
                .as_ref()
                .is_some_and(|s| *s != account.imap_security)
            || username.as_ref().is_some_and(|u| *u != account.username);

        if server_changed || password.is_some() {
            let cfg = ServerConfig {
                host: host.clone().unwrap_or_else(|| account.imap_host.clone()),
                port: p.imap_port.unwrap_or(account.imap_port),
                security: parse_security(
                    p.imap_security.as_deref().unwrap_or(&account.imap_security),
                )?,
                username: username.clone().unwrap_or_else(|| account.username.clone()),
            };
            cfg.validate().map_err(|e| mail_error(&e))?;
            let pw = match &password {
                Some(pw) => pw.clone(),
                None => self.load_password(&account.id).await.ok_or_else(|| {
                    RpcError::new("mail_no_password", "enter the password for this account")
                })?,
            };
            let used = verify_login(cfg, &account.email, &pw).await?;
            // iCloud 换了一种用户名写法才登录上的，记下能用的那个
            if used.username != account.username {
                username = Some(used.username);
            }
        }

        // 自己填了发信服务器：先试着登录一下，免得发信时才发现填错了
        let smtp_host = p.smtp_host.map(|h| h.trim().to_string());
        let smtp_changed = smtp_host.as_deref().is_some_and(|h| {
            h != account.smtp_host.as_deref().unwrap_or("")
                || (!h.is_empty()
                    && (p
                        .smtp_port
                        .is_some_and(|port| Some(port) != account.smtp_port)
                        || p.smtp_security
                            .as_ref()
                            .is_some_and(|s| Some(s) != account.smtp_security.as_ref())))
        });
        if let Some(host) = smtp_host
            .as_deref()
            .filter(|h| smtp_changed && !h.is_empty())
        {
            let security = parse_security(p.smtp_security.as_deref().unwrap_or("tls"))?;
            let mut changed = account.clone();
            changed.smtp_host = Some(host.to_string());
            changed.smtp_port = Some(p.smtp_port.unwrap_or(465));
            changed.smtp_security = Some(security_str(security).to_string());
            if let Some(name) = &username {
                changed.username = name.clone();
            }
            let cfg = crate::mail::smtp_config(&changed)?;
            let auth = match &password {
                Some(pw) => Some(Auth::Password(pw.clone())),
                None => self.mail_auth(&account.id).await?,
            }
            .ok_or_else(|| {
                RpcError::new("mail_no_password", "enter the password for this account")
            })?;
            postflow_mail::smtp::check(&cfg, &auth)
                .await
                .map_err(|e| crate::compose::send_error(&e, matches!(auth, Auth::OAuth2(_))))?;
        }

        let patch = AccountPatch {
            name: p.name,
            imap_host: host,
            imap_port: p.imap_port,
            imap_security: p.imap_security,
            username,
            smtp_host,
            smtp_port: p.smtp_port,
            smtp_security: p.smtp_security,
            sender_name: p.sender_name,
            signature: p.signature,
        };
        let id = account.id.clone();
        session.db(move |s| s.update_account(&id, patch)).await?;
        let restart = server_changed || password.is_some();
        if let Some(pw) = password {
            self.save_password(&account.id, pw).await?;
        }
        if restart {
            self.start_worker(session, &account.id);
        }
        self.emit_changed(&[Scope::Feeds, Scope::Items], None);
        let id = account.id.clone();
        reply(session.db(move |s| s.account(&id)).await?)
    }

    async fn remove_account(
        self: &Arc<Self>,
        params: Value,
        session: &Arc<Session>,
    ) -> Result<Value, RpcError> {
        let p: Id = parse(params)?;
        tracing::info!(account = %p.id, "mail account removed by the user");
        self.stop_worker(session, &p.id);
        let id = p.id.clone();
        session.db(move |s| s.remove_account(&id)).await?;
        self.forget_oauth(&p.id).await;
        let secrets = self.secrets.clone();
        let _ = tokio::task::spawn_blocking(move || secrets.delete(&p.id)).await;
        self.emit_changed(&[Scope::Feeds, Scope::Items], None);
        reply(json!({}))
    }

    /// 用微软或 Google 账户登录的邮箱：服务器默认是那家的，用户名就是邮箱地址。
    async fn add_oauth_account(
        self: &Arc<Self>,
        p: AddAccount,
        email: &str,
        flow: &str,
        session: &Arc<Session>,
    ) -> Result<Value, RpcError> {
        if !email.contains('@') {
            return Err(RpcError::new("invalid_email", "email address is invalid"));
        }
        let (tokens, client) = self.take_oauth_tokens(flow)?;
        let (default_host, default_port) = imap_server(&client.provider);
        let cfg = ServerConfig {
            host: non_empty(p.imap_host).unwrap_or_else(|| default_host.to_string()),
            port: p.imap_port.unwrap_or(default_port),
            security: match p.imap_security {
                Some(s) => parse_security(&s)?,
                None => postflow_mail::Security::Tls,
            },
            username: non_empty(p.username).unwrap_or_else(|| email.to_string()),
        };
        cfg.validate().map_err(|e| mail_error(&e))?;
        verify(&cfg, &Auth::OAuth2(tokens.access_token.clone())).await?;

        let config = AccountConfig {
            email: email.to_string(),
            name: p.name.trim().to_string(),
            imap_host: cfg.host.clone(),
            imap_port: cfg.port,
            imap_security: security_str(cfg.security).to_string(),
            username: cfg.username.clone(),
            ..Default::default()
        };
        let account = session.db(move |s| s.put_account(config)).await?;
        tracing::info!(account = %account.id, host = %account.imap_host, provider = %client.provider, "mail account added with a sign-in");
        self.save_oauth(&account.id, &client, &tokens).await?;
        self.start_worker(session, &account.id);
        self.emit_changed(&[Scope::Feeds, Scope::Items], None);
        reply(account)
    }

    async fn save_password(&self, account_id: &str, password: String) -> Result<(), RpcError> {
        let secrets = self.secrets.clone();
        let id = account_id.to_string();
        tokio::task::spawn_blocking(move || secrets.set(&id, &password))
            .await
            .map_err(|e| RpcError::new("internal", e.to_string()))?
            .map_err(|e| RpcError::new("keychain", e))
    }

    /// 从服务器取更早的邮件。每个邮箱交给自己的后台任务，等它们都取完。
    async fn load_older(&self, params: Value, session: &Arc<Session>) -> Result<Value, RpcError> {
        use crate::mail::OlderFrom;
        use postflow_mail::SpecialMailbox;
        let p: LoadOlder = parse(params)?;
        let from = match p.folder.as_str() {
            "inbox" => OlderFrom::Inbox,
            "sent" => OlderFrom::Special(SpecialMailbox::Sent),
            "drafts" => OlderFrom::Special(SpecialMailbox::Drafts),
            "junk" => OlderFrom::Special(SpecialMailbox::Junk),
            "trash" => OlderFrom::Special(SpecialMailbox::Trash),
            "folder" => match p
                .mailbox
                .as_deref()
                .map(str::trim)
                .filter(|m| !m.is_empty())
            {
                Some(name) => OlderFrom::Folder(name.to_string()),
                None => return Err(RpcError::new("bad_params", "which folder?")),
            },
            other => {
                return Err(RpcError::new(
                    "bad_params",
                    format!("unknown folder: {other}"),
                ));
            }
        };
        let accounts: Vec<String> = match p.account_id {
            Some(id) => vec![id],
            None => session.workers().keys().cloned().collect(),
        };
        let mut waiting = Vec::new();
        for account in accounts {
            let (tx, rx) = oneshot::channel();
            if self.send_mail(session, &account, Command::LoadOlder(from.clone(), tx)) {
                waiting.push(rx);
            }
        }
        if waiting.is_empty() {
            return Err(RpcError::new(
                "mail_unavailable",
                "this account is not connected",
            ));
        }
        let (mut added, mut more) = (0, false);
        let mut failure = None;
        for rx in waiting {
            match tokio::time::timeout(OLDER_TIMEOUT, rx).await {
                Ok(Ok(Ok((n, m)))) => {
                    added += n;
                    more |= m;
                }
                Ok(Ok(Err(e))) => failure = Some(e),
                Ok(Err(_)) => {
                    failure = Some(RpcError::new(
                        "mail_unavailable",
                        "this account is not connected",
                    ))
                }
                Err(_) => failure = Some(RpcError::new("mail_timeout", "timed out")),
            }
        }
        // 都失败了才报错；有的邮箱取到了就算成功
        match failure {
            Some(e) if added == 0 && !more => Err(e),
            _ => reply(json!({ "added": added, "more": more })),
        }
    }

    /// 把邮件移到同一个邮箱的另一个文件夹。本机先拿掉，后台任务去服务器上移。
    async fn move_mail(&self, params: Value, session: &Arc<Session>) -> Result<Value, RpcError> {
        let p: MoveMail = parse(params)?;
        let target = p.mailbox.trim().to_string();
        if target.is_empty() {
            return Err(RpcError::new("bad_params", "which folder?"));
        }
        let ids = p.ids.clone();
        let refs = session
            .db(move |s| {
                let refs: Vec<_> = ids
                    .iter()
                    .filter_map(|id| s.mail_ref(id).transpose())
                    .collect::<Result<_, _>>()?;
                Ok(refs)
            })
            .await?;
        let account = match refs.first() {
            Some(r) => r.account_id.clone(),
            None => return reply(json!({ "moved": 0 })),
        };
        if refs.iter().any(|r| r.account_id != account) {
            return Err(RpcError::new(
                "bad_params",
                "mail from different mailboxes can't go to one folder",
            ));
        }
        let ids: Vec<String> = refs
            .iter()
            .filter(|r| r.mailbox != target)
            .map(|r| r.id.clone())
            .collect();
        let refs = session.db(move |s| s.take_mail_for_move(&ids)).await?;
        let moved = refs.len();
        if moved > 0 {
            tracing::info!(account = %account, moved, "moving mail to another folder");
            self.queue_write(session, &account, Write::MoveTo { refs, target });
            self.emit_changed(&[Scope::Feeds, Scope::Items], None);
        }
        reply(json!({ "moved": moved }))
    }

    /// 邮箱里的普通文件夹，侧栏里列出来。交给后台任务用已有的连接去列。
    async fn folders(&self, params: Value, session: &Arc<Session>) -> Result<Value, RpcError> {
        let p: AccountId = parse(params)?;
        let (tx, rx) = oneshot::channel();
        let unavailable = || RpcError::new("mail_unavailable", "this account is not connected");
        if !self.send_mail(session, &p.account_id, Command::ListFolders(tx)) {
            return Err(unavailable());
        }
        match tokio::time::timeout(FOLDERS_TIMEOUT, rx).await {
            Ok(Ok(Ok(folders))) => reply(json!({ "folders": folders })),
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(_)) => Err(unavailable()),
            Err(_) => Err(RpcError::new("mail_timeout", "timed out")),
        }
    }

    async fn load_password(&self, account_id: &str) -> Option<String> {
        let secrets = self.secrets.clone();
        let id = account_id.to_string();
        tokio::task::spawn_blocking(move || secrets.get(&id))
            .await
            .ok()
            .flatten()
    }

    /// 邮件原文：本机有就用本机的，没有就让后台任务去服务器取。
    pub(crate) async fn raw_mail(
        &self,
        session: &Arc<Session>,
        id: &str,
    ) -> Result<Vec<u8>, RpcError> {
        let key = id.to_string();
        if let Some(raw) = session.db(move |s| s.mail_body(&key)).await? {
            return Ok(raw);
        }
        let key = id.to_string();
        let r = session
            .db(move |s| s.mail_ref(&key))
            .await?
            .ok_or_else(|| RpcError::new("not_found", "no such message"))?;
        let (tx, rx) = oneshot::channel();
        let account = r.account_id.clone();
        if !self.send_mail(session, &account, Command::FetchBody(r, tx)) {
            return Err(RpcError::new(
                "mail_unavailable",
                "this account is not connected",
            ));
        }
        match tokio::time::timeout(FETCH_BODY_TIMEOUT, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(RpcError::new(
                "mail_unavailable",
                "this account is not connected",
            )),
            Err(_) => Err(RpcError::new("mail_timeout", "timed out")),
        }
    }

    pub(crate) async fn mail_detail(
        &self,
        session: &Arc<Session>,
        item: ItemSummary,
        allow_remote: bool,
    ) -> Result<Value, RpcError> {
        let raw = self.raw_mail(session, &item.id).await?;
        let settings = session.db(|s| s.settings()).await?;
        let trusted = item.sender_address.as_ref().is_some_and(|addr| {
            let addr = addr.to_lowercase();
            settings
                .get(TRUSTED_SENDERS)
                .and_then(Value::as_array)
                .is_some_and(|list| list.iter().any(|v| v.as_str() == Some(addr.as_str())))
        });
        let remote = allow_remote || trusted;
        let rendered = postflow_mail::render(
            &raw,
            RenderOptions {
                allow_remote: remote,
            },
        )
        .ok_or_else(|| RpcError::new("mail_parse", "this message could not be read"))?;
        // 已发送的信设了「没回复就提醒」：阅读区上方说什么时候提醒，可以取消
        let follow_up = if item.sent {
            let id = item.id.clone();
            match session
                .db(move |s| s.mail_ref(&id))
                .await?
                .and_then(|r| r.message_id)
            {
                Some(mid) => session
                    .db(move |s| s.follow_up(&mid))
                    .await?
                    .map(|f| f.until),
                None => None,
            }
        } else {
            None
        };
        let invite = postflow_mail::calendar::invite(&raw);
        let rsvp = if invite.is_some() {
            let id = item.id.clone();
            session.db(move |s| s.rsvp(&id)).await?
        } else {
            None
        };
        let mut value = reply(&item)?;
        value["mail"] = json!({
            "invite": invite,
            "rsvp": rsvp,
            "follow_up": follow_up,
            "body_kind": rendered.kind,
            "body": rendered.body,
            "has_remote_content": rendered.has_remote_content,
            "remote_allowed": remote,
            "sender_trusted": trusted,
            "from": rendered.from,
            "to": rendered.to,
            "cc": rendered.cc,
            "reply_to": rendered.reply_to,
            "attachments": rendered.attachments,
            "unsubscribe": postflow_mail::unsubscribe(&raw),
            // 信里的验证码：阅读区顶上显示，一点就复制
            "code": postflow_mail::summarize(&raw).code,
        });
        Ok(value)
    }

    async fn cancel_follow_up(
        &self,
        params: Value,
        session: &Arc<Session>,
    ) -> Result<Value, RpcError> {
        let p: Id = parse(params)?;
        let message_id = session
            .db(move |s| s.mail_ref(&p.id))
            .await?
            .and_then(|r| r.message_id)
            .ok_or_else(|| RpcError::new("not_found", "no such message"))?;
        session.db(move |s| s.cancel_follow_up(&message_id)).await?;
        self.emit_changed(&[Scope::Items], None);
        reply(json!({}))
    }

    /// 一键退订：替用户向邮件列表发退订请求。不支持一键退订的，界面改为打开网页或写信。
    async fn unsubscribe(&self, params: Value, session: &Arc<Session>) -> Result<Value, RpcError> {
        let p: Id = parse(params)?;
        let raw = self.raw_mail(session, &p.id).await?;
        let info = postflow_mail::unsubscribe(&raw).ok_or_else(|| {
            RpcError::new("unsubscribe_none", "this message has no unsubscribe link")
        })?;
        let url = info.url.filter(|_| info.one_click).ok_or_else(|| {
            RpcError::new("unsubscribe_manual", "this list needs its page or an email")
        })?;
        postflow_mail::unsubscribe_one_click(&url)
            .await
            .map_err(|e| {
                tracing::warn!(item = %p.id, "one-click unsubscribe failed: {e}");
                RpcError::new("unsubscribe_failed", e)
            })?;
        tracing::info!(item = %p.id, "unsubscribed from a mailing list");
        reply(json!({}))
    }

    async fn attachment(&self, params: Value, session: &Arc<Session>) -> Result<Value, RpcError> {
        let mut p: Attachment = parse(params)?;
        let raw = self.raw_mail(session, &p.id).await?;
        // 附件总览里大邮件的附件是从邮件结构认出来的：正文下载下来以后按名字找到真正的那一个
        if p.index >= postflow_store::STRUCTURE_INDEX {
            let (id, index) = (p.id.clone(), p.index);
            let name = session
                .db(move |s| s.structure_attachment_name(&id, index))
                .await?
                .ok_or_else(|| RpcError::new("not_found", "no such attachment"))?;
            p.index = postflow_mail::search_text(&raw)
                .2
                .into_iter()
                .find(|a| a.name == name)
                .map(|a| a.index)
                .ok_or_else(|| RpcError::new("not_found", "no such attachment"))?;
        }
        let (name, mime, bytes) = postflow_mail::attachment(&raw, p.index)
            .ok_or_else(|| RpcError::new("not_found", "no such attachment"))?;
        match p.path {
            Some(path) => {
                tokio::fs::write(&path, &bytes)
                    .await
                    .map_err(|e| RpcError::new("io", e.to_string()))?;
                reply(json!({ "path": path, "name": name, "size": bytes.len() }))
            }
            None => reply(json!({
                "name": name,
                "mime": mime,
                "data": postflow_mail::base64(&bytes),
            })),
        }
    }

    async fn trust_sender(&self, params: Value, session: &Arc<Session>) -> Result<Value, RpcError> {
        let p: Address = parse(params)?;
        let address = p.address.trim().to_lowercase();
        if !address.contains('@') {
            return Err(RpcError::new("bad_params", "address is invalid"));
        }
        let settings = session.db(|s| s.settings()).await?;
        let mut list: Vec<Value> = settings
            .get(TRUSTED_SENDERS)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if !list.iter().any(|v| v.as_str() == Some(address.as_str())) {
            list.push(json!(address));
            session
                .db(move |s| s.set_setting(TRUSTED_SENDERS, Value::Array(list)))
                .await?;
            self.emit_changed(&[Scope::Settings, Scope::Items], None);
        }
        reply(json!({}))
    }
}
