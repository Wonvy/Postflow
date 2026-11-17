//! 每个任务的提示词，以及从回答里取出结果。
//!
//! 结构化的结果（关键词、摘要、译文、分类）都要求模型输出 JSON，解析时容忍多余的文字。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::client::{Message, Request};
use crate::text::{extract_json, truncate};

/// 语言代码对应的英文名称，写进提示词里。
pub fn language_name(code: &str) -> String {
    match code {
        "zh" => "Simplified Chinese",
        "zh_hant" => "Traditional Chinese",
        "en" => "English",
        "ja" => "Japanese",
        "ko" => "Korean",
        "fr" => "French",
        "de" => "German",
        "es" => "Spanish",
        "pt" => "Portuguese",
        "it" => "Italian",
        "ru" => "Russian",
        "ar" => "Arabic",
        "vi" => "Vietnamese",
        "th" => "Thai",
        "id" => "Indonesian",
        "tr" => "Turkish",
        "nl" => "Dutch",
        "pl" => "Polish",
        "uk" => "Ukrainian",
        other => other,
    }
    .to_string()
}

/// 一篇文章或一封邮件。
#[derive(Debug, Clone, Copy)]
pub struct Doc<'a> {
    pub title: &'a str,
    /// 订阅源名称或发件人。
    pub source: &'a str,
    pub text: &'a str,
}

impl Doc<'_> {
    fn render(&self, max_tokens: usize) -> String {
        let (text, cut) = truncate(self.text, max_tokens);
        let mut out = format!(
            "Title: {}\nFrom: {}\n\n{}",
            self.title,
            self.source,
            text.trim()
        );
        if cut {
            out.push_str("\n\n[The rest of the text was cut for length.]");
        }
        out
    }
}

/// 一次请求里最多放这么多 token 的正文。更长的先分段整理要点。
pub const DOC_BUDGET: usize = 12_000;
/// 分段整理时每段的长度。
pub const PART_BUDGET: usize = 6_000;

fn summary_system(lang: &str) -> String {
    format!(
        "You summarize articles and emails for a busy reader. Write in {}. \
         Start with one sentence in bold that captures the main point, then 3 to 5 short bullet points \
         with the key facts, numbers, dates, decisions or requests. If an email asks the reader to do \
         something, make that a bullet that starts with what to do. Use Markdown. \
         No heading, no preamble, no closing remarks.",
        language_name(lang)
    )
}

pub fn summary(doc: Doc<'_>, lang: &str) -> Request {
    Request {
        messages: vec![
            Message::system(summary_system(lang)),
            Message::user(doc.render(DOC_BUDGET)),
        ],
        temperature: Some(0.3),
        max_tokens: Some(900),
        ..Default::default()
    }
}

/// 长文分段：先把每一段整理成要点。
pub fn summary_notes(doc: Doc<'_>, part: &str, index: usize, total: usize, lang: &str) -> Request {
    Request {
        messages: vec![
            Message::system(format!(
                "You are reading part {index} of {total} of a long text titled \"{}\". \
                 List the key points of this part as short bullet points in {}. Only the points.",
                doc.title,
                language_name(lang)
            )),
            Message::user(part.to_string()),
        ],
        temperature: Some(0.2),
        max_tokens: Some(700),
        ..Default::default()
    }
}

/// 长文分段：再从各段的要点写出总结。
pub fn summary_from_notes(doc: Doc<'_>, notes: &[String], lang: &str) -> Request {
    let joined = notes
        .iter()
        .enumerate()
        .map(|(i, n)| format!("Part {}:\n{}", i + 1, n.trim()))
        .collect::<Vec<_>>()
        .join("\n\n");
    Request {
        messages: vec![
            Message::system(summary_system(lang)),
            Message::user(format!(
                "Title: {}\nFrom: {}\n\nNotes taken from each part of the text:\n\n{joined}",
                doc.title, doc.source
            )),
        ],
        temperature: Some(0.3),
        max_tokens: Some(900),
        ..Default::default()
    }
}

