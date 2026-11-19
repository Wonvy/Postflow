//! 一个假的 OpenAI 兼容 AI 服务，跑在本机。测试和演示都用它，不需要真的 API Key。
//!
//! 它看系统提示里要的是什么（关键词、摘要、译文、分类），给出格式正确、内容可预期的回答；
//! 其余的当作总结或对话，用正文里的句子拼一段回答。支持流式输出。
//!
//! 模型名决定它像哪一类服务，用来测「会思考的模型」：
//! - `…thinker…`：先思考再回答，思考在单独的字段里，也算在 `max_tokens` 里（像智谱 GLM-4.5）；
//!   请求里带 `thinking: {"type": "disabled"}` 就不思考。
//! - `…forced…`：不让关掉思考，带了那个参数就回 400。
//! - `…think-tags…`：把思考写在正文开头的 `<think>…</think>` 里。
//! - `…strict-cap…`：不认 `max_tokens`，带了就回 400（像 OpenAI 的推理模型）。
//! - `…legacy…`：不认 `thinking` 这个参数，带了就回 400，报错里也不说是哪个参数。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Clone)]
pub struct FakeAi {
    pub port: u16,
    key: Option<String>,
    /// 收到的每个对话请求的请求体，测试用来检查发出去了什么。
    requests: Arc<Mutex<Vec<Value>>>,
}

impl FakeAi {
    /// 起一个服务。`key` 为空时不检查密钥。端口随机。
    pub async fn start(key: Option<&str>) -> FakeAi {
        Self::start_on(key, 0).await
    }

