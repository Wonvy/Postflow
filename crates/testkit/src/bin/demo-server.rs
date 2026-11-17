//! Demo data for trying Postflow and for the README screenshots. Everything here is fictional.
//!
//! Starts two fake IMAP servers and a small HTTP server with three feeds, all on this machine:
//!
//! - `work@example.com` / `demo` on 127.0.0.1:1143
//! - `me@example.org` / `demo` on 127.0.0.1:1144
//! - feeds on http://127.0.0.1:1145/ (see the list printed at start-up)
//!
//! A new message arrives in the work mailbox every minute, to show live delivery.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use postflow_testkit::FakeImap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const FEED_PORT: u16 = 1145;
const AI_PORT: u16 = 1146;
const MINUTE: i64 = 60;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Civil date from days since 1970-01-01 (proleptic Gregorian).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

/// RFC 2822 date for `ago` seconds before now, e.g. `Mon, 16 Nov 2026 08:00:00 +0000`.
fn mail_date(ago: i64) -> String {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let t = now() - ago;
    let (days, secs) = (t.div_euclid(DAY), t.rem_euclid(DAY));
    let (y, m, d) = civil(days);
    format!(
        "{}, {d:02} {} {y} {:02}:{:02}:{:02} +0000",
        DAYS[days.rem_euclid(7) as usize],
        MONTHS[m as usize - 1],
        secs / HOUR,
        secs % HOUR / MINUTE,
        secs % MINUTE
    )
}

/// RFC 3339 date for `ago` seconds before now.
fn feed_date(ago: i64) -> String {
    let t = now() - ago;
    let (days, secs) = (t.div_euclid(DAY), t.rem_euclid(DAY));
    let (y, m, d) = civil(days);
    format!(
        "{y}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        secs / HOUR,
        secs % HOUR / MINUTE,
        secs % MINUTE
    )
}

fn text_mail(id: &str, from: &str, to: &str, subject: &str, ago: i64, body: &str) -> String {
    format!(
        "From: {from}\r\nTo: {to}\r\nSubject: {subject}\r\nDate: {}\r\nMessage-ID: <{id}>\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\r\n{body}\r\n",
        mail_date(ago)
    )
}

fn newsletter(ago: i64) -> String {
    format!(
        "From: \"Signal & Noise\" <digest@signal-noise.example>\r\n\
To: work@example.com\r\n\
Subject: Weekly digest: making the inbox a tool again\r\n\
Date: {}\r\n\
Message-ID: <digest-48@signal-noise.example>\r\n\
List-Unsubscribe: <https://signal-noise.example/unsubscribe?u=1>\r\n\
Content-Type: text/html; charset=utf-8\r\n\
\r\n\
<html><head><style>\
body{{font-family:-apple-system,Helvetica,Arial,sans-serif;color:#222}}\
.hero{{background:#f4f1ea;border-radius:10px;padding:20px 22px;margin-bottom:14px}}\
.card{{border:1px solid #e6e6e6;border-radius:10px;padding:16px 18px;margin:12px 0}}\
.tag{{display:inline-block;font-size:11px;letter-spacing:.04em;text-transform:uppercase;color:#7a6a3a}}\
h1{{font-size:22px;margin:6px 0 8px}} h2{{font-size:16px;margin:4px 0 6px}} p{{line-height:1.55;margin:0 0 8px}}\
a.btn{{display:inline-block;background:#222;color:#fff;text-decoration:none;padding:7px 12px;border-radius:6px;font-size:13px}}\
</style></head><body>\
<img src=\"https://signal-noise.example/logo.png\" width=\"132\" height=\"28\" alt=\"Signal &amp; Noise\">\
<div class=\"hero\"><span class=\"tag\">Issue 48</span><h1>Making the inbox a tool again</h1>\
<p>Mail, feeds and notifications all compete for the same attention. This week: three ways to pull them back into one calm place.</p>\
<a class=\"btn\" href=\"https://signal-noise.example/48\">Read the issue</a></div>\
<div class=\"card\"><span class=\"tag\">Workflow</span><h2>One queue, many sources</h2>\
<p>Treat every message, post and update as an item in a single queue. Archive is the default action; everything else is an exception.</p></div>\
<div class=\"card\"><span class=\"tag\">Keyboard</span><h2>J, K, E</h2>\
<p>Down, up, archive. Five minutes a day is enough when your hands never leave the keyboard.</p></div>\
<img src=\"https://tracker.signal-noise.example/open.gif\" width=\"1\" height=\"1\">\
<script>alert('never runs')</script></body></html>\r\n",
        mail_date(ago)
    )
}

