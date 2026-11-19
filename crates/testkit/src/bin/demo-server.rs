//! Demo data for trying Postflow and for the README screenshots. Everything here is fictional.
//!
//! Starts two fake IMAP servers and a small HTTP server with three feeds, all on this machine:
//!
//! - `work@example.com` / `demo` on 127.0.0.1:1143
//! - `me@example.org` / `demo` on 127.0.0.1:1144
//! - feeds on http://127.0.0.1:1145/ (see the list printed at start-up)
//! - outgoing mail (SMTP) on 127.0.0.1:1025 for all demo mailboxes. Mail sent to another demo
//!   address lands in its inbox; everything else is only printed.
//!
//! A new message arrives in the work mailbox every minute, to show live delivery.
//!
//! With `DEMO_QUIET_IDLE=1` the servers behave like providers that never announce new mail
//! during IDLE and only send keepalives, to test that Postflow still picks it up.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use postflow_testkit::{FakeImap, FakeSmtp};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const FEED_PORT: u16 = 1145;
const AI_PORT: u16 = 1146;
/// 用微软账户登录的演示邮箱（假的登录服务在 FEED_PORT 上）。
const OUTLOOK_PORT: u16 = 1147;
const OUTLOOK_USER: &str = "outlook-demo@example.com";
/// 演示邮箱共用的发信服务器。本机收信服务器的发信端口，Postflow 自动猜的就是它。
const SMTP_PORT: u16 = 1025;
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