pub fn keywords(doc: Doc<'_>, lang: &str) -> Request {
    Request {
        messages: vec![
            Message::system(format!(
                "Extract 3 to 8 keywords or short key phrases that best describe the text. Write them in {}, \
                 except names, products and technical terms that are usually kept in their original form. \
                 Reply with JSON only: {{\"keywords\": [\"...\"]}}",
                language_name(lang)
            )),
            Message::user(doc.render(4_000)),
        ],
        json: true,
        temperature: Some(0.2),
        max_tokens: Some(300),
    }
}

pub fn parse_keywords(answer: &str) -> Vec<String> {
    let value = extract_json(answer).unwrap_or(Value::Null);
    let list = value
        .get("keywords")
        .unwrap_or(&value)
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut out: Vec<String> = Vec::new();
    for word in list.iter().filter_map(Value::as_str).map(str::trim) {
        if !word.is_empty() && !out.iter().any(|w| w.eq_ignore_ascii_case(word)) {
            out.push(word.to_string());
        }
    }
    out.truncate(8);
    out
}

/// 列表里一句话的摘要。一次处理好几条，省请求次数。
pub fn abstracts(items: &[(String, Doc<'_>)], lang: &str) -> Request {
    let body = items
        .iter()
        .map(|(id, doc)| format!("id: {id}\n{}", doc.render(400)))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");
    Request {
        messages: vec![
            Message::system(format!(
                "For each item, write one short sentence in {} that says what it is about \
                 (at most 25 words, or 45 characters in Chinese or Japanese). Do not repeat the title. \
                 Reply with JSON only: {{\"items\": [{{\"id\": \"...\", \"abstract\": \"...\"}}]}}",
                language_name(lang)
            )),
            Message::user(body),
        ],
        json: true,
        temperature: Some(0.3),
        max_tokens: Some(120 * items.len() as u32 + 100),
    }
}

pub fn parse_abstracts(answer: &str) -> HashMap<String, String> {
    let value = extract_json(answer).unwrap_or(Value::Null);
    let list = value
        .get("items")
        .unwrap_or(&value)
        .as_array()
        .cloned()
        .unwrap_or_default();
    list.iter()
        .filter_map(|item| {
            let id = match item.get("id")? {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            let text = item.get("abstract")?.as_str()?.trim().to_string();
            (!text.is_empty()).then_some((id, text))
        })
        .collect()
}

/// 围绕一篇文章或一封邮件的对话。
pub fn chat(doc: Doc<'_>, history: &[Message], question: &str, lang: &str) -> Request {
    let mut messages = vec![Message::system(format!(
        "You help the reader with the text below. Answer in {} unless the reader writes in another \
         language; then answer in that language. Base your answers on the text and say so when it does \
         not contain the answer. Keep answers concise and use Markdown when it helps. When asked to draft \
         a reply to an email, write only the draft.\n\n<document>\n{}\n</document>",
        language_name(lang),
        doc.render(DOC_BUDGET)
    ))];
    // 只带最近的几轮，免得越聊越贵
    let start = history.len().saturating_sub(12);
    messages.extend(history[start..].iter().cloned());
    messages.push(Message::user(question));
    Request {
        messages,
        temperature: Some(0.5),
        max_tokens: Some(1500),
        ..Default::default()
    }
}

/// 翻译一组段落。行内元素写成 `<m0>…</m0>`，要求模型原样保留。
pub fn translate(segments: &[String], from: &str, to: &str) -> Request {
    let from = if from.is_empty() {
        "the original language".to_string()
    } else {
        language_name(from)
    };
    Request {
        messages: vec![
            Message::system(format!(
                "Translate each string in the JSON array from {from} to {to}. Keep the meaning, tone and \
                 formatting. Inline markup is written as <m0>…</m0>, <m1>…</m1> and so on: keep every tag \
                 exactly once, around the translated words it belongs to, and do not add tags. Leave URLs, \
                 email addresses, code and names that are normally not translated as they are. Reply with \
                 JSON only: {{\"segments\": [\"...\"]}} containing exactly {n} strings in the same order.",
                to = language_name(to),
                n = segments.len()
            )),
            Message::user(json!({ "segments": segments }).to_string()),
        ],
        json: true,
        temperature: Some(0.2),
        max_tokens: None,
    }
}

pub fn parse_translation(answer: &str, count: usize) -> Option<Vec<String>> {
    let value = extract_json(answer)?;
    let list = value.get("segments").unwrap_or(&value).as_array()?;
    let out: Vec<String> = list
        .iter()
        .map(|v| v.as_str().unwrap_or_default().to_string())
        .collect();
    (out.len() == count).then_some(out)
}

/// 用户定义的一个分类。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    pub name: String,
    /// 一句话说明这一类是什么，帮模型分得更准。
    #[serde(default)]
    pub description: String,
}

pub fn classify(items: &[(String, Doc<'_>)], categories: &[Category]) -> Request {
    let list = categories
        .iter()
        .map(|c| {
            if c.description.trim().is_empty() {
                format!("- {}", c.name)
            } else {
                format!("- {}: {}", c.name, c.description.trim())
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let body = items
        .iter()
        .map(|(id, doc)| format!("id: {id}\n{}", doc.render(300)))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");
    Request {
        messages: vec![
            Message::system(format!(
                "Sort each item into exactly one of these categories:\n{list}\n\n\
                 Use the category names exactly as written. If none fits, use null. \
                 Reply with JSON only: {{\"items\": [{{\"id\": \"...\", \"category\": \"...\"}}]}}"
            )),
            Message::user(body),
        ],
        json: true,
        // 不用 0：智谱的接口只接受大于 0 的值，0 会回 400
        temperature: Some(0.1),
        max_tokens: Some(40 * items.len() as u32 + 100),
    }
}

/// 每条的分类。不在列表里的名字当作没分出来。
pub fn parse_classes(answer: &str, categories: &[Category]) -> HashMap<String, Option<String>> {
    let value = extract_json(answer).unwrap_or(Value::Null);
    let list = value
        .get("items")
        .unwrap_or(&value)
        .as_array()
        .cloned()
        .unwrap_or_default();
    list.iter()
        .filter_map(|item| {
            let id = match item.get("id")? {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            let name = item.get("category").and_then(Value::as_str).map(str::trim);
            let matched = name.and_then(|n| {
                categories
                    .iter()
                    .find(|c| c.name.trim().eq_ignore_ascii_case(n))
                    .map(|c| c.name.clone())
            });
            Some((id, matched))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: Doc<'static> = Doc {
        title: "Q4 budget",
        source: "Mei Han",
        text: "The Q4 budget is attached.",
    };

    #[test]
    fn prompts_name_the_output_language() {
        let r = summary(DOC, "zh");
        assert!(r.messages[0].content.contains("Simplified Chinese"));
        assert!(r.messages[1].content.contains("Title: Q4 budget"));
        assert!(keywords(DOC, "en").json);
    }

    #[test]
    fn parses_structured_answers() {
        assert_eq!(
            parse_keywords(r#"{"keywords": ["budget", "Q4", "budget", " "]}"#),
            vec!["budget", "Q4"]
        );
        let a = parse_abstracts(
            r#"{"items":[{"id":"1","abstract":"About money."},{"id":2,"abstract":"x"}]}"#,
        );
        assert_eq!(a["1"], "About money.");
        assert_eq!(a["2"], "x");
        assert_eq!(
            parse_translation(r#"{"segments":["a","b"]}"#, 2),
            Some(vec!["a".into(), "b".into()])
        );
        assert_eq!(parse_translation(r#"{"segments":["a"]}"#, 2), None);
        let cats = vec![
            Category {
                name: "工作".into(),
                description: String::new(),
            },
            Category {
                name: "Bills".into(),
                description: String::new(),
            },
        ];
        let c = parse_classes(
            r#"{"items":[{"id":"1","category":"bills"},{"id":"2","category":"Other"},{"id":"3","category":null}]}"#,
            &cats,
        );
        assert_eq!(c["1"].as_deref(), Some("Bills"));
        assert_eq!(c["2"], None);
        assert_eq!(c["3"], None);
    }

    #[test]
    fn chat_keeps_recent_history_only() {
        let history: Vec<Message> = (0..20).map(|i| Message::user(format!("q{i}"))).collect();
        let r = chat(DOC, &history, "now?", "en");
        // 系统提示 + 最近 12 条 + 这次的问题
        assert_eq!(r.messages.len(), 14);
        assert_eq!(r.messages[1].content, "q8");
    }
}