fn with_attachment(ago: i64) -> String {
    format!(
        "From: Mei Han <mei@northwind.example>\r\n\
To: work@example.com\r\n\
Cc: Leo Park <leo@northwind.example>\r\n\
Subject: Q4 budget for review\r\n\
Date: {}\r\n\
Message-ID: <budget-q4@northwind.example>\r\n\
Content-Type: multipart/mixed; boundary=\"B\"\r\n\
\r\n\
--B\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Hi,\r\n\
\r\n\
The Q4 budget is attached. Main changes since the draft:\r\n\
- Cloud spend down 12% after the storage migration\r\n\
- Two new test machines for the release pipeline\r\n\
\r\n\
Details and the reasoning are on https://wiki.northwind.example/budget/q4 .\r\n\
\r\n\
> Can we have a first version before Wednesday's sync?\r\n\
\r\n\
Thanks,\r\n\
Mei\r\n\
--B\r\n\
Content-Type: text/csv; name=\"budget-q4.csv\"\r\n\
Content-Disposition: attachment; filename=\"budget-q4.csv\"\r\n\
\r\n\
item,amount\r\n\
cloud,12000\r\n\
test machines,8000\r\n\
--B--\r\n",
        mail_date(ago)
    )
}

fn seed_mail(work: &FakeImap, personal: &FakeImap) {
    let w = "work@example.com";
    work.deliver(
        "INBOX",
        &text_mail("offsite-1@northwind.example", "Leo Park <leo@northwind.example>", w,
            "Offsite agenda for Friday", 3 * DAY + 2 * HOUR,
            "Hi all,\r\n\r\nWe leave at 3pm on Friday. Bring a jacket, it gets cold by the lake.\r\n\r\nLeo"),
        &["\\Seen"],
    );
    // 同一个会话的三封，以及同一个发件人的几条通知，用来演示「折叠相同邮件」
    work.deliver(
        "INBOX",
        &text_mail(
            "review-5@northwind.example",
            "Mei Han <mei@northwind.example>",
            w,
            "Design review notes",
            2 * DAY + 9 * HOUR,
            "Notes from today's review are below. Two options on the table: A (full redesign) and B (incremental).",
        ),
        &["\\Seen"],
    );
    work.deliver(
        "INBOX",
        &text_mail(
            "review-6@northwind.example",
            "Leo Park <leo@northwind.example>",
            w,
            "Re: Design review notes",
            2 * DAY + 7 * HOUR,
            "I lean towards B. We can ship it in two weeks.\r\n\r\n> Two options on the table",
        ),
        &["\\Seen"],
    );
    work.deliver(
        "INBOX",
        &text_mail(
            "ci-1199@builds.example",
            "Build Bot <ci@builds.example>",
            w,
            "Build #1199 failed on main",
            DAY + 2 * HOUR,
            "1 test failed: sync::conflict_copies_are_handled",
        ),
        &["\\Seen"],
    );
    work.deliver(
        "INBOX",
        &text_mail(
            "ci-1200@builds.example",
            "Build Bot <ci@builds.example>",
            w,
            "Build #1200 passed on main",
            20 * HOUR,
            "All checks passed in 6m 40s.",
        ),
        &["\\Seen"],
    );
    work.deliver(
        "INBOX",
        &text_mail("review-7@northwind.example", "Design Review <review@northwind.example>", w,
            "Re: Design review notes", 2 * DAY + 5 * HOUR,
            "Decision: option B is approved, implementation starts Monday.\r\n\r\n> Option A felt too heavy for a first release."),
        &["\\Seen", "\\Flagged"],
    );
    work.deliver(
        "INBOX",
        &text_mail(
            "team-cn@northwind.example",
            "李雷 <lilei@northwind.example>",
            w,
            "周五团建安排",
            DAY + 6 * HOUR,
            "大家好，\r\n\r\n周五下午三点出发，地点在湖边，记得带外套。\r\n\r\n李雷",
        ),
        &["\\Seen"],
    );
    work.deliver("INBOX", &with_attachment(5 * HOUR), &[]);
    work.deliver("INBOX", &newsletter(3 * HOUR), &[]);
    work.deliver(
        "INBOX",
        &text_mail(
            "ci-1201@builds.example",
            "Build Bot <ci@builds.example>",
            w,
            "Build #1201 passed on main",
            40 * MINUTE,
            "All checks passed in 6m 12s.\r\n\r\nhttps://builds.example/postflow/1201",
        ),
        &[],
    );

    let p = "me@example.org";
    personal.deliver(
        "INBOX",
        &text_mail("receipt-88@acme-hosting.example", "Acme Hosting <billing@acme-hosting.example>", p,
            "Your receipt for November", DAY + 9 * HOUR,
            "Thanks for your payment of $5.00.\r\n\r\nView it online: https://acme-hosting.example/billing"),
        &["\\Seen"],
    );
    personal.deliver(
        "INBOX",
        &text_mail(
            "trip-1@example.org",
            "Alex Kim <alex@example.org>",
            p,
            "Photos from the trip",
            7 * HOUR,
            "Uploaded everything to the shared album. The sunset ones came out great.",
        ),
        &[],
    );
    personal.deliver(
        "INBOX",
        &text_mail(
            "dinner-1@example.org",
            "Mom <mom@example.org>",
            p,
            "Dinner on Saturday?",
            2 * HOUR,
            "Making dumplings on Saturday. Let me know if you're coming!",
        ),
        &[],
    );
}

