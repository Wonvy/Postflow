//! 解析邮件原文：列表需要的摘要信息，以及阅读时安全的正文。
//!
//! 邮件 HTML 完全不可信。正文经过两道处理：先用白名单净化，再交给界面放进
//! 不能执行脚本的沙箱 iframe，并用内容安全策略挡住远程资源。远程图片默认不加载，
//! 因为它们常被用来追踪你什么时候打开了邮件。

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use ammonia::{Builder, UrlRelative};
use mail_parser::{Address, Message, MessageParser, MimeHeaders, PartType};
use serde::Serialize;

/// 列表摘要的最大长度（字符数）。
const SNIPPET_CHARS: usize = 200;
/// 内嵌图片转成 data URI 的大小上限，再大的就不内嵌了。
const MAX_INLINE_IMAGE: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Mailbox {
    pub name: Option<String>,
    pub address: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Summary {
    /// 不带尖括号的 Message-ID。
    pub message_id: Option<String>,
    pub subject: String,
    pub from: Mailbox,
    /// 收件人和抄送。草稿和已发送在列表里显示写给谁，发过信的人也算联系人。
    pub to: Vec<Mailbox>,
    pub cc: Vec<Mailbox>,
    /// 信头里的发信时间，Unix 毫秒。
    pub date: Option<i64>,
    pub snippet: String,
    pub has_attachments: bool,
    /// 邮件列表的退订地址，用来识别订阅类邮件。
    pub list_unsubscribe: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyKind {
    /// 完整的 HTML 文档，要放进沙箱 iframe。
    Html,
    /// 由纯文本生成的安全 HTML 片段，可以直接放进页面。
    Text,
}

#[derive(Debug, Clone, Serialize)]
pub struct AttachmentInfo {
    /// 在邮件附件列表里的位置，下载时用。
    pub index: u32,
    pub name: String,
    pub mime: String,
    pub size: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Rendered {
    pub kind: BodyKind,
    pub body: String,
    /// 正文引用了远程图片或样式。界面据此提示「已阻止远程图片」。
    pub has_remote_content: bool,
    pub from: Mailbox,
    pub to: Vec<Mailbox>,
    pub cc: Vec<Mailbox>,
    pub reply_to: Vec<Mailbox>,
    pub attachments: Vec<AttachmentInfo>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RenderOptions {
    pub allow_remote: bool,
}

fn parse(raw: &[u8]) -> Option<Message<'_>> {
    MessageParser::default().parse(raw)
}

/// 信头本该只有 ASCII，别的字要按 RFC 2047 编码。有些老系统直接把 GBK 之类的字节写进信头，
/// 解析库只当 UTF-8 读，读出来是乱码。先把这样的信头行按邮件声明的字符集转成 UTF-8。
fn utf8_headers(raw: &[u8]) -> Cow<'_, [u8]> {
    let end = header_end(raw);
    let head = &raw[..end];
    if std::str::from_utf8(head).is_ok() {
        return Cow::Borrowed(raw);
    }
    let declared = declared_charset(raw);
    let mut out = Vec::with_capacity(raw.len() + end / 2);
    for line in head.split_inclusive(|&b| b == b'\n') {
        match std::str::from_utf8(line) {
            Ok(_) => out.extend_from_slice(line),
            Err(_) => out.extend_from_slice(decode_line(line, declared).as_bytes()),
        }
    }
    out.extend_from_slice(&raw[end..]);
    Cow::Owned(out)
}

/// 第一个空行从哪里开始，之前是信头。
fn header_end(raw: &[u8]) -> usize {
    let mut at = 0;
    for line in raw.split_inclusive(|&b| b == b'\n') {
        if line == b"\r\n" || line == b"\n" {
            return at;
        }
        at += line.len();
    }
    raw.len()
}

/// 邮件里第一处 `charset=` 声明的字符集。信头里的原始字节一般和正文用同一种。
fn declared_charset(raw: &[u8]) -> Option<&'static encoding_rs::Encoding> {
    const KEY: &[u8] = b"charset=";
    let scan = &raw[..raw.len().min(64 * 1024)];
    let at = scan
        .windows(KEY.len())
        .position(|w| w.eq_ignore_ascii_case(KEY))?;
    let value: Vec<u8> = scan[at + KEY.len()..]
        .iter()
        .copied()
        .skip_while(|&b| b == b'"' || b == b'\'')
        .take_while(|&b| {
            !matches!(
                b,
                b'"' | b'\'' | b';' | b' ' | b'\t' | b'\r' | b'\n' | b'>' | b'/'
            )
        })
        .collect();
    encoding_rs::Encoding::for_label(&value)
}

/// 先试声明的字符集，再试 GB18030（国内邮件最常见，也兼容 GBK 和 GB2312）。都不对就原样当 UTF-8。
fn decode_line(line: &[u8], declared: Option<&'static encoding_rs::Encoding>) -> String {
    for encoding in declared.into_iter().chain([encoding_rs::GB18030]) {
        if encoding == encoding_rs::UTF_8 {
            continue;
        }
        let (text, had_errors) = encoding.decode_without_bom_handling(line);
        if !had_errors {
            return text.into_owned();
        }
    }
    String::from_utf8_lossy(line).into_owned()
}

fn mailboxes(addr: Option<&Address<'_>>) -> Vec<Mailbox> {
    let Some(addr) = addr else { return Vec::new() };
    addr.iter()
        .map(|a| Mailbox {
            name: a.name().map(clean_text).filter(|n| !n.is_empty()),
            address: a
                .address()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        })
        .filter(|m| m.name.is_some() || m.address.is_some())
        .collect()
}

/// 去掉零宽字符之类看不见的东西，再把空白折叠成单个空格。
/// 营销邮件常在开头塞一长串零宽字符来控制收件箱里的预览文字。
fn clean_text(s: &str) -> String {
    s.chars()
        .filter(|c| {
            !matches!(c, '\u{200b}'..='\u{200f}' | '\u{2060}'..='\u{2064}' | '\u{feff}' | '\u{034f}' | '\u{00ad}')
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.truncate(out.trim_end().len());
    out.push('…');
    out
}

/// 解析列表需要的信息。只有信头时也能用，这时摘要为空。
pub fn summarize(raw: &[u8]) -> Summary {
    let raw = utf8_headers(raw);
    let Some(msg) = parse(&raw) else {
        return Summary::default();
    };
    // 只有 HTML 正文时自己取文字：库自带的预览会把相邻段落的字粘在一起
    let has_plain = msg
        .text_bodies()
        .any(|part| matches!(part.body, PartType::Text(_)));
    let preview = if !has_plain {
        msg.html_bodies().find_map(|part| match &part.body {
            PartType::Html(html) => Some(html_text(html)),
            _ => None,
        })
    } else {
        msg.body_preview(SNIPPET_CHARS * 2).map(|p| p.into_owned())
    };
    let snippet = preview
        .map(|p| truncate(&clean_text(&p), SNIPPET_CHARS))
        .unwrap_or_default();
    let list_unsubscribe = match msg.list_unsubscribe() {
        mail_parser::HeaderValue::Address(addr) => addr
            .iter()
            .filter_map(|a| a.address())
            .find(|u| {
                u.starts_with("https://") || u.starts_with("http://") || u.starts_with("mailto:")
            })
            .map(str::to_string),
        mail_parser::HeaderValue::Text(t) => Some(t.trim().trim_matches(['<', '>']).to_string()),
        _ => None,
    }
    .filter(|u| !u.is_empty());

    Summary {
        message_id: msg
            .message_id()
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty()),
        subject: msg.subject().map(clean_text).unwrap_or_default(),
        from: mailboxes(msg.from()).into_iter().next().unwrap_or_default(),
        to: mailboxes(msg.to()),
        cc: mailboxes(msg.cc()),
        date: msg.date().map(|d| d.to_timestamp() * 1000),
        snippet,
        has_attachments: msg.attachments().any(|p| !is_inline_calendar(p)),
        list_unsubscribe,
    }
}

/// 邮件列表的退订方式：信头里的 List-Unsubscribe（RFC 2369），以及能不能一键退订（RFC 8058）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Unsubscribe {
    /// 退订网页。
    pub url: Option<String>,
    /// 发一封邮件来退订。
    pub mailto: Option<String>,
    /// 往 `url` 发一个 POST 就能退订，不用打开网页。
    pub one_click: bool,
}

/// 一键退订只认 https；演示和测试时也接受本机的 http。
fn one_click_url(url: &str) -> bool {
    url.starts_with("https://")
        || ["http://127.0.0.1", "http://localhost", "http://[::1]"]
            .iter()
            .any(|p| url.starts_with(p) && url[p.len()..].starts_with([':', '/']))
}

/// 读出这封邮件的退订方式。不是邮件列表（没有 List-Unsubscribe）时返回 `None`。
pub fn unsubscribe(raw: &[u8]) -> Option<Unsubscribe> {
    let raw = utf8_headers(raw);
    let msg = parse(&raw)?;
    let value = msg.header_raw("List-Unsubscribe")?;
    let mut out = Unsubscribe::default();
    // 每个地址写在尖括号里，用逗号隔开；长的会折行，去掉中间的空白
    let mut rest = value;
    while let Some(open) = rest.find('<') {
        let Some(close) = rest[open..].find('>') else {
            break;
        };
        let uri: String = rest[open + 1..open + close].split_whitespace().collect();
        rest = &rest[open + close + 1..];
        let lower = uri.to_ascii_lowercase();
        if (lower.starts_with("https://") || lower.starts_with("http://")) && out.url.is_none() {
            out.url = Some(uri);
        } else if lower.starts_with("mailto:") && out.mailto.is_none() {
            out.mailto = Some(uri);
        }
    }
    let post = msg
        .header_raw("List-Unsubscribe-Post")
        .unwrap_or_default()
        .to_ascii_lowercase();
    out.one_click = post.contains("list-unsubscribe=one-click")
        && out.url.as_deref().is_some_and(one_click_url);
    (out.url.is_some() || out.mailto.is_some()).then_some(out)
}

/// 一键退订：按 RFC 8058 往退订地址发一个 POST。服务器回 2xx 或跳转就算成功。
pub async fn unsubscribe_one_click(url: &str) -> Result<(), String> {
    if !one_click_url(url) {
        return Err("one-click unsubscribe needs an https address".into());
    }
    let resp = crate::oauth::http()
        .post(url)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .timeout(std::time::Duration::from_secs(30))
        .body("List-Unsubscribe=One-Click")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    if status.is_success() || status.is_redirection() {
        Ok(())
    } else {
        Err(format!(
            "the list's server answered HTTP {}",
            status.as_u16()
        ))
    }
}

/// 回复和转发时引用原信最多取这么多字。
const QUOTE_CHARS: usize = 100_000;

/// 回复或转发一封信要用到的原信内容。
#[derive(Debug, Clone, Default, Serialize)]
pub struct ReplyContext {
    /// 不带尖括号的 Message-ID。
    pub message_id: Option<String>,
    pub references: Vec<String>,
    pub subject: String,
    /// 信头里的发信时间，Unix 毫秒。
    pub date: Option<i64>,
    pub from: Vec<Mailbox>,
    pub reply_to: Vec<Mailbox>,
    pub to: Vec<Mailbox>,
    pub cc: Vec<Mailbox>,
    /// 正文的纯文字，保留分行，引用时用。
    pub text: String,
    /// 转发时一起带上的附件。正文里引用的内嵌图片不算。
    pub attachments: Vec<AttachmentInfo>,
}

/// In-Reply-To、References 这类信头里的一串 Message-ID，去掉尖括号。
fn message_ids(v: &mail_parser::HeaderValue<'_>) -> Vec<String> {
    match v {
        mail_parser::HeaderValue::Text(t) => vec![t.to_string()],
        mail_parser::HeaderValue::TextList(list) => list.iter().map(|t| t.to_string()).collect(),
        _ => Vec::new(),
    }
    .into_iter()
    .map(|id| id.trim().trim_matches(['<', '>']).to_string())
    .filter(|id| !id.is_empty())
    .collect()
}

/// 会议邀请里和正文并列的那段日历（没有文件名的 text/calendar）：阅读区显示成邀请卡片，不算附件。
fn is_inline_calendar(part: &mail_parser::MessagePart<'_>) -> bool {
    part.attachment_name().is_none()
        && part.content_type().is_some_and(|ct| {
            ct.ctype().eq_ignore_ascii_case("text")
                && ct
                    .subtype()
                    .is_some_and(|s| s.eq_ignore_ascii_case("calendar"))
        })
}

/// 真正的附件：正文里用 cid: 引用的内嵌图片和会议邀请的日历部分不算。
fn real_attachments(msg: &Message<'_>) -> Vec<AttachmentInfo> {
    let html: String = msg
        .html_bodies()
        .filter_map(|p| match &p.body {
            PartType::Html(h) => Some(h.to_ascii_lowercase()),
            _ => None,
        })
        .collect();
    msg.attachments()
        .enumerate()
        .filter(|(_, part)| {
            part.content_id().is_none_or(|cid| {
                let cid = cid.trim_matches(['<', '>']).to_ascii_lowercase();
                !html.contains(&format!("cid:{cid}"))
            }) && !is_inline_calendar(part)
        })
        .map(|(index, part)| AttachmentInfo {
            index: index as u32,
            name: attachment_name(part, index as u32),
            mime: content_type(part),
            size: match &part.body {
                PartType::Message(inner) => inner.raw_message().len(),
                _ => part.contents().len(),
            },
        })
        .collect()
}

/// 写信时允许的格式：字体样式、链接、列表、引用和标题。样式、脚本、图片和表格都去掉。
const COMPOSE_TAGS: &[&str] = &[
    "a",
    "b",
    "strong",
    "i",
    "em",
    "u",
    "s",
    "strike",
    "del",
    "p",
    "div",
    "br",
    "span",
    "ul",
    "ol",
    "li",
    "blockquote",
    "pre",
    "code",
    "h1",
    "h2",
    "h3",
    "h4",
    "hr",
];

/// 写信窗口交来的 HTML 只留 `COMPOSE_TAGS`。从网页粘贴来的样式、脚本、追踪图片都去掉。
pub fn clean_compose_html(html: &str) -> String {
    let mut builder = Builder::empty();
    builder
        .add_tags(COMPOSE_TAGS)
        .add_tag_attributes("a", ["href"])
        .url_schemes(HashSet::from(["http", "https", "mailto"]))
        .link_rel(Some("noopener noreferrer"))
        .url_relative(UrlRelative::Deny);
    builder.clean(html).to_string()
}

/// 一封草稿里写了什么，接着写时用。
#[derive(Debug, Clone, Default, Serialize)]
pub struct DraftContents {
    pub message_id: Option<String>,
    pub to: Vec<Mailbox>,
    pub cc: Vec<Mailbox>,
    pub bcc: Vec<Mailbox>,
    pub subject: String,
    /// 正文原样，只把换行统一成 \n。
    pub text: String,
    /// 带格式的草稿的 HTML，已经按写信允许的格式清理过。纯文本草稿为空。
    pub html: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub attachments: Vec<AttachmentInfo>,
}

/// 读出一封草稿的内容。
pub fn draft_contents(raw: &[u8]) -> Option<DraftContents> {
    let raw = utf8_headers(raw);
    let msg = parse(&raw)?;
    let mut text = msg
        .body_text(0)
        .map(|t| t.replace("\r\n", "\n"))
        .unwrap_or_default();
    // 生成时在末尾补过一个换行
    if text.ends_with('\n') {
        text.pop();
    }
    let html = msg.html_bodies().find_map(|part| match &part.body {
        PartType::Html(h) => Some(clean_compose_html(h)),
        _ => None,
    });
    Some(DraftContents {
        html,
        message_id: msg
            .message_id()
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty()),
        to: mailboxes(msg.to()),
        cc: mailboxes(msg.cc()),
        bcc: mailboxes(msg.bcc()),
        subject: msg
            .subject()
            .map(|s| s.trim().to_string())
            .unwrap_or_default(),
        text,
        in_reply_to: message_ids(msg.in_reply_to()).into_iter().next(),
        references: message_ids(msg.references()),
        attachments: real_attachments(&msg),
    })
}