/// 国内很多服务还在用 GB2312 发信：信头和正文都要按它解码，不然是乱码。
fn gb2312_notice(to: &str, ago: i64) -> String {
    format!(
        "From: =?gb2312?B?1MbFzA==?= <notice@cloud-drive.example>\r\n\
To: {to}\r\n\
Subject: =?gb2312?B?0MLJ6LG4tcfCvMzh0NE=?=\r\n\
Date: {}\r\n\
Message-ID: <login-77@cloud-drive.example>\r\n\
MIME-Version: 1.0\r\n\
Content-Type: text/html; charset=gb2312\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
PGh0bWw+PGJvZHk+PHA+xOO6w6OsPC9wPjxwPsTjtcTUxsXM1cu6xbjVuNXU2tK7zKjQwsnosbjJ\r\n\
z7XHwryjuk1hY6Osyc+6o6GjPC9wPjxwPsjnufvV4rK7ysfE47G+yMu1xLLZ1/ejrMfrvqG/7NDe\r\n\
uMTD3MLroaM8L3A+PC9ib2R5PjwvaHRtbD4=\r\n",
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
List-Unsubscribe: <mailto:leave@signal-noise.example?subject=unsubscribe>, <http://127.0.0.1:1145/unsubscribe?list=signal-noise>\r\n\
List-Unsubscribe-Post: List-Unsubscribe=One-Click\r\n\
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

/// 会议邀请：带一段 iCalendar（明天上午十点，按 UTC 写，界面按当地时间显示）。
fn meeting_invite(ago: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    // 明天这个时候，取整到整点
    let start = (now + 86_400) / 3600 * 3600;
    let ics_time = |t: i64| {
        let days = t.div_euclid(86_400);
        let secs = t.rem_euclid(86_400);
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + i64::from(m <= 2);
        format!(
            "{y:04}{m:02}{d:02}T{:02}{:02}00Z",
            secs / 3600,
            secs % 3600 / 60
        )
    };
    format!(
        "From: Leo Park <leo@northwind.example>\r\n\
To: work@example.com, Mei Han <mei@northwind.example>\r\n\
Subject: Invitation: Release planning\r\n\
Date: {date}\r\n\
Message-ID: <invite-release@northwind.example>\r\n\
Content-Type: multipart/alternative; boundary=\"C\"\r\n\
\r\n\
--C\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Let's plan the next release: scope, dates and who does what.\r\n\
\r\n\
Join online: https://meet.google.com/abc-defg-hij\r\n\
--C\r\n\
Content-Type: text/calendar; charset=utf-8; method=REQUEST\r\n\
\r\n\
BEGIN:VCALENDAR\r\n\
PRODID:-//Northwind//Calendar//EN\r\n\
VERSION:2.0\r\n\
METHOD:REQUEST\r\n\
BEGIN:VEVENT\r\n\
UID:release-planning@northwind.example\r\n\
SEQUENCE:0\r\n\
DTSTAMP:{stamp}\r\n\
DTSTART:{start}\r\n\
DTEND:{end}\r\n\
SUMMARY:Release planning\r\n\
LOCATION:Meeting room 2\r\n\
ORGANIZER;CN=Leo Park:mailto:leo@northwind.example\r\n\
ATTENDEE;PARTSTAT=NEEDS-ACTION;RSVP=TRUE;CN=Sam Lee:mailto:work@example.com\r\n\
ATTENDEE;PARTSTAT=ACCEPTED;CN=Mei Han:mailto:mei@northwind.example\r\n\
DESCRIPTION:Scope\\, dates and owners.\\nJoin: https://meet.google.com/abc-defg-hij\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n\
--C--\r\n",
        date = mail_date(ago),
        stamp = ics_time(now - ago),
        start = ics_time(start),
        end = ics_time(start + 3600),
    )
}

/// 工作邮箱里已经发过的几封信，和几个自己建的文件夹。
fn seed_folders(work: &FakeImap) {
    let w = "Sam Lee <work@example.com>";
    work.deliver(
        "Sent",
        &text_mail(
            "sent-agenda@example.com",
            w,
            "Leo Park <leo@northwind.example>",
            "Re: Offsite agenda for Friday",
            3 * DAY,
            "Looks good. I'll book the room.\r\n\r\nSam",
        ),
        &["\\Seen"],
    );
    work.deliver(
        "Sent",
        &text_mail(
            "sent-budget@example.com",
            w,
            "Mei Han <mei@northwind.example>",
            "Q4 budget comments",
            5 * HOUR,
            "Two small notes on the cloud line, otherwise approved.",
        ),
        &["\\Seen"],
    );
    for name in ["Projects", "Projects/Launch", "Receipts"] {
        work.add_mailbox(name, None);
    }
    work.deliver(
        "Projects/Launch",
        &text_mail(
            "launch-1@northwind.example",
            "Mei Han <mei@northwind.example>",
            "work@example.com",
            "Launch checklist",
            2 * DAY,
            "Docs, release notes and the store listing are ready for review.",
        ),
        &[],
    );
    for (n, (shop, amount)) in [("Acme Hosting", "$24.00"), ("Paper & Ink", "$12.50")]
        .into_iter()
        .enumerate()
    {
        work.deliver(
            "Receipts",
            &text_mail(
                &format!("receipt-{n}@shop.example"),
                &format!("{shop} <billing@shop.example>"),
                "work@example.com",
                &format!("Your receipt from {shop}"),
                (n as i64 + 4) * DAY,
                &format!("Total paid: {amount}"),
            ),
            &["\\Seen"],
        );
    }
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
    work.deliver("INBOX", &meeting_invite(90 * MINUTE), &[]);
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
    personal.deliver("INBOX", &gb2312_notice(p, 2 * HOUR), &[]);
    // Gmail 那样标了用途的垃圾邮件和已删除
    personal.add_mailbox("[Gmail]/Spam", Some("\\Junk"));
    personal.add_mailbox("[Gmail]/Trash", Some("\\Trash"));
    personal.deliver(
        "[Gmail]/Spam",
        &text_mail(
            "prize-1@lucky-draw.example",
            "Lucky Draw <win@lucky-draw.example>",
            p,
            "You have been selected!",
            5 * HOUR,
            "Claim your prize today. This offer expires soon.",
        ),
        &[],
    );
    personal.deliver(
        "[Gmail]/Spam",
        &text_mail(
            "tickets-1@example.org",
            "Concert Hall <tickets@example.org>",
            p,
            "Your tickets for Saturday",
            9 * HOUR,
            "Two seats, row F. Show this email at the door.",
        ),
        &[],
    );
    personal.deliver(
        "[Gmail]/Trash",
        &text_mail(
            "old-plan-1@example.org",
            "Alex Kim <alex@example.org>",
            p,
            "Old travel plan",
            2 * DAY,
            "Ignore the earlier plan, we changed the dates.",
        ),
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

struct Episode {
    slug: &'static str,
    title: &'static str,
    ago: i64,
    /// 秒
    duration: u32,
    notes: &'static str,
}

/// 一档虚构的播客。音频文件不存在，只为让「播客」这一类和节目页有内容可看。
const PODCAST: &[Episode] = &[
    Episode {
        slug: "local-first-two-years-later",
        title: "Local-first, two years later",
        ago: 5 * HOUR,
        duration: 48 * 60 + 12,
        notes: "<p>Two years after moving everything onto their own devices, Mira and Jonas look back at what \
got easier, what got harder, and what they would do differently.</p>\
<ul><li>00:00 Hello again</li><li>04:30 Sync folders instead of servers</li>\
<li>21:15 When two laptops disagree</li><li>39:40 Listener questions</li></ul>",
    },
    Episode {
        slug: "why-inboxes-feel-loud",
        title: "Why inboxes feel loud",
        ago: 7 * DAY + 2 * HOUR,
        duration: 41 * 60 + 5,
        notes: "<p>Notifications, badges and bold rows: a tour of everything an inbox does to get your attention, \
and the few things worth keeping.</p>",
    },
    Episode {
        slug: "designing-for-the-keyboard",
        title: "Designing for the keyboard",
        ago: 14 * DAY + 5 * HOUR,
        duration: 52 * 60 + 30,
        notes: "<p>Single-key shortcuts, discoverable hints, and why the best shortcut is the one shown next to \
the button.</p>",
    },
    Episode {
        slug: "reading-on-your-own-terms",
        title: "Reading on your own terms",
        ago: 21 * DAY + 3 * HOUR,
        duration: 37 * 60 + 48,
        notes: "<p>Feeds, newsletters and read-later lists: how to keep up with the writing you care about without \
an algorithm in between.</p>",
    },
];

fn podcast() -> String {
    let base = format!("http://127.0.0.1:{FEED_PORT}/podcast");
    let items: String = PODCAST
        .iter()
        .map(|e| {
            format!(
                "<item><guid>{base}/{s}</guid><title>{t}</title><link>{base}/{s}</link>\
                 <pubDate>{d}</pubDate><description>{n}</description>\
                 <enclosure url=\"{base}/{s}.mp3\" type=\"audio/mpeg\" length=\"{len}\"/>\
                 <itunes:duration>{dur}</itunes:duration></item>",
                s = e.slug,
                t = escape(e.title),
                d = mail_date(e.ago),
                n = escape(e.notes),
                len = e.duration * 16_000,
                dur = e.duration,
            )
        })
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
         <rss version=\"2.0\" xmlns:itunes=\"http://www.itunes.com/dtds/podcast-1.0.dtd\"><channel>\
         <title>Slow Signals</title><link>{base}/</link>\
         <description>Conversations about calm, personal software. A fictional show for the Postflow demo.</description>\
         <itunes:author>Mira &amp; Jonas</itunes:author>\
         <itunes:image href=\"{base}/artwork.svg\"/>\
         <image><url>{base}/artwork.svg</url><title>Slow Signals</title><link>{base}/</link></image>\
         {items}</channel></rss>"
    )
}

/// 节目封面：方形渐变上一个声波图案。
fn podcast_artwork() -> String {
    let bars: String = [40, 90, 140, 70, 160, 110, 60, 120, 80]
        .iter()
        .enumerate()
        .map(|(i, h)| {
            format!(
                "<rect x=\"{}\" y=\"{}\" width=\"22\" height=\"{h}\" rx=\"11\" fill=\"#fff\" fill-opacity=\"0.85\"/>",
                64 + i * 32,
                200 - h / 2
            )
        })
        .collect();
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 400 400\">\
         <defs><linearGradient id=\"g\" x1=\"0\" y1=\"0\" x2=\"1\" y2=\"1\">\
         <stop offset=\"0\" stop-color=\"#6b4fbb\"/><stop offset=\"1\" stop-color=\"#e0789a\"/></linearGradient></defs>\
         <rect width=\"400\" height=\"400\" fill=\"url(#g)\"/>{bars}\
         <text x=\"200\" y=\"350\" font-family=\"Helvetica,Arial,sans-serif\" font-size=\"34\" font-weight=\"700\" \
         fill=\"#fff\" text-anchor=\"middle\">SLOW SIGNALS</text></svg>"
    )
}

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

/// 文章的网页：订阅里只有开头，这里多几段，「抓取全文」时能看出区别。
fn page(post: &Post) -> String {
    let more = "<p>This is the full article from the website. The feed only carries the opening, so \
                fetching the page brings in the rest: background, details and the conclusion.</p>\
                <p>Readers that keep everything on your own device can still fetch the original page when \
                you ask for it, and keep a copy so it opens instantly the next time.</p>\
                <p>That is the end of the article. Thanks for reading.</p>";
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{t}</title></head>\
         <body><nav><a href=\"/\">Home</a></nav><main><article><h1>{t}</h1>{html}{more}</article></main>\
         <footer>Demo site</footer></body></html>",
        t = escape(post.title),
        html = post.html,
    )
}

fn route(path: &str) -> Option<(&'static str, String)> {
    let all = || FIELD_NOTES.iter().chain(WEEKLY).chain(CN_WEEKLY);
    if let Some((_, slug)) = path.trim_start_matches('/').split_once('/')
        && let Some(post) = all().find(|p| p.slug == slug)
    {
        return Some(("text/html; charset=utf-8", page(post)));
    }
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
        "/podcast/rss.xml" => ("application/rss+xml; charset=utf-8", podcast()),
        "/podcast/artwork.svg" => (svg, podcast_artwork()),
        "/icons/field-notes.svg" => (svg, icon("F", "#3f7d6b")),
        "/icons/weekly.svg" => (svg, icon("I", "#b4553c")),
        "/icons/cn.svg" => (svg, icon("开", "#4b5fb8")),
        _ => return None,
    })
}

