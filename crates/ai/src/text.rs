//! 控制发给模型的文字有多长，以及从回答里取出 JSON。

use serde_json::Value;

fn wide(c: char) -> bool {
    matches!(c as u32,
        0x3000..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF | 0x20000..=0x2FFFF)
}

/// 粗略估计 token 数：中日韩文字大约一字一个，其他文字大约四个字符一个。
pub fn estimate_tokens(s: &str) -> usize {
    let (mut wide_chars, mut other) = (0usize, 0usize);
    for c in s.chars() {
        if wide(c) {
            wide_chars += 1;
        } else {
            other += 1;
        }
    }
    wide_chars + other.div_ceil(4)
}

/// 截到大约 `max` 个 token。尽量断在段落或句子之间。返回截后的文字和是否截过。
pub fn truncate(s: &str, max: usize) -> (&str, bool) {
    let mut cost = 0.0f64;
    let mut end = s.len();
    for (i, c) in s.char_indices() {
        cost += if wide(c) { 1.0 } else { 0.25 };
        if cost > max as f64 {
            end = i;
            break;
        }
    }
    if end == s.len() {
        return (s, false);
    }
    let head = &s[..end];
    // 往回找一个自然的断点，但不要丢掉太多
    let floor = head.len().saturating_sub(800);
    let cut = head[floor..]
        .rfind(['\n', '。', '.', '！', '!', '？', '?'])
        .map_or(end, |i| {
            let at = floor + i;
            at + head[at..].chars().next().map_or(0, char::len_utf8)
        });
    (&s[..cut], true)
}

/// 按段落切成每块大约 `max` 个 token。单独一段太长的再硬切。
pub fn split(s: &str, max: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for para in s.split('\n').map(str::trim).filter(|p| !p.is_empty()) {
        let mut para = para;
        while estimate_tokens(para) > max {
            let (head, _) = truncate(para, max);
            let head = if head.is_empty() { para } else { head };
            if !current.is_empty() {
                chunks.push(std::mem::take(&mut current));
            }
            chunks.push(head.to_string());
            para = para[head.len()..].trim_start();
        }
        if estimate_tokens(&current) + estimate_tokens(para) > max && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(para);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

/// 从回答里取出 JSON。模型有时会加上 ```json 围栏或者前后多说几句。
pub fn extract_json(s: &str) -> Option<Value> {
    let s = s.trim();
    if let Ok(v) = serde_json::from_str(s) {
        return Some(v);
    }
    for (open, close) in [('{', '}'), ('[', ']')] {
        if let (Some(a), Some(b)) = (s.find(open), s.rfind(close))
            && a < b
            && let Ok(v) = serde_json::from_str(&s[a..=b])
        {
            return Some(v);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates() {
        assert_eq!(estimate_tokens("你好世界"), 4);
        assert_eq!(estimate_tokens("abcdefgh"), 2);
    }

    #[test]
    fn truncates_at_a_sentence() {
        let text = "第一句话。第二句话。第三句话。";
        let (head, cut) = truncate(text, 8);
        assert!(cut);
        assert_eq!(head, "第一句话。");
        assert_eq!(truncate("short", 100), ("short", false));
    }

    #[test]
    fn splits_by_paragraph() {
        let text = "aaaa aaaa\nbbbb bbbb\ncccc cccc";
        let parts = split(text, 7);
        assert_eq!(parts, vec!["aaaa aaaa\nbbbb bbbb", "cccc cccc"]);
        let long = "字".repeat(25);
        let parts = split(&long, 10);
        assert_eq!(parts.len(), 3);
        assert_eq!(parts.concat(), long);
    }

    #[test]
    fn pulls_json_out_of_chatty_answers() {
        let v = extract_json("Sure! ```json\n{\"keywords\": [\"a\", \"b\"]}\n``` Hope it helps.")
            .unwrap();
        assert_eq!(v["keywords"][1], "b");
        assert!(extract_json("no json here").is_none());
    }
}