/// 读出回复、转发一封信时要用的东西。
pub fn reply_context(raw: &[u8]) -> Option<ReplyContext> {
    let raw = utf8_headers(raw);
    let msg = parse(&raw)?;
    let ids = message_ids;
    let has_plain = msg
        .text_bodies()
        .any(|part| matches!(part.body, PartType::Text(_)));
    let mut text = String::new();
    for part in msg.text_bodies() {
        let piece = match &part.body {
            PartType::Text(t) if has_plain => t.to_string(),
            PartType::Html(h) if !has_plain => html_lines(h),
            _ => continue,
        };
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&piece);
    }
    if !has_plain && text.is_empty() {
        for part in msg.html_bodies() {
            if let PartType::Html(h) = &part.body {
                text.push_str(&html_lines(h));
            }
        }
    }
    Some(ReplyContext {
        message_id: msg
            .message_id()
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty()),
        references: ids(msg.references()),
        subject: msg.subject().map(clean_text).unwrap_or_default(),
        date: msg.date().map(|d| d.to_timestamp() * 1000),
        from: mailboxes(msg.from()),
        reply_to: mailboxes(msg.reply_to()),
        to: mailboxes(msg.to()),
        cc: mailboxes(msg.cc()),
        text: tidy_lines(&text, QUOTE_CHARS),
        attachments: real_attachments(&msg),
    })
}

