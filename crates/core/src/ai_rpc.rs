//! AI 功能的方法：配置服务、总结、关键词、列表摘要、对话。
//!
//! 服务的配置（地址、模型）放在同步的设置里，API Key 只存在这台设备的系统钥匙串里。
//! 生成的内容和对话记录只存在本机。邮件内容默认不发给云端服务，本机运行的服务不受限制。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use postflow_ai::{Client, Doc, Message, Provider, Request, prompts, text};
use postflow_feed::html_to_paragraphs;
use postflow_mail::RenderOptions;
use postflow_store::{AiResult, ItemSummary, day_of, ids};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::rpc::{RpcError, parse, reply};
use crate::{Core, Event, Scope, Session};

const PROVIDERS: &str = "ai.providers";
const DEFAULT: &str = "ai.default";
const FAST: &str = "ai.fast";
const ALLOW_MAIL: &str = "ai.allow_mail";
/// 长文分段整理要点时最多分这么多段，更长的只看前面这些。
const MAX_PARTS: usize = 8;
/// 列表摘要一次请求处理几条。
const ABSTRACT_BATCH: usize = 8;

/// 正在进行的请求，可以随时取消。
#[derive(Default)]
pub(crate) struct AiRuntime {
    pub(crate) client: Client,
    running: Mutex<HashMap<String, tokio::task::AbortHandle>>,
    next: AtomicU64,
}

impl From<postflow_ai::Error> for RpcError {
    fn from(err: postflow_ai::Error) -> Self {
        RpcError::new(err.code(), err.to_string())
    }
}

/// 用哪个服务：主要的（总结、对话、翻译），还是快速便宜的（关键词、摘要、分类）。
#[derive(Clone, Copy)]
pub(crate) enum Use {
    Main,
    Fast,
}

pub(crate) struct AiSettings {
    pub providers: Vec<Provider>,
    pub default: Option<String>,
    pub fast: Option<String>,
    pub allow_mail: bool,
}

impl AiSettings {
    pub(crate) fn pick(&self, usage: Use) -> Result<Provider, RpcError> {
        let wanted = match usage {
            Use::Fast => self.fast.as_ref().or(self.default.as_ref()),
            Use::Main => self.default.as_ref(),
        };
        wanted
            .and_then(|id| self.providers.iter().find(|p| &p.id == id))
            .or_else(|| self.providers.first())
            .cloned()
            .ok_or_else(|| RpcError::new("ai_not_configured", "no AI service has been set up"))
    }

    /// 邮件内容只有在用户允许、或者服务在本机运行时才发出去。
    pub(crate) fn check(&self, item: &ItemSummary, provider: &Provider) -> Result<(), RpcError> {
        if item.kind == "mail" && !provider.is_local() && !self.allow_mail {
            return Err(RpcError::new(
                "ai_mail_blocked",
                "sending mail to cloud AI services is turned off",
            ));
        }
        Ok(())
    }
}

/// 交给 AI 读的一条：标题、来源、正文的纯文字。
pub(crate) struct ItemDoc {
    pub item: ItemSummary,
    pub text: String,
}

impl ItemDoc {
    pub(crate) fn doc(&self) -> Doc<'_> {
        let source = if self.item.kind == "mail" {
            self.item
                .author
                .as_deref()
                .or(self.item.sender_address.as_deref())
                .unwrap_or("")
        } else {
            self.item.source_title.as_str()
        };
        Doc {
            title: &self.item.title,
            source,
            text: &self.text,
        }
    }
}

#[derive(Serialize)]
struct ProviderView {
    #[serde(flatten)]
    provider: Provider,
    has_key: bool,
    local: bool,
}

#[derive(Deserialize)]
struct SaveProvider {
    provider: Provider,
    /// 不给就保留原来的密钥。
    #[serde(default)]
    api_key: Option<String>,
}

#[derive(Deserialize)]
struct ProviderParams {
    provider: Provider,
    #[serde(default)]
    api_key: Option<String>,
}

#[derive(Deserialize)]
struct IdParam {
    id: String,
}

