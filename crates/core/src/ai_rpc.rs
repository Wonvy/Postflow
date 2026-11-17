//! AI 功能的方法：配置服务、总结、关键词、列表摘要、对话。
//!
//! 服务的配置（地址、模型）放在同步的设置里，API Key 只存在这台设备的系统钥匙串里。
//! 生成的内容和对话记录只存在本机。邮件内容默认不发给云端服务，本机运行的服务不受限制。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use postflow_ai::{Client, Doc, Message, Provider, Request, markup, prompts, text};
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
const CATEGORIES: &str = "ai.categories";
const AUTO_CLASSIFY: &str = "ai.auto_classify";
/// 分类一次请求处理几条。只发标题和开头一小段，几条一起省请求。
const CLASSIFY_BATCH: usize = 10;
/// 后台自动分类每次最多处理这么多条，只看最近几天的。
const AUTO_CLASSIFY_LIMIT: u32 = 30;
const AUTO_CLASSIFY_DAYS: i64 = 3;
/// 长文分段整理要点时最多分这么多段，更长的只看前面这些。
const MAX_PARTS: usize = 8;
/// 列表摘要一次请求处理几条。
const ABSTRACT_BATCH: usize = 8;
/// AI 翻译时每次请求最多带这么多段、这么多 token 的原文。
const TRANSLATE_CHUNK_ITEMS: usize = 30;
const TRANSLATE_CHUNK_TOKENS: usize = 1_800;
/// 同时进行的翻译请求数。
const TRANSLATE_PARALLEL: usize = 3;