/// 去掉每行末尾的空白和看不见的字符，连续的空行只留一行，太长的截断。
fn tidy_lines(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut chars = 0;
    let mut blank = 0;
    for line in text.replace("\r\n", "\n").replace('\r', "\n").lines() {
        let line: String = line
            .chars()
            .filter(|c| !matches!(c, '\u{200b}'..='\u{200f}' | '\u{2060}'..='\u{2064}' | '\u{feff}' | '\u{034f}' | '\u{00ad}'))
            .collect();
        let line = line.trim_end();
        if line.trim().is_empty() {
            blank += 1;
            continue;
        }
        if !out.is_empty() {
            out.push_str(if blank > 0 { "\n\n" } else { "\n" });
        }
        blank = 0;
        out.push_str(line);
        chars += line.chars().count() + 2;
        if chars > max_chars {
            return truncate(&out, max_chars);
        }
    }
    out
}

/// 全文搜索最多收录正文的这么多字。
const SEARCH_CHARS: usize = 50_000;

/// 正文的纯文字，给全文搜索用。有纯文本部分就用它们，没有时从 HTML 部分里取字。
pub fn plain_text(raw: &[u8]) -> String {
    search_text(raw).0
}

/// 给全文搜索用的东西：正文（同 `plain_text`）、收件人和抄送的名字和地址（`to:` 搜索用），
/// 以及真正的附件（附件总览用）。
pub fn search_text(raw: &[u8]) -> (String, String, Vec<AttachmentInfo>) {
    let raw = utf8_headers(raw);
    let Some(msg) = parse(&raw) else {
        return (String::new(), String::new(), Vec::new());
    };
    let mut recipients = Vec::new();
    for list in [msg.to(), msg.cc()].into_iter().flatten() {
        for addr in list.iter() {
            recipients.extend(addr.name().map(str::to_string));
            recipients.extend(addr.address().map(str::to_string));
        }
    }
    (
        body_text(&msg),
        recipients.join(" "),
        real_attachments(&msg),
    )
}

