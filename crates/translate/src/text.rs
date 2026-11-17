//! 翻译一段段文字，并把段落里的链接、粗体等标记搬到译文对应的位置上。
//!
//! 界面把每个段落交过来：纯文字，加上里面每个行内元素覆盖的范围。这里按句子切开逐句翻译，
//! 用模型给出的词对齐找出每个标记在译文里落在哪，再拼回整段。位置一律按 UTF-16 计，和界面里
//! JavaScript 字符串的下标一致。

use fxtranslate::engine::Engine;
use fxtranslate::segment::{IcuSegmenter, MAX_SOURCE_TOKENS, Segmenter};
use serde::{Deserialize, Serialize};

use crate::detect::{is_cjk_char, is_cjk_lang};

/// 一个段落。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    pub text: String,
    #[serde(default)]
    pub marks: Vec<Mark>,
}

/// 段落里一个行内元素覆盖的范围，`[start, end)`，UTF-16 下标。`id` 指回原文里的第几个元素：
/// 一个元素在译文里可能被拆成几段，也可能整个找不到位置而丢掉。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mark {
    pub start: usize,
    pub end: usize,
    pub id: usize,
}

impl Segment {
    pub fn plain(text: impl Into<String>) -> Segment {
        Segment {
            text: text.into(),
            marks: Vec::new(),
        }
    }
}

/// 用一个模型翻译一段。`to` 是这一步的目标语言，决定句子之间要不要空格。
pub fn translate_segment(engine: &Engine, to: &str, segment: &Segment) -> Segment {
    let text = segment.text.as_str();
    let to_byte = utf16_to_byte(text);
    let marks: Vec<(usize, usize, usize)> = segment
        .marks
        .iter()
        .filter_map(|m| {
            let (s, e) = (*to_byte.get(m.start)?, *to_byte.get(m.end)?);
            (s < e).then_some((s, e, m.id))
        })
        .collect();

    let cjk = is_cjk_lang(to);
    let mut out = String::new();
    let mut out_marks = Vec::new();
    for (i, span) in IcuSegmenter::new().sentences(text).iter().enumerate() {
        if i > 0 && !cjk {
            out.push(' ');
        }
        let local: Vec<(usize, usize, usize)> = marks
            .iter()
            .filter_map(|&(s, e, id)| {
                let (s, e) = (s.max(span.start), e.min(span.end));
                (s < e).then(|| (s - span.start, e - span.start, id))
            })
            .collect();
        let (translated, marks) = translate_sentence(engine, span.of(text), &local);
        let base = utf16_len(&out);
        out_marks.extend(marks.into_iter().map(|m| Mark {
            start: m.start + base,
            end: m.end + base,
            id: m.id,
        }));
        out.push_str(&translated);
    }

    let mut result = Segment {
        text: out,
        marks: out_marks,
    };
    if cjk {
        result = tidy_cjk(&result);
    }
    result.marks = non_overlapping(result.marks);
    result
}

