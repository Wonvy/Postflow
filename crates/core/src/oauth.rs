//! 用 OAuth 登录的邮箱：Outlook.com、Hotmail、Microsoft 365，以及 Gmail 和 Google Workspace。
//!
//! 登录在系统浏览器里完成（流程见 `shibapost_mail::oauth`）。换来的刷新令牌和密码一样只存在
//! 这台设备的钥匙串里，单独一条，名字是「账户 ID#oauth」；访问令牌只放在内存里，快过期时用
//! 刷新令牌换新的。账户本身会同步到其他设备，但不记登录方式：界面按服务商判断，
//! Outlook 和 Microsoft 365 一律走 OAuth，别的设备上同样要登录一次。
//!
//! 登录要用一个在微软或 Google 注册的应用。ShibaPost 不自带，由用户自己注册后填进设置
//! （`mail.microsoft_client_id`；Google 还要 `mail.google_client_secret`，会同步到其他设备）；
//! 开发时也可以用环境变量 `SHIBAPOST_MICROSOFT_CLIENT_ID`、`SHIBAPOST_GOOGLE_CLIENT_ID`、`SHIBAPOST_GOOGLE_CLIENT_SECRET`。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use shibapost_mail::Auth;
use shibapost_mail::oauth::{self, OAuthError, Pkce, Provider, Redirect, Tokens};

use crate::rpc::{parse, reply};
use crate::{Core, RpcError, Session};

/// 用户自己注册的应用 ID 存在这些设置里。
const CLIENT_ID_SETTING: &str = "mail.microsoft_client_id";
const GOOGLE_ID_SETTING: &str = "mail.google_client_id";
const GOOGLE_SECRET_SETTING: &str = "mail.google_client_secret";
/// 在浏览器里登录最多等这么久。
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// 访问令牌离过期不到这么久就提前换。
const REFRESH_MARGIN: Duration = Duration::from_secs(5 * 60);

/// 各家的 IMAP 服务器，用 OAuth 添加邮箱时默认用它。
pub(crate) fn imap_server(provider: &str) -> (&'static str, u16) {
    match provider {
        "google" => ("imap.gmail.com", 993),
        _ => ("outlook.office365.com", 993),
    }
}

fn env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 登录服务的地址。测试和演示时用 `SHIBAPOST_MICROSOFT_AUTHORITY`、`SHIBAPOST_GOOGLE_AUTHORITY`
/// 换成本机的模拟服务。
fn provider(name: &str) -> Provider {
    match name {
        "google" => match env("SHIBAPOST_GOOGLE_AUTHORITY") {
            Some(base) => {
                let base = base.trim_end_matches('/');
                Provider::google(
                    &format!("{base}/o/oauth2/v2/auth"),
                    &format!("{base}/token"),
                )
            }
            None => Provider::google(oauth::GOOGLE_AUTHORIZE_URL, oauth::GOOGLE_TOKEN_URL),
        },
        _ => Provider::microsoft(
            env("SHIBAPOST_MICROSOFT_AUTHORITY")
                .as_deref()
                .unwrap_or(oauth::MICROSOFT_AUTHORITY),
        ),
    }
}

/// 用哪个应用登录：哪家、应用 ID，Google 还有客户端密钥。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OAuthClient {
    pub provider: String,
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
}

impl OAuthClient {
    fn as_client(&self) -> oauth::Client<'_> {
        oauth::Client {
            id: &self.client_id,
            secret: self.client_secret.as_deref(),
        }
    }
}

/// 一次正在进行的登录。
struct Flow {
    client: OAuthClient,
    pkce: Pkce,
    redirect: Option<Redirect>,
    redirect_uri: String,
    tokens: Option<Tokens>,
    started: Instant,
    /// 正在等浏览器回调时，丢掉它就能让等待马上结束（用户点了取消）。
    cancel: Option<tokio::sync::oneshot::Sender<()>>,
}

fn flows() -> &'static Mutex<HashMap<String, Flow>> {
    static FLOWS: OnceLock<Mutex<HashMap<String, Flow>>> = OnceLock::new();
    FLOWS.get_or_init(Default::default)
}

