//! OAuth 2.0 登录邮箱。
//!
//! Outlook.com、Hotmail 和 Microsoft 365 已经不接受第三方应用用密码登录 IMAP，只认 OAuth 换来的
//! 访问令牌（IMAP 里叫 XOAUTH2）。这里是桌面应用用的「授权码 + PKCE」流程：
//!
//! 1. 在本机开一个临时端口，生成 PKCE 校验码；
//! 2. 在系统浏览器里打开微软的登录页，用户登录、同意；
//! 3. 微软把浏览器带回 `http://localhost:端口/?code=…`，这里收下授权码；
//! 4. 用授权码换访问令牌和刷新令牌。访问令牌一小时过期，之后用刷新令牌换新的。
//!
//! 没有服务器参与，令牌只在这台设备上。

use std::time::Duration;

use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

/// 微软的登录服务。`common` 同时接受个人账户（Outlook.com）和工作、学校账户（Microsoft 365）。
pub const MICROSOFT_AUTHORITY: &str = "https://login.microsoftonline.com/common";
/// 读写 IMAP 邮箱和用 SMTP 发信的权限，加上 `offline_access` 才会给刷新令牌。
pub const MICROSOFT_SCOPE: &str = "offline_access https://outlook.office.com/IMAP.AccessAsUser.All https://outlook.office.com/SMTP.Send";
/// 加上发信以前登录的邮箱只同意过收信。刷新时要不到发信权限就退回到这个，收信照常。
const MICROSOFT_IMAP_SCOPE: &str =
    "offline_access https://outlook.office.com/IMAP.AccessAsUser.All";

/// 一个 OAuth 服务的地址。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    pub authorize_url: String,
    pub token_url: String,
    pub scope: String,
    /// 刷新时拿不到 `scope` 里全部权限的话，退而求其次要这些。
    pub fallback_scope: Option<String>,
}

impl Provider {
    /// 微软。`authority` 一般是 [`MICROSOFT_AUTHORITY`]，测试时换成本机的模拟服务。
    pub fn microsoft(authority: &str) -> Provider {
        let base = authority.trim_end_matches('/');
        Provider {
            authorize_url: format!("{base}/oauth2/v2.0/authorize"),
            token_url: format!("{base}/oauth2/v2.0/token"),
            scope: MICROSOFT_SCOPE.to_string(),
            fallback_scope: Some(MICROSOFT_IMAP_SCOPE.to_string()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    #[error("the sign-in was cancelled or not allowed: {0}")]
    Denied(String),
    #[error("the sign-in has expired: {0}")]
    Expired(String),
    #[error("sign-in failed: {0}")]
    Failed(String),
    #[error("could not reach the sign-in service: {0}")]
    Network(String),
    #[error("nobody finished signing in in the browser")]
    Timeout,
}

impl OAuthError {
    /// 给界面用的稳定错误码。
    pub fn code(&self) -> &'static str {
        match self {
            OAuthError::Denied(_) => "oauth_denied",
            OAuthError::Expired(_) => "oauth_expired",
            OAuthError::Failed(_) => "oauth_failed",
            OAuthError::Network(_) => "oauth_network",
            OAuthError::Timeout => "oauth_timeout",
        }
    }

    /// 重试也没用，要用户重新登录。
    pub fn needs_user(&self) -> bool {
        matches!(self, OAuthError::Denied(_) | OAuthError::Expired(_))
    }
}

/// base64url，不补 `=`。PKCE 和 state 都用这种写法。
fn base64url(data: &[u8]) -> String {
    crate::base64(data)
        .trim_end_matches('=')
        .replace('+', "-")
        .replace('/', "_")
}

/// PKCE：校验码自己留着，摘要放进登录地址。换令牌时出示校验码，证明是同一个应用发起的。
#[derive(Debug, Clone)]
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    pub fn new() -> Pkce {
        let verifier = base64url(&rand::random::<[u8; 32]>());
        let challenge = base64url(&Sha256::digest(verifier.as_bytes()));
        Pkce {
            verifier,
            challenge,
        }
    }
}

impl Default for Pkce {
    fn default() -> Self {
        Pkce::new()
    }
}

/// 随机的 state，防止别的网页伪造回调。
pub fn random_state() -> String {
    base64url(&rand::random::<[u8; 16]>())
}

/// 在浏览器里打开的登录地址。
pub fn authorize_url(
    provider: &Provider,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    challenge: &str,
    login_hint: &str,
) -> String {
    let mut url = url::Url::parse(&provider.authorize_url)
        .unwrap_or_else(|_| url::Url::parse("https://invalid.invalid/").expect("valid url"));
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_mode", "query")
        .append_pair("scope", &provider.scope)
        .append_pair("state", state)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("login_hint", login_hint)
        // 已经登录着别的微软账户时，让用户选对账户
        .append_pair("prompt", "select_account");
    url.into()
}

/// 换到的令牌。
#[derive(Debug, Clone, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    /// 微软每次刷新都可能给一个新的刷新令牌，要换着存。
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// 访问令牌还有多少秒过期。
    #[serde(default = "default_expiry")]
    pub expires_in: u64,
}