/// 翻译一句。`marks` 是句内的字节范围；返回的标记是译文里的 UTF-16 范围。
fn translate_sentence(
    engine: &Engine,
    sentence: &str,
    marks: &[(usize, usize, usize)],
) -> (String, Vec<Mark>) {
    if marks.is_empty() {
        return (
            merge_byte_pieces(&engine.translate_long(sentence)).0,
            Vec::new(),
        );
    }
    // 超过模型窗口的长句要拆开翻，词对齐对不上，标记只好放弃
    if engine.src_ids(sentence).len() > MAX_SOURCE_TOKENS {
        return (
            merge_byte_pieces(&engine.translate_long(sentence)).0,
            Vec::new(),
        );
    }
    let aligned = engine.translate_aligned(sentence);
    let target = aligned.target_text;
    let target_u16: Vec<u16> = target.encode_utf16().collect();
    let whole =
        |s: usize, e: usize| sentence[..s].trim().is_empty() && sentence[e..].trim().is_empty();

    // 源词元的位置指向分词器规范化以后的文字，先对回原句
    let to_byte = normalized_to_byte(&aligned.source_normalized, sentence);
    let src_spans: Vec<(usize, usize)> = aligned
        .source_tokens
        .iter()
        .map(|t| {
            let at = |i: usize| to_byte[i.min(to_byte.len() - 1)];
            (at(t.begin), at(t.end))
        })
        .collect();
    // 最后一个源词元是句尾标记，不参与对齐
    let src_count = src_spans.len().saturating_sub(1);

    let mut out = Vec::new();
    for &(s, e, id) in marks {
        // 标记盖住了整句（比如整段就是一个链接），译文整句都给它
        if whole(s, e) {
            if let Some((start, end)) = trim_range(&target_u16, 0, target_u16.len()) {
                out.push(Mark { start, end, id });
            }
            continue;
        }
        let mut range: Option<(usize, usize)> = None;
        for (t, token) in aligned.target_tokens.iter().enumerate() {
            if token.begin >= token.end {
                continue;
            }
            let Some(row) = aligned.alignments.get(t) else {
                continue;
            };
            let best = row[..src_count.min(row.len())]
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map(|(i, _)| i);
            let Some((bs, be)) = best.map(|i| src_spans[i]) else {
                continue;
            };
            if bs < e && s < be {
                range = Some(match range {
                    Some((a, b)) => (a.min(token.begin), b.max(token.end)),
                    None => (token.begin, token.end),
                });
            }
        }
        if let Some((start, end)) = range.and_then(|(a, b)| trim_range(&target_u16, a, b)) {
            out.push(Mark { start, end, id });
        }
    }
    let (text, map) = merge_byte_pieces(&target);
    let out = out
        .into_iter()
        .map(|m| Mark {
            start: map[m.start],
            end: map[m.end],
            id: m.id,
        })
        .filter(|m| m.start < m.end)
        .collect();
    (text, out)
}

/// 模型遇到词表里没有的字时逐个字节输出，形如 `<0xE7><0x9A><0x84>`，翻译库把它们原样拼进了译文。
/// 这里拼回字符，拼不成合法字符的字节丢掉。
///
/// 同时返回原文每个 UTF-16 位置在新文字里的位置，长度是原文的 UTF-16 长度加一。
fn merge_byte_pieces(text: &str) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(text.len());
    let mut map = Vec::with_capacity(text.len() + 1);
    let mut at = 0;
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        let (bytes, len) = byte_run(rest);
        if len == 0 {
            for k in 0..c.len_utf16() {
                map.push(at + k);
            }
            out.push(c);
            at += c.len_utf16();
            rest = &rest[c.len_utf8()..];
            continue;
        }
        let decoded: String = bytes.utf8_chunks().map(|chunk| chunk.valid()).collect();
        let next = at + utf16_len(&decoded);
        // 落在这串字节中间的位置，算到它解出来的字的后面
        map.push(at);
        map.extend(std::iter::repeat_n(next, len - 1));
        out.push_str(&decoded);
        at = next;
        rest = &rest[len..];
    }
    map.push(at);
    (out, map)
}

/// 开头连续的 `<0xNN>` 有几个字节，以及它们占了多少个字符。
fn byte_run(s: &str) -> (Vec<u8>, usize) {
    const PIECE: usize = "<0x00>".len();
    let mut bytes = Vec::new();
    let mut rest = s.as_bytes();
    while let [b'<', b'0', b'x', hi, lo, b'>', ..] = rest {
        let hex = |b: u8| (b as char).to_digit(16);
        let (Some(hi), Some(lo)) = (hex(*hi), hex(*lo)) else {
            break;
        };
        bytes.push((hi * 16 + lo) as u8);
        rest = &rest[PIECE..];
    }
    let len = bytes.len() * PIECE;
    (bytes, len)
}

/// 去掉范围两头的空白。全是空白时返回 `None`。
fn trim_range(text: &[u16], mut start: usize, mut end: usize) -> Option<(usize, usize)> {
    let end_cap = text.len();
    end = end.min(end_cap);
    let space = |c: u16| char::from_u32(c as u32).is_some_and(char::is_whitespace);
    while start < end && space(text[start]) {
        start += 1;
    }
    while end > start && space(text[end - 1]) {
        end -= 1;
    }
    (start < end).then_some((start, end))
}