struct Post {
    slug: &'static str,
    title: &'static str,
    author: &'static str,
    ago: i64,
    html: &'static str,
    /// 封面图的两个颜色。卡片视图里显示，由这个服务现场画出来。
    cover: Option<[&'static str; 2]>,
}

const FIELD_NOTES: &[Post] = &[
    Post {
        slug: "append-only-sync",
        title: "Syncing without a server: append-only logs",
        author: "Ana Ribeiro",
        ago: HOUR + 20 * MINUTE,
        html: "<p>Most sync systems assume a server that decides who wins. If you only have a shared folder, \
you need data that merges itself.</p>\
<h2>One writer per file</h2>\
<p>Every device appends to its own log and never touches anyone else's. File-sync tools then never see \
two devices editing the same file, so they never produce conflicts.</p>\
<pre><code>{\"ts\":\"1794859065899-000000-a0fc\",\"k\":\"item\",\"id\":\"4be0\",\"set\":{\"read\":true}}</code></pre>\
<h2>Last writer wins, per field</h2>\
<p>Each change carries a hybrid logical clock. When two devices edit the same field, the later timestamp \
wins; edits to different fields both survive.</p>\
<blockquote>The index is just a cache. Delete it and it rebuilds from the logs.</blockquote>",
        cover: Some(["#3f7d6b", "#a8d5c2"]),
    },
    Post {
        slug: "idle",
        title: "IMAP IDLE in practice",
        author: "Ana Ribeiro",
        ago: DAY + 3 * HOUR,
        html: "<p>IDLE lets a server push new mail to a connected client. Servers drop idle connections after \
about 30 minutes, so re-issue it every ten.</p>",
        cover: None,
    },
    Post {
        slug: "sandboxed-mail",
        title: "Rendering untrusted HTML mail safely",
        author: "Tom Weller",
        ago: 4 * DAY,
        html: "<p>Sanitize first, then render inside a sandboxed frame with a strict content security policy. \
Remote images stay off until you ask for them.</p>",
        cover: Some(["#2f4b7c", "#9bb7e0"]),
    },
];

const WEEKLY: &[Post] = &[
    Post {
        slug: "quiet-interfaces",
        title: "Quiet interfaces",
        author: "Interface Weekly",
        ago: 9 * HOUR,
        html: "<p>Fewer borders, more alignment. A quiet interface lets content carry the hierarchy.</p>",
        cover: Some(["#b4553c", "#f2c6a0"]),
    },
    Post {
        slug: "keyboard-first",
        title: "Designing keyboard-first tools",
        author: "Interface Weekly",
        ago: 2 * DAY + 8 * HOUR,
        html: "<p>Every frequent action deserves a single key. Show the key next to the action, and people learn it.</p>",
        cover: Some(["#5b4b8a", "#c9b8f0"]),
    },
];

const CN_WEEKLY: &[Post] = &[
    Post {
        slug: "issue-48",
        title: "开源周刊第 48 期：本地优先的软件",
        author: "开源周刊",
        ago: 6 * HOUR,
        html: "<p>本期介绍几款数据保存在用户自己设备上的软件，以及它们怎样在多台设备之间同步。</p>",
        cover: Some(["#4b5fb8", "#b8c4f2"]),
    },
    Post {
        slug: "issue-47",
        title: "开源周刊第 47 期：键盘优先的工作流",
        author: "开源周刊",
        ago: 7 * DAY + HOUR,
        html: "<p>用快捷键处理信息流：上下移动、归档、加星标，手不离开键盘。</p>",
        cover: None,
    },
];

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn atom(name: &str, slug: &str, icon: &str, posts: &[Post]) -> String {
    let base = format!("http://127.0.0.1:{FEED_PORT}/{slug}");
    let entries: String = posts
        .iter()
        .map(|p| {
            format!(
                "<entry><id>{base}/{s}</id><title>{t}</title><link href=\"{base}/{s}\"/>\
                 <updated>{d}</updated><author><name>{a}</name></author>{m}\
                 <content type=\"html\">{c}</content></entry>",
                m = if p.cover.is_some() {
                    format!(
                        "<media:thumbnail url=\"http://127.0.0.1:{FEED_PORT}/covers/{}.svg\"/>",
                        p.slug
                    )
                } else {
                    String::new()
                },
                s = p.slug,
                t = escape(p.title),
                d = feed_date(p.ago),
                a = escape(p.author),
                c = escape(p.html)
            )
        })
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
         <feed xmlns=\"http://www.w3.org/2005/Atom\" xmlns:media=\"http://search.yahoo.com/mrss/\">\
         <title>{}</title><link href=\"{base}/\"/><id>{base}/</id>\
         <icon>http://127.0.0.1:{FEED_PORT}/icons/{icon}.svg</icon><updated>{}</updated>{entries}</feed>",
        escape(name),
        feed_date(0)
    )
}

