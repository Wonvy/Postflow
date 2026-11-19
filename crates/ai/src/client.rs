//! OpenAI 兼容接口的客户端：对话补全（一次返回或流式返回）和模型列表。
//!
//! DeepSeek、通义千问、Gemini、OpenAI、Kimi、Ollama 等都提供这套接口，只是地址不同。
//!
//! 会「思考」的模型（智谱 GLM-4.5 以后、DeepSeek Reasoner、OpenAI 的推理模型……）要特别对待：
//! 它们先写一段思考再写回答，思考也算在输出上限（`max_tokens`）里。总结、关键词这类短任务带的上限
//! 只够写回答，思考一占，回答就被截断甚至一个字都没有。这里的做法：
//! - 能关掉思考的服务（智谱、DeepSeek、通义千问、硅基流动、Kimi、Ollama、Gemini、OpenRouter），
//!   短任务直接让它别思考，又快又省。各家的参数不一样，见 `quiet_param`；
//! - 关不掉的，一发现模型在思考（回答之前先来了思考内容，或者回答被截断、是空的），就不带上限
//!   重新请求一次，并记住这个模型，以后直接不带上限。
//! - 思考内容不算回答：单独字段里的不读，写在正文开头 `<think>…</think>` 里的滤掉。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
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
    /// 用 f64：f32 的 0.3 转成 JSON 会变成 0.30000001192092896。
    pub temperature: Option<f64>,
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
    /// 回答是空的。`thinking`：模型思考了，但没写出回答。
    #[error("the model returned an empty answer (thinking: {thinking})")]
    Empty { thinking: bool },
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
            Error::Empty { thinking: false } => "ai_empty_answer",
            Error::Empty { thinking: true } => "ai_thinking_only",
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
    /// 这次运行里发现会「思考」的模型（服务地址加模型名）：给它们发请求不带输出上限。
    thinkers: Mutex<HashSet<String>>,
    /// 不让关掉思考的模型：不再给它们发「别思考」的参数。
    always_think: Mutex<HashSet<String>>,
    /// 上面两样存在哪儿（本机），下次启动不用再试一遍。不给就只记在内存里。
    memory: Option<PathBuf>,
}

/// 存下来的「这个模型会思考」「这个模型关不掉思考」。
#[derive(Default, Serialize, Deserialize)]
struct Remembered {
    #[serde(default)]
    thinkers: Vec<String>,
    #[serde(default)]
    always_think: Vec<String>,
}

/// 一次请求带哪些可选参数。服务不认某个参数、或者发现模型在思考时，去掉对应的一项再试。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shape {
    /// 要求输出 JSON。
    json: bool,
    /// 带输出上限。
    cap: bool,
    /// 让模型别思考。
    quiet: bool,
}

/// 一次回答里读出来的东西。
struct Parsed {
    text: String,
    usage: Usage,
    /// 模型思考了（有思考内容，或者用量里记着思考用的 token）。
    reasoned: bool,
    /// 因为到了输出上限被截断。
    cut: bool,
}

/// 服务地址加模型名，记「会思考的模型」用。
fn model_key(provider: &Provider) -> String {
    format!(
        "{}|{}",
        provider.base_url.trim().trim_end_matches('/'),
        provider.model.trim()
    )
}

/// 让这个服务的模型别思考的参数（加在请求体的最外层）。按预设或者地址认服务；认不出的不发，
/// 靠「发现在思考就不限长度重来」兜底。各家文档（2026 年 11 月）：
/// - 智谱、DeepSeek、Kimi：`thinking: {"type": "disabled"}`
/// - 通义千问（百炼）、硅基流动：`enable_thinking: false`
/// - Ollama、Gemini（OpenAI 兼容接口）：`reasoning_effort: "none"`
/// - OpenRouter：`reasoning: {"effort": "none"}`
///
/// 有的模型不让关（智谱 GLM-5.3、Kimi K2.7 Code、Gemini 2.5 Pro……），会回 4xx：去掉参数再试，并记住。
fn quiet_param(provider: &Provider) -> Option<(&'static str, Value)> {
    let url = provider.base_url.to_ascii_lowercase();
    let host = |h: &str| url.contains(h);
    let preset = provider.preset.as_str();
    if preset == "zhipu" || host("bigmodel.cn") || host("api.z.ai") {
        return Some(("thinking", json!({ "type": "disabled" })));
    }
    if preset == "deepseek" || host("api.deepseek.com") {
        return Some(("thinking", json!({ "type": "disabled" })));
    }
    if preset == "moonshot" || host("api.moonshot.cn") || host("api.moonshot.ai") {
        return Some(("thinking", json!({ "type": "disabled" })));
    }
    if matches!(preset, "qwen" | "qwen_intl" | "siliconflow")
        || host("dashscope")
        || host("siliconflow")
    {
        return Some(("enable_thinking", json!(false)));
    }
    if matches!(preset, "ollama" | "gemini")
        || host("generativelanguage.googleapis.com")
        || host(":11434")
    {
        return Some(("reasoning_effort", json!("none")));
    }
    if preset == "openrouter" || host("openrouter.ai") {
        return Some(("reasoning", json!({ "effort": "none" })));
    }
    None
}

