//! 朗读前把一句话整理成语音模型念得好的样子。
//!
//! Kokoro 靠词典把字词变成音素：中文按字查，英文按词查，查不到的英文词直接跳过不念。新闻里偏偏全是
//! 词典里没有的词——缩写（CEO、GPU）、连写的品牌（iPhone、OpenAI）、人名地名（Biden、Huawei）。
//! 数字也一样：模型自带的中文规则（`number-zh.fst` 等）会把阿拉伯数字读成中文，但不认识 `%`、千分位逗号和
//! `10:30`，英文句子里的数字也会被读成中文。这里先处理一遍：
//!
//! - 网址不念。
//! - 中文句子：`5.2%` → `百分之5.2`，`1,234` → `1234`，`10:30` → `10点30分`，剩下的数字交给模型自带的规则。
//! - 英文句子：数字、年份、序数、百分比、金额、时间都写成英文单词。
//! - 英文词：词典里有的不动；连写的拆开（`MacBook` → `Mac Book`）；短的全大写缩写逐个字母念（`GPU` → `G P U`）；
//!   其余的试着拼成词典里的几个词（`Tencent` → `ten cent`），拼不出来的短词也逐个字母念。

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::{Captures, Regex};

/// 英文词典里的词（小写）。只用来判断一个词念不念得出来。
#[derive(Debug, Default, Clone)]
pub struct Words(HashSet<String>);

impl Words {
    /// 读 Kokoro 的英文词典（每行「词 音素…」）。
    pub fn from_lexicon(text: &str) -> Words {
        Words(
            text.lines()
                .filter_map(|line| line.split_whitespace().next())
                .filter(|w| w.bytes().all(|b| b.is_ascii_alphabetic() || b == b'\''))
                .map(|w| w.to_ascii_lowercase())
                .collect(),
        )
    }

    pub fn contains(&self, word: &str) -> bool {
        self.0.contains(&word.to_ascii_lowercase())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<S: Into<String>> FromIterator<S> for Words {
    fn from_iter<I: IntoIterator<Item = S>>(iter: I) -> Self {
        Words(
            iter.into_iter()
                .map(|w| w.into().to_ascii_lowercase())
                .collect(),
        )
    }
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid pattern")
}

// 注意：regex 的 `\b` 按 Unicode 算，汉字也是「词」的一部分，所以中文句子里的规则不能用 `\b`。
static URL: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)(?:https?://|www\.)[^\s\p{Han}，。！？；、）」』】]+"));
static CJK: LazyLock<Regex> = LazyLock::new(|| re(r"[\p{Han}\p{Hiragana}\p{Katakana}]"));
/// 字母和数字连在一起的地方（`M5`、`GPT-4`、`5G`）。序数（`25th`）和年代（`1990s`）不拆。
static LETTER_DIGIT: LazyLock<Regex> = LazyLock::new(|| re(r"([A-Za-z])-?(\d)"));
static DIGIT_LETTER: LazyLock<Regex> = LazyLock::new(|| re(r"(\d)([A-Za-z]+)"));
static DATE: LazyLock<Regex> = LazyLock::new(|| re(r"(\d{4})[-/.](\d{1,2})[-/.](\d{1,2})"));

// 中文句子
static ZH_PERCENT: LazyLock<Regex> = LazyLock::new(|| re(r"(\d+(?:\.\d+)?)\s?[%％]"));
static ZH_TIME: LazyLock<Regex> = LazyLock::new(|| re(r"(^|\D)(\d{1,2})[:：](\d{2})"));
static ZH_RANGE: LazyLock<Regex> = LazyLock::new(|| re(r"(\d)\s?[-–~～]\s?(\d)"));

// 英文句子
static EN_MONEY: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)([$€£¥])\s?(\d+(?:\.\d+)?)(?:\s?(thousand|million|billion|trillion|bn|[kmb])\b)?")
});
static EN_PERCENT: LazyLock<Regex> = LazyLock::new(|| re(r"(\d+(?:\.\d+)?)\s?%"));
static EN_TIME: LazyLock<Regex> = LazyLock::new(|| re(r"\b([01]?\d|2[0-3]):([0-5]\d)\b"));
static EN_RANGE: LazyLock<Regex> = LazyLock::new(|| re(r"(\d)\s?[–-]\s?(\d)"));
static EN_ORDINAL: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\b(\d+)(st|nd|rd|th)\b"));
static EN_DECADE: LazyLock<Regex> = LazyLock::new(|| re(r"\b(1[1-9]\d0|20\d0)'?s\b"));
/// 年份：前面不是数字或小数点；后面跟着小数部分的（第 3 组）不算。
static EN_YEAR: LazyLock<Regex> = LazyLock::new(|| re(r"(^|[^.\d])(1[1-9]\d\d|20\d\d)(\.\d+)?\b"));
static EN_NUMBER: LazyLock<Regex> = LazyLock::new(|| re(r"\d+(?:\.\d+)?"));
static ENGLISH_WORD: LazyLock<Regex> = LazyLock::new(|| re(r"[A-Za-z][A-Za-z'’]*"));