/// 标记之间不能交叉，否则界面没法把它们还原成元素。后面的和前面的重叠时，截掉重叠的部分。
fn non_overlapping(mut marks: Vec<Mark>) -> Vec<Mark> {
    marks.sort_by_key(|m| (m.start, m.end));
    let mut out: Vec<Mark> = Vec::with_capacity(marks.len());
    for mut m in marks {
        if let Some(prev) = out.last() {
            m.start = m.start.max(prev.end);
        }
        if m.start < m.end {
            out.push(m);
        }
    }
    out
}

/// 中文、日文译文里，模型常输出半角标点，句子之间还带着英文式的空格。
/// 汉字后面的半角标点换成全角，两个中文字符之间的空格去掉。标记的位置跟着挪。
pub fn tidy_cjk(segment: &Segment) -> Segment {
    let chars: Vec<char> = segment.text.chars().collect();
    let wide = |c: char| is_cjk_char(c) || matches!(c as u32, 0x3000..=0x303F | 0xFF00..=0xFFEF);
    let mut out = String::with_capacity(segment.text.len());
    // 原文每个 UTF-16 位置对应到新文本里的位置
    let mut map = Vec::with_capacity(segment.text.len() + 1);
    let mut pos = 0usize;
    let mut last: Option<char> = None;
    for (i, &c) in chars.iter().enumerate() {
        let after_cjk = last.is_some_and(is_cjk_char);
        let replaced = match c {
            ',' if after_cjk => Some('，'),
            '?' if after_cjk => Some('？'),
            '!' if after_cjk => Some('！'),
            ':' if after_cjk => Some('：'),
            ';' if after_cjk => Some('；'),
            _ => None,
        };
        let next = chars.get(i + 1).copied();
        let drop = c == ' ' && last.is_some_and(wide) && next.is_some_and(wide);
        for _ in 0..c.len_utf16() {
            map.push(pos);
        }
        if drop {
            continue;
        }
        let c = replaced.unwrap_or(c);
        out.push(c);
        pos += c.len_utf16();
        last = Some(c);
    }
    map.push(pos);
    let at = |i: usize| map[i.min(map.len() - 1)];
    Segment {
        marks: segment
            .marks
            .iter()
            .map(|m| Mark {
                start: at(m.start),
                end: at(m.end),
                id: m.id,
            })
            .filter(|m| m.start < m.end)
            .collect(),
        text: out,
    }
}

/// 每个 UTF-16 下标对应的字节位置，末尾多一项指向结尾。
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

