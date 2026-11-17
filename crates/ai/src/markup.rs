//! 翻译时把段落里的行内元素写成 `<m0>…</m0>` 交给模型，翻完再拆回文字和位置。
//!
//! 位置一律是 UTF-16 下标，和界面里 JavaScript 字符串的下标一致。

/// 一个行内元素覆盖的 `[start, end)`，以及它是原文里的第几个元素。
pub type Span = (usize, usize, usize);

/// 每个 UTF-16 下标对应的字节位置，末尾多一项。
fn utf16_to_byte(s: &str) -> Vec<usize> {
    let mut map = Vec::with_capacity(s.len() + 1);
    for (b, c) in s.char_indices() {
        for _ in 0..c.len_utf16() {
            map.push(b);
        }
    }
    map.push(s.len());
    map
}

/// 把标记写进文字里。
pub fn tag(text: &str, marks: &[Span]) -> String {
    let map = utf16_to_byte(text);
    let at = |i: usize| map[i.min(map.len() - 1)];
    // (字节位置, 先关后开, 标签)
    let mut events: Vec<(usize, u8, String)> = Vec::new();
    for &(start, end, id) in marks {
        if start >= end {
            continue;
        }
        events.push((at(start), 1, format!("<m{id}>")));
        events.push((at(end), 0, format!("</m{id}>")));
    }
    events.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut out = String::with_capacity(text.len() + events.len() * 6);
    let mut pos = 0;
    for (byte, _, tag) in events {
        out.push_str(&text[pos..byte]);
        out.push_str(&tag);
        pos = byte;
    }
    out.push_str(&text[pos..]);
    out
}

/// 拆出文字和标记。配不上对的、出现两次的标签直接丢掉，文字照常保留。
pub fn untag(tagged: &str) -> (String, Vec<Span>) {
    let mut text = String::with_capacity(tagged.len());
    let mut len16 = 0usize;
    let mut open: Vec<(usize, usize)> = Vec::new();
    let mut spans: Vec<Span> = Vec::new();
    let mut rest = tagged;
    while let Some(lt) = rest.find('<') {
        let before = &rest[..lt];
        text.push_str(before);
        len16 += before.encode_utf16().count();
        let after = &rest[lt..];
        if let Some((closing, id, size)) = parse_tag(after) {
            if closing {
                if let Some(i) = open.iter().rposition(|(oid, _)| *oid == id) {
                    let (_, start) = open.remove(i);
                    if start < len16 && !spans.iter().any(|s| s.2 == id) {
                        spans.push((start, len16, id));
                    }
                }
            } else {
                open.push((id, len16));
            }
            rest = &after[size..];
        } else {
            text.push('<');
            len16 += 1;
            rest = &after[1..];
        }
    }
    text.push_str(rest);
    spans.sort();
    (text, spans)
}

/// `<m12>` 或 `</m12>`：返回是否结束标签、编号和长度。
fn parse_tag(s: &str) -> Option<(bool, usize, usize)> {
    let closing = s.starts_with("</");
    let body = s.strip_prefix(if closing { "</m" } else { "<m" })?;
    let digits: String = body.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() || !body[digits.len()..].starts_with('>') {
        return None;
    }
    let size = if closing { 3 } else { 2 } + digits.len() + 1;
    Some((closing, digits.parse().ok()?, size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let text = "Please review it before Friday.";
        let marks = vec![(7, 13, 0), (17, 30, 1)];
        let tagged = tag(text, &marks);
        assert_eq!(tagged, "Please <m0>review</m0> it <m1>before Friday</m1>.");
        assert_eq!(untag(&tagged), (text.to_string(), marks));
    }

    #[test]
    fn reads_translated_markup() {
        let (text, spans) = untag("请在<m1>周五之前</m1><m0>审阅</m0>。");
        assert_eq!(text, "请在周五之前审阅。");
        assert_eq!(spans, vec![(2, 6, 1), (6, 8, 0)]);
    }

    #[test]
    fn tolerates_broken_markup() {
        // 只开不关、只关不开、重复的，都丢掉标记但保留文字；不是标签的尖括号原样保留
        let (text, spans) = untag("a <m0>b <m1>c</m1> d</m2> <m1>e</m1> 1 < 2");
        assert_eq!(text, "a b c d e 1 < 2");
        assert_eq!(spans, vec![(4, 5, 1)]);
    }

    #[test]
    fn counts_utf16_like_javascript() {
        let text = "😀 hi";
        let tagged = tag(text, &[(3, 5, 0)]);
        assert_eq!(tagged, "😀 <m0>hi</m0>");
        assert_eq!(untag(&tagged).1, vec![(3, 5, 0)]);
    }
}