/// 整理一句话。`words` 是英文词典里的词。
pub fn prepare(text: &str, words: &Words) -> String {
    let text = URL.replace_all(text, " ");
    let zh = CJK.is_match(&text);
    let text = strip_thousands(&text);
    let text = dates(&text, zh);
    let text = split_letters_and_digits(&text);
    let text = if zh {
        chinese_numbers(&text)
    } else {
        english_numbers(&text)
    };
    let text = if zh { text } else { text.replace('&', " and ") };
    let text = ENGLISH_WORD.replace_all(&text, |c: &Captures| speakable_word(&c[0], words));
    collapse_spaces(&text)
}

/// 多个空白并成一个，去掉开头结尾和标点前面的空白。
fn collapse_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            space = !out.is_empty();
        } else {
            let punct = matches!(
                ch,
                '.' | ','
                    | '!'
                    | '?'
                    | ';'
                    | ':'
                    | ')'
                    | '，'
                    | '。'
                    | '！'
                    | '？'
                    | '；'
                    | '：'
                    | '、'
                    | '）'
            );
            if space && !punct {
                out.push(' ');
            }
            space = false;
            out.push(ch);
        }
    }
    out
}

fn split_letters_and_digits(text: &str) -> String {
    let text = LETTER_DIGIT.replace_all(text, "$1 $2");
    DIGIT_LETTER
        .replace_all(&text, |c: &Captures| {
            let tail = &c[2];
            let lower = tail.to_ascii_lowercase();
            // 25th、1990s、5G 里的 G 要拆，前两种不拆
            if matches!(lower.as_str(), "st" | "nd" | "rd" | "th" | "s") {
                c[0].to_string()
            } else {
                format!("{} {}", &c[1], tail)
            }
        })
        .into_owned()
}

fn chinese_numbers(text: &str) -> String {
    let text = ZH_PERCENT.replace_all(text, "百分之$1");
    let text = ZH_TIME.replace_all(&text, |c: &Captures| {
        let hour: u32 = c[2].parse().unwrap_or(99);
        let minute: u32 = c[3].parse().unwrap_or(99);
        let before = &c[1];
        match (hour, minute) {
            (0..=23, 0) => format!("{before}{hour}点"),
            (0..=23, 1..=59) => format!("{before}{hour}点{minute}分"),
            _ => c[0].to_string(),
        }
    });
    ZH_RANGE.replace_all(&text, "${1}到$2").into_owned()
}