/// 正在进行的请求，可以随时取消。
#[derive(Default)]
pub(crate) struct AiRuntime {
    pub(crate) client: Client,
    running: Mutex<HashMap<String, tokio::task::AbortHandle>>,
    next: AtomicU64,
    /// 后台分类正在进行，同一时间只跑一个。
    classifying: AtomicBool,
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
    pub categories: Vec<postflow_ai::Category>,
    pub auto_classify: bool,
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

/// 要翻译的一段：文字和行内元素的位置（UTF-16），和离线翻译的格式一样。
#[derive(Clone, Serialize, Deserialize)]
struct SourceSegment {
    text: String,
    #[serde(default)]
    marks: Vec<[usize; 2]>,
}

/// 译文：每个标记是 `[start, end, 原文里第几个元素]`。
#[derive(Clone, Serialize, Deserialize)]
struct TranslatedSegment {
    text: String,
    marks: Vec<[usize; 3]>,
}

#[derive(Deserialize)]
struct TranslateParams {
    /// 哪一条，用来判断能不能发给云端服务，以及缓存译文。
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    from: String,
    to: String,
    segments: Vec<SourceSegment>,
}

#[derive(Serialize, Deserialize)]
struct CachedTranslation {
    hash: String,
    segments: Vec<TranslatedSegment>,
}

/// 一批段落的译文：第几段，以及翻出来的文字（没翻出来是 `None`）。
type Translated = Vec<(usize, Option<String>)>;

/// 原文、语言和所用模型的指纹，原文变了缓存就不算数。
fn fingerprint(p: &TranslateParams, provider: &Provider) -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    (&p.from, &p.to, &provider.id, &provider.model).hash(&mut hasher);
    serde_json::to_string(&p.segments)
        .unwrap_or_default()
        .hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[derive(Deserialize)]
struct ClassifyParams {
    /// 不给就分最近还没分类的。
    #[serde(default)]
    ids: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct LabelParams {
    ids: Vec<String>,
    /// 空表示「哪一类都不是」。
    #[serde(default)]
    label: Option<String>,
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
            "ai.translate" => {
                let p: TranslateParams = parse(params)?;
                if p.segments.len() > 3000 {
                    return Err(RpcError::new(
                        "bad_params",
                        "too much text to translate at once",
                    ));
                }
                let settings = self.ai_settings(session).await?;
                let provider = settings.pick(Use::Main)?;
                if let Some(id) = p.id.clone()
                    && let Some(detail) = session.db(move |s| s.item(&id)).await?
                {
                    settings.check(&detail.item, &provider)?;
                }
                let hash = fingerprint(&p, &provider);
                if let Some(id) = p.id.clone() {
                    let to = p.to.clone();
                    if let Some(cached) = session
                        .db(move |s| s.ai_result(&id, "translate", &to))
                        .await?
                        && let Ok(cached) =
                            serde_json::from_str::<CachedTranslation>(&cached.content)
                        && cached.hash == hash
                    {
                        return reply(json!({ "segments": cached.segments }));
                    }
                }
                let key = self.ai_key(&provider.id).await;
                let tagged: Vec<String> = p
                    .segments
                    .iter()
                    .map(|s| {
                        let marks: Vec<markup::Span> = s
                            .marks
                            .iter()
                            .enumerate()
                            .map(|(i, [a, b])| (*a, *b, i))
                            .collect();
                        markup::tag(&s.text, &marks)
                    })
                    .collect();
                let translated = self
                    .translate_tagged(session, &provider, key.as_deref(), &tagged, &p.from, &p.to)
                    .await?;
                let segments: Vec<TranslatedSegment> = p
                    .segments
                    .iter()
                    .zip(translated)
                    .map(|(source, out)| match out {
                        Some(text) => {
                            let (text, spans) = markup::untag(&text);
                            TranslatedSegment {
                                text,
                                marks: spans.into_iter().map(|(a, b, i)| [a, b, i]).collect(),
                            }
                        }
                        // 这一段没翻出来就保留原文
                        None => TranslatedSegment {
                            text: source.text.clone(),
                            marks: source
                                .marks
                                .iter()
                                .enumerate()
                                .map(|(i, [a, b])| [*a, *b, i])
                                .collect(),
                        },
                    })
                    .collect();
                if let Some(id) = p.id.clone() {
                    let content = serde_json::to_string(&CachedTranslation {
                        hash,
                        segments: segments.clone(),
                    })
                    .unwrap_or_default();
                    self.save_result(session, &id, "translate", &p.to, &provider, content)
                        .await?;
                }
                reply(json!({ "segments": segments }))
            }
            "ai.classify" => {
                let p: ClassifyParams = parse(params)?;
                let settings = self.ai_settings(session).await?;
                let items = match p.ids {
                    Some(ids) => {
                        session
                            .db(move |s| {
                                let mut out = Vec::new();
                                for id in ids.iter().take(200) {
                                    if let Some(d) = s.item(id)? {
                                        out.push(d.item);
                                    }
                                }
                                Ok(out)
                            })
                            .await?
                    }
                    None => {
                        let mail = settings.allow_mail || settings.pick(Use::Fast)?.is_local();
                        let since = now() - 7 * 86_400_000;
                        session.db(move |s| s.unlabeled(since, mail, 60)).await?
                    }
                };
                let labels = self.classify(session, &settings, items).await?;
                reply(json!({ "labels": labels }))
            }
            "items.label" => {
                let p: LabelParams = parse(params)?;
                let label = p
                    .label
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty());
                let pairs: Vec<(String, Option<String>)> =
                    p.ids.into_iter().map(|id| (id, label.clone())).collect();
                session.db(move |s| s.set_labels(&pairs, true)).await?;
                self.emit_changed(&[Scope::Feeds, Scope::Items], client);
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
            categories: settings
                .get(CATEGORIES)
                .cloned()
                .and_then(|v| serde_json::from_value::<Vec<postflow_ai::Category>>(v).ok())
                .unwrap_or_default()
                .into_iter()
                .filter(|c| !c.name.trim().is_empty())
                .collect(),
            auto_classify: settings
                .get(AUTO_CLASSIFY)
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
        tracing::debug!(provider = %provider.preset, model = %provider.model, "ai request (streamed)");
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
            .await
            .inspect_err(|e| {
                tracing::warn!(provider = %provider.preset, model = %provider.model, code = e.code(), "ai request failed: {e}")
            })?;
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
        tracing::debug!(provider = %provider.preset, model = %provider.model, "ai request");
        let answer = self.ai.client.complete(provider, key, request).await.inspect_err(|e| {
            tracing::warn!(provider = %provider.preset, model = %provider.model, code = e.code(), "ai request failed: {e}")
        })?;
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

    pub(crate) async fn save_result(
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

    /// 给一组条目分类，结果存在本机。只发标题、来源和摘要开头，不取邮件正文。
    async fn classify(
        &self,
        session: &Arc<Session>,
        settings: &AiSettings,
        items: Vec<ItemSummary>,
    ) -> Result<HashMap<String, Option<String>>, RpcError> {
        if settings.categories.is_empty() {
            return Err(RpcError::new(
                "ai_no_categories",
                "no categories have been set up",
            ));
        }
        let provider = settings.pick(Use::Fast)?;
        let key = self.ai_key(&provider.id).await;
        let items: Vec<ItemSummary> = items
            .into_iter()
            .filter(|i| settings.check(i, &provider).is_ok())
            .collect();
        let mut out = HashMap::new();
        for batch in items.chunks(CLASSIFY_BATCH) {
            let docs: Vec<(String, Doc<'_>)> = batch
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let source = if item.kind == "mail" {
                        item.author
                            .as_deref()
                            .or(item.sender_address.as_deref())
                            .unwrap_or("")
                    } else {
                        item.source_title.as_str()
                    };
                    (
                        (i + 1).to_string(),
                        Doc {
                            title: &item.title,
                            source,
                            text: &item.summary,
                        },
                    )
                })
                .collect();
            let answer = self
                .complete_ai(
                    session,
                    &provider,
                    key.as_deref(),
                    &prompts::classify(&docs, &settings.categories),
                )
                .await?;
            let parsed = prompts::parse_classes(&answer, &settings.categories);
            let mut labels = Vec::new();
            for (i, item) in batch.iter().enumerate() {
                // 回答里漏掉的条目不记，下次再分
                if let Some(label) = parsed.get(&(i + 1).to_string()) {
                    labels.push((item.id.clone(), label.clone()));
                    out.insert(item.id.clone(), label.clone());
                }
            }
            session.db(move |s| s.set_labels(&labels, false)).await?;
        }
        if !out.is_empty() {
            self.emit_changed(&[Scope::Feeds, Scope::Items], None);
        }
        Ok(out)
    }

    /// 后台定时调用：打开了自动分类时，把最近几天新来的条目分好。
    pub(crate) async fn auto_classify(self: &Arc<Self>, session: &Arc<Session>) {
        if self.ai.classifying.swap(true, Ordering::AcqRel) {
            return;
        }
        let result = async {
            let settings = self.ai_settings(session).await?;
            if !settings.auto_classify
                || settings.categories.is_empty()
                || settings.providers.is_empty()
            {
                return Ok(());
            }
            let mail = settings.allow_mail || settings.pick(Use::Fast)?.is_local();
            let since = now() - AUTO_CLASSIFY_DAYS * 86_400_000;
            let items = session
                .db(move |s| s.unlabeled(since, mail, AUTO_CLASSIFY_LIMIT))
                .await?;
            if !items.is_empty() {
                self.classify(session, &settings, items).await?;
            }
            Ok::<(), RpcError>(())
        }
        .await;
        if let Err(e) = result {
            tracing::warn!("auto classify: {e}");
        }
        self.ai.classifying.store(false, Ordering::Release);
    }

    /// 分批翻译带标记的段落。某一批的回答对不上数时拆成两半再试，单独一段也不行就留空（保留原文）。
    pub(crate) async fn translate_tagged(
        &self,
        session: &Arc<Session>,
        provider: &Provider,
        key: Option<&str>,
        texts: &[String],
        from: &str,
        to: &str,
    ) -> Result<Vec<Option<String>>, RpcError> {
        let mut chunks: Vec<Vec<usize>> = Vec::new();
        let mut current: Vec<usize> = Vec::new();
        let mut cost = 0;
        for (i, t) in texts.iter().enumerate() {
            if t.trim().is_empty() {
                continue;
            }
            let c = text::estimate_tokens(t);
            if !current.is_empty()
                && (cost + c > TRANSLATE_CHUNK_TOKENS || current.len() >= TRANSLATE_CHUNK_ITEMS)
            {
                chunks.push(std::mem::take(&mut current));
                cost = 0;
            }
            current.push(i);
            cost += c;
        }
        if !current.is_empty() {
            chunks.push(current);
        }
        let results: Vec<Result<Translated, RpcError>> = futures::stream::iter(
            chunks
                .into_iter()
                .map(|chunk| self.translate_chunk(session, provider, key, texts, chunk, from, to)),
        )
        .buffer_unordered(TRANSLATE_PARALLEL)
        .collect()
        .await;
        let mut out = vec![None; texts.len()];
        for result in results {
            for (i, t) in result? {
                out[i] = t;
            }
        }
        Ok(out)
    }

    #[allow(clippy::too_many_arguments)]
    async fn translate_chunk(
        &self,
        session: &Arc<Session>,
        provider: &Provider,
        key: Option<&str>,
        texts: &[String],
        chunk: Vec<usize>,
        from: &str,
        to: &str,
    ) -> Result<Translated, RpcError> {
        let mut pending = vec![chunk];
        let mut done = Vec::new();
        while let Some(part) = pending.pop() {
            let inputs: Vec<String> = part.iter().map(|&i| texts[i].clone()).collect();
            let answer = self
                .complete_ai(
                    session,
                    provider,
                    key,
                    &prompts::translate(&inputs, from, to),
                )
                .await?;
            match prompts::parse_translation(&answer, inputs.len()) {
                Some(list) => done.extend(part.iter().copied().zip(list.into_iter().map(Some))),
                None if part.len() > 1 => {
                    let mid = part.len() / 2;
                    pending.push(part[mid..].to_vec());
                    pending.push(part[..mid].to_vec());
                }
                None => done.push((part[0], None)),
            }
        }
        Ok(done)
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