/// 一条消息或者流式的一小段里有没有思考内容。各家字段名不一样。
fn has_reasoning(part: Option<&Value>) -> bool {
    part.is_some_and(|p| {
        ["reasoning_content", "reasoning"].iter().any(|k| {
            p.get(*k)
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty())
        })
    })
}

/// 有的模型把思考过程写在正文最前面，用 `<think>…</think>` 包着。这部分不是回答，滤掉。
/// 流式输出时标签可能被切成几段，所以一段一段喂。
#[derive(Default)]
struct ThinkFilter {
    state: FilterState,
    held: String,
    /// 正文里出现过思考块。
    saw: bool,
}

#[derive(Default, PartialEq)]
enum FilterState {
    /// 还不知道开头是不是思考块。
    #[default]
    Start,
    Inside,
    /// 思考块刚结束：回答前面的空行不要。
    After,
    Passing,
}

const THINK_OPEN: &str = "<think>";
const THINK_CLOSE: &str = "</think>";

impl ThinkFilter {
    /// 喂一段正文，返回其中可以交出去的部分。
    fn push(&mut self, delta: &str) -> String {
        match self.state {
            FilterState::Passing => delta.to_string(),
            FilterState::After => {
                let rest = delta.trim_start();
                if !rest.is_empty() {
                    self.state = FilterState::Passing;
                }
                rest.to_string()
            }
            FilterState::Start => {
                self.held.push_str(delta);
                let trimmed = self.held.trim_start();
                if trimmed.is_empty()
                    || (trimmed.len() < THINK_OPEN.len() && THINK_OPEN.starts_with(trimmed))
                {
                    return String::new();
                }
                if let Some(rest) = trimmed.strip_prefix(THINK_OPEN) {
                    self.saw = true;
                    self.state = FilterState::Inside;
                    let rest = rest.to_string();
                    self.held.clear();
                    return self.push(&rest);
                }
                self.state = FilterState::Passing;
                std::mem::take(&mut self.held)
            }
            FilterState::Inside => {
                self.held.push_str(delta);
                if let Some(at) = self.held.find(THINK_CLOSE) {
                    let rest = self.held[at + THINK_CLOSE.len()..].to_string();
                    self.held.clear();
                    self.state = FilterState::After;
                    return self.push(&rest);
                }
                // 只留最后几个字，够认出被切开的结束标签就行
                let keep = self.held.len().saturating_sub(THINK_CLOSE.len());
                let keep = (keep..=self.held.len())
                    .find(|i| self.held.is_char_boundary(*i))
                    .unwrap_or(self.held.len());
                self.held.drain(..keep);
                String::new()
            }
        }
    }

    /// 输出结束了：开头攒着没交出去的（不是思考块）交出来。
    fn finish(&mut self) -> String {
        if self.state == FilterState::Start {
            self.state = FilterState::Passing;
            return std::mem::take(&mut self.held);
        }
        String::new()
    }
}

/// 一次性回答的正文：去掉开头的思考块。
fn strip_think(text: &str) -> (String, bool) {
    let mut filter = ThinkFilter::default();
    let mut out = filter.push(text);
    out.push_str(&filter.finish());
    (out, filter.saw)
}

/// 读一份完整的（非流式）回答。
fn parse_reply(body: &str) -> Result<Parsed> {
    let value: Value = serde_json::from_str(body).map_err(|e| Error::BadResponse(e.to_string()))?;
    let choice = value
        .pointer("/choices/0")
        .ok_or_else(|| Error::BadResponse(server_message(body)))?;
    let message = choice.get("message");
    let raw = message
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let (text, tagged) = strip_think(raw);
    let usage = usage_of(&value);
    Ok(Parsed {
        text,
        reasoned: tagged || has_reasoning(message) || reasoning_tokens(&value) > 0,
        cut: choice.get("finish_reason").and_then(Value::as_str) == Some("length"),
        usage,
    })
}