    pub async fn start_on(key: Option<&str>, port: u16) -> FakeAi {
        let listener = TcpListener::bind(("127.0.0.1", port))
            .await
            .expect("bind fake ai");
        let port = listener.local_addr().expect("local addr").port();
        let server = FakeAi {
            port,
            key: key.map(str::to_string),
            requests: Arc::new(Mutex::new(Vec::new())),
        };
        let accept = server.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let conn = accept.clone();
                tokio::spawn(async move {
                    let _ = conn.serve(stream).await;
                });
            }
        });
        server
    }

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/v1", self.port)
    }

    pub fn requests(&self) -> Vec<Value> {
        self.requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    async fn serve(&self, mut stream: TcpStream) -> std::io::Result<()> {
        let mut data = Vec::new();
        let mut buf = [0u8; 8192];
        let header_end = loop {
            let n = stream.read(&mut buf).await?;
            if n == 0 {
                return Ok(());
            }
            data.extend_from_slice(&buf[..n]);
            if let Some(i) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let head = String::from_utf8_lossy(&data[..header_end]).to_string();
        let mut lines = head.lines();
        let request_line = lines.next().unwrap_or_default();
        let mut parts = request_line.split_whitespace();
        let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        let mut length = 0usize;
        let mut auth = String::new();
        for line in lines {
            if let Some((name, value)) = line.split_once(':') {
                match name.trim().to_ascii_lowercase().as_str() {
                    "content-length" => length = value.trim().parse().unwrap_or(0),
                    "authorization" => auth = value.trim().to_string(),
                    _ => {}
                }
            }
        }
        while data.len() < header_end + length {
            let n = stream.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            data.extend_from_slice(&buf[..n]);
        }
        let body: Value = serde_json::from_slice(&data[header_end..]).unwrap_or(Value::Null);

        if let Some(key) = &self.key
            && auth != format!("Bearer {key}")
        {
            return respond(
                &mut stream,
                401,
                &json!({"error": {"message": "Incorrect API key provided"}}),
            )
            .await;
        }
        match (method, path) {
            ("GET", "/v1/models") => {
                let list =
                    json!({"object": "list", "data": [{"id": "fake-large"}, {"id": "fake-small"}]});
                respond(&mut stream, 200, &list).await
            }
            ("POST", "/v1/chat/completions") => {
                self.requests
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(body.clone());
                let model = body["model"].as_str().unwrap_or_default().to_string();
                let max = body["max_tokens"].as_u64().map(|m| m as usize);
                if model.contains("strict-cap") && max.is_some() {
                    let err = json!({"error": {
                        "message": "Unsupported parameter: 'max_tokens' is not supported with this model. Use 'max_completion_tokens' instead.",
                        "type": "invalid_request_error", "param": "max_tokens",
                    }});
                    return respond(&mut stream, 400, &err).await;
                }
                let quiet = body["thinking"]["type"] == "disabled";
                if model.contains("forced") && quiet {
                    let err = json!({"error": {"code": "1210", "message": "thinking.type 不支持设置为 disabled"}});
                    return respond(&mut stream, 400, &err).await;
                }
                if model.contains("legacy") && body.get("thinking").is_some() {
                    let err = json!({"error": {"code": "1210", "message": "API 调用参数有误，请检查文档。"}});
                    return respond(&mut stream, 400, &err).await;
                }
                let answer = answer(&body);
                // 会思考的模型：思考和回答加起来不能超过 max_tokens（按四个字节一个 token 算），超了就截断
                let limit = max.map_or(usize::MAX, |m| m.saturating_mul(4));
                let (thought, text, full) = if model.contains("thinker") && !quiet {
                    let thought = clip(THOUGHT, limit);
                    let text = clip(&answer, limit - thought.len());
                    (thought, text, THOUGHT.len() + answer.len())
                } else if model.contains("think-tags") {
                    let all = format!("<think>\n{THOUGHT}\n</think>\n\n{answer}");
                    let full = all.len();
                    (String::new(), clip(&all, limit), full)
                } else {
                    (String::new(), answer, 0)
                };
                let finish = if full > limit { "length" } else { "stop" };
                let usage = json!({
                    "prompt_tokens": body.to_string().len() / 4,
                    "completion_tokens": (thought.len() + text.len()) / 4,
                    "completion_tokens_details": { "reasoning_tokens": thought.len() / 4 },
                });
                if body["stream"].as_bool() == Some(true) {
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
                        )
                        .await?;
                    for (field, body_text, pace) in
                        [("reasoning_content", &thought, 1), ("content", &text, 12)]
                    {
                        let chars: Vec<char> = body_text.chars().collect();
                        for piece in chars.chunks(if field == "content" { 6 } else { 40 }) {
                            let delta: String = piece.iter().collect();
                            let event = json!({"choices": [{"delta": {field: delta}}]});
                            stream
                                .write_all(format!("data: {event}\n\n").as_bytes())
                                .await?;
                            tokio::time::sleep(Duration::from_millis(pace)).await;
                        }
                    }
                    let last = json!({"choices": [{"delta": {}, "finish_reason": finish}], "usage": usage});
                    stream
                        .write_all(format!("data: {last}\n\ndata: [DONE]\n\n").as_bytes())
                        .await?;
                    stream.shutdown().await
                } else {
                    let mut message = json!({"role": "assistant", "content": text});
                    if !thought.is_empty() {
                        message["reasoning_content"] = json!(thought);
                    }
                    let reply = json!({
                        "choices": [{"message": message, "finish_reason": finish}],
                        "usage": usage,
                    });
                    respond(&mut stream, 200, &reply).await
                }
            }
            _ => {
                respond(
                    &mut stream,
                    404,
                    &json!({"error": {"message": "not found"}}),
                )
                .await
            }
        }
    }
}

/// 「会思考」的假模型每次想的内容：三百多个 token，比关键词任务的输出上限（300）还多。
const THOUGHT: &str = "Let me read the text carefully and work out what matters most to the reader before I answer. \
First I identify who wrote it and what it is about. Then I look for the main claim, the numbers, the dates and \
anything the reader is asked to do. I should keep the answer short and follow the requested format exactly. \
I will double check that I am writing in the requested language and that I am not adding a heading or closing \
remarks. Now I restate the key points to myself: the first sentence usually carries the main point, the rest adds \
supporting facts. I compare a few possible phrasings and choose the clearest one. I also make sure the output is \
valid for the requested structure, with no extra commentary before or after it. One more pass over the text to \
confirm nothing important was missed, including qualifiers, deadlines and names. That should be enough; I can \
write the final answer now, carefully, one piece at a time, keeping it faithful to the source text throughout. \
Before writing I list the candidate points once more and drop the ones that are merely background or repetition. \
Finally I check the length: the reader is busy, so every sentence has to earn its place, and anything that only \
restates the title can go. With that settled, the structure is clear and I am ready to produce the answer itself.";

