//! 把一段识别结果切成一句一句的字幕。
//!
//! 识别是按「一口气说完的一段」来的（十秒上下），直接当字幕太长。这里按标点切开，
//! 每句的时间优先用模型给的逐字时间，没有就按字数在这一段里平均分。

use serde::{Deserialize, Serialize};

/// 一句字幕。时间是它在整期节目里的位置，秒。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub text: String,
    /// 每个字（按 Unicode 字符数）从这句开始算起第几个百分之一秒说出来，「唱词」那样一个字一个字地亮。
    /// 模型给了词元时间就用它的，没给的按字数在这一段里平均分。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub t: Vec<u32>,
    /// 第几个说话的人（从 0 数），分出了说话人才有。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<u32>,
}

/// 秒换成百分之一秒。
fn centis(seconds: f64) -> u32 {
    (seconds * 100.0).round().max(0.0) as u32
}

/// 一句最多这么宽（汉字算 2，字母算 1），大约两行。超过了就算没有标点也切开。
const MAX_WIDTH: usize = 76;
/// 到了这个宽度，遇到逗号也切。
const SOFT_WIDTH: usize = 36;
/// 句号这类标点后面，太短的不单独成句（比如「嗯。」「OK.」跟着下一句走）。
const MIN_WIDTH: usize = 8;

fn width(c: char) -> usize {
    if wide(c) { 2 } else { 1 }
}

/// 中日韩文字和全角标点。
fn wide(c: char) -> bool {
    matches!(u32::from(c),
        0x1100..=0x115F | 0x2E80..=0x303E | 0x3041..=0x33FF | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF | 0xA000..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6 | 0x20000..=0x3FFFD)
}

fn sentence_end(c: char) -> bool {
    matches!(c, '。' | '！' | '？' | '!' | '?' | '…' | '．')
}

fn clause_end(c: char) -> bool {
    matches!(c, '，' | ',' | '、' | '；' | ';' | '：' | ':')
}

/// 跟在句末标点后面、应该留在这一句里的符号。
fn closer(c: char) -> bool {
    matches!(
        c,
        '”' | '’' | '」' | '』' | '）' | ')' | '》' | '"' | '\'' | '】' | ']'
    ) || sentence_end(c)
        || c == '.'
}

/// 只有标点和空白的不算一句话。
fn has_words(text: &str) -> bool {
    text.chars().any(|c| c.is_alphanumeric())
}

/// 逐字的时间：模型给的词元能和文字对上就用它的，否则按字数平均。
/// 返回每个字和它相对这一段开头的时间。
fn timed_chars(
    text: &str,
    tokens: &[String],
    timestamps: Option<&[f32]>,
    duration: f64,
) -> Vec<(char, f64)> {
    let squeeze = |s: &str| -> String { s.chars().filter(|c| !c.is_whitespace()).collect() };
    if let Some(times) = timestamps
        && !tokens.is_empty()
        && times.len() == tokens.len()
    {
        // SentencePiece 用「▁」表示词的开头，换成空格
        let pieces: Vec<String> = tokens.iter().map(|t| t.replace('▁', " ")).collect();
        let joined: String = pieces.concat();
        if squeeze(&joined) == squeeze(text) {
            let mut out = Vec::with_capacity(joined.len());
            for (piece, &t) in pieces.iter().zip(times) {
                let t = f64::from(t).clamp(0.0, duration.max(0.0));
                out.extend(piece.chars().map(|c| (c, t)));
            }
            return out;
        }
    }
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len().max(1) as f64;
    chars
        .into_iter()
        .enumerate()
        .map(|(i, c)| (c, duration * i as f64 / n))
        .collect()
}

/// 每一句在 `chars` 里的范围。太短的（「嗯。」）跟着下一句。
fn sentences(chars: &[(char, f64)]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut from = 0usize;
    let mut w = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i].0;
        w += width(c);
        let next = chars.get(i + 1).map(|x| x.0);
        // 英文句号：后面是空白或结尾才算句末，免得把 3.14 切开
        let period = c == '.' && next.is_none_or(char::is_whitespace);
        if (sentence_end(c) || period) && w >= MIN_WIDTH {
            let mut end = i + 1;
            while end < chars.len() && closer(chars[end].0) {
                end += 1;
            }
            ranges.push((from, end));
            from = end;
            i = end;
            w = 0;
        } else {
            i += 1;
        }
    }
    if from < chars.len() {
        ranges.push((from, chars.len()));
    }
    ranges
}