fn body_text(msg: &Message<'_>) -> String {
    let mut out = String::new();
    let has_plain = msg
        .text_bodies()
        .any(|part| matches!(part.body, PartType::Text(_)));
    if has_plain {
        for part in msg.text_bodies() {
            if let PartType::Text(text) = &part.body {
                out.push_str(text);
                out.push('\n');
            }
        }
    } else {
        for part in msg.html_bodies() {
            if let PartType::Html(html) = &part.body {
                out.push_str(&html_text(html));
                out.push('\n');
            }
        }
    }
    truncate(&clean_text(&out), SEARCH_CHARS)
}

fn content_type(part: &mail_parser::MessagePart<'_>) -> String {
    match part.content_type() {
        Some(ct) => match &ct.c_subtype {
            Some(sub) => format!("{}/{}", ct.c_type, sub).to_ascii_lowercase(),
            None => ct.c_type.to_ascii_lowercase(),
        },
        None => "application/octet-stream".to_string(),
    }
}

fn attachment_name(part: &mail_parser::MessagePart<'_>, index: u32) -> String {
    if let Some(name) = part
        .attachment_name()
        .map(clean_text)
        .filter(|n| !n.is_empty())
    {
        return name;
    }
    if matches!(part.body, PartType::Message(_)) {
        return format!("message-{}.eml", index + 1);
    }
    format!("attachment-{}", index + 1)
}

/// 解码标准 Base64（也认 base64url），忽略空白和末尾的「=」。有不认识的字符就返回 `None`。
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut acc: u32 = 0;
    let mut bits = 0;
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b' ' | b'\r' | b'\n' | b'\t' => continue,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// 标准 Base64 编码，内嵌图片和附件下载都用它。
pub fn base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// 把邮件渲染成可以安全展示的正文。
pub fn render(raw: &[u8], opts: RenderOptions) -> Option<Rendered> {
    let raw = utf8_headers(raw);
    let msg = parse(&raw)?;

    // 正文里用 cid: 引用的内嵌图片，换成 data URI
    let mut inline: HashMap<String, (u32, String)> = HashMap::new();
    for (index, part) in msg.attachments().enumerate() {
        let Some(cid) = part.content_id() else {
            continue;
        };
        let mime = content_type(part);
        let data = part.contents();
        if mime.starts_with("image/") && data.len() <= MAX_INLINE_IMAGE {
            let uri = format!("data:{mime};base64,{}", base64(data));
            inline.insert(
                cid.trim().trim_matches(['<', '>']).to_string(),
                (index as u32, uri),
            );
        }
    }

    let html_part = msg.html_bodies().find_map(|part| match &part.body {
        PartType::Html(html) => Some(html.as_ref()),
        _ => None,
    });

    let used_inline = Arc::new(Mutex::new(HashSet::new()));
    let (kind, body, has_remote_content) = match html_part {
        Some(html) => {
            let (clean, remote) = sanitize_email_html(html, opts, inline, used_inline.clone());
            (
                BodyKind::Html,
                html_document(&clean, opts.allow_remote),
                remote,
            )
        }
        None => {
            let text = msg.body_text(0).unwrap_or_default();
            (BodyKind::Text, text_to_html(&text), false)
        }
    };

    let used = used_inline.lock().map(|u| u.clone()).unwrap_or_default();
    let attachments = msg
        .attachments()
        .enumerate()
        .filter(|(index, part)| !used.contains(&(*index as u32)) && !is_inline_calendar(part))
        .map(|(index, part)| AttachmentInfo {
            index: index as u32,
            name: attachment_name(part, index as u32),
            mime: content_type(part),
            size: part.contents().len(),
        })
        .collect();

    Some(Rendered {
        kind,
        body,
        has_remote_content,
        from: mailboxes(msg.from()).into_iter().next().unwrap_or_default(),
        to: mailboxes(msg.to()),
        cc: mailboxes(msg.cc()),
        reply_to: mailboxes(msg.reply_to()),
        attachments,
    })
}