fn english_numbers(text: &str) -> String {
    let text = EN_MONEY.replace_all(text, |c: &Captures| {
        let unit = match &c[1] {
            "$" => "dollars",
            "€" => "euros",
            "£" => "pounds",
            _ => "yuan",
        };
        let scale = c.get(3).map(|m| m.as_str().to_ascii_lowercase());
        let scale = match scale.as_deref() {
            Some("k") => " thousand",
            Some("m") => " million",
            Some("b" | "bn") => " billion",
            Some("thousand") => " thousand",
            Some("million") => " million",
            Some("billion") => " billion",
            Some("trillion") => " trillion",
            _ => "",
        };
        format!(" {}{scale} {unit} ", number_words(&c[2]))
    });
    let text = EN_PERCENT.replace_all(&text, |c: &Captures| {
        format!("{} percent", number_words(&c[1]))
    });
    let text = EN_TIME.replace_all(&text, |c: &Captures| {
        let hour = cardinal(c[1].parse().unwrap_or(0));
        let minute: u64 = c[2].parse().unwrap_or(0);
        match minute {
            0 => format!("{hour} o'clock"),
            1..=9 => format!("{hour} oh {}", cardinal(minute)),
            _ => format!("{hour} {}", cardinal(minute)),
        }
    });
    let text = EN_RANGE.replace_all(&text, "$1 to $2");
    let text = EN_ORDINAL.replace_all(&text, |c: &Captures| {
        c[1].parse::<u64>()
            .map(ordinal)
            .unwrap_or_else(|_| c[0].to_string())
    });
    let text = EN_DECADE.replace_all(&text, |c: &Captures| {
        let year = year_words(c[1].parse().unwrap_or(0));
        // nineteen ninety → nineteen nineties；two thousand → two thousands
        match year.strip_suffix('y') {
            Some(stem) => format!("{stem}ies"),
            None => format!("{year}s"),
        }
    });
    let text = EN_YEAR.replace_all(&text, |c: &Captures| {
        // 小数里的四位数（3.1415、2026.5）不当年份
        if c.get(3).is_some() {
            c[0].to_string()
        } else {
            format!("{}{}", &c[1], year_words(c[2].parse().unwrap_or(0)))
        }
    });
    EN_NUMBER
        .replace_all(&text, |c: &Captures| number_words(&c[0]))
        .into_owned()
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// `2026-11-19`、`2026/11/19` 这样的日期。中文读成「2026年11月19日」，英文读成「November nineteenth, twenty twenty-six」。
fn dates(text: &str, zh: bool) -> String {
    DATE.replace_all(text, |c: &Captures| {
        let (year, month, day): (u64, usize, u64) = (
            c[1].parse().unwrap_or(0),
            c[2].parse().unwrap_or(0),
            c[3].parse().unwrap_or(0),
        );
        if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            return c[0].to_string();
        }
        if zh {
            format!("{year}年{month}月{day}日")
        } else {
            format!(
                " {} {}, {} ",
                MONTHS[month - 1],
                ordinal(day),
                year_words(year)
            )
        }
    })
    .into_owned()
}

/// 去掉千分位逗号：`1,234,567` → `1234567`。逗号后面要正好是三位数字。
fn strip_thousands(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (i, &ch) in chars.iter().enumerate() {
        if ch == ','
            && i > 0
            && chars[i - 1].is_ascii_digit()
            && chars.len() >= i + 4
            && chars[i + 1..i + 4].iter().all(char::is_ascii_digit)
            && !chars.get(i + 4).is_some_and(char::is_ascii_digit)
        {
            continue;
        }
        out.push(ch);
    }
    out
}

const ONES: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];

/// 0–999 万亿以内的整数念成英文。
pub fn cardinal(n: u64) -> String {
    fn below_thousand(n: u64) -> String {
        let mut parts = Vec::new();
        if n >= 100 {
            parts.push(format!("{} hundred", ONES[(n / 100) as usize]));
        }
        let rest = n % 100;
        if rest > 0 || n == 0 {
            parts.push(if rest < 20 {
                ONES[rest as usize].to_string()
            } else if rest.is_multiple_of(10) {
                TENS[(rest / 10) as usize].to_string()
            } else {
                format!(
                    "{}-{}",
                    TENS[(rest / 10) as usize],
                    ONES[(rest % 10) as usize]
                )
            });
        }
        parts.join(" ")
    }
    if n < 1000 {
        return below_thousand(n);
    }
    const SCALES: [(u64, &str); 4] = [
        (1_000_000_000_000, "trillion"),
        (1_000_000_000, "billion"),
        (1_000_000, "million"),
        (1_000, "thousand"),
    ];
    let mut rest = n;
    let mut parts = Vec::new();
    for (size, name) in SCALES {
        if rest >= size {
            parts.push(format!("{} {name}", cardinal(rest / size)));
            rest %= size;
        }
    }
    if rest > 0 {
        parts.push(below_thousand(rest));
    }
    parts.join(" ")
}

