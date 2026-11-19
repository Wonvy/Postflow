//! 从一封邮件里找验证码（登录、注册时发来的那种一次性数字）。
//!
//! 宁可漏掉也不乱认：先要在主题或正文里看到「验证码」这类说法，再在它附近找 4–8 位的数字
//! （也认 `123 456`、`123-456` 这种分开写的，和紧跟在说法后面的字母数字混合码）。
//! 日期、金额、电话、订单号这些长得像的数字不算。

/// 只看正文开头这么多字：验证码都在前面，后面是页脚和法律声明。
const BODY_CHARS: usize = 3000;
/// 验证码在说法后面最多隔这么多字。
const AFTER: usize = 48;
/// 验证码在说法前面最多隔这么多字（「123456 is your verification code」）。
const BEFORE: usize = 28;

/// 看到就能认定在说验证码的说法（都用小写）。
const STRONG: &[&str] = &[
    "验证码",
    "校验码",
    "动态码",
    "动态密码",
    "确认码",
    "安全码",
    "登录码",
    "认证码",
    "验证代码",
    "安全代码",
    "一次性密码",
    "驗證碼",
    "確認碼",
    "安全碼",
    "驗證代碼",
    "認証コード",
    "確認コード",
    "検証コード",
    "인증번호",
    "인증 코드",
    "verification code",
    "verification pin",
    "security code",
    "confirmation code",
    "authentication code",
    "authorization code",
    "login code",
    "log-in code",
    "sign-in code",
    "sign in code",
    "access code",
    "one-time code",
    "one time code",
    "one-time password",
    "one time password",
    "one-time passcode",
    "one time passcode",
    "single-use code",
    "passcode",
    "otp",
    "2fa code",
    "code de vérification",
    "código de verificación",
    "código de verificação",
    "bestätigungscode",
    "verifizierungscode",
    "код подтверждения",
];

/// 单独一个「code」什么都可能是（优惠码、邮编），要邮件里还有这些词才算。
const WEAK: &[&str] = &["code"];
const CONTEXT: &[&str] = &[
    "verify",
    "verification",
    "confirm",
    "sign in",
    "sign-in",
    "signing in",
    "log in",
    "login",
    "authenticat",
    "one-time",
    "two-factor",
    "two-step",
    "2fa",
    "expire",
];

/// 说法和字母数字混合码之间只能有这些（「验证码：」「code is: 」）。
fn separator(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            ':' | '：'
                | '-'
                | '—'
                | '"'
                | '“'
                | '”'
                | '\''
                | '*'
                | '【'
                | '】'
                | '['
                | ']'
                | '('
                | ')'
                | '（'
                | '）'
                | '是'
                | '为'
                | '為'
        )
}

struct Candidate {
    start: usize,
    end: usize,
    code: String,
    /// 字母数字混合的码：只有紧跟在说法后面才算。
    mixed: bool,
}

fn word_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
}

/// `needle` 在 `hay` 里出现的位置（字的下标）。拉丁字母的说法要是完整的词。
fn occurrences(hay: &[char], needle: &str) -> Vec<(usize, usize)> {
    let needle: Vec<char> = needle.chars().collect();
    let mut out = Vec::new();
    if needle.is_empty() || hay.len() < needle.len() {
        return out;
    }
    let whole_word = needle.iter().any(|c| c.is_ascii_alphabetic());
    for start in 0..=hay.len() - needle.len() {
        if hay[start..start + needle.len()] != needle[..] {
            continue;
        }
        let end = start + needle.len();
        if whole_word {
            let before = start.checked_sub(1).map(|i| hay[i]);
            let after = hay.get(end).copied();
            // 「codes」「code's」也算；「barcode」「encoded」不算
            // 只看拉丁字母和数字：紧挨着汉字的（「您的otp是」）也算完整的词
            let tail_ok = match after {
                Some('s') => !hay.get(end + 1).is_some_and(char::is_ascii_alphanumeric),
                Some(c) => !c.is_ascii_alphanumeric(),
                None => true,
            };
            if before.is_some_and(|c| c.is_ascii_alphanumeric()) || !tail_ok {
                continue;
            }
        }
        out.push((start, end));
    }
    out
}

/// 这串数字是不是日期、金额、电话、小数、编号的一部分。
fn looks_like_something_else(chars: &[char], start: usize, end: usize) -> bool {
    let prev = start.checked_sub(1).map(|i| chars[i]);
    let prev2 = start.checked_sub(2).map(|i| chars[i]);
    let next = chars.get(end).copied();
    let next2 = chars.get(end + 1).copied();
    let digit = |c: Option<char>| c.is_some_and(|c| c.is_ascii_digit());
    // 小数、版本号、IP、千分位
    if matches!(prev, Some('.' | ',')) && digit(prev2) {
        return true;
    }
    if matches!(next, Some('.' | ',')) && digit(next2) {
        return true;
    }
    // 日期、时间、电话、范围：2026-11-19、12:30、400-800-1234
    if matches!(prev, Some('-' | '/' | ':')) && digit(prev2) {
        return true;
    }
    if matches!(next, Some('-' | '/' | ':')) && digit(next2) {
        return true;
    }
    // 年份、金额、百分比、电话、编号
    matches!(next, Some('年' | '元' | '%' | '％' | '円' | '원'))
        || matches!(
            prev,
            Some('$' | '¥' | '￥' | '€' | '£' | '+' | '#' | '№' | '@' | '/' | '=' | '_')
        )
        || matches!(next, Some('@' | '_' | '='))
}