/// 取出一个附件：文件名、类型、内容。
pub fn attachment(raw: &[u8], index: u32) -> Option<(String, String, Vec<u8>)> {
    let raw = utf8_headers(raw);
    let msg = parse(&raw)?;
    let part = msg.attachment(index)?;
    let bytes = match &part.body {
        // 转发的整封邮件，按原文保存成 .eml
        PartType::Message(inner) => inner.raw_message().to_vec(),
        _ => part.contents().to_vec(),
    };
    Some((attachment_name(part, index), content_type(part), bytes))
}

fn sanitize_email_html(
    html: &str,
    opts: RenderOptions,
    inline: HashMap<String, (u32, String)>,
    used_inline: Arc<Mutex<HashSet<u32>>>,
) -> (String, bool) {
    let remote = Arc::new(AtomicBool::new(false));
    let lower = html.to_ascii_lowercase();
    // <style> 块里的远程背景图、字体也算远程内容
    if lower.contains("url(http")
        || lower.contains("url('http")
        || lower.contains("url(\"http")
        || lower.contains("@import")
    {
        remote.store(true, Ordering::Relaxed);
    }

    let mut builder = Builder::default();
    builder
        .add_tags(["style", "font", "center", "big"])
        .rm_clean_content_tags(["style"])
        .add_generic_attributes([
            "style", "class", "align", "valign", "bgcolor", "width", "height", "dir", "color",
            "border",
        ])
        .add_tag_attributes("table", ["cellpadding", "cellspacing"])
        .add_tag_attributes("font", ["face", "size"])
        .add_tag_attributes("a", ["name"])
        .url_schemes(HashSet::from(["http", "https", "mailto", "data", "cid"]))
        .url_relative(UrlRelative::Deny)
        .link_rel(Some("noopener noreferrer"));

    let remote_flag = remote.clone();
    let allow_remote = opts.allow_remote;
    builder.attribute_filter(move |element, attribute, value| {
        let v = value.trim();
        let lower = v.to_ascii_lowercase();
        match (element, attribute) {
            ("img", "src") => {
                if let Some(cid) = lower.strip_prefix("cid:") {
                    let key = v[4..].trim().trim_matches(['<', '>']).to_string();
                    let found = inline.get(&key).or_else(|| {
                        inline
                            .iter()
                            .find(|(k, _)| k.eq_ignore_ascii_case(cid))
                            .map(|(_, v)| v)
                    });
                    return found.map(|(index, uri)| {
                        if let Ok(mut used) = used_inline.lock() {
                            used.insert(*index);
                        }
                        Cow::Owned(uri.clone())
                    });
                }
                if lower.starts_with("data:image/") {
                    return Some(Cow::Borrowed(value));
                }
                if lower.starts_with("http://")
                    || lower.starts_with("https://")
                    || lower.starts_with("//")
                {
                    remote_flag.store(true, Ordering::Relaxed);
                    // 不允许远程图片时干脆去掉地址：一个请求都不发
                    return allow_remote.then_some(Cow::Borrowed(value));
                }
                None
            }
            (_, "href") => {
                let ok = lower.starts_with("http://")
                    || lower.starts_with("https://")
                    || lower.starts_with("mailto:");
                ok.then_some(Cow::Borrowed(value))
            }
            (_, "style") => {
                if lower.contains("url(") {
                    remote_flag.store(true, Ordering::Relaxed);
                }
                Some(Cow::Borrowed(value))
            }
            _ => Some(Cow::Borrowed(value)),
        }
    });
    let clean = builder.clean(html).to_string();
    (clean, remote.load(Ordering::Relaxed))
}

/// 包成一个完整的文档，交给界面放进沙箱 iframe。
fn html_document(body: &str, allow_remote: bool) -> String {
    let img_src = if allow_remote {
        "data: https: http:"
    } else {
        "data:"
    };
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; img-src {img_src}; font-src data:\">\
<meta name=\"referrer\" content=\"no-referrer\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<style>\
html,body{{margin:0;padding:0;background:#ffffff;color:#1b1c1f}}\
body{{padding:20px 24px;font:14px/1.55 -apple-system,BlinkMacSystemFont,\"PingFang SC\",\"Segoe UI\",\"Microsoft YaHei\",sans-serif;overflow-wrap:anywhere}}\
img{{max-width:100%;height:auto}}\
table{{max-width:100%}}\
pre{{white-space:pre-wrap}}\
a{{color:#4a55bd}}\
</style></head><body>{body}</body></html>"
    )
}

/// 从 HTML 里取出给人看的文字，块级元素之间留一个空格。只用来生成列表摘要。
fn html_text(html: &str) -> String {
    html_to_text(html, false)
}

/// 从 HTML 里取出给人看的文字，块级元素分成不同的行。引用原信时用。
fn html_lines(html: &str) -> String {
    html_to_text(html, true)
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n")
}