fn default_expiry() -> u64 {
    3600
}

#[derive(Deserialize)]
struct ErrorBody {
    #[serde(default)]
    error: String,
    #[serde(default)]
    error_description: String,
}

/// 换令牌、一键退订用的 HTTP 客户端，整个进程共用一个。
pub(crate) fn http() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(concat!("Postflow/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default()
    })
}

/// 用授权码换令牌。
pub async fn exchange_code(
    provider: &Provider,
    client_id: &str,
    code: &str,
    redirect_uri: &str,
    verifier: &str,
) -> Result<Tokens, OAuthError> {
    token_request(
        provider,
        &[
            ("client_id", client_id),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("code_verifier", verifier),
            ("scope", &provider.scope),
        ],
    )
    .await
}

/// 用刷新令牌换新的访问令牌。
pub async fn refresh(
    provider: &Provider,
    client_id: &str,
    refresh_token: &str,
) -> Result<Tokens, OAuthError> {
    let request = |scope: &str| {
        let form = [
            ("client_id", client_id),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("scope", scope),
        ]
        .map(|(k, v)| (k, v.to_string()));
        async move {
            let form: Vec<(&str, &str)> = form.iter().map(|(k, v)| (*k, v.as_str())).collect();
            token_request(provider, &form).await
        }
    };
    match request(&provider.scope).await {
        Err(err @ (OAuthError::Expired(_) | OAuthError::Denied(_) | OAuthError::Failed(_))) => {
            match &provider.fallback_scope {
                // 早先登录时没同意过新加的权限：用原来的权限刷新，不同意的那部分功能用不了
                Some(fallback) => request(fallback).await.map_err(|_| err),
                None => Err(err),
            }
        }
        other => other,
    }
}

async fn token_request(provider: &Provider, form: &[(&str, &str)]) -> Result<Tokens, OAuthError> {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(form)
        .finish();
    let resp = http()
        .post(&provider.token_url)
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(ACCEPT, "application/json")
        .timeout(Duration::from_secs(30))
        .body(body)
        .send()
        .await
        .map_err(|e| OAuthError::Network(e.to_string()))?;
    let status = resp.status();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| OAuthError::Network(e.to_string()))?;
    if status.is_success() {
        return serde_json::from_slice::<Tokens>(&bytes)
            .map_err(|e| OAuthError::Failed(format!("unexpected answer: {e}")));
    }
    let err: ErrorBody = serde_json::from_slice(&bytes).unwrap_or(ErrorBody {
        error: format!("HTTP {}", status.as_u16()),
        error_description: String::new(),
    });
    // 错误说明里常有一大段跟踪信息，只留第一行
    let detail = err
        .error_description
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    let detail = if detail.is_empty() {
        err.error.clone()
    } else {
        detail
    };
    Err(match err.error.as_str() {
        // 刷新令牌过期、被吊销，或者用户改了密码
        "invalid_grant" | "interaction_required" => OAuthError::Expired(detail),
        "access_denied" | "consent_required" => OAuthError::Denied(detail),
        _ => OAuthError::Failed(detail),
    })
}

