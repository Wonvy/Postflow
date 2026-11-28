//! 翻译列表：把列表里看得到的条目的标题和摘要翻成目标语言。
//!
//! 用设置里选的翻译方式：离线模型，或者 AI 服务。已经是目标语言的跳过；离线模型缺了的告诉界面，
//! 让用户决定要不要下载。译文按条目和语言存在本机，滚回来不用再翻。

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use shibapost_store::ItemSummary;
use shibapost_translate::{Segment, detect};

use crate::ai_rpc::Use;
use crate::rpc::{RpcError, parse, reply};
use crate::{Core, Session};

/// 缓存的任务名，两种翻译方式各存各的：换了方式就重新翻。
///
/// 离线的缓存在 0.1.8 换了名字：之前的译文里可能夹着 `<0xE7>` 这样没拼回去的字节。
fn task(engine: &str) -> &'static str {
    if engine == "ai" {
        "list_translation_ai"
    } else {
        "list_translation_offline_2"
    }
}
/// 一次最多处理这么多条。
const MAX_ITEMS: usize = 60;

#[derive(Deserialize)]
struct ItemsParams {
    ids: Vec<String>,
    to: String,
    /// offline 或 ai。
    #[serde(default)]
    engine: String,
}

/// 一条的译文。
#[derive(Clone, Serialize, Deserialize)]
struct Translated {
    from: String,
    title: String,
    summary: String,
}

/// 简体和繁体、两种挪威语之间不翻。
fn same_language(a: &str, b: &str) -> bool {
    let base = |c: &str| match c {
        "zh_hant" => "zh".to_string(),
        "no" => "nb".to_string(),
        other => other.to_string(),
    };
    base(a) == base(b)
}

impl Core {
    pub(crate) async fn translate_items(
        self: &Arc<Self>,
        params: Value,
        session: &Arc<Session>,
    ) -> Result<Value, RpcError> {
        let p: ItemsParams = parse(params)?;
        let task = task(&p.engine);
        let ids: Vec<String> = p.ids.into_iter().take(MAX_ITEMS).collect();
        let to = p.to.clone();
        let (wanted, lang) = (ids.clone(), to.clone());
        let cached = session
            .db(move |s| s.ai_results_for(&wanted, task, &lang))
            .await?;
        let mut out: HashMap<String, Translated> = cached
            .into_iter()
            .filter_map(|(id, content)| Some((id, serde_json::from_str(&content).ok()?)))
            .collect();

        let missing_ids: Vec<String> = ids.into_iter().filter(|id| !out.contains_key(id)).collect();
        let items: Vec<ItemSummary> = session
            .db(move |s| {
                let mut list = Vec::new();
                for id in &missing_ids {
                    if let Some(d) = s.item(id)? {
                        list.push(d.item);
                    }
                }
                Ok(list)
            })
            .await?;

        // 认出每条的语言，已经是目标语言的不翻
        let mut same = Vec::new();
        let mut todo: Vec<(ItemSummary, String)> = Vec::new();
        for item in items {
            match item_language(&item.title, &item.summary) {
                Some(from) if !same_language(&from, &to) => todo.push((item, from)),
                _ => same.push(item.id.clone()),
            }
        }

        let mut missing_models = Vec::new();
        let translated = if p.engine == "ai" {
            self.list_with_ai(session, &todo, &to).await?
        } else {
            let (done, missing) = self.list_offline(&todo, &to).await;
            missing_models = missing;
            done
        };
        for (id, t) in translated {
            let content = serde_json::to_string(&t).unwrap_or_default();
            let (item, lang) = (id.clone(), to.clone());
            let _ = session
                .db(move |s| {
                    s.put_ai_result(
                        &item,
                        &shibapost_store::AiResult {
                            task: task.to_string(),
                            lang,
                            provider: String::new(),
                            model: String::new(),
                            content,
                            created_at: crate::refresh::now(),
                        },
                    )
                })
                .await;
            out.insert(id, t);
        }
        reply(json!({ "items": out, "same": same, "missing": missing_models }))
    }