fn candidates(chars: &[char]) -> Vec<Candidate> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !word_char(chars[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && word_char(chars[i]) {
            i += 1;
        }
        let end = i;
        let token = &chars[start..end];
        let digits = token.iter().filter(|c| c.is_ascii_digit()).count();
        if digits == token.len() {
            // 分开写的：123 456、123-456、1234 5678（后面不能再跟一组，那是电话或卡号）
            let sep = chars.get(end).copied();
            if matches!(token.len(), 3 | 4) && matches!(sep, Some(' ' | '-')) {
                let mut j = end + 1;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                }
                let second = j - (end + 1);
                let after = chars.get(j).copied();
                let more = matches!(after, Some(' ' | '-'))
                    && chars.get(j + 1).is_some_and(char::is_ascii_digit);
                let glued = after.is_some_and(word_char);
                if second == token.len()
                    && !more
                    && !glued
                    && !looks_like_something_else(chars, start, j)
                {
                    let code: String = token.iter().chain(&chars[end + 1..j]).collect();
                    out.push(Candidate {
                        start,
                        end: j,
                        code,
                        mixed: false,
                    });
                    i = j;
                    continue;
                }
            }
            if (4..=8).contains(&token.len()) && !looks_like_something_else(chars, start, end) {
                out.push(Candidate {
                    start,
                    end,
                    code: token.iter().collect(),
                    mixed: false,
                });
            }
        } else if (5..=8).contains(&token.len())
            && digits >= 1
            && token
                .iter()
                .all(|c| c.is_ascii_digit() || c.is_ascii_uppercase())
        {
            // 字母数字混合的码都是大写的（Steam 的 F4K2B 这种）
            out.push(Candidate {
                start,
                end,
                code: token.iter().collect(),
                mixed: true,
            });
        }
    }
    out
}

/// 邮件里的验证码。没有，或者拿不准，就是 `None`。
pub fn verification_code(subject: &str, body: &str) -> Option<String> {
    let chars: Vec<char> = subject
        .chars()
        .chain(std::iter::once('\n'))
        .chain(body.chars().take(BODY_CHARS))
        .collect();
    // 逐个字转小写，下标和原文对得上
    let lower: Vec<char> = chars
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();

    let mut keywords: Vec<(usize, usize)> =
        STRONG.iter().flat_map(|k| occurrences(&lower, k)).collect();
    let context = CONTEXT
        .iter()
        .any(|w| !occurrences_loose(&lower, w).is_empty());
    if context {
        keywords.extend(WEAK.iter().flat_map(|k| occurrences(&lower, k)));
    }
    if keywords.is_empty() {
        return None;
    }
    keywords.sort_unstable();

    let all = candidates(&chars);
    // (说法在前还是在后, 隔了多少字) 最小的那个
    let mut best: Option<((u8, usize), &Candidate)> = None;
    let mut consider = |rank: (u8, usize), c| {
        if best.as_ref().is_none_or(|(r, _)| rank < *r) {
            best = Some((rank, c));
        }
    };
    for &(ks, ke) in &keywords {
        let after = all.iter().find(|c| {
            c.start >= ke
                && c.start - ke <= AFTER
                && (!c.mixed || chars[ke..c.start].iter().all(|ch| separator(*ch)))
        });
        if let Some(c) = after {
            consider((0, c.start - ke), c);
        }
        let before = all
            .iter()
            .rev()
            .find(|c| !c.mixed && c.end <= ks && ks - c.end <= BEFORE);
        if let Some(c) = before {
            consider((1, ks - c.end), c);
        }
    }
    best.map(|(_, c)| c.code.clone())
}