/// 把 `[a, b)` 这一句断成不超过 `MAX_WIDTH` 的几段：优先在逗号处，其次在空格处，
/// 都没有（没标点的中文）就大致均分，免得最后剩一两个字单独一行。
fn break_long(chars: &[(char, f64)], a: usize, b: usize, out: &mut Vec<(usize, usize)>) {
    let mut from = a;
    loop {
        let rest: usize = chars[from..b].iter().map(|x| width(x.0)).sum();
        if rest <= MAX_WIDTH {
            if from < b {
                out.push((from, b));
            }
            return;
        }
        // 还要断成几段，每段大概多宽
        let target = rest / rest.div_ceil(MAX_WIDTH);
        let mut w = 0usize;
        let mut clause: Option<(usize, usize)> = None;
        let mut space: Option<(usize, usize)> = None;
        let mut even: Option<usize> = None;
        for (i, &(c, _)) in chars.iter().enumerate().take(b).skip(from) {
            w += width(c);
            if w > MAX_WIDTH {
                break;
            }
            if clause_end(c) && w >= SOFT_WIDTH.min(target) {
                // 离均分点最近的逗号
                let off = w.abs_diff(target);
                if clause.is_none_or(|(_, best)| off <= best) {
                    clause = Some((i + 1, off));
                }
            }
            if c.is_whitespace() {
                // 离均分点最近的空格
                let off = w.abs_diff(target);
                if space.is_none_or(|(_, best)| off <= best) {
                    space = Some((i + 1, off));
                }
            }
            if even.is_none() && w >= target {
                even = Some(i + 1);
            }
        }
        let cut = clause
            .or(space)
            .map(|(i, _)| i)
            .or(even)
            .unwrap_or(b)
            .clamp(from + 1, b);
        out.push((from, cut));
        from = cut;
    }
}

/// 把从 `start` 秒开始、长 `duration` 秒的一段识别结果切成字幕。
pub fn split(
    start: f64,
    duration: f64,
    text: &str,
    tokens: &[String],
    timestamps: Option<&[f32]>,
) -> Vec<Cue> {
    let text = text.trim();
    if !has_words(text) {
        return Vec::new();
    }
    let chars = timed_chars(text, tokens, timestamps, duration);

    // 先按句末标点分成句子，太长的句子再在逗号或空格处断开
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for (a, b) in sentences(&chars) {
        break_long(&chars, a, b, &mut ranges);
    }

    let mut cues: Vec<Cue> = Vec::new();
    for (a, b) in ranges {
        // 去掉两头的空白，字和它的时间一起去
        let slice = &chars[a..b];
        let (Some(first), Some(last)) = (
            slice.iter().position(|x| !x.0.is_whitespace()),
            slice.iter().rposition(|x| !x.0.is_whitespace()),
        ) else {
            continue;
        };
        let part = &slice[first..=last];
        let text: String = part.iter().map(|x| x.0).collect();
        if !has_words(&text) {
            // 落单的标点并到上一句
            if let Some(prev) = cues.last_mut() {
                prev.text.push_str(&text);
                prev.t
                    .extend(part.iter().map(|x| centis(start + x.1 - prev.start)));
            }
            continue;
        }
        let t0 = part[0].1;
        cues.push(Cue {
            start: start + t0,
            end: start + duration,
            text,
            t: part.iter().map(|x| centis(x.1 - t0)).collect(),
            speaker: None,
        });
    }
    // 每一句到下一句开始为止
    for i in 1..cues.len() {
        let next = cues[i].start;
        cues[i - 1].end = next.max(cues[i - 1].start);
    }
    cues
}

/// 已经转写过的时间段，按开始时间排好、互不相连。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Covered(Vec<(f64, f64)>);

/// 两段之间差这么一点就算连上了。
const GAP: f64 = 0.05;

impl Covered {
    pub fn ranges(&self) -> &[(f64, f64)] {
        &self.0
    }

    pub fn add(&mut self, from: f64, to: f64) {
        if !(from.is_finite() && to.is_finite()) || to <= from {
            return;
        }
        self.0.push((from.max(0.0), to));
        self.0.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut merged: Vec<(f64, f64)> = Vec::with_capacity(self.0.len());
        for &(a, b) in &self.0 {
            match merged.last_mut() {
                Some(last) if a <= last.1 + GAP => last.1 = last.1.max(b),
                _ => merged.push((a, b)),
            }
        }
        self.0 = merged;
    }

    /// `at` 落在哪一段里，返回那一段的结尾。
    pub fn end_of(&self, at: f64) -> Option<f64> {
        self.0
            .iter()
            .find(|(a, b)| *a <= at + GAP && at < *b)
            .map(|r| r.1)
    }