fn icon(letter: &str, color: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 32 32\"><rect width=\"32\" height=\"32\" rx=\"7\" fill=\"{color}\"/>\
         <text x=\"16\" y=\"22\" font-family=\"Helvetica,Arial,sans-serif\" font-size=\"17\" font-weight=\"700\" \
         fill=\"#fff\" text-anchor=\"middle\">{letter}</text></svg>"
    )
}

/// 一张抽象的封面：斜向渐变加几个圆。
fn cover(colors: [&str; 2], seed: usize) -> String {
    let [a, b] = colors;
    let circles: String = (0..3)
        .map(|i| {
            let n = seed * 7 + i * 13;
            format!(
                "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"#fff\" fill-opacity=\"0.{}\"/>",
                80 + (n * 37) % 480,
                40 + (n * 53) % 260,
                40 + (n * 11) % 90,
                12 + i * 6
            )
        })
        .collect();
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 640 360\">\
         <defs><linearGradient id=\"g\" x1=\"0\" y1=\"0\" x2=\"1\" y2=\"1\">\
         <stop offset=\"0\" stop-color=\"{a}\"/><stop offset=\"1\" stop-color=\"{b}\"/></linearGradient></defs>\
         <rect width=\"640\" height=\"360\" fill=\"url(#g)\"/>{circles}</svg>"
    )
}

