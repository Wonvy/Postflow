//! 和邮箱有关的接口方法。

use std::sync::Arc;
use std::time::Duration;

use postflow_mail::{RenderOptions, ServerConfig};
use postflow_store::{AccountConfig, AccountPatch, ItemSummary};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::oneshot;

use crate::mail::{Command, mail_error, parse_security, security_str, verify};
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
    password: String,
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
    imap_host: Option<String>,
    imap_port: Option<u16>,
    imap_security: Option<String>,
    username: Option<String>,
}

#[derive(Deserialize)]
struct Id {
    id: String,
}

#[derive(Deserialize)]
struct OptionalId {
    id: Option<String>,
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
            "accounts.sync" => match parse::<OptionalId>(params) {
                Ok(OptionalId { id: Some(id) }) => {
                    self.send_mail(session, &id, Command::Sync);
                    reply(json!({}))
                }
                Ok(OptionalId { id: None }) => {
                    self.sync_all_mail(session);
                    reply(json!({}))
                }
                Err(e) => Err(e),
            },
            "mail.attachment" => self.attachment(params, session).await,
            "mail.trust_sender" => self.trust_sender(params, session).await,
            _ => return None,
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
        verify(&cfg, &p.password).await?;

        let config = AccountConfig {
            email,
            name: p.name.trim().to_string(),
            imap_host: cfg.host.clone(),
            imap_port: cfg.port,
            imap_security: security_str(cfg.security).to_string(),
            username: cfg.username.clone(),
        };
        let account = session.db(move |s| s.put_account(config)).await?;
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

        let host = non_empty(p.imap_host);
        let username = non_empty(p.username);
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
            verify(&cfg, &pw).await?;
        }

        let patch = AccountPatch {
            name: p.name,
            imap_host: host,
            imap_port: p.imap_port,
            imap_security: p.imap_security,
            username,
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
        self.stop_worker(session, &p.id);
        let id = p.id.clone();
        session.db(move |s| s.remove_account(&id)).await?;
        let secrets = self.secrets.clone();
        let _ = tokio::task::spawn_blocking(move || secrets.delete(&p.id)).await;
        self.emit_changed(&[Scope::Feeds, Scope::Items], None);
        reply(json!({}))
    }

    async fn save_password(&self, account_id: &str, password: String) -> Result<(), RpcError> {
        let secrets = self.secrets.clone();
        let id = account_id.to_string();
        tokio::task::spawn_blocking(move || secrets.set(&id, &password))
            .await
            .map_err(|e| RpcError::new("internal", e.to_string()))?
            .map_err(|e| RpcError::new("keychain", e))
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
        let mut value = reply(&item)?;
        value["mail"] = json!({
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
        });
        Ok(value)
    }

    async fn attachment(&self, params: Value, session: &Arc<Session>) -> Result<Value, RpcError> {
        let p: Attachment = parse(params)?;
        let raw = self.raw_mail(session, &p.id).await?;
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
