//! OpenAI 兼容接口的客户端：对话补全（一次返回或流式返回）和模型列表。
//!
//! DeepSeek、通义千问、Gemini、OpenAI、Kimi、Ollama 等都提供这套接口，只是地址不同。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// 一个配置好的 AI 服务。存在同步的设置里，不含密钥：密钥在每台设备的系统钥匙串里。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provider {
    #[serde(default)]
    pub id: String,
    /// 来自哪个预设，自定义的是 `custom`。
    #[serde(default)]
    pub preset: String,
    #[serde(default)]
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    /// 只给这个服务用的 HTTP 代理，例如 `http://127.0.0.1:7890`。不填就跟随系统代理。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<String>,
}

impl Provider {
    /// 服务运行在这台电脑上（Ollama、LM Studio），内容不会离开本机。
    pub fn is_local(&self) -> bool {
        let rest = self
            .base_url
            .trim()
            .split_once("://")
            .map_or(self.base_url.as_str(), |(_, r)| r);
        let host = if let Some(v6) = rest.strip_prefix('[') {
            v6.split(']').next().unwrap_or("")
        } else {
            rest.split(['/', ':']).next().unwrap_or("")
        };
        let host = host.to_ascii_lowercase();
        host == "localhost" || host == "127.0.0.1" || host == "::1" || host.ends_with(".localhost")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Message {
        Message {
            role: Role::System,
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Message {
        Message {
            role: Role::User,
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Message {
        Message {
            role: Role::Assistant,
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Request {
    pub messages: Vec<Message>,
    /// 要求输出 JSON。不支持这个参数的服务会自动退回普通输出。
    pub json: bool,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
}

#[derive(Debug, Clone)]
pub struct Reply {
    pub text: String,
    pub usage: Usage,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the service rejected the API key: {0}")]
    Auth(String),
    #[error("the account has run out of balance or quota: {0}")]
    Balance(String),
    #[error("too many requests: {0}")]
    RateLimited(String),
    #[error("model not found: {0}")]
    NotFound(String),
    #[error("the text is too long for this model: {0}")]
    TooLong(String),
    #[error("the service is not available in this region: {0}")]
    Region(String),
    #[error("the service returned {0}: {1}")]
    Server(u16, String),
    #[error("cannot reach the service: {0}")]
    Network(String),
    #[error("the service did not answer in time")]
    Timeout,
    #[error("unexpected answer from the service: {0}")]
    BadResponse(String),
    #[error("no model has been chosen")]
    NoModel,
    #[error("invalid proxy address: {0}")]
    Proxy(String),
}

impl Error {
    /// 稳定的错误码，界面据此显示本地化的提示。
    pub fn code(&self) -> &'static str {
        match self {
            Error::Auth(_) => "ai_auth",
            Error::Balance(_) => "ai_balance",
            Error::RateLimited(_) => "ai_rate_limited",
            Error::NotFound(_) => "ai_model_not_found",
            Error::TooLong(_) => "ai_too_long",
            Error::Region(_) => "ai_region",
            Error::Server(..) => "ai_server",
            Error::Network(_) => "ai_network",
            Error::Timeout => "ai_timeout",
            Error::BadResponse(_) => "ai_bad_response",
            Error::NoModel => "ai_no_model",
            Error::Proxy(_) => "ai_proxy",
        }
    }

    fn from_reqwest(e: reqwest::Error) -> Error {
        if e.is_timeout() {
            Error::Timeout
        } else {
            Error::Network(e.to_string())
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// 把服务返回的错误归类。各家的格式不尽相同，按状态码和关键字判断。
fn classify_error(status: u16, body: &str) -> Error {
    let message = server_message(body);
    let lower = message.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    if has(&[
        "location is not supported",
        "unsupported_country",
        "not available in your",
    ]) {
        return Error::Region(message);
    }
    if status == 402 || has(&["insufficient", "balance", "quota", "billing", "arrearage"]) {
        return Error::Balance(message);
    }
    match status {
        401 | 403 => Error::Auth(message),
        404 => Error::NotFound(message),
        429 => Error::RateLimited(message),
        400 | 413
            if has(&[
                "context",
                "too long",
                "maximum",
                "too many tokens",
                "length",
            ]) =>
        {
            Error::TooLong(message)
        }
        _ => Error::Server(status, message),
    }
}

/// 从错误响应里取出给人看的那句话。
fn server_message(body: &str) -> String {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let value = parsed.as_ref().map(|v| match v {
        Value::Array(list) => list.first().cloned().unwrap_or(Value::Null),
        other => other.clone(),
    });
    let message = value.as_ref().and_then(|v| {
        v.pointer("/error/message")
            .or_else(|| v.get("message"))
            .or_else(|| v.get("error"))
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    let message = message.unwrap_or_else(|| body.trim().to_string());
    message.chars().take(400).collect()
}

pub struct Client {
    /// 每种代理设置一个 HTTP 客户端，连接可以复用。
    clients: Mutex<HashMap<String, reqwest::Client>>,
}

impl Default for Client {
    fn default() -> Self {
        Client::new()
    }
}

impl Client {
    pub fn new() -> Client {
        Client {
            clients: Mutex::new(HashMap::new()),
        }
    }

    fn http(&self, provider: &Provider) -> Result<reqwest::Client> {
        let proxy = provider
            .proxy
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty());
        let key = match (provider.is_local(), proxy) {
            (true, _) => "local".to_string(),
            (false, Some(p)) => format!("proxy:{p}"),
            (false, None) => "system".to_string(),
        };
        let mut clients = self.clients.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(client) = clients.get(&key) {
            return Ok(client.clone());
        }
        let mut builder = reqwest::Client::builder()
            .user_agent(concat!("Postflow/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            // 流式输出时两段之间可能隔得比较久，长思考的模型尤其如此
            .read_timeout(Duration::from_secs(180));
        if provider.is_local() {
            // 本机服务不走代理
            builder = builder.no_proxy();
        } else if let Some(p) = proxy {
            builder =
                builder.proxy(reqwest::Proxy::all(p).map_err(|e| Error::Proxy(e.to_string()))?);
        }
        let client = builder.build().map_err(|e| Error::Network(e.to_string()))?;
        clients.insert(key, client.clone());
        Ok(client)
    }

    fn url(provider: &Provider, path: &str) -> String {
        format!("{}/{path}", provider.base_url.trim().trim_end_matches('/'))
    }

    /// 服务提供的模型。按名称排序。
    pub async fn models(&self, provider: &Provider, key: Option<&str>) -> Result<Vec<String>> {
        let mut request = self.http(provider)?.get(Self::url(provider, "models"));
        if let Some(key) = key.filter(|k| !k.is_empty()) {
            request = request.bearer_auth(key);
        }
        let response = request
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(Error::from_reqwest)?;
        let status = response.status().as_u16();
        let body = response.text().await.map_err(Error::from_reqwest)?;
        if status >= 400 {
            return Err(classify_error(status, &body));
        }
        let value: Value =
            serde_json::from_str(&body).map_err(|e| Error::BadResponse(e.to_string()))?;
        let list = value
            .get("data")
            .or_else(|| value.get("models"))
            .and_then(Value::as_array)
            .ok_or_else(|| Error::BadResponse("no model list".into()))?;
        let mut names: Vec<String> = list
            .iter()
            .filter_map(|m| {
                m.get("id")
                    .or_else(|| m.get("name"))
                    .and_then(Value::as_str)
            })
            // Gemini 的列表带 `models/` 前缀，对话接口用不带前缀的名字
            .map(|n| n.trim_start_matches("models/").to_string())
            .collect();
        names.sort();
        names.dedup();
        Ok(names)
    }

    fn body(provider: &Provider, request: &Request, stream: bool, json_mode: bool) -> Value {
        let mut body = json!({
            "model": provider.model.trim(),
            "messages": request.messages,
            "stream": stream,
        });
        if let Some(max) = request.max_tokens {
            body["max_tokens"] = json!(max);
        }
        if let Some(t) = request.temperature {
            body["temperature"] = json!(t);
        }
        if json_mode {
            body["response_format"] = json!({ "type": "json_object" });
        }
        body
    }

    async fn send(
        &self,
        provider: &Provider,
        key: Option<&str>,
        request: &Request,
        stream: bool,
    ) -> Result<reqwest::Response> {
        if provider.model.trim().is_empty() {
            return Err(Error::NoModel);
        }
        let http = self.http(provider)?;
        let mut json_mode = request.json;
        loop {
            let mut builder = http
                .post(Self::url(provider, "chat/completions"))
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(Self::body(provider, request, stream, json_mode).to_string());
            if let Some(key) = key.filter(|k| !k.is_empty()) {
                builder = builder.bearer_auth(key);
            }
            if !stream {
                builder = builder.timeout(Duration::from_secs(300));
            }
            let response = builder.send().await.map_err(Error::from_reqwest)?;
            let status = response.status().as_u16();
            if status < 400 {
                return Ok(response);
            }
            let body = response.text().await.unwrap_or_default();
            // 不认识 JSON 输出参数的服务，去掉它再试一次。提示词里本来就要求了 JSON
            if json_mode && status == 400 && body.contains("response_format") {
                json_mode = false;
                continue;
            }
            return Err(classify_error(status, &body));
        }
    }

    /// 一次拿到完整回答。
    pub async fn complete(
        &self,
        provider: &Provider,
        key: Option<&str>,
        request: &Request,
    ) -> Result<Reply> {
        let response = self.send(provider, key, request, false).await?;
        let body = response.text().await.map_err(Error::from_reqwest)?;
        let value: Value =
            serde_json::from_str(&body).map_err(|e| Error::BadResponse(e.to_string()))?;
        let text = value
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::BadResponse(server_message(&body)))?
            .to_string();
        Ok(Reply {
            text,
            usage: usage_of(&value),
        })
    }

    /// 边生成边返回。每收到一段文字调用一次 `on_delta`，最后返回完整回答。
    pub async fn stream(
        &self,
        provider: &Provider,
        key: Option<&str>,
        request: &Request,
        mut on_delta: impl FnMut(&str),
    ) -> Result<Reply> {
        let mut response = self.send(provider, key, request, true).await?;
        // 有的服务忽略 stream 参数，直接给完整的 JSON
        let is_json = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|t| t.contains("application/json"));
        if is_json {
            let body = response.text().await.map_err(Error::from_reqwest)?;
            let value: Value =
                serde_json::from_str(&body).map_err(|e| Error::BadResponse(e.to_string()))?;
            let text = value
                .pointer("/choices/0/message/content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            on_delta(&text);
            return Ok(Reply {
                text,
                usage: usage_of(&value),
            });
        }

        let mut buffer: Vec<u8> = Vec::new();
        let mut text = String::new();
        let mut usage = Usage::default();
        'read: while let Some(chunk) = response.chunk().await.map_err(Error::from_reqwest)? {
            buffer.extend_from_slice(&chunk);
            while let Some(end) = buffer.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = buffer.drain(..=end).collect();
                let line = String::from_utf8_lossy(&line);
                let Some(data) = line.trim().strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim();
                if data == "[DONE]" {
                    break 'read;
                }
                let Ok(value) = serde_json::from_str::<Value>(data) else {
                    continue;
                };
                if value.get("error").is_some() {
                    return Err(classify_error(500, data));
                }
                if let Some(delta) = value
                    .pointer("/choices/0/delta/content")
                    .and_then(Value::as_str)
                    && !delta.is_empty()
                {
                    text.push_str(delta);
                    on_delta(delta);
                }
                let u = usage_of(&value);
                if u != Usage::default() {
                    usage = u;
                }
            }
        }
        Ok(Reply { text, usage })
    }
}

fn usage_of(value: &Value) -> Usage {
    let get = |key: &str| {
        value
            .pointer(&format!("/usage/{key}"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    Usage {
        input: get("prompt_tokens"),
        output: get("completion_tokens"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(url: &str) -> Provider {
        Provider {
            id: "p".into(),
            preset: "custom".into(),
            name: "P".into(),
            base_url: url.into(),
            model: "m".into(),
            proxy: None,
        }
    }

    #[test]
    fn recognizes_local_services() {
        assert!(provider("http://localhost:11434/v1").is_local());
        assert!(provider("http://127.0.0.1:1234/v1").is_local());
        assert!(provider("http://[::1]:8080").is_local());
        assert!(!provider("https://api.deepseek.com").is_local());
        assert!(!provider("https://localhost.evil.example").is_local());
    }

    #[test]
    fn sorts_out_errors() {
        assert_eq!(
            classify_error(401, r#"{"error":{"message":"Incorrect API key"}}"#).code(),
            "ai_auth"
        );
        assert_eq!(
            classify_error(402, r#"{"error":{"message":"Insufficient Balance"}}"#).code(),
            "ai_balance"
        );
        assert_eq!(
            classify_error(429, r#"{"error":{"message":"You exceeded your current quota","code":"insufficient_quota"}}"#).code(),
            "ai_balance"
        );
        assert_eq!(
            classify_error(429, r#"{"error":{"message":"Rate limit reached"}}"#).code(),
            "ai_rate_limited"
        );
        assert_eq!(
            classify_error(
                400,
                r#"[{"error":{"message":"User location is not supported for the API use."}}]"#
            )
            .code(),
            "ai_region"
        );
        assert_eq!(
            classify_error(
                400,
                r#"{"error":{"message":"This model's maximum context length is 65536 tokens"}}"#
            )
            .code(),
            "ai_too_long"
        );
        assert_eq!(classify_error(500, "upstream died").code(), "ai_server");
    }
}
