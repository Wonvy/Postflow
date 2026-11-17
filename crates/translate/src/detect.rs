//! 识别一段文字是什么语言，给出和模型目录一致的语言代码。

use whatlang::Lang;

/// 最多看这么多字。邮件后面常是引用和签名，前面的正文更能代表它的语言。
const SAMPLE_CHARS: usize = 2000;

pub fn detect(text: &str) -> Option<String> {
    let end = text
        .char_indices()
        .nth(SAMPLE_CHARS)
        .map_or(text.len(), |(i, _)| i);
    let sample = &text[..end];
    let info = whatlang::detect(sample)?;
    let code = match info.lang() {
        Lang::Cmn => {
            return Some(
                if is_traditional(sample) {
                    "zh_hant"
                } else {
                    "zh"
                }
                .to_string(),
            );
        }
        // whatlang 对短句不太准，一句英文常被认成丹麦语、荷兰语。邮件里英文最常见，先单独认一下
        Lang::Eng => "en",
        lang if info.script() == whatlang::Script::Latin
            && (looks_english(sample) || english_beats(sample, lang)) =>
        {
            "en"
        }
        lang => to_code(lang)?,
    };
    Some(code.to_string())
}

/// 英语里最常见的虚词占了足够多的比例。
fn looks_english(text: &str) -> bool {
    let words = words(text);
    let hits = count(&words, ENGLISH);
    hits >= 2 && hits * 4 >= words.len()
}

const ENGLISH: &[&str] = &[
    "the", "a", "an", "and", "or", "but", "of", "to", "in", "on", "at", "for", "with", "from",
    "by", "is", "are", "was", "were", "be", "been", "it", "this", "that", "you", "your", "we",
    "our", "i", "my", "me", "he", "she", "they", "them", "will", "would", "could", "can", "should",
    "have", "has", "had", "do", "does", "not", "if", "as", "please", "thanks", "hi", "hello",
    "before", "after", "about", "what", "when", "there", "here", "all", "any", "let", "lets",
    "know", "more", "every",
];

/// 和英语容易混的几种语言最常见的虚词。
fn function_words(lang: Lang) -> &'static [&'static str] {
    match lang {
        Lang::Fra => &[
            "le", "la", "les", "des", "du", "de", "un", "une", "et", "est", "pour", "dans", "avec",
            "sur", "pas", "que", "qui", "vous", "nous", "il", "elle", "ce", "cette", "au", "aux",
            "ne", "sont",
        ],
        Lang::Deu => &[
            "der", "die", "das", "und", "ist", "nicht", "mit", "ein", "eine", "zu", "den", "dem",
            "für", "auf", "ich", "sie", "es", "wir", "du", "bitte", "danke", "von", "im",
        ],
        Lang::Spa => &[
            "el", "la", "los", "las", "de", "que", "y", "en", "un", "una", "es", "por", "para",
            "con", "no", "su", "se", "lo", "al", "del",
        ],
        Lang::Ita => &[
            "il", "lo", "la", "gli", "le", "di", "che", "e", "è", "un", "una", "per", "con", "non",
            "sono", "del", "della", "nel",
        ],
        Lang::Por => &[
            "o", "os", "as", "de", "que", "e", "é", "um", "uma", "para", "com", "não", "do", "da",
            "em", "por",
        ],
        Lang::Nld => &[
            "de", "het", "een", "en", "van", "is", "niet", "met", "voor", "op", "dat", "die", "ik",
            "je", "we", "zijn",
        ],
        Lang::Dan | Lang::Nob | Lang::Swe => &[
            "og", "och", "er", "är", "det", "at", "att", "en", "til", "till", "på", "med", "for",
            "för", "ikke", "inte", "jeg", "jag", "du", "vi", "som", "kan", "den",
        ],
        _ => &[],
    }
}

/// whatlang 认成了别的拉丁字母语言，但英语的常用词比那种语言的还多：其实是英语。
fn english_beats(text: &str, lang: Lang) -> bool {
    let words = words(text);
    let english = count(&words, ENGLISH);
    english >= 1 && english > count(&words, function_words(lang))
}

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphabetic() && c != '\'')
        .filter(|w| !w.is_empty())
        .take(400)
        .map(|w| w.to_lowercase())
        .collect()
}