/// 分词器会把空白换成 `▁`、在句首补一个 `▁`、把连续空白并成一个，偶尔还做 Unicode 规范化。
/// 逐字对齐，给出规范化文字里每个 UTF-16 位置对应的原句字节位置。规范化改了长度的少数字符
/// （比如连字）会让后面的位置略有偏差，只影响标记的边界。
fn normalized_to_byte(normalized: &str, original: &str) -> Vec<usize> {
    let chars: Vec<(usize, char)> = original.char_indices().collect();
    let at = |j: usize| chars.get(j).map_or(original.len(), |c| c.0);
    let mut j = 0;
    let mut map = Vec::with_capacity(normalized.len() + 1);
    for c in normalized.chars() {
        for _ in 0..c.len_utf16() {
            map.push(at(j));
        }
        if c == '▁' {
            // 原句这里没有空白，说明是补在句首的那个
            while chars.get(j).is_some_and(|c| c.1.is_whitespace()) {
                j += 1;
            }
        } else {
            j += 1;
        }
    }
    map.push(at(j));
    map
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// 这一段需要交给模型吗。从中文翻出去时，一个汉字都没有的段落（英文签名、网址）原样保留；
/// 翻成中文时，汉字比外文单词还多的段落已经是中文了（「Microsoft 帐户安全信息」），也不动：
/// 交给英译中的模型，它只会把汉字拆成字节吐回来。只有数字和符号的段落都不翻。
pub fn needs_translation(text: &str, from: &str, to: &str) -> bool {
    let letters = text.chars().filter(|c| c.is_alphabetic());
    let (mut cjk, mut other) = (0, 0);
    for c in letters {
        if is_cjk_char(c) {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    if cjk + other == 0 {
        return false;
    }
    let cjk_source = matches!(from, "zh" | "zh_hant" | "ja" | "ko");
    if cjk_source && cjk == 0 {
        return false;
    }
    if is_cjk_lang(to) && !cjk_source && cjk > foreign_words(text) {
        return false;
    }
    true
}

/// 汉字、假名以外的单词有几个。
fn foreign_words(text: &str) -> usize {
    text.split(|c: char| !c.is_alphabetic() || is_cjk_char(c))
        .filter(|w| !w.is_empty())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(start: usize, end: usize, id: usize) -> Mark {
        Mark { start, end, id }
    }

    #[test]
    fn tidies_chinese_punctuation() {
        let seg = Segment {
            text: "嗨,米亚,你好吗? 我很好。 谢谢".into(),
            marks: vec![mark(2, 4, 0), mark(15, 17, 1)],
        };
        let out = tidy_cjk(&seg);
        assert_eq!(out.text, "嗨，米亚，你好吗？我很好。谢谢");
        // 「米亚」位置不变，「谢谢」前面去掉了两个空格，往前挪两位
        assert_eq!(out.marks, vec![mark(2, 4, 0), mark(13, 15, 1)]);
        let chars: Vec<char> = out.text.chars().collect();
        assert_eq!(chars[2..4].iter().collect::<String>(), "米亚");
        assert_eq!(chars[13..15].iter().collect::<String>(), "谢谢");
    }

    #[test]
    fn keeps_latin_spacing_and_numbers() {
        let seg = Segment::plain("增长了约12%, 主要是 Q4 的会议");
        assert_eq!(tidy_cjk(&seg).text, "增长了约12%, 主要是 Q4 的会议");
    }

    #[test]
    fn marks_do_not_overlap() {
        let marks = non_overlapping(vec![mark(5, 9, 1), mark(0, 6, 0), mark(6, 8, 2)]);
        assert_eq!(marks, vec![mark(0, 6, 0), mark(6, 9, 1)]);
    }

    #[test]
    fn skips_segments_without_source_text() {
        assert!(!needs_translation("Best regards, Mia", "zh", "en"));
        assert!(needs_translation("谢谢，Mia", "zh", "en"));
        assert!(!needs_translation("张三", "en", "zh"));
        assert!(!needs_translation("2026-11-16 15:00", "en", "zh"));
        assert!(needs_translation("Thanks!", "en", "zh"));
        assert!(needs_translation("Hello", "en", "de"));
    }

    #[test]
    fn maps_normalized_source_back() {
        let original = "Hi  Mia, the Q4";
        let map = normalized_to_byte("▁Hi▁Mia,▁the▁Q4", original);
        // 「▁Mia」从原句的空白开始，到 a 后面结束
        assert_eq!(&original[map[3]..map[7]], "  Mia");
        assert_eq!(&original[map[12]..map[15]], " Q4");
        assert_eq!(map[0], 0);
    }

    #[test]
    fn byte_pieces_become_characters() {
        let (text, map) = merge_byte_pieces("微软 <0xE6><0x88><0xB7>拉<0x90>!");
        assert_eq!(text, "微软 户拉!");
        // 「拉」原来在第 21 个位置，现在在第 4 个
        assert_eq!(map[21], 4);
        assert_eq!(*map.last().unwrap(), utf16_len(&text));
        assert_eq!(merge_byte_pieces("a <0xZZ> b").0, "a <0xZZ> b");
    }

    #[test]
    fn chinese_titles_are_not_sent_to_an_english_model() {
        assert!(!needs_translation("Microsoft 帐户安全信息", "en", "zh"));
        assert!(!needs_translation("ada@example.com 的安全提醒", "en", "zh"));
        assert!(needs_translation(
            "Your order from 京东 has shipped",
            "en",
            "zh"
        ));
        assert!(needs_translation("Meeting about 東京", "en", "ja"));
    }

    #[test]
    fn utf16_offsets() {
        // emoji 在 UTF-16 里占两个位置
        let map = utf16_to_byte("a😀b");
        assert_eq!(map, vec![0, 1, 1, 5, 6]);
    }
}