/// 截到最多 `max` 个字节（不切断一个字）。
fn clip(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

async fn respond(stream: &mut TcpStream, status: u16, body: &Value) -> std::io::Result<()> {
    let body = body.to_string();
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        _ => "Not Found",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body.as_bytes()).await?;
    stream.shutdown().await
}

/// 按系统提示要的东西编一个回答。
fn answer(body: &Value) -> String {
    let messages = body["messages"].as_array().cloned().unwrap_or_default();
    let system = messages
        .iter()
        .find(|m| m["role"] == "system")
        .and_then(|m| m["content"].as_str())
        .unwrap_or_default()
        .to_string();
    let user = messages
        .iter()
        .rev()
        .find(|m| m["role"] == "user")
        .and_then(|m| m["content"].as_str())
        .unwrap_or_default()
        .to_string();
    let chinese = system.contains("Simplified Chinese");

    if system.contains("\"keywords\"") {
        let words: Vec<String> = text_of(&user)
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.chars().count() >= 5)
            .take(5)
            .map(str::to_string)
            .collect();
        return json!({ "keywords": words }).to_string();
    }
    if system.contains("\"abstract\"") {
        let items: Vec<Value> = blocks(&user)
            .into_iter()
            .map(|(id, title)| {
                let text = if chinese {
                    format!("关于「{title}」的简短介绍。")
                } else {
                    format!("A short note about {title}.")
                };
                json!({ "id": id, "abstract": text })
            })
            .collect();
        return json!({ "items": items }).to_string();
    }
    if system.contains("\"segments\"") {
        let input: Value = serde_json::from_str(&user).unwrap_or(Value::Null);
        let segments: Vec<String> = input["segments"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|s| format!("〔译〕{}", s.as_str().unwrap_or_default()))
            .collect();
        return json!({ "segments": segments }).to_string();
    }
    if system.starts_with("Sort each item") {
        let first = system
            .lines()
            .find_map(|l| l.strip_prefix("- "))
            .map(|l| l.split(':').next().unwrap_or(l).trim().to_string());
        let items: Vec<Value> = blocks(&user)
            .into_iter()
            .map(|(id, _)| json!({ "id": id, "category": first }))
            .collect();
        return json!({ "items": items }).to_string();
    }
    if system.contains("<document>") {
        return if chinese {
            format!(
                "关于你的问题「{user}」：根据原文，{}",
                first_sentence(&system)
            )
        } else {
            format!(
                "About \"{user}\": according to the text, {}",
                first_sentence(&system)
            )
        };
    }
    // 总结，或长文分段整理要点
    let sentences = sentences(&text_of(&user));
    let lead = sentences
        .first()
        .cloned()
        .unwrap_or_else(|| "Nothing to summarize.".into());
    let mut out = format!("**{lead}**\n");
    for s in sentences.iter().skip(1).take(4) {
        out.push_str(&format!("\n- {s}"));
    }
    out
}

/// 提示词里正文的部分：去掉 Title、From 这几行。
fn text_of(user: &str) -> String {
    user.lines()
        .filter(|l| !l.starts_with("Title:") && !l.starts_with("From:") && !l.starts_with("id:"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn sentences(text: &str) -> Vec<String> {
    text.split_inclusive(['.', '。', '!', '?', '！', '？'])
        .map(str::trim)
        .filter(|s| s.chars().count() > 3)
        .map(str::to_string)
        .collect()
}

fn first_sentence(text: &str) -> String {
    let body = text.split("<document>").nth(1).unwrap_or(text);
    sentences(&text_of(body))
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// 一批条目里每条的 ID 和标题。
fn blocks(user: &str) -> Vec<(String, String)> {
    user.split("\n---\n")
        .filter_map(|block| {
            let id = block
                .lines()
                .find_map(|l| l.trim().strip_prefix("id: "))?
                .trim()
                .to_string();
            let title = block
                .lines()
                .find_map(|l| l.trim().strip_prefix("Title: "))
                .unwrap_or_default()
                .to_string();
            Some((id, title))
        })
        .collect()
}