fn html_to_text(html: &str, lines: bool) -> String {
    const BLOCKS: &[&str] = &[
        "p",
        "br",
        "div",
        "li",
        "tr",
        "td",
        "th",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "blockquote",
        "pre",
        "section",
        "article",
        "table",
        "hr",
        "header",
        "footer",
    ];
    let mut out = String::with_capacity(html.len() / 2);
    let mut rest = html;
    let push_text = |out: &mut String, text: &str| {
        if lines {
            // 源码里的换行只是空白，分行看标签
            out.extend(
                text.chars()
                    .map(|c| if c.is_whitespace() { ' ' } else { c }),
            );
        } else {
            out.push_str(text);
        }
    };
    while let Some(start) = rest.find('<') {
        push_text(&mut out, &rest[..start]);
        let Some(end) = rest[start..].find('>') else {
            rest = "";
            break;
        };
        let tag = &rest[start + 1..start + end];
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        rest = &rest[start + end + 1..];
        // 样式、脚本、头部的内容不是正文，整段跳过
        if !tag.starts_with('/') && matches!(name.as_str(), "style" | "script" | "head" | "title") {
            let close = format!("</{name}");
            match rest.to_ascii_lowercase().find(&close) {
                Some(at) => {
                    rest = &rest[at..];
                    rest = rest.find('>').map_or("", |gt| &rest[gt + 1..]);
                }
                None => rest = "",
            }
            continue;
        }
        if BLOCKS.contains(&name.as_str()) {
            out.push(if lines { '\n' } else { ' ' });
        }
    }
    push_text(&mut out, rest);
    out.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// 纯文本正文：转义后把网址变成链接，保留换行。
fn text_to_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 64);
    out.push_str("<div class=\"mail-text\">");
    for (i, line) in text.lines().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let quoted = line.trim_start().starts_with('>');
        if quoted {
            out.push_str("<span class=\"mail-quote\">");
        }
        linkify(line, &mut out);
        if quoted {
            out.push_str("</span>");
        }
    }
    out.push_str("</div>");
    out
}

fn escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
}

fn linkify(line: &str, out: &mut String) {
    let mut rest = line;
    while let Some(start) = find_url(rest) {
        escape(&rest[..start], out);
        let tail = &rest[start..];
        let mut end = tail
            .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '\''))
            .unwrap_or(tail.len());
        // 句末的标点通常不属于网址
        while end > 0 && tail[..end].ends_with(['.', ',', ';', ':', ')', ']', '!', '?', '。', '，'])
        {
            end -= tail[..end].chars().next_back().map_or(1, char::len_utf8);
        }
        let url = &tail[..end];
        out.push_str("<a href=\"");
        escape(url, out);
        out.push_str("\" rel=\"noopener noreferrer\">");
        escape(url, out);
        out.push_str("</a>");
        rest = &tail[end..];
    }
    escape(rest, out);
}

