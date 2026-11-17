//! 离线翻译的方法。和数据目录无关，模型放在本机私有目录里，不参与同步。

use std::sync::Arc;

use postflow_translate::{Mark, Segment};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::rpc::{RpcError, parse, reply};
use crate::{Core, Event, ModelState};

/// 一次最多翻这么多段、这么多字，防止误传一整个邮箱进来。
const MAX_SEGMENTS: usize = 5000;
const MAX_CHARS: usize = 500_000;

impl From<postflow_translate::Error> for RpcError {
    fn from(err: postflow_translate::Error) -> Self {
        RpcError::new(err.code(), err.to_string())
    }
}

#[derive(Deserialize)]
struct PlanParams {
    /// 用来识别原文语言的一段文字。
    #[serde(default)]
    text: String,
    #[serde(default)]
    from: Option<String>,
    to: String,
}

#[derive(Deserialize)]
struct DetectParams {
    text: String,
}

#[derive(Deserialize)]
struct DownloadParams {
    pairs: Vec<String>,
}

#[derive(Deserialize)]
struct PairParam {
    pair: String,
}

#[derive(Deserialize)]
struct TextParams {
    from: String,
    to: String,
    segments: Vec<SegmentIn>,
}

/// 界面传来的一段：文字，加上每个行内元素覆盖的 `[start, end)`（UTF-16 下标）。
#[derive(Deserialize)]
struct SegmentIn {
    text: String,
    #[serde(default)]
    marks: Vec<[usize; 2]>,
}

/// 译文：每个标记是 `[start, end, 原文里第几个元素]`。
#[derive(Serialize)]
struct SegmentOut {
    text: String,
    marks: Vec<[usize; 3]>,
}

impl Core {
    pub(crate) async fn translate_rpc(
        self: &Arc<Self>,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        match method {
            "translate.languages" => reply(self.translator.languages().await),
            // 只识别语言，不需要模型。用 AI 翻译时先用它判断原文是不是已经是目标语言
            "translate.detect" => {
                let p: DetectParams = parse(params)?;
                reply(json!({ "from": postflow_translate::detect::detect(&p.text) }))
            }
            "translate.plan" => {
                let p: PlanParams = parse(params)?;
                let plan = self
                    .translator
                    .plan(&p.text, p.from.as_deref(), &p.to)
                    .await?;
                reply(plan)
            }
            "translate.download" => {
                let p: DownloadParams = parse(params)?;
                for pair in p.pairs {
                    if self.translator.begin_download(&pair)? {
                        tokio::spawn(download(self.clone(), pair));
                    }
                }
                reply(json!({}))
            }
            "translate.remove" => {
                let p: PairParam = parse(params)?;
                let translator = self.translator.clone();
                tokio::task::spawn_blocking(move || translator.remove(&p.pair))
                    .await
                    .map_err(|e| RpcError::new("internal", e.to_string()))??;
                reply(json!({}))
            }
            "translate.text" => {
                let p: TextParams = parse(params)?;
                let chars: usize = p.segments.iter().map(|s| s.text.len()).sum();
                if p.segments.len() > MAX_SEGMENTS || chars > MAX_CHARS {
                    return Err(RpcError::new(
                        "bad_params",
                        "too much text to translate at once",
                    ));
                }
                let segments = p
                    .segments
                    .into_iter()
                    .map(|s| Segment {
                        text: s.text,
                        marks: s
                            .marks
                            .into_iter()
                            .enumerate()
                            .map(|(id, [start, end])| Mark { start, end, id })
                            .collect(),
                    })
                    .collect();
                let translator = self.translator.clone();
                let out = tokio::task::spawn_blocking(move || {
                    translator.translate(&p.from, &p.to, segments)
                })
                .await
                .map_err(|e| RpcError::new("internal", e.to_string()))??;
                let out: Vec<SegmentOut> = out
                    .into_iter()
                    .map(|s| SegmentOut {
                        text: s.text,
                        marks: s
                            .marks
                            .into_iter()
                            .map(|m| [m.start, m.end, m.id])
                            .collect(),
                    })
                    .collect();
                reply(json!({ "segments": out }))
            }
            _ => Err(RpcError::new(
                "unknown_method",
                format!("unknown method: {method}"),
            )),
        }
    }
}

/// 在后台下载一个语言对，进度通过事件告诉界面。
async fn download(core: Arc<Core>, pair: String) {
    let result = core
        .translator
        .download(&pair, |p| {
            core.emit(Event::Model {
                pair: p.pair.clone(),
                received: p.received,
                total: p.total,
                state: ModelState::Downloading,
                error: None,
            })
        })
        .await;
    let (state, error) = match result {
        Ok(_) => (ModelState::Installed, None),
        Err(e) => {
            tracing::warn!("translation model {pair}: {e}");
            (ModelState::Failed, Some(e.code().to_string()))
        }
    };
    core.emit(Event::Model {
        pair,
        received: 0,
        total: 0,
        state,
        error,
    });
}