/// 内存里的访问令牌，按账户。
fn access_tokens() -> &'static Mutex<HashMap<String, (String, Instant)>> {
    static TOKENS: OnceLock<Mutex<HashMap<String, (String, Instant)>>> = OnceLock::new();
    TOKENS.get_or_init(Default::default)
}

/// 钥匙串里存的东西。记下用的是哪家、哪个应用：刷新令牌只能由换出它的那个应用使用。
#[derive(Serialize, Deserialize)]
struct Credential {
    #[serde(flatten)]
    client: OAuthClient,
    refresh_token: String,
}

fn credential_key(account_id: &str) -> String {
    format!("{account_id}#oauth")
}

pub(crate) fn oauth_error(e: &OAuthError) -> RpcError {
    RpcError::new(e.code(), e.to_string())
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Deserialize)]
struct Start {
    email: String,
    /// microsoft（默认）或 google。
    #[serde(default)]
    provider: Option<String>,
}

#[derive(Deserialize)]
struct AccountRef {
    id: String,
}

#[derive(Deserialize)]
struct FlowId {
    flow_id: String,
}

impl Core {
    /// 用哪个应用登录这家：环境变量优先，其次是用户填进设置的。Google 要 ID 和密钥都有。
    async fn oauth_client(&self, session: &Arc<Session>, name: &str) -> Option<OAuthClient> {
        let settings = session.db(|s| s.settings()).await.ok();
        let setting = |key: &str| {
            settings
                .as_ref()?
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        match name {
            "google" => Some(OAuthClient {
                provider: "google".into(),
                client_id: env("SHIBAPOST_GOOGLE_CLIENT_ID")
                    .or_else(|| setting(GOOGLE_ID_SETTING))?,
                client_secret: Some(
                    env("SHIBAPOST_GOOGLE_CLIENT_SECRET")
                        .or_else(|| setting(GOOGLE_SECRET_SETTING))?,
                ),
            }),
            _ => Some(OAuthClient {
                provider: "microsoft".into(),
                client_id: env("SHIBAPOST_MICROSOFT_CLIENT_ID")
                    .or_else(|| setting(CLIENT_ID_SETTING))?,
                client_secret: None,
            }),
        }
    }

    pub(crate) async fn oauth_rpc(
        self: &Arc<Self>,
        method: &str,
        params: Value,
        session: &Arc<Session>,
    ) -> Option<Result<Value, RpcError>> {
        Some(match method {
            // 能不能用微软、Google 账户登录：要有注册好的应用
            "accounts.oauth_config" => {
                let microsoft = self.oauth_client(session, "microsoft").await.is_some();
                let google = self.oauth_client(session, "google").await.is_some();
                reply(json!({ "microsoft": microsoft, "google": google }))
            }
            "accounts.oauth_start" => self.oauth_start(params, session).await,
            // 这个邮箱在这台设备上怎么登录：密码，还是哪家的账户
            "accounts.sign_in_method" => match parse::<AccountRef>(params) {
                Ok(p) => {
                    let secrets = self.secrets.clone();
                    let id = p.id;
                    let found = tokio::task::spawn_blocking(move || {
                        (secrets.get(&credential_key(&id)), secrets.get(&id))
                    })
                    .await;
                    match found {
                        Ok((Some(credential), _)) => {
                            let provider = serde_json::from_str::<Credential>(&credential)
                                .map(|c| c.client.provider)
                                .unwrap_or_else(|_| "microsoft".into());
                            reply(json!({ "method": provider }))
                        }
                        Ok((None, Some(_))) => reply(json!({ "method": "password" })),
                        Ok((None, None)) => reply(json!({ "method": "none" })),
                        Err(e) => Err(RpcError::new("internal", e.to_string())),
                    }
                }
                Err(e) => Err(e),
            },
            "accounts.oauth_wait" => self.oauth_wait(params).await,
            "accounts.oauth_cancel" => match parse::<FlowId>(params) {
                Ok(p) => {
                    lock(flows()).remove(&p.flow_id);
                    reply(json!({}))
                }
                Err(e) => Err(e),
            },
            _ => return None,
        })
    }

    /// 开始登录：开好本机的回调端口，返回要在浏览器里打开的地址。
    async fn oauth_start(&self, params: Value, session: &Arc<Session>) -> Result<Value, RpcError> {
        let p: Start = parse(params)?;
        let name = match p.provider.as_deref() {
            Some("google") => "google",
            _ => "microsoft",
        };
        let client = self.oauth_client(session, name).await.ok_or_else(|| {
            RpcError::new(
                "oauth_no_client",
                format!("no {name} application is set up for signing in"),
            )
        })?;
        let state = oauth::random_state();
        let redirect = Redirect::listen(state.clone())
            .await
            .map_err(|e| RpcError::new("oauth_failed", e.to_string()))?;
        let pkce = Pkce::new();
        let url = oauth::authorize_url(
            &provider(name),
            &client.client_id,
            &redirect.uri,
            &state,
            &pkce.challenge,
            p.email.trim(),
        );
        let flow_id = oauth::random_state();
        let mut all = lock(flows());
        // 顺手清掉早就没人等的
        all.retain(|_, f| f.started.elapsed() < SIGN_IN_TIMEOUT * 2);
        all.insert(
            flow_id.clone(),
            Flow {
                client,
                pkce,
                redirect_uri: redirect.uri.clone(),
                redirect: Some(redirect),
                tokens: None,
                started: Instant::now(),
                cancel: None,
            },
        );
        tracing::info!(provider = name, "sign-in started");
        reply(json!({ "flow_id": flow_id, "url": url }))
    }

    /// 等用户在浏览器里登录完，用授权码换令牌。令牌先留在这次登录里，添加或更新账户时取走。
    async fn oauth_wait(&self, params: Value) -> Result<Value, RpcError> {
        let p: FlowId = parse(params)?;
        let (cancel_tx, cancelled) = tokio::sync::oneshot::channel::<()>();
        let (redirect, client, verifier, redirect_uri) = {
            let mut all = lock(flows());
            let flow = all
                .get_mut(&p.flow_id)
                .ok_or_else(|| RpcError::new("oauth_failed", "this sign-in is no longer active"))?;
            let redirect = flow
                .redirect
                .take()
                .ok_or_else(|| RpcError::new("oauth_failed", "already waiting for this sign-in"))?;
            flow.cancel = Some(cancel_tx);
            (
                redirect,
                flow.client.clone(),
                flow.pkce.verifier.clone(),
                flow.redirect_uri.clone(),
            )
        };
        let signed_in = async {
            let code = redirect.code(SIGN_IN_TIMEOUT).await?;
            oauth::exchange_code(
                &provider(&client.provider),
                client.as_client(),
                &code,
                &redirect_uri,
                &verifier,
            )
            .await
        };
        let result = tokio::select! {
            result = signed_in => result,
            // 取消时登录被移走，发送端随之丢掉；回调端口也跟着关掉
            _ = cancelled => return Err(RpcError::new("oauth_cancelled", "the sign-in was cancelled")),
        };
        let mut all = lock(flows());
        match result {
            Ok(tokens) => {
                if tokens.refresh_token.is_none() {
                    all.remove(&p.flow_id);
                    return Err(RpcError::new(
                        "oauth_failed",
                        "the sign-in didn't grant offline access",
                    ));
                }
                tracing::info!(provider = %client.provider, "sign-in completed");
                if let Some(flow) = all.get_mut(&p.flow_id) {
                    flow.tokens = Some(tokens);
                }
                reply(json!({}))
            }
            Err(e) => {
                tracing::warn!(provider = %client.provider, code = e.code(), "sign-in failed: {e}");
                all.remove(&p.flow_id);
                Err(oauth_error(&e))
            }
        }
    }

    /// 取走一次登录换来的令牌，以及换令牌用的应用。
    pub(crate) fn take_oauth_tokens(
        &self,
        flow_id: &str,
    ) -> Result<(Tokens, OAuthClient), RpcError> {
        let flow = lock(flows())
            .remove(flow_id)
            .ok_or_else(|| RpcError::new("oauth_failed", "this sign-in is no longer active"))?;
        let tokens = flow
            .tokens
            .ok_or_else(|| RpcError::new("oauth_failed", "the sign-in hasn't finished"))?;
        Ok((tokens, flow.client))
    }

    /// 账户登录成功后存下刷新令牌，删掉可能还在的旧密码。
    pub(crate) async fn save_oauth(
        &self,
        account_id: &str,
        client: &OAuthClient,
        tokens: &Tokens,
    ) -> Result<(), RpcError> {
        let refresh_token = tokens
            .refresh_token
            .clone()
            .ok_or_else(|| RpcError::new("oauth_failed", "no refresh token"))?;
        let credential = serde_json::to_string(&Credential {
            client: client.clone(),
            refresh_token,
        })
        .map_err(|e| RpcError::new("internal", e.to_string()))?;
        let secrets = self.secrets.clone();
        let id = account_id.to_string();
        tokio::task::spawn_blocking(move || {
            secrets.delete(&id);
            secrets.set(&credential_key(&id), &credential)
        })
        .await
        .map_err(|e| RpcError::new("internal", e.to_string()))?
        .map_err(|e| RpcError::new("keychain", e))?;
        lock(access_tokens()).insert(
            account_id.to_string(),
            (
                tokens.access_token.clone(),
                Instant::now() + Duration::from_secs(tokens.expires_in),
            ),
        );
        Ok(())
    }

    /// 移除账户时连令牌一起删掉。
    pub(crate) async fn forget_oauth(&self, account_id: &str) {
        lock(access_tokens()).remove(account_id);
        let secrets = self.secrets.clone();
        let key = credential_key(account_id);
        let _ = tokio::task::spawn_blocking(move || secrets.delete(&key)).await;
    }

    /// 服务器拒绝了访问令牌：丢掉内存里的，下次重新换。
    pub(crate) fn drop_access_token(&self, account_id: &str) {
        lock(access_tokens()).remove(account_id);
    }

    /// 这个账户在这台设备上怎么登录：OAuth 令牌、密码，或者都没有。
    pub(crate) async fn mail_auth(&self, account_id: &str) -> Result<Option<Auth>, RpcError> {
        let secrets = self.secrets.clone();
        let id = account_id.to_string();
        let (credential, password) = tokio::task::spawn_blocking(move || {
            (secrets.get(&credential_key(&id)), secrets.get(&id))
        })
        .await
        .map_err(|e| RpcError::new("internal", e.to_string()))?;

        let Some(credential) = credential else {
            return Ok(password.map(Auth::Password));
        };
        let credential: Credential = serde_json::from_str(&credential)
            .map_err(|_| RpcError::new("oauth_expired", "the saved sign-in can't be read"))?;

        if let Some((token, expires)) = lock(access_tokens()).get(account_id)
            && *expires > Instant::now() + REFRESH_MARGIN
        {
            return Ok(Some(Auth::OAuth2(token.clone())));
        }
        let client = &credential.client;
        let tokens = oauth::refresh(
            &provider(&client.provider),
            client.as_client(),
            &credential.refresh_token,
        )
        .await
            .map_err(|e| {
                tracing::warn!(account = %account_id, code = e.code(), "refreshing the sign-in failed: {e}");
                oauth_error(&e)
            })?;
        // 微软会换一个新的刷新令牌给我们，旧的过一阵就失效；Google 一般不换
        if tokens.refresh_token.is_some() {
            self.save_oauth(account_id, &credential.client, &tokens)
                .await?;
        } else {
            lock(access_tokens()).insert(
                account_id.to_string(),
                (
                    tokens.access_token.clone(),
                    Instant::now() + Duration::from_secs(tokens.expires_in),
                ),
            );
        }
        Ok(Some(Auth::OAuth2(tokens.access_token)))
    }
}

/// 换令牌失败时要不要等用户重新登录。网络问题过一会儿自己重试。
pub(crate) fn needs_sign_in(code: &str) -> bool {
    matches!(code, "oauth_expired" | "oauth_denied" | "oauth_failed")
}