fn reasoning_tokens(value: &Value) -> u64 {
    value
        .pointer("/usage/completion_tokens_details/reasoning_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

fn add(a: Usage, b: Usage) -> Usage {
    Usage {
        input: a.input + b.input,
        output: a.output + b.output,
    }
}

/// 一个字都没有的回答。
fn empty_answer(reasoned: bool) -> Error {
    Error::Empty { thinking: reasoned }
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
            thinkers: Mutex::new(HashSet::new()),
            always_think: Mutex::new(HashSet::new()),
            memory: None,
        }
    }

    /// 和 `new` 一样，但把发现的「会思考」「关不掉思考」的模型存在 `path`（本机的一个 JSON 文件），
    /// 下次启动直接用，不用每个模型再白试一次。
    pub fn with_memory(path: impl Into<PathBuf>) -> Client {
        let path = path.into();
        let saved: Remembered = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Client {
            thinkers: Mutex::new(saved.thinkers.into_iter().collect()),
            always_think: Mutex::new(saved.always_think.into_iter().collect()),
            memory: Some(path),
            ..Client::new()
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

    /// 这次请求一开始带哪些可选参数。
    fn shape(&self, provider: &Provider, request: &Request) -> Shape {
        let key = model_key(provider);
        let known = |set: &Mutex<HashSet<String>>| {
            set.lock().unwrap_or_else(|e| e.into_inner()).contains(&key)
        };
        // 带上限的都是总结、关键词这类短任务，用不着思考；对话不带上限，让模型自己决定
        let short = request.max_tokens.is_some();
        Shape {
            json: request.json,
            cap: short && !known(&self.thinkers),
            quiet: short && quiet_param(provider).is_some() && !known(&self.always_think),
        }
    }

    fn remember(&self, set: &Mutex<HashSet<String>>, provider: &Provider) {
        let added = set
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(model_key(provider));
        if added {
            self.save();
        }
    }

    /// 把记住的模型写到本机。写不了只记一条日志：下次启动再试一遍而已。
    fn save(&self) {
        let Some(path) = &self.memory else { return };
        let list = |set: &Mutex<HashSet<String>>| {
            let mut v: Vec<String> = set
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .cloned()
                .collect();
            v.sort();
            v
        };
        let data = Remembered {
            thinkers: list(&self.thinkers),
            always_think: list(&self.always_think),
        };
        let write = || -> std::io::Result<()> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let tmp = path.with_extension("tmp");
            std::fs::write(&tmp, serde_json::to_vec_pretty(&data)?)?;
            std::fs::rename(&tmp, path)
        };
        if let Err(e) = write() {
            tracing::warn!("cannot save which AI models think: {e}");
        }
    }

    fn body(provider: &Provider, request: &Request, stream: bool, shape: Shape) -> Value {
        let mut body = json!({
            "model": provider.model.trim(),
            "messages": request.messages,
            "stream": stream,
        });
        if let Some(max) = request.max_tokens.filter(|_| shape.cap) {
            body["max_tokens"] = json!(max);
        }
        if let Some(t) = request.temperature {
            // 智谱等服务只接受最多两位小数，多一位就回 400
            body["temperature"] = json!((t * 100.0).round() / 100.0);
        }
        if shape.json {
            body["response_format"] = json!({ "type": "json_object" });
        }
        if shape.quiet
            && let Some((key, value)) = quiet_param(provider)
        {
            body[key] = value;
        }
        body
    }

    /// 发请求。服务不认某个可选参数（回 4xx，错误里提到它）时去掉那一项再试，`shape` 跟着改。
    async fn send(
        &self,
        provider: &Provider,
        key: Option<&str>,
        request: &Request,
        stream: bool,
        shape: &mut Shape,
    ) -> Result<reqwest::Response> {
        if provider.model.trim().is_empty() {
            return Err(Error::NoModel);
        }
        let http = self.http(provider)?;
        // 去掉了哪一项再试的。只有去掉以后真的成了，才记住这个模型就是这样
        let (mut dropped_quiet, mut dropped_cap) = (false, false);
        loop {
            let mut builder = http
                .post(Self::url(provider, "chat/completions"))
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(Self::body(provider, request, stream, *shape).to_string());
            if let Some(key) = key.filter(|k| !k.is_empty()) {
                builder = builder.bearer_auth(key);
            }
            if !stream {
                builder = builder.timeout(Duration::from_secs(300));
            }
            let response = builder.send().await.map_err(Error::from_reqwest)?;
            let status = response.status().as_u16();
            if status < 400 {
                if dropped_quiet {
                    self.remember(&self.always_think, provider);
                }
                if dropped_cap {
                    self.remember(&self.thinkers, provider);
                }
                return Ok(response);
            }
            let body = response.text().await.unwrap_or_default();
            let rejected = matches!(status, 400 | 422);
            let lower = body.to_ascii_lowercase();
            // 不认识 JSON 输出参数的服务，去掉它再试一次。提示词里本来就要求了 JSON
            if shape.json && rejected && lower.contains("response_format") {
                shape.json = false;
                continue;
            }
            // 不认 max_tokens 的模型（OpenAI 的推理模型要的是另一个参数）：不带上限
            if shape.cap && rejected && lower.contains("max_tokens") {
                shape.cap = false;
                dropped_cap = true;
                continue;
            }
            // 「别思考」是我们额外加的参数：有的模型不让关（智谱 GLM-5.3），老模型可能根本不认。
            // 各家报错的说法不一样，所以只要请求被拒就去掉它再试一次；还不行，报的就是真正的错
            if shape.quiet && rejected {
                shape.quiet = false;
                dropped_quiet = true;
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
        let mut shape = self.shape(provider, request);
        let mut spent = Usage::default();
        loop {
            let response = self.send(provider, key, request, false, &mut shape).await?;
            let body = response.text().await.map_err(Error::from_reqwest)?;
            let parsed = parse_reply(&body)?;
            spent = add(spent, parsed.usage);
            // 思考把输出上限占掉了，回答没写完：不带上限再来一次
            if shape.cap && parsed.reasoned && (parsed.cut || parsed.text.trim().is_empty()) {
                tracing::debug!(model = %provider.model, "the model's thinking used up max_tokens; retrying without the limit");
                self.remember(&self.thinkers, provider);
                shape.cap = false;
                continue;
            }
            if parsed.text.trim().is_empty() {
                return Err(empty_answer(parsed.reasoned));
            }
            return Ok(Reply {
                text: parsed.text,
                usage: spent,
            });
        }
    }

    /// 边生成边返回。每收到一段文字调用一次 `on_delta`，最后返回完整回答。
    pub async fn stream(
        &self,
        provider: &Provider,
        key: Option<&str>,
        request: &Request,
        mut on_delta: impl FnMut(&str),
    ) -> Result<Reply> {
        let mut shape = self.shape(provider, request);
        let mut spent = Usage::default();
        'attempt: loop {
            let mut response = self.send(provider, key, request, true, &mut shape).await?;
            // 有的服务忽略 stream 参数，直接给完整的 JSON
            let is_json = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|t| t.contains("application/json"));
            if is_json {
                let body = response.text().await.map_err(Error::from_reqwest)?;
                let parsed = parse_reply(&body)?;
                spent = add(spent, parsed.usage);
                if shape.cap && parsed.reasoned && (parsed.cut || parsed.text.trim().is_empty()) {
                    self.remember(&self.thinkers, provider);
                    shape.cap = false;
                    continue 'attempt;
                }
                if parsed.text.trim().is_empty() {
                    return Err(empty_answer(parsed.reasoned));
                }
                on_delta(&parsed.text);
                return Ok(Reply {
                    text: parsed.text,
                    usage: spent,
                });
            }

            let mut buffer: Vec<u8> = Vec::new();
            let mut text = String::new();
            let mut usage = Usage::default();
            let mut filter = ThinkFilter::default();
            let mut reasoned = false;
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
                    let delta = value.pointer("/choices/0/delta");
                    let mut piece = String::new();
                    if let Some(content) =
                        delta.and_then(|d| d.get("content")).and_then(Value::as_str)
                        && !content.is_empty()
                    {
                        piece = filter.push(content);
                    }
                    reasoned = reasoned || filter.saw || has_reasoning(delta);
                    // 模型先思考再回答，思考也算在输出上限里，带着上限回答多半写不完。
                    // 趁回答还一个字没给出去，不带上限重新来
                    if shape.cap && reasoned && text.is_empty() {
                        tracing::debug!(model = %provider.model, "the model is thinking; restarting without max_tokens");
                        self.remember(&self.thinkers, provider);
                        shape.cap = false;
                        continue 'attempt;
                    }
                    if !piece.is_empty() {
                        text.push_str(&piece);
                        on_delta(&piece);
                    }
                    let u = usage_of(&value);
                    if u != Usage::default() {
                        usage = u;
                    }
                }
            }
            let tail = filter.finish();
            if !tail.is_empty() {
                text.push_str(&tail);
                on_delta(&tail);
            }
            spent = add(spent, usage);
            if text.trim().is_empty() {
                return Err(empty_answer(reasoned));
            }
            return Ok(Reply { text, usage: spent });
        }
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
    fn thinking_blocks_are_filtered_out_of_streamed_text() {
        let run = |pieces: &[&str]| {
            let mut f = ThinkFilter::default();
            let mut out = String::new();
            for p in pieces {
                out.push_str(&f.push(p));
            }
            out.push_str(&f.finish());
            (out, f.saw)
        };
        // 标签被切成几段、回答前有空行
        assert_eq!(
            run(&[
                "\n<th",
                "ink>let me",
                " see… 好的</thi",
                "nk>",
                "\n\n",
                "**答案**",
                "\n- 一"
            ]),
            ("**答案**\n- 一".to_string(), true)
        );
        assert_eq!(
            strip_think("<think>abc</think>ok"),
            ("ok".to_string(), true)
        );
        // 没有思考块的原样给出，开头的空白也不动
        assert_eq!(
            run(&["  **Hi", "** there"]),
            ("  **Hi** there".to_string(), false)
        );
        assert_eq!(
            run(&["<", "b>bold</b>"]),
            ("<b>bold</b>".to_string(), false)
        );
        assert_eq!(run(&["<thi"]), ("<thi".to_string(), false));
        // 正文中间提到 <think> 不算
        assert_eq!(
            strip_think("Use <think> tags sparingly."),
            ("Use <think> tags sparingly.".to_string(), false)
        );
        // 思考没写完就断了：没有回答
        assert_eq!(run(&["<think>still going"]), (String::new(), true));
    }

    #[test]
    fn each_service_gets_its_own_switch_for_thinking() {
        let with = |preset: &str, url: &str| {
            quiet_param(&Provider {
                preset: preset.into(),
                ..provider(url)
            })
        };
        let disabled = Some(("thinking", json!({ "type": "disabled" })));
        assert_eq!(
            with("zhipu", "https://open.bigmodel.cn/api/paas/v4"),
            disabled
        );
        // 选了「自定义」但填的是智谱的地址
        assert_eq!(
            with("custom", "https://open.bigmodel.cn/api/paas/v4/"),
            disabled
        );
        assert_eq!(with("deepseek", "https://api.deepseek.com"), disabled);
        assert_eq!(with("moonshot", "https://api.moonshot.cn/v1"), disabled);
        let off = Some(("enable_thinking", json!(false)));
        assert_eq!(
            with("qwen", "https://dashscope.aliyuncs.com/compatible-mode/v1"),
            off
        );
        assert_eq!(with("siliconflow", "https://api.siliconflow.cn/v1"), off);
        let none = Some(("reasoning_effort", json!("none")));
        assert_eq!(with("ollama", "http://localhost:11434/v1"), none);
        assert_eq!(with("custom", "http://127.0.0.1:11434/v1"), none);
        assert_eq!(
            with(
                "gemini",
                "https://generativelanguage.googleapis.com/v1beta/openai"
            ),
            none
        );
        assert_eq!(
            with("openrouter", "https://openrouter.ai/api/v1"),
            Some(("reasoning", json!({ "effort": "none" })))
        );
        // OpenAI 的普通模型不认 reasoning_effort，LM Studio 和不认识的服务都不发
        assert_eq!(with("openai", "https://api.openai.com/v1"), None);
        assert_eq!(with("lmstudio", "http://localhost:1234/v1"), None);
        assert_eq!(with("custom", "https://llm.example.com/v1"), None);
    }

    #[test]
    fn empty_answers_have_their_own_codes() {
        assert_eq!(Error::Empty { thinking: false }.code(), "ai_empty_answer");
        assert_eq!(Error::Empty { thinking: true }.code(), "ai_thinking_only");
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
