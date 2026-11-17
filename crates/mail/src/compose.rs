//! 把写好的一封信变成可以发出去的邮件原文。
//!
//! 只写纯文本：回复时引用原信用「>」，和大多数邮件程序的纯文本模式一样。附件放在后面，
//! 整封信是 multipart/mixed。草稿也用这里生成，只是多保留了密送一栏。

use mail_builder::MessageBuilder;
use mail_builder::headers::address::Address;
use mail_builder::headers::raw::Raw;

use crate::message::Mailbox;
use crate::{Error, Result};

/// 一次最多发给多少人。再多的多半是填错了，服务器也会拒绝。
const MAX_RECIPIENTS: usize = 100;
/// References 里最多保留多少个 Message-ID。很长的讨论只留最早的一个和最近的几个。
const MAX_REFERENCES: usize = 20;
/// 附件加起来最多这么大。多数邮箱的上限是 25 MB 左右，发信服务器会按自己的上限再检查一次。
pub const MAX_ATTACHMENTS: usize = 50 * 1024 * 1024;

/// 信里带的一个附件。
#[derive(Clone, Default)]
pub struct Attachment {
    pub name: String,
    pub mime: String,
    pub data: Vec<u8>,
}

impl std::fmt::Debug for Attachment {
    // 内容可能很大，也可能是私人文件，不打出来
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Attachment")
            .field("name", &self.name)
            .field("mime", &self.mime)
            .field("size", &self.data.len())
            .finish()
    }
}

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
    pub attachments: Vec<Attachment>,
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

/// 生成要发出去的邮件原文。`now` 是 Unix 秒，写进 Date。
pub fn build(out: &Outgoing, now: i64) -> Result<Built> {
    build_with(out, now, false)
}

/// 生成存进「草稿」的邮件原文。和要发的信一样，只是密送也写进信头（下次接着写时要用），
/// 也可以还没有收件人。写得不像地址的收件人不存。
pub fn build_draft(out: &Outgoing, now: i64) -> Result<Built> {
    let keep = |list: &[Mailbox]| -> Vec<Mailbox> {
        list.iter()
            .filter(|m| {
                m.address
                    .as_deref()
                    .is_some_and(|a| valid_address(a.trim()))
            })
            .cloned()
            .collect()
    };
    let out = Outgoing {
        to: keep(&out.to),
        cc: keep(&out.cc),
        bcc: keep(&out.bcc),
        ..out.clone()
    };
    build_with(&out, now, true)
}

fn build_with(out: &Outgoing, now: i64, draft: bool) -> Result<Built> {
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
    if recipients.is_empty() && !draft {
        return Err(Error::Config("no recipients".into()));
    }
    if recipients.len() > MAX_RECIPIENTS {
        return Err(Error::Config(format!(
            "too many recipients (at most {MAX_RECIPIENTS})"
        )));
    }

    let total: usize = out.attachments.iter().map(|a| a.data.len()).sum();
    if total > MAX_ATTACHMENTS {
        return Err(Error::TooLarge(MAX_ATTACHMENTS));
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
    if draft {
        if !out.bcc.is_empty() {
            builder = builder.bcc(list(&out.bcc));
        }
    } else if out.to.is_empty() && out.cc.is_empty() {
        // 只有密送时，信头里也要有收件人一栏，写成匿名的空组
        builder = builder.header("To", Raw::new("undisclosed-recipients:;"));
    }
    if let Some(id) = out.in_reply_to.as_deref().and_then(clean_id) {
        builder = builder.in_reply_to(id);
    }
    let references: Vec<String> = out.references.iter().filter_map(|r| clean_id(r)).collect();
    if !references.is_empty() {
        builder = builder.references(references);
    }
    for a in &out.attachments {
        builder = builder.attachment(mime_type(&a.mime), file_name(&a.name), a.data.as_slice());
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

/// 信头里的 Message-ID：去掉尖括号，有空白、换行或尖括号的不要，免得写坏信头。
fn clean_id(id: &str) -> Option<String> {
    let id = id.trim().trim_start_matches('<').trim_end_matches('>');
    let ok = !id.is_empty()
        && id.len() <= 250
        && !id
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '<' || c == '>');
    ok.then(|| id.to_string())
}

/// 附件的类型。看不懂的一律当成二进制文件。
fn mime_type(mime: &str) -> String {
    let mime = mime.trim().to_ascii_lowercase();
    let token = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "!#$&^_.+-".contains(c))
    };
    match mime.split_once('/') {
        Some((a, b)) if token(a) && token(b) => mime,
        _ => "application/octet-stream".to_string(),
    }
}

