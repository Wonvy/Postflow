//! 一个假的 OpenAI 兼容 AI 服务，跑在本机。测试和演示都用它，不需要真的 API Key。
//!
//! 它看系统提示里要的是什么（关键词、摘要、译文、分类），给出格式正确、内容可预期的回答；
//! 其余的当作总结或对话，用正文里的句子拼一段回答。支持流式输出。

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
                let text = answer(&body);
                let usage = json!({
                    "prompt_tokens": body.to_string().len() / 4,
                    "completion_tokens": text.len() / 4,
                });
                if body["stream"].as_bool() == Some(true) {
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
                        )
                        .await?;
                    let chars: Vec<char> = text.chars().collect();
                    for piece in chars.chunks(6) {
                        let delta: String = piece.iter().collect();
                        let event = json!({"choices": [{"delta": {"content": delta}}]});
                        stream
                            .write_all(format!("data: {event}\n\n").as_bytes())
                            .await?;
                        tokio::time::sleep(Duration::from_millis(12)).await;
                    }
                    let last = json!({"choices": [{"delta": {}, "finish_reason": "stop"}], "usage": usage});
                    stream
                        .write_all(format!("data: {last}\n\ndata: [DONE]\n\n").as_bytes())
                        .await?;
                    stream.shutdown().await
                } else {
                    let reply = json!({
                        "choices": [{"message": {"role": "assistant", "content": text}, "finish_reason": "stop"}],
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

async fn respond(stream: &mut TcpStream, status: u16, body: &Value) -> std::io::Result<()> {
    let body = body.to_string();
    let reason = match status {
        200 => "OK",
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