fn count(words: &[String], list: &[&str]) -> usize {
    words.iter().filter(|w| list.contains(&w.as_str())).count()
}

/// whatlang 用 ISO 639-3，模型目录用 ISO 639-1。
fn to_code(lang: Lang) -> Option<&'static str> {
    Some(match lang.code() {
        "afr" => "af",
        "ara" => "ar",
        "aze" => "az",
        "bel" => "be",
        "ben" => "bn",
        "bul" => "bg",
        "cat" => "ca",
        "ces" => "cs",
        "dan" => "da",
        "deu" => "de",
        "ell" => "el",
        "eng" => "en",
        "est" => "et",
        "fin" => "fi",
        "fra" => "fr",
        "guj" => "gu",
        "heb" => "he",
        "hin" => "hi",
        "hrv" => "hr",
        "hun" => "hu",
        "ind" => "id",
        "ita" => "it",
        "jpn" => "ja",
        "kan" => "kn",
        "kor" => "ko",
        "lav" => "lv",
        "lit" => "lt",
        "mal" => "ml",
        "mar" => "mr",
        "nld" => "nl",
        "nob" => "nb",
        "pes" => "fa",
        "pol" => "pl",
        "por" => "pt",
        "ron" => "ro",
        "rus" => "ru",
        "slk" => "sk",
        "slv" => "sl",
        "spa" => "es",
        "srp" => "sr",
        "swe" => "sv",
        "tam" => "ta",
        "tel" => "te",
        "tgl" => "tl",
        "tha" => "th",
        "tur" => "tr",
        "ukr" => "uk",
        "urd" => "ur",
        "vie" => "vi",
        _ => return None,
    })
}

/// 繁体和简体在 whatlang 里是同一种语言。数一数常用字里只在繁体或只在简体里出现的写法。
fn is_traditional(text: &str) -> bool {
    const TRADITIONAL: &str = "們這個說時來為會對國學後過還與開關問題點經發實電長現東車書買賣錢見從應該讓請謝給號裡愛覺變處體歡區務廣辦員";
    const SIMPLIFIED: &str = "们这个说时来为会对国学后过还与开关问题点经发实电长现东车书买卖钱见从应该让请谢给号里爱觉变处体欢区务广办员";
    let (mut trad, mut simp) = (0, 0);
    for c in text.chars() {
        if TRADITIONAL.contains(c) {
            trad += 1;
        } else if SIMPLIFIED.contains(c) {
            simp += 1;
        }
    }
    trad > simp
}

/// 中文、日文这类不用空格分词的语言。
pub fn is_cjk_lang(code: &str) -> bool {
    matches!(code, "zh" | "zh_hant" | "ja")
}

/// 汉字、假名、韩文字母。
pub fn is_cjk_char(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF   // 平假名、片假名
        | 0x3400..=0x4DBF // 扩展 A
        | 0x4E00..=0x9FFF // 基本汉字
        | 0xF900..=0xFAFF
        | 0xAC00..=0xD7AF // 韩文
        | 0x20000..=0x2FFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_languages() {
        assert_eq!(
            detect("今天天气很好，我们下午去公园散步吧。").as_deref(),
            Some("zh")
        );
        assert_eq!(
            detect("這個問題我們明天開會時再說，謝謝大家。").as_deref(),
            Some("zh_hant")
        );
        assert_eq!(
            detect("Could you take a look at the budget before Friday?").as_deref(),
            Some("en")
        );
        assert_eq!(
            detect("今日はとても良い天気ですね。散歩に行きましょう。").as_deref(),
            Some("ja")
        );
        assert_eq!(
            detect("Hi Mia, thanks for the notes.").as_deref(),
            Some("en")
        );
        assert_eq!(
            detect("Pouvez-vous regarder le budget avant vendredi, s'il vous plaît ?").as_deref(),
            Some("fr")
        );
        assert_eq!(
            detect("Kannst du dir das Budget bitte vor Freitag ansehen? Danke dir.").as_deref(),
            Some("de")
        );
        assert_eq!(
            detect("Kan du kigge på budgettet inden fredag? Det er vigtigt for os.").as_deref(),
            Some("da")
        );
    }

    #[test]
    fn nothing_to_detect() {
        assert_eq!(detect("1234 5678"), None);
    }
}
