//! 把写好的一封信变成可以发出去的邮件原文。
//!
//! 只写纯文本：回复时引用原信用「>」，和大多数邮件程序的纯文本模式一样。

use mail_builder::MessageBuilder;
use mail_builder::headers::address::Address;
use mail_builder::headers::raw::Raw;

use crate::message::Mailbox;
use crate::{Error, Result};

/// 一次最多发给多少人。再多的多半是填错了，服务器也会拒绝。
const MAX_RECIPIENTS: usize = 100;
/// References 里最多保留多少个 Message-ID。很长的讨论只留最早的一个和最近的几个。
const MAX_REFERENCES: usize = 20;

/// 写好的一封信。
#[derive(Debug, Clone, Default)]
pub struct Outgoing {
    pub from: Mailbox,
    pub to: Vec<Mailbox>,
    pub cc: Vec<Mailbox>,
    /// 密送：只写进信封，不写进信头，其他收件人看不到。
    pub bcc: Vec<Mailbox>,
    pub subject: String,
    pub text: String,
    /// 回复的那封信的 Message-ID，不带尖括号。
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
}

/// 生成好的邮件。
#[derive(Debug, Clone)]
pub struct Built {
    /// 不带尖括号的 Message-ID。
    pub message_id: String,
    pub raw: Vec<u8>,
    /// 信封上的发件人地址。
    pub sender: String,
    /// 信封上的收件人：收件人、抄送和密送，去掉重复。
    pub recipients: Vec<String>,
}

/// 把用户输入的一个收件人解析成名字和地址。认 `a@example.com`、`名字 <a@example.com>`
/// 和 `"名字" <a@example.com>`。地址不像样就返回 `None`。
pub fn parse_address(input: &str) -> Option<Mailbox> {
    let input = input.trim();
    let (name, address) = match input.rfind('<') {
        Some(open) if input.ends_with('>') => {
            let name = input[..open].trim().trim_matches('"').trim();
            (name, input[open + 1..input.len() - 1].trim())
        }
        _ => ("", input),
    };
    if !valid_address(address) {
        return None;
    }
    Some(Mailbox {
        name: (!name.is_empty()).then(|| name.to_string()),
        address: Some(address.to_string()),
    })
}

fn valid_address(address: &str) -> bool {
    let Some((local, domain)) = address.rsplit_once('@') else {
        return false;
    };
    let bad = |c: char| c.is_whitespace() || c.is_control() || "<>()[],;:\"\\@".contains(c);
    !local.is_empty()
        && !domain.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !local.chars().any(bad)
        && !domain.chars().any(bad)
        && address.len() <= 254
}

/// 回复一封信时的 In-Reply-To 和 References：接在原信的 References 后面加上原信自己。
pub fn thread_headers(
    message_id: Option<&str>,
    references: &[String],
) -> (Option<String>, Vec<String>) {
    let Some(id) = message_id.map(str::trim).filter(|id| !id.is_empty()) else {
        return (None, Vec::new());
    };
    let mut refs: Vec<String> = references
        .iter()
        .map(|r| r.trim().to_string())
        .filter(|r| !r.is_empty() && r != id)
        .collect();
    refs.push(id.to_string());
    if refs.len() > MAX_REFERENCES {
        // 第一个是整个讨论的开头，留着；其余只留最近的
        let tail = refs.split_off(refs.len() - (MAX_REFERENCES - 1));
        refs.truncate(1);
        refs.extend(tail);
    }
    (Some(id.to_string()), refs)
}