    /// 从 `at` 往后，第一处还没转写的地方和它的尽头（下一段的开头，或者 `duration`）。
    /// 后面都转完了返回 `None`。
    pub fn next_gap(&self, at: f64, duration: f64) -> Option<(f64, f64)> {
        let mut from = at.max(0.0);
        for &(a, b) in &self.0 {
            if b <= from {
                continue;
            }
            if a > from + GAP {
                return Some((from, a.min(duration)));
            }
            from = from.max(b);
        }
        (from + GAP < duration).then_some((from, duration))
    }

    /// 是不是从头到 `duration` 都转完了。
    pub fn complete(&self, duration: f64) -> bool {
        self.next_gap(0.0, duration).is_none()
    }
}

/// 把新转出来的几句并进已有的字幕：按时间排好，落在 `[from, to)` 里的旧句子先去掉（重转的那一段）。
pub fn merge(all: &mut Vec<Cue>, from: f64, to: f64, new: Vec<Cue>) {
    all.retain(|c| c.start < from || c.start >= to);
    all.extend(new);
    all.sort_by(|a, b| a.start.total_cmp(&b.start));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(cues: &[Cue]) -> Vec<&str> {
        cues.iter().map(|c| c.text.as_str()).collect()
    }

    #[test]
    fn chinese_sentences_split_at_punctuation() {
        let cues = split(
            100.0,
            10.0,
            "今天我们聊一聊开源软件。它到底是怎么赚钱的？好，我们开始吧！",
            &[],
            None,
        );
        assert_eq!(
            texts(&cues),
            [
                "今天我们聊一聊开源软件。",
                "它到底是怎么赚钱的？",
                "好，我们开始吧！"
            ]
        );
        assert_eq!(cues[0].start, 100.0);
        assert!(cues[1].start > 100.0 && cues[1].start < 110.0);
        assert_eq!(cues[0].end, cues[1].start);
        assert_eq!(cues[2].end, 110.0);
    }

    #[test]
    fn long_sentences_split_at_commas_and_by_length() {
        let cues = split(
            0.0,
            12.0,
            "如果你是第一次收听我们的节目，那么欢迎你，这期节目我们请到了一位做了十年独立开发的朋友，来聊聊他这些年的经历",
            &[],
            None,
        );
        assert!(cues.len() >= 2, "{cues:?}");
        assert!(
            cues.iter().all(|c| c.text.chars().count() <= 38),
            "{cues:?}"
        );
        assert!(cues.iter().all(|c| c.text.chars().count() >= 8), "{cues:?}");
        // 没有标点的也不会无限长，而且大致均分，不会最后剩一个字
        let cues = split(0.0, 30.0, &"字".repeat(100), &[], None);
        let lens: Vec<usize> = cues.iter().map(|c| c.text.chars().count()).collect();
        assert_eq!(lens, [33, 34, 33]);
        let cues = split(0.0, 10.0, &"字".repeat(39), &[], None);
        let lens: Vec<usize> = cues.iter().map(|c| c.text.chars().count()).collect();
        assert_eq!(lens, [20, 19]);
        // 一句话放得下就不在逗号处断开
        let cues = split(
            0.0,
            6.0,
            "I am here with students at Wakefield High School in Arlington, Virginia.",
            &[],
            None,
        );
        assert_eq!(cues.len(), 1);
    }

    #[test]
    fn english_keeps_decimals_and_splits_at_spaces() {
        let cues = split(
            0.0,
            8.0,
            "Pi is about 3.14 in most cases. That is close enough for us.",
            &[],
            None,
        );
        assert_eq!(
            texts(&cues),
            [
                "Pi is about 3.14 in most cases.",
                "That is close enough for us."
            ]
        );
        let long = "word ".repeat(40);
        let cues = split(0.0, 20.0, &long, &[], None);
        assert!(cues.len() >= 2);
        assert!(cues.iter().all(|c| !c.text.ends_with("wor")), "{cues:?}");
        // 稍微超长的一句从中间断开，不会只剩最后一个词
        let cues = split(
            0.0,
            8.0,
            "I imagine there's some seniors out there who are feeling pretty good right now.",
            &[],
            None,
        );
        assert_eq!(
            texts(&cues),
            [
                "I imagine there's some seniors out there",
                "who are feeling pretty good right now."
            ]
        );
    }

    #[test]
    fn short_interjections_join_the_next_sentence_and_noise_is_dropped() {
        let cues = split(0.0, 4.0, "嗯。那我们继续往下说。", &[], None);
        assert_eq!(texts(&cues), ["嗯。那我们继续往下说。"]);
        assert!(split(0.0, 1.0, "。。", &[], None).is_empty());
        assert!(split(0.0, 1.0, "  ", &[], None).is_empty());
    }

    #[test]
    fn token_times_are_used_when_they_line_up() {
        let tokens: Vec<String> = ["你", "好", "。", "再", "见", "了", "朋", "友", "们", "。"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let times = [0.5, 0.7, 0.9, 5.0, 5.2, 5.4, 5.6, 5.8, 6.0, 6.2];
        // 「你好。」太短，跟下一句走；再配一个够长的
        let cues = split(10.0, 8.0, "你好。再见了朋友们。", &tokens, Some(&times));
        assert_eq!(texts(&cues), ["你好。再见了朋友们。"]);
        assert_eq!(cues[0].start, 10.5);

        let tokens: Vec<String> = [
            "▁hello",
            "▁there",
            ",",
            "▁friend",
            ".",
            "▁bye",
            "▁for",
            "▁now",
            ".",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let times = [1.0, 1.3, 1.5, 1.6, 1.9, 4.0, 4.2, 4.4, 4.6];
        let cues = split(
            0.0,
            6.0,
            "hello there, friend. bye for now.",
            &tokens,
            Some(&times),
        );
        assert_eq!(texts(&cues), ["hello there, friend.", "bye for now."]);
        assert_eq!(cues[0].start, 1.0);
        assert_eq!(cues[1].start, 4.0);
        assert_eq!(cues[0].end, 4.0);
        // 对不上（比如模型给的是半个字的词元）就退回按字数平均
        let cues = split(
            0.0,
            6.0,
            "hello there, friend. bye for now.",
            &tokens[..3],
            Some(&times[..3]),
        );
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].start, 0.0);
    }

    #[test]
    fn every_character_gets_its_time() {
        // 模型给了逐字时间
        let tokens: Vec<String> = ["你", "好", "朋", "友", "们", "。", "再", "见", "了", "。"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let times = [0.5, 0.7, 0.9, 1.1, 1.3, 1.4, 3.0, 3.2, 3.6, 3.7];
        let cues = split(10.0, 5.0, "你好朋友们。再见了。", &tokens, Some(&times));
        assert_eq!(texts(&cues), ["你好朋友们。", "再见了。"]);
        assert_eq!(cues[0].t, [0, 20, 40, 60, 80, 90]);
        assert_eq!(cues[1].t, [0, 20, 60, 70]);
        assert_eq!(cues[1].start, 13.0);
        assert!(cues.iter().all(|c| c.t.len() == c.text.chars().count()));

        // 没给的按字数平均分；两头的空白去掉，不占时间
        let cues = split(0.0, 4.0, "  one two  ", &[], None);
        assert_eq!(cues[0].text, "one two");
        assert_eq!(cues[0].t.len(), 7);
        assert_eq!(cues[0].start, 0.0);
        assert_eq!(cues[0].t.last(), Some(&343));
        assert!(cues[0].t.windows(2).all(|w| w[0] <= w[1]));

        // 并到上一句的标点也有时间
        let cues = split(0.0, 4.0, "Is it you? !", &[], None);
        assert_eq!(texts(&cues), ["Is it you?!"]);
        assert_eq!(cues[0].t.len(), cues[0].text.chars().count());

        // 存成 JSON 时，没有逐字时间、没有说话人就不写这两项
        let plain = Cue {
            start: 1.0,
            end: 2.0,
            text: "hi".into(),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_string(&plain).unwrap(),
            r#"{"start":1.0,"end":2.0,"text":"hi"}"#
        );
    }

    #[test]
    fn covered_ranges_merge_and_report_gaps() {
        let mut c = Covered::default();
        assert_eq!(c.next_gap(0.0, 100.0), Some((0.0, 100.0)));
        c.add(30.0, 50.0);
        c.add(50.0, 60.0);
        assert_eq!(c.ranges(), [(30.0, 60.0)]);
        assert_eq!(c.end_of(40.0), Some(60.0));
        assert_eq!(c.end_of(10.0), None);
        assert_eq!(c.next_gap(40.0, 100.0), Some((60.0, 100.0)));
        assert_eq!(c.next_gap(0.0, 100.0), Some((0.0, 30.0)));
        c.add(60.0, 100.0);
        assert_eq!(c.next_gap(40.0, 100.0), None);
        assert!(!c.complete(100.0));
        c.add(0.0, 30.0);
        assert!(c.complete(100.0));
        assert_eq!(c.ranges(), [(0.0, 100.0)]);
    }

    #[test]
    fn merging_replaces_the_redone_range() {
        let cue = |s: f64, t: &str| Cue {
            start: s,
            end: s + 1.0,
            text: t.into(),
            ..Default::default()
        };
        let mut all = vec![cue(1.0, "a"), cue(5.0, "old"), cue(9.0, "c")];
        merge(&mut all, 4.0, 8.0, vec![cue(6.0, "new2"), cue(4.5, "new1")]);
        assert_eq!(texts(&all), ["a", "new1", "new2", "c"]);
    }
}