/// 附件的文件名：只留最后一段，去掉控制字符和引号。
fn file_name(name: &str) -> String {
    let name = name.rsplit(['/', '\\']).next().unwrap_or("");
    let name: String = name
        .chars()
        .filter(|c| !c.is_control() && *c != '"')
        .collect();
    let name = name.trim();
    if name.is_empty() {
        "attachment".to_string()
    } else {
        name.chars().take(200).collect()
    }
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
            attachments: vec![],
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

    #[test]
    fn attachments_go_along_and_parse_back() {
        use mail_parser::MimeHeaders;
        let out = Outgoing {
            from: mb(None, "me@example.com"),
            to: vec![mb(None, "ada@example.com")],
            subject: "Files".into(),
            text: "See attached.".into(),
            attachments: vec![
                Attachment {
                    name: "报告 2026.pdf".into(),
                    mime: "application/pdf".into(),
                    data: vec![0, 1, 2, 255, 13, 10, 46],
                },
                Attachment {
                    name: "../../etc/notes.txt".into(),
                    mime: "text/plain\r\nX-Evil: 1".into(),
                    data: b"plain notes".to_vec(),
                },
            ],
            ..Outgoing::default()
        };
        let built = build(&out, 0).unwrap();
        let text = String::from_utf8_lossy(&built.raw);
        assert!(!text.contains("X-Evil"), "{text}");
        let parsed = mail_parser::MessageParser::default()
            .parse(&built.raw)
            .unwrap();
        assert_eq!(
            parsed.body_text(0).as_deref().map(str::trim_end),
            Some("See attached.")
        );
        let files: Vec<(Option<&str>, Vec<u8>)> = parsed
            .attachments()
            .map(|a| (a.attachment_name(), a.contents().to_vec()))
            .collect();
        assert_eq!(
            files,
            [
                (Some("报告 2026.pdf"), vec![0, 1, 2, 255, 13, 10, 46]),
                (Some("notes.txt"), b"plain notes".to_vec()),
            ]
        );
        assert_eq!(
            parsed
                .attachment(1)
                .and_then(|a| a.content_type())
                .map(|c| c.ctype()),
            Some("application")
        );
    }

    #[test]
    fn too_much_attached_is_refused() {
        let out = Outgoing {
            from: mb(None, "me@example.com"),
            to: vec![mb(None, "ada@example.com")],
            attachments: vec![Attachment {
                name: "big.bin".into(),
                mime: "application/octet-stream".into(),
                data: vec![0; MAX_ATTACHMENTS + 1],
            }],
            ..Outgoing::default()
        };
        assert!(matches!(build(&out, 0), Err(Error::TooLarge(_))));
    }

    #[test]
    fn drafts_keep_bcc_and_may_have_no_recipients() {
        let out = Outgoing {
            from: mb(None, "me@example.com"),
            bcc: vec![mb(None, "hidden@example.com"), mb(None, "half typed")],
            subject: "Unfinished".into(),
            text: "Dear".into(),
            in_reply_to: Some("<orig@example.org>".into()),
            references: vec!["bad id with spaces".into(), "orig@example.org".into()],
            ..Outgoing::default()
        };
        // 发信时写错的地址要挡住，存草稿时只是不存它
        assert!(build(&out, 0).is_err());
        let draft = build_draft(&out, 0).unwrap();
        let back = crate::draft_contents(&draft.raw).unwrap();
        assert_eq!(back.message_id.as_deref(), Some(draft.message_id.as_str()));
        assert_eq!(back.bcc, [mb(None, "hidden@example.com")]);
        assert!(back.to.is_empty());
        assert_eq!(back.subject, "Unfinished");
        assert_eq!(back.text, "Dear");
        assert_eq!(back.in_reply_to.as_deref(), Some("orig@example.org"));
        assert_eq!(back.references, ["orig@example.org"]);

        let empty = Outgoing {
            from: mb(None, "me@example.com"),
            text: "just started".into(),
            ..Outgoing::default()
        };
        assert!(build(&empty, 0).is_err());
        assert!(build_draft(&empty, 0).is_ok());
    }
}