/// 生成邮件原文。`now` 是 Unix 秒，写进 Date。
pub fn build(out: &Outgoing, now: i64) -> Result<Built> {
    let sender = out
        .from
        .address
        .as_deref()
        .map(str::trim)
        .filter(|a| valid_address(a))
        .ok_or_else(|| Error::Config("the sender address is invalid".into()))?
        .to_string();

    let mut recipients: Vec<String> = Vec::new();
    for mailbox in out.to.iter().chain(&out.cc).chain(&out.bcc) {
        let address = mailbox.address.as_deref().map(str::trim).unwrap_or("");
        if !valid_address(address) {
            return Err(Error::Config(format!("invalid recipient: {address}")));
        }
        if !recipients.iter().any(|r| r.eq_ignore_ascii_case(address)) {
            recipients.push(address.to_string());
        }
    }
    if recipients.is_empty() {
        return Err(Error::Config("no recipients".into()));
    }
    if recipients.len() > MAX_RECIPIENTS {
        return Err(Error::Config(format!(
            "too many recipients (at most {MAX_RECIPIENTS})"
        )));
    }

    let domain = sender.rsplit_once('@').map_or("localhost", |(_, d)| d);
    let message_id = format!("{}@{domain}", random_id());

    let mut builder = MessageBuilder::new()
        .from(address(&out.from))
        .subject(one_line(&out.subject))
        .date(now)
        .message_id(message_id.clone())
        .header("User-Agent", Raw::new("Postflow"))
        .text_body(crlf(&out.text));
    if !out.to.is_empty() {
        builder = builder.to(list(&out.to));
    }
    if !out.cc.is_empty() {
        builder = builder.cc(list(&out.cc));
    }
    // 只有密送时，信头里也要有收件人一栏，写成匿名的空组
    if out.to.is_empty() && out.cc.is_empty() {
        builder = builder.header("To", Raw::new("undisclosed-recipients:;"));
    }
    if let Some(id) = out
        .in_reply_to
        .as_deref()
        .filter(|id| !id.trim().is_empty())
    {
        builder = builder.in_reply_to(id.trim().to_string());
    }
    if !out.references.is_empty() {
        builder = builder.references(out.references.clone());
    }
    let raw = builder
        .write_to_vec()
        .map_err(|e| Error::Config(e.to_string()))?;
    Ok(Built {
        message_id,
        raw,
        sender,
        recipients,
    })
}

fn address(m: &Mailbox) -> Address<'static> {
    let name = m.name.as_deref().map(one_line).filter(|n| !n.is_empty());
    Address::new_address(
        name,
        m.address.clone().unwrap_or_default().trim().to_string(),
    )
}

fn list(mailboxes: &[Mailbox]) -> Address<'static> {
    Address::new_list(mailboxes.iter().map(address).collect())
}

/// 信头里不能有换行。
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 邮件正文按协议用 CRLF 换行。
fn crlf(text: &str) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = text.replace('\n', "\r\n");
    if !out.ends_with("\r\n") {
        out.push_str("\r\n");
    }
    out
}