fn find_url(s: &str) -> Option<usize> {
    let a = s.find("https://");
    let b = s.find("http://");
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTML_MAIL: &str = "From: \"Ada Lovelace\" <ada@example.com>\r\n\
To: Bob <bob@example.com>, carol@example.com\r\n\
Cc: dave@example.com\r\n\
Subject: =?UTF-8?B?5pyI5bqm5oql5ZGK?=\r\n\
Date: Mon, 16 Nov 2026 08:00:00 +0000\r\n\
Message-ID: <abc123@example.com>\r\n\
List-Unsubscribe: <https://example.com/unsub?u=1>, <mailto:unsub@example.com>\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/related; boundary=\"B1\"\r\n\
\r\n\
--B1\r\n\
Content-Type: multipart/alternative; boundary=\"B2\"\r\n\
\r\n\
--B2\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
\u{200c}\u{200c}Hello Bob, the report is attached.\r\n\
--B2\r\n\
Content-Type: text/html; charset=utf-8\r\n\
\r\n\
<html><head><style>p{color:red}</style><script>alert(1)</script></head>\
<body onload=\"evil()\"><p style=\"margin:0\">Hello <b>Bob</b></p>\
<img src=\"cid:logo@x\"><img src=\"https://tracker.example/pixel.gif\" width=\"1\" height=\"1\">\
<a href=\"https://example.com/read\">Read</a><a href=\"javascript:alert(1)\">bad</a>\
<iframe src=\"https://evil.example\"></iframe><form action=\"https://x\"><input></form></body></html>\r\n\
--B2--\r\n\
--B1\r\n\
Content-Type: image/png\r\n\
Content-ID: <logo@x>\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
iVBORw0KGgo=\r\n\
--B1\r\n\
Content-Type: application/pdf; name=\"report.pdf\"\r\n\
Content-Disposition: attachment; filename=\"report.pdf\"\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
JVBERi0xLjQK\r\n\
--B1--\r\n";

    const TEXT_MAIL: &str = "From: carol@example.com\r\n\
Subject: Plain\r\n\
Message-ID: <plain@example.com>\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
See https://example.com/a?b=1&c=2. Thanks!\r\n\
> quoted <b>line</b>\r\n";

    #[test]
    fn summary() {
        let s = summarize(HTML_MAIL.as_bytes());
        assert_eq!(s.message_id.as_deref(), Some("abc123@example.com"));
        assert_eq!(s.subject, "月度报告");
        assert_eq!(s.from.name.as_deref(), Some("Ada Lovelace"));
        assert_eq!(s.from.address.as_deref(), Some("ada@example.com"));
        assert_eq!(s.date, Some(1_794_816_000_000));
        assert_eq!(s.snippet, "Hello Bob, the report is attached.");
        assert!(s.has_attachments);
        assert_eq!(
            s.list_unsubscribe.as_deref(),
            Some("https://example.com/unsub?u=1")
        );
    }

    #[test]
    fn header_only_summary_has_no_snippet() {
        let header = HTML_MAIL.split("\r\n\r\n").next().unwrap().to_string() + "\r\n\r\n";
        let s = summarize(header.as_bytes());
        assert_eq!(s.subject, "月度报告");
        assert_eq!(s.snippet, "");
    }

    #[test]
    fn html_is_sanitized_and_remote_images_are_blocked() {
        let r = render(HTML_MAIL.as_bytes(), RenderOptions::default()).unwrap();
        assert_eq!(r.kind, BodyKind::Html);
        for bad in [
            "<script",
            "onload",
            "javascript:",
            "<iframe",
            "<form",
            "<input",
            "tracker.example",
        ] {
            assert!(!r.body.contains(bad), "{bad} survived:\n{}", r.body);
        }
        assert!(r.has_remote_content);
        assert!(
            r.body.contains("img-src data:;"),
            "CSP must block remote images"
        );
        // 内嵌图片换成了 data URI，样式块和行内样式保留
        assert!(
            r.body
                .contains("src=\"data:image/png;base64,iVBORw0KGgo=\""),
            "{}",
            r.body
        );
        assert!(r.body.contains("p{color:red}"));
        assert!(r.body.contains("style=\"margin:0\""));
        assert!(r.body.contains("href=\"https://example.com/read\""));
        // 内嵌图片不算附件
        assert_eq!(r.attachments.len(), 1);
        assert_eq!(r.attachments[0].name, "report.pdf");
        assert_eq!(r.attachments[0].mime, "application/pdf");
        assert_eq!(r.to.len(), 2);
        assert_eq!(r.cc[0].address.as_deref(), Some("dave@example.com"));

        let allowed = render(HTML_MAIL.as_bytes(), RenderOptions { allow_remote: true }).unwrap();
        assert!(allowed.body.contains("https://tracker.example/pixel.gif"));
        assert!(allowed.body.contains("img-src data: https: http:"));
    }

    #[test]
    fn attachments_can_be_extracted() {
        let r = render(HTML_MAIL.as_bytes(), RenderOptions::default()).unwrap();
        let (name, mime, bytes) = attachment(HTML_MAIL.as_bytes(), r.attachments[0].index).unwrap();
        assert_eq!(name, "report.pdf");
        assert_eq!(mime, "application/pdf");
        assert_eq!(bytes, b"%PDF-1.4\n");
    }

    #[test]
    fn plain_text_is_escaped_and_linkified() {
        let r = render(TEXT_MAIL.as_bytes(), RenderOptions::default()).unwrap();
        assert_eq!(r.kind, BodyKind::Text);
        assert!(
            r.body.contains(
                "<a href=\"https://example.com/a?b=1&amp;c=2\" rel=\"noopener noreferrer\">"
            ),
            "{}",
            r.body
        );
        assert!(r.body.contains("</a>. Thanks!"));
        assert!(
            r.body
                .contains("<span class=\"mail-quote\">&gt; quoted &lt;b&gt;line&lt;/b&gt;</span>")
        );
        assert!(!r.has_remote_content);
    }

    #[test]
    fn base64_matches_reference() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn html_only_snippets_keep_paragraphs_apart() {
        let raw = "From: a@x\r\nSubject: s\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
<html><head><title>T</title><style>p{color:red}</style></head><body>\
<div>Issue 48</div><h1>Making the inbox <b>a tool</b> again</h1><p>Fish &amp; chips</p>\
<script>alert(1)</script></body></html>\r\n";
        assert_eq!(
            summarize(raw.as_bytes()).snippet,
            "Issue 48 Making the inbox a tool again Fish & chips"
        );
    }

    /// 国内很多服务（微博、银行、旧版邮箱）还在用 GB2312/GBK 发信。
    const GB2312_MAIL: &str = "From: =?gb2312?B?zqKyqQ==?= <noreply@service.weibo.com>\r\n\
Subject: =?gb2312?B?zqKyqbCyyKvM4dDR?=\r\n\
MIME-Version: 1.0\r\n\
Content-Type: text/html; charset=gb2312\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
PGh0bWw+PGJvZHk+PHA+x9ewrrXE08O7p6OsxOO1xNXLusXU2tDCyeixuMnPtcfCvKGjPC9wPjxwPsjnt8exvsjLstnX96Osx+u8sMqx0N64xMPcwuuhozwvcD48L2JvZHk+PC9odG1sPg==\r\n";

    #[test]
    fn gb2312_mail_is_decoded() {
        let s = summarize(GB2312_MAIL.as_bytes());
        assert_eq!(s.subject, "微博安全提醒");
        assert_eq!(s.from.name.as_deref(), Some("微博"));
        assert!(s.snippet.starts_with("亲爱的用户"), "{}", s.snippet);
        let r = render(GB2312_MAIL.as_bytes(), RenderOptions::default()).unwrap();
        assert!(r.body.contains("如非本人操作"));
    }

    #[test]
    fn raw_gbk_headers_are_decoded() {
        let mut raw = b"From: a@example.com\r\nSubject: ".to_vec();
        raw.extend_from_slice(b"\xd0\xc2\xc9\xe8\xb1\xb8\xb5\xc7\xc2\xbc\xcc\xe1\xd0\xd1");
        raw.extend_from_slice(b"\r\nContent-Type: text/plain; charset=gbk\r\n\r\nhi\r\n");
        assert_eq!(summarize(&raw).subject, "新设备登录提醒");

        // 声明的是 UTF-8，信头却是 GBK：退回 GB18030
        let mut raw = b"From: ".to_vec();
        raw.extend_from_slice(b"\xce\xa2\xb2\xa9");
        raw.extend_from_slice(b" <a@example.com>\r\nSubject: ok\r\n");
        raw.extend_from_slice(
            b"Content-Type: text/plain; charset=\"utf-8\"\r\n\r\n\xe4\xbd\xa0\xe5\xa5\xbd\r\n",
        );
        let s = summarize(&raw);
        assert_eq!(s.from.name.as_deref(), Some("微博"));
        assert_eq!(s.snippet, "你好");
    }

    #[test]
    fn invisible_characters_are_removed() {
        assert_eq!(
            clean_text("\u{200c}\u{034f} hi\u{feff}  there "),
            "hi there"
        );
    }
}