/// 不要求是完整的词（「authenticat」要匹配 authenticate、authentication）。
fn occurrences_loose(hay: &[char], needle: &str) -> Vec<usize> {
    let needle: Vec<char> = needle.chars().collect();
    if needle.is_empty() || hay.len() < needle.len() {
        return Vec::new();
    }
    (0..=hay.len() - needle.len())
        .filter(|&i| hay[i..i + needle.len()] == needle[..])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::verification_code;

    fn code(subject: &str, body: &str) -> Option<String> {
        verification_code(subject, body)
    }

    #[test]
    fn chinese_codes() {
        assert_eq!(
            code(
                "【小米】验证码",
                "您的验证码是：482913，5 分钟内有效，请勿泄露给他人。"
            )
            .as_deref(),
            Some("482913")
        );
        assert_eq!(code("登录验证码 6021", "").as_deref(), Some("6021"));
        assert_eq!(
            code(
                "安全提醒",
                "你正在登录微信，动态密码为 73920184。如非本人操作请忽略。"
            )
            .as_deref(),
            Some("73920184")
        );
        // 数字在前
        assert_eq!(
            code("通知", "558203 是你的验证码，10 分钟内有效。").as_deref(),
            Some("558203")
        );
        assert_eq!(
            code(
                "Microsoft 帐户安全代码",
                "请使用以下安全代码：\n\n安全代码: 1934052\n\n谢谢"
            )
            .as_deref(),
            Some("1934052")
        );
        assert_eq!(
            code("驗證碼", "您的驗證碼為 204815").as_deref(),
            Some("204815")
        );
    }

    #[test]
    fn english_codes() {
        assert_eq!(
            code(
                "[GitHub] Please verify your device",
                "Hey octocat!\n\nA sign in attempt requires further verification because we did not recognize \
                 your device. To complete the sign in, enter the verification code on the unrecognized device.\n\n\
                 Device: Safari on macOS\nVerification code: 482019\n\nIf you did not attempt to sign in…"
            )
            .as_deref(),
            Some("482019")
        );
        assert_eq!(
            code("384756 is your Twitter verification code", "").as_deref(),
            Some("384756")
        );
        assert_eq!(
            code(
                "Your Google verification code",
                "G-719284 is your Google verification code."
            )
            .as_deref(),
            Some("719284")
        );
        assert_eq!(
            code(
                "Sign in to Example",
                "Enter this code to sign in:\n\n 123 456 \n\nThis code expires in 10 minutes."
            )
            .as_deref(),
            Some("123456")
        );
        assert_eq!(
            code(
                "Your login code",
                "Your one-time passcode is 9041-2277. Do not share it."
            )
            .as_deref(),
            Some("90412277")
        );
        // 字母数字混合码，紧跟在说法后面
        assert_eq!(
            code(
                "Your Steam account: Access from new computer",
                "Here is the Steam Guard code you need to login:\n\nLogin Code: F4K2B\n"
            )
            .as_deref(),
            Some("F4K2B")
        );
        assert_eq!(code("Your OTP is 5512", "").as_deref(), Some("5512"));
    }

    #[test]
    fn other_languages() {
        assert_eq!(
            code("認証コード", "認証コード: 660193").as_deref(),
            Some("660193")
        );
        assert_eq!(
            code("인증번호 안내", "인증번호 [482913] 를 입력해 주세요.").as_deref(),
            Some("482913")
        );
        assert_eq!(
            code("Votre code", "Votre code de vérification est 204815.").as_deref(),
            Some("204815")
        );
    }

    #[test]
    fn ordinary_mail_has_no_code() {
        // 没有提到验证码
        assert_eq!(
            code(
                "发票 20261119",
                "订单号 88203341，金额 1280 元，电话 400-800-1234。"
            ),
            None
        );
        assert_eq!(
            code(
                "Weekly report",
                "We shipped 1234 units in 2026. Call 555-0100."
            ),
            None
        );
        // 「code」不是验证码
        assert_eq!(
            code(
                "Black Friday",
                "Use promo code SAVE2026 for 20% off orders over $1000."
            ),
            None
        );
        assert_eq!(
            code(
                "Your order",
                "Zip code 94107. Order 12345678 ships tomorrow."
            ),
            None
        );
        assert_eq!(
            code(
                "Code review",
                "PR 4821 changes the source code of module 2048."
            ),
            None
        );
        // 提到了验证码，但附近的数字是日期、金额、时间
        assert_eq!(
            code(
                "安全提醒",
                "你在 2026-11-19 12:30 申请了验证码，如非本人操作请修改密码。"
            ),
            None
        );
        assert_eq!(
            code(
                "Security alert",
                "A verification code was requested on 2026/11/19. Amount: $1500.00"
            ),
            None
        );
        assert_eq!(
            code(
                "验证码登录已开启",
                "从现在起可以用验证码登录。客服电话 400-800-1234"
            ),
            None
        );
        // 说法离数字太远
        let far = format!("验证码{}123456", "啊".repeat(80));
        assert_eq!(code("", &far), None);
        assert_eq!(code("", ""), None);
    }

    #[test]
    fn picks_the_code_next_to_the_phrase() {
        assert_eq!(
            code(
                "Confirm your email",
                "Hi! Thanks for joining on 19 Nov 2026. Order 77120934 is unrelated.\n\
                 Your confirmation code is 815204. It expires in 15 minutes. Ticket 5567."
            )
            .as_deref(),
            Some("815204")
        );
        // 正文很长时只看前面
        let long = format!("{}验证码 123456", "正文。".repeat(2000));
        assert_eq!(code("通知", &long), None);
    }
}