/// 本机的临时端口，等浏览器把授权码送回来。
pub struct Redirect {
    /// 登录地址里的回调地址。
    pub uri: String,
    rx: oneshot::Receiver<Result<String, OAuthError>>,
}

impl Redirect {
    /// 在本机随便一个空闲端口上等回调。`state` 对不上的请求不理。
    pub async fn listen(state: String) -> std::io::Result<Redirect> {
        let v4 = TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = v4.local_addr()?.port();
        // 有的系统里 localhost 先解析成 IPv6，同一个端口也在 ::1 上等
        let v6 = TcpListener::bind(("::1", port)).await.ok();
        let (tx, rx) = oneshot::channel();
        tokio::spawn(serve(v4, v6, state, tx));
        Ok(Redirect {
            uri: format!("http://localhost:{port}"),
            rx,
        })
    }

    /// 等用户在浏览器里登录完，拿到授权码。
    pub async fn code(self, timeout: Duration) -> Result<String, OAuthError> {
        match tokio::time::timeout(timeout, self.rx).await {
            Err(_) => Err(OAuthError::Timeout),
            Ok(Err(_)) => Err(OAuthError::Failed("the sign-in listener stopped".into())),
            Ok(Ok(result)) => result,
        }
    }
}

async fn accept(v4: &TcpListener, v6: Option<&TcpListener>) -> std::io::Result<TcpStream> {
    match v6 {
        Some(v6) => tokio::select! {
            r = v4.accept() => r.map(|(s, _)| s),
            r = v6.accept() => r.map(|(s, _)| s),
        },
        None => v4.accept().await.map(|(s, _)| s),
    }
}

/// 回调页面上显示的话。中英文都写，浏览器的语言不一定和应用一致。
fn page(ok: bool) -> String {
    let (zh, en) = if ok {
        (
            "登录完成，可以关掉这个页面回到 Postflow 了。",
            "Signed in. You can close this tab and go back to Postflow.",
        )
    } else {
        (
            "没有登录成功，回到 Postflow 再试一次。",
            "Sign-in didn't complete. Go back to Postflow and try again.",
        )
    };
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Postflow</title>\
         <meta name=\"viewport\" content=\"width=device-width\">\
         <style>body{{font:15px -apple-system,system-ui,sans-serif;display:grid;place-items:center;\
         height:90vh;margin:0;color:#333;background:#f6f6f8}}p{{margin:.4em 0}}</style></head>\
         <body><div><p><b>{zh}</b></p><p>{en}</p></div></body></html>"
    )
}

async fn respond(mut stream: TcpStream, status: &str, body: &str) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes()).await;
    let _ = stream.write_all(body.as_bytes()).await;
    let _ = stream.shutdown().await;
}

/// 从回调请求的第一行里取出参数。
fn query_of(request: &str) -> Option<Vec<(String, String)>> {
    let line = request.lines().next()?;
    let mut parts = line.split_whitespace();
    if parts.next()? != "GET" {
        return None;
    }
    let target = parts.next()?;
    let query = target.split_once('?')?.1;
    Some(
        url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect(),
    )
}

/// 回调里的结果：授权码，或者用户拒绝、出错的说明。参数不全、state 不对时返回 `None`。
fn outcome(params: &[(String, String)], state: &str) -> Option<Result<String, OAuthError>> {
    let get = |k: &str| {
        params
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.as_str())
    };
    if get("state") != Some(state) {
        return None;
    }
    if let Some(code) = get("code").filter(|c| !c.is_empty()) {
        return Some(Ok(code.to_string()));
    }
    let error = get("error")?;
    let detail = get("error_description").unwrap_or(error).to_string();
    Some(Err(if error == "access_denied" {
        OAuthError::Denied(detail)
    } else {
        OAuthError::Failed(detail)
    }))
}