#[cfg(test)]
mod unsubscribe_tests {
    use super::*;

    fn mail(headers: &str) -> Vec<u8> {
        format!("From: a@example.com\r\nSubject: hi\r\n{headers}\r\n\r\nbody\r\n").into_bytes()
    }

    #[test]
    fn reads_list_unsubscribe() {
        let both = mail(
            "List-Unsubscribe: <mailto:leave@example.com?subject=unsubscribe>,\r\n <https://example.com/u?id=1>\r\nList-Unsubscribe-Post: List-Unsubscribe=One-Click",
        );
        assert_eq!(
            unsubscribe(&both),
            Some(Unsubscribe {
                url: Some("https://example.com/u?id=1".into()),
                mailto: Some("mailto:leave@example.com?subject=unsubscribe".into()),
                one_click: true,
            })
        );
        // 没有 List-Unsubscribe-Post：只能打开网页
        let page = mail("List-Unsubscribe: <https://example.com/u>");
        assert!(!unsubscribe(&page).unwrap().one_click);
        // 一键退订不接受普通的 http
        let plain = mail(
            "List-Unsubscribe: <http://example.com/u>\r\nList-Unsubscribe-Post: List-Unsubscribe=One-Click",
        );
        assert!(!unsubscribe(&plain).unwrap().one_click);
        let local = mail(
            "List-Unsubscribe: <http://127.0.0.1:1145/u>\r\nList-Unsubscribe-Post: List-Unsubscribe=One-Click",
        );
        assert!(unsubscribe(&local).unwrap().one_click);
        assert_eq!(unsubscribe(&mail("X-Other: 1")), None);
    }
}

#[cfg(test)]
mod reply_tests {
    use super::*;

    #[test]
    fn reply_context_keeps_ids_addresses_and_lines() {
        let raw = b"From: Ada <ada@example.com>\r\nReply-To: list@example.com\r\nTo: me@example.com, Bob <bob@example.com>\r\nCc: carol@example.com\r\nSubject: Plans\r\nDate: Tue, 14 Nov 2023 22:13:20 +0000\r\nMessage-ID: <c@example.com>\r\nReferences: <a@example.com>\r\n <b@example.com>\r\n\r\nFirst line\r\n\r\n\r\n\r\nSecond line   \r\n";
        let ctx = reply_context(raw).unwrap();
        assert_eq!(ctx.message_id.as_deref(), Some("c@example.com"));
        assert_eq!(ctx.references, ["a@example.com", "b@example.com"]);
        assert_eq!(ctx.subject, "Plans");
        assert_eq!(ctx.date, Some(1_700_000_000_000));
        assert_eq!(ctx.from[0].name.as_deref(), Some("Ada"));
        assert_eq!(ctx.reply_to[0].address.as_deref(), Some("list@example.com"));
        assert_eq!(ctx.to.len(), 2);
        assert_eq!(ctx.cc[0].address.as_deref(), Some("carol@example.com"));
        assert_eq!(ctx.text, "First line\n\nSecond line");
    }

    #[test]
    fn html_only_mail_is_quoted_line_by_line() {
        let raw = b"From: a@example.com\r\nSubject: hi\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<html><head><style>p{}</style></head><body><p>Hello\r\n  there</p><p>Fish &amp; chips</p><div>a<br>b</div></body></html>\r\n";
        let ctx = reply_context(raw).unwrap();
        // 段落之间空一行
        assert_eq!(ctx.text, "Hello there\n\nFish & chips\n\na\nb");
        assert!(ctx.message_id.is_none());
        assert!(ctx.references.is_empty());
    }
}

#[cfg(test)]
mod forward_tests {
    use super::*;

    #[test]
    fn inline_images_are_not_forwarded_as_attachments() {
        let raw = b"From: a@example.com\r\nSubject: hi\r\nContent-Type: multipart/mixed; boundary=\"M\"\r\n\r\n\
--M\r\nContent-Type: multipart/related; boundary=\"R\"\r\n\r\n\
--R\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>Logo <img src=\"cid:logo@x\"></p>\r\n\
--R\r\nContent-Type: image/png\r\nContent-ID: <logo@x>\r\nContent-Disposition: inline; filename=\"logo.png\"\r\nContent-Transfer-Encoding: base64\r\n\r\niVBORw0KGgo=\r\n\
--R--\r\n\
--M\r\nContent-Type: text/csv; name=\"data.csv\"\r\nContent-Disposition: attachment; filename=\"data.csv\"\r\n\r\na,b\r\n\
--M--\r\n";
        let ctx = reply_context(raw).unwrap();
        let names: Vec<&str> = ctx.attachments.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["data.csv"]);
        let (name, _, data) = attachment(raw, ctx.attachments[0].index).unwrap();
        assert_eq!(
            (name.as_str(), data.as_slice()),
            ("data.csv", b"a,b".as_slice())
        );
    }

    #[test]
    fn base64_round_trips() {
        for data in [&b""[..], b"a", b"ab", b"abc", &[0, 255, 254, 13, 10]] {
            assert_eq!(base64_decode(&base64(data)).as_deref(), Some(data));
        }
        assert_eq!(
            base64_decode("aGVs\r\nbG8=").as_deref(),
            Some(&b"hello"[..])
        );
        assert_eq!(base64_decode("not*base64"), None);
    }
}