    /// 离线模型：按原文语言分组翻。缺模型的那组跳过，把要下载的模型告诉界面。
    async fn list_offline(
        &self,
        todo: &[(ItemSummary, String)],
        to: &str,
    ) -> (Vec<(String, Translated)>, Vec<Value>) {
        let mut groups: BTreeMap<&str, Vec<&ItemSummary>> = BTreeMap::new();
        for (item, from) in todo {
            groups.entry(from.as_str()).or_default().push(item);
        }
        let mut done = Vec::new();
        let mut missing: Vec<Value> = Vec::new();
        for (from, items) in groups {
            match self.translator.plan("", Some(from), to).await {
                Ok(plan) if plan.missing.is_empty() => {}
                Ok(plan) => {
                    for m in plan.missing {
                        if !missing.iter().any(|x| x["pair"] == m.pair) {
                            missing.push(json!(m));
                        }
                    }
                    continue;
                }
                Err(e) => {
                    tracing::debug!(%from, %to, "list translation skipped: {e}");
                    continue;
                }
            }
            let segments: Vec<Segment> = items
                .iter()
                .flat_map(|i| {
                    [
                        Segment::plain(i.title.clone()),
                        Segment::plain(i.summary.clone()),
                    ]
                })
                .collect();
            let translator = self.translator.clone();
            let (f, t) = (from.to_string(), to.to_string());
            let result =
                tokio::task::spawn_blocking(move || translator.translate(&f, &t, segments)).await;
            let Ok(Ok(out)) = result else {
                tracing::warn!(%from, %to, "list translation failed");
                continue;
            };
            for (item, pair) in items.iter().zip(out.chunks(2)) {
                if let [title, summary] = pair {
                    done.push((
                        item.id.clone(),
                        Translated {
                            from: from.to_string(),
                            title: title.text.clone(),
                            summary: summary.text.clone(),
                        },
                    ));
                }
            }
        }
        (done, missing)
    }

    /// AI 服务：标题和摘要一起分批翻。不能发给云端的邮件跳过。
    async fn list_with_ai(
        &self,
        session: &Arc<Session>,
        todo: &[(ItemSummary, String)],
        to: &str,
    ) -> Result<Vec<(String, Translated)>, RpcError> {
        if todo.is_empty() {
            return Ok(Vec::new());
        }
        let settings = self.ai_settings(session).await?;
        let provider = settings.pick(Use::Main)?;
        let allowed: Vec<&(ItemSummary, String)> = todo
            .iter()
            .filter(|(item, _)| settings.check(item, &provider).is_ok())
            .collect();
        let key = self.ai_key(&provider.id).await;
        let texts: Vec<String> = allowed
            .iter()
            .flat_map(|(i, _)| [i.title.clone(), i.summary.clone()])
            .collect();
        let out = self
            .translate_tagged(session, &provider, key.as_deref(), &texts, "", to)
            .await?;
        Ok(allowed
            .iter()
            .zip(out.chunks(2))
            .filter_map(|((item, from), pair)| match pair {
                [Some(title), summary] => Some((
                    item.id.clone(),
                    Translated {
                        from: from.clone(),
                        title: title.clone(),
                        summary: summary.clone().unwrap_or_else(|| item.summary.clone()),
                    },
                )),
                _ => None,
            })
            .collect())
    }
}

/// 标题和摘要各自识别后再决定来源语言。订阅描述常混有英文站点名、引用或模板文字；
/// 合并识别会把整条内容误判成另一种语言，进而要求用户下载不需要的模型。
fn item_language(title: &str, summary: &str) -> Option<String> {
    match (detect::detect(title), detect::detect(summary)) {
        (Some(title), Some(summary)) if same_language(&title, &summary) => Some(title),
        // 两部分互相矛盾，无法可靠代表整条内容，跳过以免提示错误语言模型。
        (Some(_), Some(_)) => None,
        (Some(language), None) | (None, Some(language)) => Some(language),
        (None, None) => None,
    }
}