fn route(path: &str) -> Option<(&'static str, String)> {
    if let Some(slug) = path
        .strip_prefix("/covers/")
        .and_then(|p| p.strip_suffix(".svg"))
    {
        let all = FIELD_NOTES.iter().chain(WEEKLY).chain(CN_WEEKLY);
        let (seed, post) = all.enumerate().find(|(_, p)| p.slug == slug)?;
        return Some(("image/svg+xml", cover(post.cover?, seed)));
    }
    let xml = "application/atom+xml; charset=utf-8";
    let svg = "image/svg+xml";
    Some(match path {
        "/field-notes/atom.xml" => (
            xml,
            atom("Field Notes", "field-notes", "field-notes", FIELD_NOTES),
        ),
        "/weekly/atom.xml" => (xml, atom("Interface Weekly", "weekly", "weekly", WEEKLY)),
        "/cn/atom.xml" => (xml, atom("开源周刊", "cn", "cn", CN_WEEKLY)),
        "/icons/field-notes.svg" => (svg, icon("F", "#3f7d6b")),
        "/icons/weekly.svg" => (svg, icon("I", "#b4553c")),
        "/icons/cn.svg" => (svg, icon("开", "#4b5fb8")),
        _ => return None,
    })
}

async fn serve_feeds() {
    let listener = TcpListener::bind(("127.0.0.1", FEED_PORT))
        .await
        .expect("bind feed server");
    while let Ok((mut stream, _)) = listener.accept().await {
        tokio::spawn(async move {
            let mut buf = vec![0u8; 8192];
            let Ok(n) = stream.read(&mut buf).await else {
                return;
            };
            let request = String::from_utf8_lossy(&buf[..n]);
            let path = request
                .split_whitespace()
                .nth(1)
                .unwrap_or("/")
                .split('?')
                .next()
                .unwrap_or("/");
            let response = match route(path) {
                Some((kind, body)) => format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                ),
                None => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_string(),
            };
            let _ = stream.write_all(response.as_bytes()).await;
        });
    }
}

#[tokio::main]
async fn main() {
    let work = FakeImap::start_on("work@example.com", "demo", 1143).await;
    let personal = FakeImap::start_on("me@example.org", "demo", 1144).await;
    seed_mail(&work, &personal);
    tokio::spawn(serve_feeds());
    // 假的 AI 服务，不需要密钥，回答是从正文里拼出来的
    let _ai = postflow_testkit::FakeAi::start_on(None, AI_PORT).await;

    println!("Postflow demo data is ready. Everything below is fictional.");
    println!();
    println!("Mail (server 127.0.0.1, encryption: none, password: demo):");
    println!("  work@example.com  port 1143");
    println!("  me@example.org    port 1144");
    println!();
    println!("Feeds:");
    for path in ["field-notes/atom.xml", "weekly/atom.xml", "cn/atom.xml"] {
        println!("  http://127.0.0.1:{FEED_PORT}/{path}");
    }
    println!();
    println!("AI (custom, no key, any model name): http://127.0.0.1:{AI_PORT}/v1");

    let mut n = 0;
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        n += 1;
        let raw = text_mail(
            &format!("tick-{n}@northwind.example"),
            "Leo Park <leo@northwind.example>",
            "work@example.com",
            &format!("Status update #{n}"),
            0,
            "Quick update on the release: everything is on track.",
        );
        work.deliver("INBOX", &raw, &[]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_773), (2026, 11, 16));
        assert_eq!(civil(19_782), (2024, 2, 29));
    }
}