fn random_id() -> String {
    let bytes: [u8; 16] = rand::random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mb(name: Option<&str>, address: &str) -> Mailbox {
        Mailbox {
            name: name.map(str::to_string),
            address: Some(address.to_string()),
        }
    }

    #[test]
    fn addresses_are_parsed() {
        assert_eq!(
            parse_address(" a@example.com "),
            Some(mb(None, "a@example.com"))
        );
        assert_eq!(
            parse_address("Ada Lovelace <ada@example.com>"),
            Some(mb(Some("Ada Lovelace"), "ada@example.com"))
        );
        assert_eq!(
            parse_address("\"王 小明\" <wang@example.com>"),
            Some(mb(Some("王 小明"), "wang@example.com"))
        );
        assert_eq!(
            parse_address("<b@example.com>"),
            Some(mb(None, "b@example.com"))
        );
        for bad in [
            "",
            "nobody",
            "a@b",
            "a b@example.com",
            "@example.com",
            "a@example.com.",
            "a@@example.com",
            "a@exa mple.com",
            "x <a@example.com",
            "a;b@example.com",
        ] {
            assert_eq!(parse_address(bad), None, "{bad}");
        }
    }

    #[test]
    fn replies_continue_the_thread() {
        assert_eq!(thread_headers(None, &[]), (None, vec![]));
        let (id, refs) = thread_headers(Some("c@x"), &["a@x".into(), "b@x".into()]);
        assert_eq!(id.as_deref(), Some("c@x"));
        assert_eq!(refs, ["a@x", "b@x", "c@x"]);

        let long: Vec<String> = (0..40).map(|i| format!("{i}@x")).collect();
        let (_, refs) = thread_headers(Some("new@x"), &long);
        assert_eq!(refs.len(), MAX_REFERENCES);
        assert_eq!(refs.first().map(String::as_str), Some("0@x"));
        assert_eq!(refs.last().map(String::as_str), Some("new@x"));
    }

    #[test]
    fn builds_a_message_that_parses_back() {
        let out = Outgoing {
            from: mb(Some("示例 用户"), "me@example.com"),
            to: vec![mb(Some("Ada"), "ada@example.com")],
            cc: vec![mb(None, "cc@example.com"), mb(None, "ADA@example.com")],
            bcc: vec![mb(None, "hidden@example.com")],
            subject: "Re: 你好\r\nBcc: injected@example.com".into(),
            text: "第一行\n\n> quoted\n.leading dot".into(),
            in_reply_to: Some("orig@example.org".into()),
            references: vec!["first@example.org".into(), "orig@example.org".into()],
        };
        let built = build(&out, 1_700_000_000).unwrap();
        assert!(built.message_id.ends_with("@example.com"));
        assert_eq!(built.sender, "me@example.com");
        assert_eq!(
            built.recipients,
            ["ada@example.com", "cc@example.com", "hidden@example.com"]
        );

        let text = String::from_utf8_lossy(&built.raw);
        assert!(
            !text.contains("hidden@example.com"),
            "bcc stays off the headers"
        );
        assert!(
            !text.contains("\nBcc:"),
            "no header injection through the subject"
        );

        let parsed = mail_parser::MessageParser::default()
            .parse(&built.raw)
            .unwrap();
        assert_eq!(parsed.subject(), Some("Re: 你好 Bcc: injected@example.com"));
        assert_eq!(parsed.message_id(), Some(built.message_id.as_str()));
        assert_eq!(parsed.in_reply_to().as_text(), Some("orig@example.org"));
        assert_eq!(
            parsed.references().as_text_list(),
            Some(&["first@example.org".into(), "orig@example.org".into()][..])
        );
        assert_eq!(
            parsed.from().and_then(|a| a.first()).and_then(|a| a.name()),
            Some("示例 用户")
        );
        assert_eq!(parsed.to().map(|a| a.iter().count()), Some(1));
        assert_eq!(parsed.cc().map(|a| a.iter().count()), Some(2));
        assert_eq!(
            parsed
                .body_text(0)
                .as_deref()
                .map(|t| t.replace("\r\n", "\n")),
            Some("第一行\n\n> quoted\n.leading dot\n".to_string())
        );
        assert_eq!(parsed.date().map(|d| d.to_timestamp()), Some(1_700_000_000));
    }

    #[test]
    fn bcc_only_gets_an_anonymous_to() {
        let out = Outgoing {
            from: mb(None, "me@example.com"),
            bcc: vec![mb(None, "hidden@example.com")],
            subject: "hi".into(),
            text: "hello".into(),
            ..Outgoing::default()
        };
        let built = build(&out, 0).unwrap();
        let text = String::from_utf8_lossy(&built.raw);
        assert!(text.contains("To: undisclosed-recipients:;\r\n"));
        assert_eq!(built.recipients, ["hidden@example.com"]);
    }

    #[test]
    fn bad_input_is_refused() {
        let base = Outgoing {
            from: mb(None, "me@example.com"),
            to: vec![mb(None, "ada@example.com")],
            ..Outgoing::default()
        };
        assert!(build(&base, 0).is_ok());
        let no_one = Outgoing {
            to: vec![],
            ..base.clone()
        };
        assert!(matches!(build(&no_one, 0), Err(Error::Config(_))));
        let bad_from = Outgoing {
            from: mb(None, "me"),
            ..base.clone()
        };
        assert!(matches!(build(&bad_from, 0), Err(Error::Config(_))));
        let bad_to = Outgoing {
            to: vec![mb(None, "ada@example.com\r\nX: y")],
            ..base
        };
        assert!(matches!(build(&bad_to, 0), Err(Error::Config(_))));
    }
}