/// 读完整个请求：先读到空行，再按 Content-Length 把正文读全。
async fn read_request(stream: &mut tokio::net::TcpStream) -> Option<String> {
    let mut buf = Vec::new();
    let mut chunk = vec![0u8; 8192];
    loop {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        let text = String::from_utf8_lossy(&buf);
        if let Some(end) = text.find("\r\n\r\n") {
            let length = text[..end]
                .lines()
                .find_map(|l| {
                    let (k, v) = l.split_once(':')?;
                    k.eq_ignore_ascii_case("content-length")
                        .then(|| v.trim().parse::<usize>().ok())?
                })
                .unwrap_or(0);
            if buf.len() >= end + 4 + length {
                break;
            }
        }
        if buf.len() > 1 << 20 {
            break;
        }
    }
    Some(String::from_utf8_lossy(&buf).to_string())
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                Ok(b) => {
                    out.push(b);
                    i += 2;
                }
                Err(_) => out.push(b'%'),
            },
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn form(s: &str) -> Vec<(String, String)> {
    s.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (percent_decode(k), percent_decode(v))
        })
        .collect()
}

fn get<'a>(params: &'a [(String, String)], key: &str) -> &'a str {
    params
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
        .unwrap_or("")
}

fn base64url(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..=chunk.len() {
            out.push(TABLE[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

/// 演示用的几个假服务：一键退订，以及微软登录（登录页上点「允许」或「拒绝」，换令牌时检查 PKCE；
/// 发出的访问令牌就是演示邮箱的密码 `demo`，假 IMAP 服务器认它）。
fn fake_services(
    method: &str,
    path: &str,
    query: &str,
    body: &str,
) -> Option<(&'static str, &'static str, String)> {
    let json = "application/json";
    match (method, path) {
        // 假的微软和 Google 登录页：Google 的在 /google 下面
        ("GET", "/common/oauth2/v2.0/authorize" | "/google/o/oauth2/v2/auth") => {
            let who = if path.starts_with("/google") {
                "Google"
            } else {
                "Microsoft"
            };
            let q = form(query);
            let back = get(&q, "redirect_uri");
            let state = get(&q, "state");
            if !back.starts_with("http://localhost:") || get(&q, "code_challenge_method") != "S256"
            {
                return Some((
                    "400 Bad Request",
                    "text/plain",
                    "bad sign-in request".into(),
                ));
            }
            let allow = format!(
                "{back}?code=demo.{}&state={state}",
                get(&q, "code_challenge")
            );
            let deny = format!(
                "{back}?error=access_denied&error_description=The+user+said+no&state={state}"
            );
            let user = escape(get(&q, "login_hint"));
            Some((
                "200 OK",
                "text/html; charset=utf-8",
                format!(
                    "<!doctype html><html><head><meta charset=\"utf-8\"><title>Demo sign-in</title></head>\
                     <body style=\"font:15px system-ui;max-width:420px;margin:12vh auto\">\
                     <h2>Demo {who} sign-in</h2><p>Postflow wants to read mail for <b>{user}</b>.</p>\
                     <p>This page is fake and only exists for the demo.</p>\
                     <p><a id=\"allow\" href=\"{}\">Allow</a> &nbsp; <a id=\"deny\" href=\"{}\">Deny</a></p></body></html>",
                    escape(&allow),
                    escape(&deny)
                ),
            ))
        }
        // 演示邮件里的一键退订
        ("POST", "/unsubscribe") => {
            let ok = body.contains("List-Unsubscribe=One-Click");
            if ok {
                println!("demo: unsubscribed {query}");
            }
            Some(if ok {
                ("200 OK", "text/plain", "You're unsubscribed.".into())
            } else {
                (
                    "400 Bad Request",
                    "text/plain",
                    "missing List-Unsubscribe=One-Click".into(),
                )
            })
        }
        ("POST", "/common/oauth2/v2.0/token" | "/google/token") => {
            let f = form(body);
            // Google 的桌面应用换令牌时还要客户端密钥
            let ok = !get(&f, "client_id").is_empty()
                && (path != "/google/token" || !get(&f, "client_secret").is_empty())
                && match get(&f, "grant_type") {
                    "authorization_code" => {
                        use sha2::{Digest, Sha256};
                        let challenge =
                            base64url(&Sha256::digest(get(&f, "code_verifier").as_bytes()));
                        get(&f, "code").strip_prefix("demo.") == Some(challenge.as_str())
                    }
                    "refresh_token" => get(&f, "refresh_token") == "demo-refresh",
                    _ => false,
                };
            Some(if ok {
                (
                    "200 OK",
                    json,
                    r#"{"token_type":"Bearer","access_token":"demo","refresh_token":"demo-refresh","expires_in":120}"#.into(),
                )
            } else {
                (
                    "400 Bad Request",
                    json,
                    r#"{"error":"invalid_grant","error_description":"The code or refresh token is not valid."}"#.into(),
                )
            })
        }
        _ => None,
    }
}

async fn serve_feeds() {
    let listener = TcpListener::bind(("127.0.0.1", FEED_PORT))
        .await
        .expect("bind feed server");
    while let Ok((mut stream, _)) = listener.accept().await {
        tokio::spawn(async move {
            let Some(request) = read_request(&mut stream).await else {
                return;
            };
            let method = request.split_whitespace().next().unwrap_or("GET");
            let target = request.split_whitespace().nth(1).unwrap_or("/");
            let (path, query) = target.split_once('?').unwrap_or((target, ""));
            let body = request.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
            if let Some((status, kind, body)) = fake_services(method, path, query, body) {
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
                return;
            }
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
    let outlook = FakeImap::start_on(OUTLOOK_USER, "demo", OUTLOOK_PORT).await;
    outlook.deliver(
        "INBOX",
        &text_mail(
            "welcome-1@outlook-demo.example",
            "Demo Team <team@outlook-demo.example>",
            OUTLOOK_USER,
            "Signed in with OAuth",
            10 * MINUTE,
            "This mailbox only accepts XOAUTH2 sign-in, like Outlook.com.\r\n\r\nIf you can read this, Postflow signed in through the browser.",
        ),
        &[],
    );
    if std::env::var_os("DEMO_QUIET_IDLE").is_some() {
        work.quiet_idle(Duration::from_secs(20));
        personal.quiet_idle(Duration::from_secs(20));
    }
    seed_mail(&work, &personal);
    work.add_mailbox("Sent", Some("\\Sent"));
    work.add_mailbox("Drafts", Some("\\Drafts"));
    seed_folders(&work);
    personal.add_mailbox("[Gmail]/Sent Mail", Some("\\Sent"));
    personal.add_mailbox("[Gmail]/Drafts", Some("\\Drafts"));
    outlook.add_mailbox("Sent Items", Some("\\Sent"));
    outlook.add_mailbox("Drafts", Some("\\Drafts"));

    // 发给演示邮箱的信投进它的收件箱，发给别人的只打印出来
    let smtp = FakeSmtp::start_on(SMTP_PORT).await;
    for user in ["work@example.com", "me@example.org", OUTLOOK_USER] {
        smtp.add_user(user, "demo");
    }
    let inboxes = [
        ("work@example.com", work.clone()),
        ("me@example.org", personal.clone()),
        (OUTLOOK_USER, outlook.clone()),
    ];
    smtp.on_deliver(move |mail| {
        let raw = String::from_utf8_lossy(&mail.raw);
        for to in &mail.to {
            match inboxes
                .iter()
                .find(|(user, _)| to.eq_ignore_ascii_case(user))
            {
                Some((_, server)) => {
                    server.deliver("INBOX", &raw, &[]);
                }
                None => println!("demo: {} sent mail to {to}", mail.from),
            }
        }
    });
    tokio::spawn(serve_feeds());
    // 假的 AI 服务，不需要密钥，回答是从正文里拼出来的
    let _ai = postflow_testkit::FakeAi::start_on(None, AI_PORT).await;

    println!("Postflow demo data is ready. Everything below is fictional.");
    println!();
    println!("Mail (server 127.0.0.1, encryption: none, password: demo):");
    println!("  work@example.com  port 1143");
    println!("  me@example.org    port 1144");
    println!("  sending (SMTP)    port {SMTP_PORT}, chosen automatically");
    println!();
    println!("Feeds:");
    for path in [
        "field-notes/atom.xml",
        "weekly/atom.xml",
        "cn/atom.xml",
        "podcast/rss.xml",
    ] {
        println!("  http://127.0.0.1:{FEED_PORT}/{path}");
    }
    println!();
    println!("AI (custom, no key, any model name): http://127.0.0.1:{AI_PORT}/v1");
    println!();
    println!("Microsoft sign-in (OAuth): start the app with");
    println!(
        "  POSTFLOW_MICROSOFT_AUTHORITY=http://127.0.0.1:{FEED_PORT}/common POSTFLOW_MICROSOFT_CLIENT_ID=demo"
    );
    println!(
        "  then add an Outlook account {OUTLOOK_USER}, server 127.0.0.1 port {OUTLOOK_PORT}, encryption none."
    );
    println!("Google sign-in (OAuth): start the app with");
    println!(
        "  POSTFLOW_GOOGLE_AUTHORITY=http://127.0.0.1:{FEED_PORT}/google POSTFLOW_GOOGLE_CLIENT_ID=demo POSTFLOW_GOOGLE_CLIENT_SECRET=demo"
    );
    println!(
        "  then add a Gmail account me@example.org, server 127.0.0.1 port 1144, encryption none."
    );

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