/// 序数：1 → first，25 → twenty-fifth。
pub fn ordinal(n: u64) -> String {
    let words = cardinal(n);
    let (head, last) = match words.rfind([' ', '-']) {
        Some(i) => words.split_at(i + 1),
        None => ("", words.as_str()),
    };
    let last = match last {
        "one" => "first".to_string(),
        "two" => "second".to_string(),
        "three" => "third".to_string(),
        "five" => "fifth".to_string(),
        "eight" => "eighth".to_string(),
        "nine" => "ninth".to_string(),
        "twelve" => "twelfth".to_string(),
        w if w.ends_with('y') => format!("{}ieth", &w[..w.len() - 1]),
        w => format!("{w}th"),
    };
    format!("{head}{last}")
}

/// 年份：1999 → nineteen ninety-nine，2005 → two thousand five，2026 → twenty twenty-six。
pub fn year_words(year: u64) -> String {
    match year {
        2000..=2009 => cardinal(year),
        _ if year.is_multiple_of(100) => format!("{} hundred", cardinal(year / 100)),
        _ if year % 100 < 10 => format!("{} oh {}", cardinal(year / 100), cardinal(year % 100)),
        _ => format!("{} {}", cardinal(year / 100), cardinal(year % 100)),
    }
}

/// `12`、`3.14` 念成英文。太长的整数（电话号码之类）一位一位念。
fn number_words(digits: &str) -> String {
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    let one_by_one = |s: &str| -> String {
        s.chars()
            .filter_map(|c| c.to_digit(10))
            .map(|d| ONES[d as usize])
            .collect::<Vec<_>>()
            .join(" ")
    };
    let whole_words = if whole.len() > 15 || (whole.len() > 1 && whole.starts_with('0')) {
        one_by_one(whole)
    } else {
        cardinal(whole.parse().unwrap_or(0))
    };
    if fraction.is_empty() {
        format!(" {whole_words} ")
    } else {
        format!(" {whole_words} point {} ", one_by_one(fraction))
    }
}

/// 一个英文词怎么念。
fn speakable_word(word: &str, words: &Words) -> String {
    let word = word.replace('’', "'");
    if words.contains(&word) {
        return word;
    }
    // 所有格：apple's 和 apples 念法一样
    if let Some(base) = word.strip_suffix("'s").or_else(|| word.strip_suffix("'S")) {
        let plural = format!("{base}s");
        if words.contains(&plural) {
            return plural;
        }
        return speakable_word(base, words);
    }
    let word = word.trim_matches('\'');
    if word.is_empty() {
        return String::new();
    }
    // 连写的拆开：MacBook、iPhone、OpenAI、ChatGPT
    let parts = camel_parts(word);
    if parts.len() > 1 {
        return parts
            .iter()
            .map(|p| speakable_word(p, words))
            .collect::<Vec<_>>()
            .join(" ");
    }
    let upper = word.bytes().all(|b| b.is_ascii_uppercase());
    // 缩写（GPU、CEO、NBA）和单个字母（iPhone 的 i）逐个字母念；复数 GPUs 念 G P U s 不如直接念 G P U
    if (upper && word.len() <= 4) || word.len() == 1 {
        return spell(word);
    }
    if let Some(stem) = word.strip_suffix('s')
        && stem.len() >= 2
        && stem.len() <= 4
        && stem.bytes().all(|b| b.is_ascii_uppercase())
    {
        return spell(stem);
    }
    if let Some(pieces) = compose(&word.to_ascii_lowercase(), words) {
        return pieces.join(" ");
    }
    // 拼不出来的：短的大写词逐个字母念，其余原样留着（模型会跳过，总比念错字母强）
    if upper && word.len() <= 6 {
        return spell(word);
    }
    word.to_string()
}