async fn serve(
    v4: TcpListener,
    v6: Option<TcpListener>,
    state: String,
    mut tx: oneshot::Sender<Result<String, OAuthError>>,
) {
    loop {
        let stream = tokio::select! {
            // 没人等了（取消、超时）就收工
            _ = tx.closed() => return,
            s = accept(&v4, v6.as_ref()) => match s {
                Ok(s) => s,
                Err(_) => continue,
            },
        };
        let mut stream = stream;
        let mut buf = vec![0u8; 8192];
        let n = match tokio::time::timeout(Duration::from_secs(10), stream.read(&mut buf)).await {
            Ok(Ok(n)) => n,
            _ => continue,
        };
        let request = String::from_utf8_lossy(&buf[..n]).to_string();
        // 浏览器顺手要的 favicon 之类，以及 state 不对的请求，都不算
        let Some(result) = query_of(&request).and_then(|q| outcome(&q, &state)) else {
            respond(stream, "404 Not Found", "").await;
            continue;
        };
        respond(stream, "200 OK", &page(result.is_ok())).await;
        let _ = tx.send(result);
        return;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_is_the_hash_of_the_verifier() {
        let p = Pkce::new();
        assert_eq!(p.verifier.len(), 43);
        assert!(
            p.verifier
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        );
        assert_eq!(
            p.challenge,
            base64url(&Sha256::digest(p.verifier.as_bytes()))
        );
        assert_ne!(Pkce::new().verifier, p.verifier);
        // RFC 7636 附录 B 的例子
        assert_eq!(
            base64url(&Sha256::digest(
                b"dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
            )),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn builds_the_sign_in_address() {
        let p = Provider::microsoft("https://login.example.com/common/");
        assert_eq!(
            p.token_url,
            "https://login.example.com/common/oauth2/v2.0/token"
        );
        let url = authorize_url(
            &p,
            "client",
            "http://localhost:4000",
            "st",
            "ch",
            "me@example.com",
        );
        let parsed = url::Url::parse(&url).unwrap();
        assert_eq!(parsed.path(), "/common/oauth2/v2.0/authorize");
        let get = |k: &str| {
            parsed
                .query_pairs()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.into_owned())
        };
        assert_eq!(
            get("redirect_uri").as_deref(),
            Some("http://localhost:4000")
        );
        assert_eq!(get("code_challenge_method").as_deref(), Some("S256"));
        assert_eq!(get("scope").as_deref(), Some(MICROSOFT_SCOPE));
        assert_eq!(get("login_hint").as_deref(), Some("me@example.com"));
    }

    #[test]
    fn reads_the_callback() {
        let q = query_of("GET /?code=abc&state=xyz HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        assert_eq!(outcome(&q, "xyz").unwrap().unwrap(), "abc");
        assert!(outcome(&q, "other").is_none());
        let denied =
            query_of("GET /?error=access_denied&error_description=no&state=s HTTP/1.1\r\n")
                .unwrap();
        assert!(matches!(
            outcome(&denied, "s"),
            Some(Err(OAuthError::Denied(_)))
        ));
        assert!(query_of("GET /favicon.ico HTTP/1.1\r\n").is_none());
    }

    #[tokio::test]
    async fn listener_delivers_the_code() {
        let redirect = Redirect::listen("st".into()).await.unwrap();
        let port: u16 = redirect.uri.rsplit(':').next().unwrap().parse().unwrap();
        let client = tokio::spawn(async move {
            let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
            s.write_all(b"GET /favicon.ico HTTP/1.1\r\n\r\n")
                .await
                .unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).await.unwrap();
            assert!(out.starts_with("HTTP/1.1 404"));
            let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
            s.write_all(b"GET /?code=the-code&state=st HTTP/1.1\r\n\r\n")
                .await
                .unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).await.unwrap();
            assert!(out.contains("Postflow"));
        });
        let code = redirect.code(Duration::from_secs(5)).await.unwrap();
        assert_eq!(code, "the-code");
        client.await.unwrap();
    }
}