#[derive(Deserialize)]
struct RunParams {
    id: String,
    task: String,
    #[serde(default)]
    lang: String,
    #[serde(default)]
    force: bool,
}

#[derive(Deserialize)]
struct ResultsParams {
    id: String,
    #[serde(default)]
    lang: String,
}

#[derive(Deserialize)]
struct AbstractsParams {
    ids: Vec<String>,
    #[serde(default)]
    lang: String,
}

#[derive(Deserialize)]
struct ChatParams {
    id: String,
    message: String,
    #[serde(default)]
    lang: String,
}

#[derive(Deserialize)]
struct CancelParams {
    request_id: String,
}

fn secret_id(provider_id: &str) -> String {
    format!("ai:{provider_id}")
}

fn now() -> i64 {
    crate::refresh::now()
}

impl Core {
    pub(crate) async fn ai_rpc(
        self: &Arc<Self>,
        method: &str,
        params: Value,
        session: &Arc<Session>,
        client: Option<String>,
    ) -> Result<Value, RpcError> {
        match method {
            "ai.presets" => reply(json!({ "presets": postflow_ai::PRESETS })),
            "ai.providers" => {
                let settings = self.ai_settings(session).await?;
                let mut providers = Vec::new();
                for provider in settings.providers {
                    let has_key = self.ai_key(&provider.id).await.is_some();
                    let local = provider.is_local();
                    providers.push(ProviderView {
                        provider,
                        has_key,
                        local,
                    });
                }
                reply(json!({
                    "providers": providers,
                    "default": settings.default,
                    "fast": settings.fast,
                    "allow_mail": settings.allow_mail,
                }))
            }
            "ai.providers.save" => {
                let p: SaveProvider = parse(params)?;
                let mut provider = p.provider;
                provider.base_url = provider.base_url.trim().trim_end_matches('/').to_string();
                provider.model = provider.model.trim().to_string();
                provider.name = provider.name.trim().to_string();
                provider.proxy = provider
                    .proxy
                    .map(|x| x.trim().to_string())
                    .filter(|x| !x.is_empty());
                if !(provider.base_url.starts_with("https://")
                    || provider.base_url.starts_with("http://"))
                {
                    return Err(RpcError::new(
                        "bad_params",
                        "the address must start with https:// or http://",
                    ));
                }
                if provider.id.is_empty() {
                    provider.id = ids::new_device_id();
                }
                if provider.name.is_empty() {
                    provider.name = postflow_ai::preset(&provider.preset)
                        .map_or_else(|| provider.base_url.clone(), |p| p.name.to_string());
                }
                if let Some(key) = p
                    .api_key
                    .map(|k| k.trim().to_string())
                    .filter(|k| !k.is_empty())
                {
                    let secrets = self.secrets.clone();
                    let id = secret_id(&provider.id);
                    tokio::task::spawn_blocking(move || secrets.set(&id, &key))
                        .await
                        .map_err(|e| RpcError::new("internal", e.to_string()))?
                        .map_err(|e| RpcError::new("keychain", e))?;
                }
                let mut settings = self.ai_settings(session).await?;
                match settings.providers.iter_mut().find(|x| x.id == provider.id) {
                    Some(existing) => *existing = provider.clone(),
                    None => settings.providers.push(provider.clone()),
                }
                let default_ok = settings
                    .default
                    .as_ref()
                    .is_some_and(|d| settings.providers.iter().any(|x| &x.id == d));
                let providers = serde_json::to_value(&settings.providers).unwrap_or_default();
                let id = provider.id.clone();
                session
                    .db(move |s| {
                        s.set_setting(PROVIDERS, providers)?;
                        if !default_ok {
                            s.set_setting(DEFAULT, json!(id))?;
                        }
                        Ok(())
                    })
                    .await?;
                self.emit_changed(&[Scope::Settings], client);
                reply(provider)
            }
            "ai.providers.remove" => {
                let p: IdParam = parse(params)?;
                let mut settings = self.ai_settings(session).await?;
                settings.providers.retain(|x| x.id != p.id);
                let secrets = self.secrets.clone();
                let sid = secret_id(&p.id);
                let _ = tokio::task::spawn_blocking(move || secrets.delete(&sid)).await;
                let providers = serde_json::to_value(&settings.providers).unwrap_or_default();
                let first = settings.providers.first().map(|x| x.id.clone());
                let (default, fast) = (settings.default.clone(), settings.fast.clone());
                session
                    .db(move |s| {
                        s.set_setting(PROVIDERS, providers)?;
                        if default.as_deref() == Some(p.id.as_str()) {
                            s.set_setting(DEFAULT, json!(first))?;
                        }
                        if fast.as_deref() == Some(p.id.as_str()) {
                            s.set_setting(FAST, Value::Null)?;
                        }
                        Ok(())
                    })
                    .await?;
                self.emit_changed(&[Scope::Settings], client);
                reply(json!({}))
            }
            "ai.providers.test" => {
                let p: ProviderParams = parse(params)?;
                let key = self.key_for(&p.provider, p.api_key).await;
                let request = Request {
                    messages: vec![Message::user("Reply with the single word OK.")],
                    max_tokens: Some(16),
                    ..Default::default()
                };
                let answer = self
                    .ai
                    .client
                    .complete(&p.provider, key.as_deref(), &request)
                    .await?;
                reply(json!({ "ok": true, "reply": answer.text.trim() }))
            }
            "ai.models" => {
                let p: ProviderParams = parse(params)?;
                let key = self.key_for(&p.provider, p.api_key).await;
                let models = self.ai.client.models(&p.provider, key.as_deref()).await?;
                reply(json!({ "models": models }))
            }
            "ai.results" => {
                let p: ResultsParams = parse(params)?;
                let (id, lang) = (p.id.clone(), p.lang.clone());
                let (summary, keywords) = session
                    .db(move |s| {
                        Ok((
                            s.ai_result(&id, "summary", &lang)?,
                            s.ai_result(&id, "keywords", &lang)?,
                        ))
                    })
                    .await?;
                reply(json!({ "summary": summary, "keywords": keywords }))
            }
            "ai.run" => {
                let p: RunParams = parse(params)?;
                if !matches!(p.task.as_str(), "summary" | "keywords") {
                    return Err(RpcError::new(
                        "bad_params",
                        format!("unknown task: {}", p.task),
                    ));
                }
                if !p.force {
                    let (id, task, lang) = (p.id.clone(), p.task.clone(), p.lang.clone());
                    if let Some(cached) =
                        session.db(move |s| s.ai_result(&id, &task, &lang)).await?
                    {
                        return reply(json!({ "cached": cached }));
                    }
                }
                let settings = self.ai_settings(session).await?;
                let usage = if p.task == "keywords" {
                    Use::Fast
                } else {
                    Use::Main
                };
                let provider = settings.pick(usage)?;
                let doc = self.item_doc(session, &p.id).await?;
                settings.check(&doc.item, &provider)?;
                let key = self.ai_key(&provider.id).await;
                let (core, session) = (self.clone(), session.clone());
                let request_id = self.spawn_ai(move |request_id| async move {
                    let result = match p.task.as_str() {
                        "keywords" => {
                            core.run_keywords(&session, &doc, &provider, key, &p.lang)
                                .await
                        }
                        _ => {
                            core.run_summary(&session, &request_id, &doc, &provider, key, &p.lang)
                                .await
                        }
                    };
                    core.finish_ai(&request_id, result);
                });
                reply(json!({ "request_id": request_id }))
            }
            "ai.abstracts" => {
                let p: AbstractsParams = parse(params)?;
                reply(json!({ "abstracts": self.abstracts(session, p.ids, &p.lang).await? }))
            }
            "ai.chat" => {
                let p: ChatParams = parse(params)?;
                let question = p.message.trim().to_string();
                if question.is_empty() {
                    return Err(RpcError::new("bad_params", "message is empty"));
                }
                let settings = self.ai_settings(session).await?;
                let provider = settings.pick(Use::Main)?;
                let doc = self.item_doc(session, &p.id).await?;
                settings.check(&doc.item, &provider)?;
                let key = self.ai_key(&provider.id).await;
                let id = p.id.clone();
                let history = session.db(move |s| s.ai_messages(&id)).await?;
                let (id, q) = (p.id.clone(), question.clone());
                let asked = session
                    .db(move |s| s.add_ai_message(&id, "user", &q))
                    .await?;
                let (core, session) = (self.clone(), session.clone());
                let request_id = self.spawn_ai(move |request_id| async move {
                    let history: Vec<Message> = history
                        .iter()
                        .map(|m| {
                            if m.role == "assistant" {
                                Message::assistant(m.content.clone())
                            } else {
                                Message::user(m.content.clone())
                            }
                        })
                        .collect();
                    let request = prompts::chat(doc.doc(), &history, &question, &p.lang);
                    let result = async {
                        let answer = core
                            .stream_ai(&session, &request_id, &provider, key, &request)
                            .await?;
                        let item = doc.item.id.clone();
                        let saved = session
                            .db(move |s| s.add_ai_message(&item, "assistant", &answer))
                            .await?;
                        Ok(json!({ "message": saved }))
                    }
                    .await;
                    core.finish_ai(&request_id, result);
                });
                reply(json!({ "request_id": request_id, "message": asked }))
            }
            "ai.chat.history" => {
                let p: IdParam = parse(params)?;
                reply(json!({ "messages": session.db(move |s| s.ai_messages(&p.id)).await? }))
            }
            "ai.chat.clear" => {
                let p: IdParam = parse(params)?;
                session.db(move |s| s.clear_ai_messages(&p.id)).await?;
                reply(json!({}))
            }
            "ai.cancel" => {
                let p: CancelParams = parse(params)?;
                let handle = self
                    .ai
                    .running
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&p.request_id);
                if let Some(handle) = handle {
                    handle.abort();
                    self.finish_ai(
                        &p.request_id,
                        Err(RpcError::new("ai_cancelled", "cancelled")),
                    );
                }
                reply(json!({}))
            }
            "ai.usage" => {
                let today = day_of(now());
                let (day, month) = session
                    .db(move |s| Ok((s.ai_usage(today)?, s.ai_usage(today - 29)?)))
                    .await?;
                reply(json!({ "today": day, "last30": month }))
            }
            _ => Err(RpcError::new(
                "unknown_method",
                format!("unknown method: {method}"),
            )),
        }
    }

    pub(crate) async fn ai_settings(&self, session: &Arc<Session>) -> Result<AiSettings, RpcError> {
        let settings = session.db(|s| s.settings()).await?;
        let text = |key: &str| {
            settings
                .get(key)
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        Ok(AiSettings {
            providers: settings
                .get(PROVIDERS)
                .cloned()
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default(),
            default: text(DEFAULT),
            fast: text(FAST),
            allow_mail: settings
                .get(ALLOW_MAIL)
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }

    pub(crate) async fn ai_key(&self, provider_id: &str) -> Option<String> {
        let secrets = self.secrets.clone();
        let id = secret_id(provider_id);
        tokio::task::spawn_blocking(move || secrets.get(&id))
            .await
            .ok()
            .flatten()
    }

    /// 测试连接、拉模型列表时：表单里填了密钥就用填的，否则用存着的。
    async fn key_for(&self, provider: &Provider, typed: Option<String>) -> Option<String> {
        match typed
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
        {
            Some(key) => Some(key),
            None if !provider.id.is_empty() => self.ai_key(&provider.id).await,
            None => None,
        }
    }

    /// 取出一条的标题、来源和正文文字。邮件可能要先从服务器取正文。
    pub(crate) async fn item_doc(
        self: &Arc<Self>,
        session: &Arc<Session>,
        id: &str,
    ) -> Result<ItemDoc, RpcError> {
        let key = id.to_string();
        let detail = session
            .db(move |s| s.item(&key))
            .await?
            .ok_or_else(|| RpcError::new("not_found", "no such item"))?;
        let text = if detail.item.kind == "mail" {
            let raw = self.raw_mail(session, id).await?;
            let rendered = postflow_mail::render(
                &raw,
                RenderOptions {
                    allow_remote: false,
                },
            )
            .ok_or_else(|| RpcError::new("mail_parse", "this message could not be read"))?;
            html_to_paragraphs(&rendered.body)
        } else {
            let html = detail
                .full_text
                .as_deref()
                .filter(|t| !t.trim().is_empty())
                .unwrap_or(&detail.content);
            html_to_paragraphs(html)
        };
        let text = if text.trim().is_empty() {
            detail.item.summary.clone()
        } else {
            text
        };
        Ok(ItemDoc {
            item: detail.item,
            text,
        })
    }

    /// 在后台跑一个请求，返回请求 ID。结果通过事件告诉界面。
    pub(crate) fn spawn_ai<F, Fut>(self: &Arc<Self>, task: F) -> String
    where
        F: FnOnce(String) -> Fut,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let request_id = format!("ai-{}", self.ai.next.fetch_add(1, Ordering::Relaxed) + 1);
        let handle = tokio::spawn(task(request_id.clone()));
        if !handle.is_finished() {
            self.ai
                .running
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(request_id.clone(), handle.abort_handle());
        }
        request_id
    }

    pub(crate) fn finish_ai(&self, request_id: &str, result: Result<Value, RpcError>) {
        self.ai
            .running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(request_id);
        let (result, error, message) = match result {
            Ok(value) => (Some(value), None, None),
            Err(e) => {
                tracing::warn!("ai request failed: {e}");
                (None, Some(e.code), Some(e.message))
            }
        };
        self.emit(Event::Ai {
            request_id: request_id.to_string(),
            delta: None,
            done: true,
            result,
            error,
            message,
        });
    }

    /// 流式请求：每段文字都推给界面，最后记下用量，返回完整回答。
    pub(crate) async fn stream_ai(
        &self,
        session: &Arc<Session>,
        request_id: &str,
        provider: &Provider,
        key: Option<String>,
        request: &Request,
    ) -> Result<String, RpcError> {
        let answer = self
            .ai
            .client
            .stream(provider, key.as_deref(), request, |delta| {
                self.emit(Event::Ai {
                    request_id: request_id.to_string(),
                    delta: Some(delta.to_string()),
                    done: false,
                    result: None,
                    error: None,
                    message: None,
                })
            })
            .await?;
        self.record_usage(session, provider, answer.usage).await;
        Ok(answer.text)
    }

    pub(crate) async fn complete_ai(
        &self,
        session: &Arc<Session>,
        provider: &Provider,
        key: Option<&str>,
        request: &Request,
    ) -> Result<String, RpcError> {
        let answer = self.ai.client.complete(provider, key, request).await?;
        self.record_usage(session, provider, answer.usage).await;
        Ok(answer.text)
    }

    async fn record_usage(
        &self,
        session: &Arc<Session>,
        provider: &Provider,
        usage: postflow_ai::Usage,
    ) {
        let id = provider.id.clone();
        let _ = session
            .db(move |s| s.add_ai_usage(&id, usage.input, usage.output))
            .await;
    }

    async fn save_result(
        &self,
        session: &Arc<Session>,
        item_id: &str,
        task: &str,
        lang: &str,
        provider: &Provider,
        content: String,
    ) -> Result<AiResult, RpcError> {
        let result = AiResult {
            task: task.to_string(),
            lang: lang.to_string(),
            provider: provider.id.clone(),
            model: provider.model.clone(),
            content,
            created_at: now(),
        };
        let (id, saved) = (item_id.to_string(), result.clone());
        session.db(move |s| s.put_ai_result(&id, &saved)).await?;
        Ok(result)
    }

    /// 总结。太长的先分段整理要点，再从要点写总结。
    async fn run_summary(
        &self,
        session: &Arc<Session>,
        request_id: &str,
        doc: &ItemDoc,
        provider: &Provider,
        key: Option<String>,
        lang: &str,
    ) -> Result<Value, RpcError> {
        let request = if text::estimate_tokens(&doc.text) > prompts::DOC_BUDGET {
            let mut notes = Vec::new();
            let parts = text::split(&doc.text, prompts::PART_BUDGET);
            let total = parts.len().min(MAX_PARTS);
            for (i, part) in parts.iter().take(MAX_PARTS).enumerate() {
                let request = prompts::summary_notes(doc.doc(), part, i + 1, total, lang);
                notes.push(
                    self.complete_ai(session, provider, key.as_deref(), &request)
                        .await?,
                );
            }
            prompts::summary_from_notes(doc.doc(), &notes, lang)
        } else {
            prompts::summary(doc.doc(), lang)
        };
        let text = self
            .stream_ai(session, request_id, provider, key, &request)
            .await?;
        let saved = self
            .save_result(session, &doc.item.id, "summary", lang, provider, text)
            .await?;
        Ok(json!({ "task": "summary", "result": saved }))
    }

    async fn run_keywords(
        &self,
        session: &Arc<Session>,
        doc: &ItemDoc,
        provider: &Provider,
        key: Option<String>,
        lang: &str,
    ) -> Result<Value, RpcError> {
        let answer = self
            .complete_ai(
                session,
                provider,
                key.as_deref(),
                &prompts::keywords(doc.doc(), lang),
            )
            .await?;
        let words = prompts::parse_keywords(&answer);
        if words.is_empty() {
            return Err(RpcError::new(
                "ai_bad_response",
                "no keywords in the answer",
            ));
        }
        let content = serde_json::to_string(&words).unwrap_or_default();
        let saved = self
            .save_result(session, &doc.item.id, "keywords", lang, provider, content)
            .await?;
        Ok(json!({ "task": "keywords", "result": saved }))
    }

    /// 列表里一句话的摘要。已有的直接返回，缺的几条一批生成。不能发的邮件跳过。
    async fn abstracts(
        self: &Arc<Self>,
        session: &Arc<Session>,
        ids: Vec<String>,
        lang: &str,
    ) -> Result<HashMap<String, String>, RpcError> {
        let ids: Vec<String> = ids.into_iter().take(48).collect();
        let (wanted, l) = (ids.clone(), lang.to_string());
        let mut out = session
            .db(move |s| s.ai_results_for(&wanted, "abstract", &l))
            .await?;
        let missing: Vec<String> = ids.into_iter().filter(|id| !out.contains_key(id)).collect();
        if missing.is_empty() {
            return Ok(out);
        }
        let settings = self.ai_settings(session).await?;
        let provider = settings.pick(Use::Fast)?;
        let key = self.ai_key(&provider.id).await;
        let mut docs = Vec::new();
        for id in &missing {
            match self.item_doc(session, id).await {
                Ok(doc) if settings.check(&doc.item, &provider).is_ok() => docs.push(doc),
                _ => {}
            }
        }
        for batch in docs.chunks(ABSTRACT_BATCH) {
            // 提示词里用短编号，省 token
            let items: Vec<(String, Doc<'_>)> = batch
                .iter()
                .enumerate()
                .map(|(i, d)| ((i + 1).to_string(), d.doc()))
                .collect();
            let answer = self
                .complete_ai(
                    session,
                    &provider,
                    key.as_deref(),
                    &prompts::abstracts(&items, lang),
                )
                .await?;
            for (number, text) in prompts::parse_abstracts(&answer) {
                let Some(doc) = number
                    .parse::<usize>()
                    .ok()
                    .and_then(|n| batch.get(n.wrapping_sub(1)))
                else {
                    continue;
                };
                self.save_result(
                    session,
                    &doc.item.id,
                    "abstract",
                    lang,
                    &provider,
                    text.clone(),
                )
                .await?;
                out.insert(doc.item.id.clone(), text);
            }
        }
        Ok(out)
    }
}