/// 逐个字母念：`GPU` → `G P U`。
fn spell(word: &str) -> String {
    word.chars()
        .filter(char::is_ascii_alphabetic)
        .map(|c| c.to_ascii_uppercase().to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

/// 在小写变大写、和一串大写后面跟小写的地方拆开：`OpenAI` → `Open`、`AI`；`GPTs` 不拆。
fn camel_parts(word: &str) -> Vec<String> {
    let chars: Vec<char> = word.chars().collect();
    let mut parts = Vec::new();
    let mut start = 0;
    for i in 1..chars.len() {
        let (prev, cur) = (chars[i - 1], chars[i]);
        let next = chars.get(i + 1).copied();
        let lower_to_upper = prev.is_ascii_lowercase() && cur.is_ascii_uppercase();
        // XMLParser → XML Parser：一串大写的最后一个属于下一个词
        let acronym_end = prev.is_ascii_uppercase()
            && cur.is_ascii_uppercase()
            && next.is_some_and(|n| n.is_ascii_lowercase())
            && !(i + 2 == chars.len() && next == Some('s'));
        if lower_to_upper || acronym_end {
            parts.push(chars[start..i].iter().collect());
            start = i;
        }
    }
    parts.push(chars[start..].iter().collect());
    // 只有一个字母的开头（iPhone 的 i）单独念字母，没问题；拆不出东西就是一整个词
    parts
}

/// 把词典里没有的词拼成词典里的几个词，用的词越少越好，每个至少两个字母，最多四个。
fn compose(word: &str, words: &Words) -> Option<Vec<String>> {
    const MAX_PIECES: usize = 4;
    let n = word.len();
    if n < 4 || !word.is_ascii() {
        return None;
    }
    // best[i]：拼出前 i 个字母最少要几个词，以及最后一个词从哪儿开始
    let mut best: Vec<Option<(usize, usize)>> = vec![None; n + 1];
    best[0] = Some((0, 0));
    for end in 2..=n {
        for start in 0..=end - 2 {
            let Some((count, _)) = best[start] else {
                continue;
            };
            if count >= MAX_PIECES || !words.contains(&word[start..end]) {
                continue;
            }
            let better = match best[end] {
                None => true,
                // 一样多的时候要前面的词长的（`ten cent` 而不是 `te ncent`）
                Some((c, s)) => count + 1 < c || (count + 1 == c && start > s),
            };
            if better {
                best[end] = Some((count + 1, start));
            }
        }
    }
    best[n]?;
    let mut pieces = Vec::new();
    let mut end = n;
    while end > 0 {
        let (_, start) = best[end]?;
        pieces.push(word[start..end].to_string());
        end = start;
    }
    pieces.reverse();
    if pieces.len() < 2 {
        return None;
    }
    Some(pieces)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用的小词典：几个普通词，加上念数字要用到的词（真正的词典里都有）。
    fn words() -> Words {
        let plain = [
            "the", "new", "laptop", "ships", "on", "november", "mac", "book", "pro", "open", "ai",
            "chat", "phone", "deep", "seek", "ten", "cent", "ali", "baba", "apples", "apple",
            "price", "is", "it's", "percent", "dollars", "rose", "in", "said", "a", "bi", "den",
            "byte", "dance", "you", "tube", "parser", "and", "see", "for", "more", "to", "it",
            "sold", "units", "at", "from", "deal", "revenue", "point", "oh", "o'clock", "nineties",
            "was", "pi",
        ];
        let numbers = (0..100u64)
            .flat_map(|n| [cardinal(n), ordinal(n)])
            .chain(["hundred", "thousand", "million", "billion"].map(String::from))
            .flat_map(|w| w.split([' ', '-']).map(String::from).collect::<Vec<_>>());
        plain.into_iter().map(String::from).chain(numbers).collect()
    }

    #[test]
    fn numbers_in_english() {
        assert_eq!(cardinal(0), "zero");
        assert_eq!(cardinal(21), "twenty-one");
        assert_eq!(cardinal(105), "one hundred five");
        assert_eq!(cardinal(12_999), "twelve thousand nine hundred ninety-nine");
        assert_eq!(cardinal(3_000_000), "three million");
        assert_eq!(ordinal(1), "first");
        assert_eq!(ordinal(25), "twenty-fifth");
        assert_eq!(ordinal(40), "fortieth");
        assert_eq!(ordinal(112), "one hundred twelfth");
        assert_eq!(year_words(1999), "nineteen ninety-nine");
        assert_eq!(year_words(2005), "two thousand five");
        assert_eq!(year_words(2026), "twenty twenty-six");
        assert_eq!(year_words(1900), "nineteen hundred");
        assert_eq!(year_words(1907), "nineteen oh seven");
    }

    #[test]
    fn english_sentences() {
        let w = words();
        assert_eq!(
            prepare("The new MacBook Pro ships on November 25th.", &w),
            "The new Mac Book Pro ships on November twenty-fifth."
        );
        assert_eq!(
            prepare("Revenue rose 12.5% to $3.2 billion in 2026.", &w),
            "Revenue rose twelve point five percent to three point two billion dollars in twenty twenty-six."
        );
        assert_eq!(
            prepare("It sold 1,234,567 units", &w),
            "It sold one million two hundred thirty-four thousand five hundred sixty-seven units"
        );
        assert_eq!(
            prepare("at 9:05 and 10:00", &w),
            "at nine oh five and ten o'clock"
        );
        assert_eq!(prepare("in the 1990s", &w), "in the nineteen nineties");
        assert_eq!(
            prepare("from 2020-2025", &w),
            "from twenty twenty to twenty twenty-five"
        );
        assert_eq!(prepare("a $5m deal", &w), "a five million dollars deal");
        assert_eq!(
            prepare("On 2026-11-19, pi was 3.1415.", &w),
            "On November nineteenth, twenty twenty-six, pi was three point one four one five."
        );
    }

    #[test]
    fn dates_in_chinese() {
        let w = words();
        assert_eq!(prepare("发布于2026-11-19，", &w), "发布于2026年11月19日，");
        assert_eq!(prepare("日期2026/13/40", &w), "日期2026/13/40");
        assert_eq!(prepare("比分是110:30", &w), "比分是110:30");
    }

    #[test]
    fn unknown_words() {
        let w = words();
        assert_eq!(prepare("OpenAI and ChatGPT", &w), "Open AI and Chat G P T");
        assert_eq!(prepare("the CEO said GPUs", &w), "the C E O said G P U");
        assert_eq!(prepare("iPhone", &w), "I Phone");
        assert_eq!(
            prepare("Tencent, Alibaba, ByteDance, YouTube", &w),
            "ten cent, ali baba, Byte Dance, You Tube"
        );
        assert_eq!(prepare("Biden", &w), "bi den");
        assert_eq!(prepare("Apple's price", &w), "Apples price");
        assert_eq!(prepare("it’s", &w), "it's");
        assert_eq!(prepare("XMLParser", &w), "X M L Parser");
        // 拼不出来的长词原样留着（模型会跳过）
        assert_eq!(prepare("Zelensky", &w), "Zelensky");
    }

    #[test]
    fn chinese_sentences() {
        let w = words();
        assert_eq!(
            prepare("国内生产总值增长5.2%，同比增长1,234万。", &w),
            "国内生产总值增长百分之5.2，同比增长1234万。"
        );
        assert_eq!(
            prepare("会议于10:30开始，10:00结束", &w),
            "会议于10点30分开始，10点结束"
        );
        assert_eq!(prepare("2020-2025年", &w), "2020到2025年");
        // 中文里的英文照样处理，数字留给模型自带的规则
        assert_eq!(
            prepare(
                "英伟达CEO表示，OpenAI发布了GPT-5，iPhone 17售价5999元。",
                &w
            ),
            "英伟达C E O表示，Open AI发布了G P T 5，I Phone 17售价5999元。"
        );
        assert_eq!(prepare("5G网络", &w), "5 G网络");
    }

    #[test]
    fn links_are_not_read() {
        let w = words();
        assert_eq!(prepare("详见 https://example.com/a?b=1 。", &w), "详见。");
        assert_eq!(prepare("see www.example.com for more", &w), "see for more");
    }

    #[test]
    fn lexicon_words() {
        let w = Words::from_lexicon("kokoro k ˈ O k ə ɹ O\nIt's ɪ t s\n中 ʈʂ\n");
        assert!(w.contains("Kokoro"));
        assert!(w.contains("it's"));
        assert_eq!(w.len(), 2);
    }
}
