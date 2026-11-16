//! 开发用：在本机起两个假的 IMAP 服务器，带一些样例邮件，用来在界面里试邮箱功能。
//!
//! 运行后在 Postflow 里添加邮箱，服务器填 127.0.0.1、加密选「不加密」：
//!
//! - work@example.com / demo，端口 1143
//! - me@example.org / demo，端口 1144
//!
//! 每隔一分钟会往第一个邮箱投递一封新邮件，用来看实时收信。

use std::time::Duration;

use postflow_testkit::{FakeImap, mail};

fn html_newsletter() -> String {
    "From: \"少数派 Newsletter\" <newsletter@sspai.example>\r\n\
To: work@example.com\r\n\
Subject: 本周精选：把收件箱变回工具\r\n\
Date: Mon, 16 Nov 2026 09:30:00 +0800\r\n\
Message-ID: <weekly-48@sspai.example>\r\n\
List-Unsubscribe: <https://sspai.example/unsubscribe?u=1>\r\n\
Content-Type: text/html; charset=utf-8\r\n\
\r\n\
<html><head><style>.card{border:1px solid #e5e5e5;border-radius:8px;padding:16px;margin:12px 0}h2{margin:0 0 8px}</style></head>\
<body><img src=\"https://sspai.example/logo.png\" width=\"120\" height=\"32\" alt=\"logo\">\
<h1 style=\"font-size:22px\">本周精选</h1><p>这一期我们聊聊如何把信息流收拢到一个地方。</p>\
<div class=\"card\"><h2>把收件箱变回工具</h2><p>邮件、订阅、通知各管各的，注意力就被切成了碎片……</p>\
<a href=\"https://sspai.example/post/1\">阅读全文</a></div>\
<div class=\"card\"><h2>键盘优先的工作流</h2><p>J、K 上下，E 归档，一天的信息五分钟处理完。</p></div>\
<img src=\"https://tracker.sspai.example/open.gif\" width=\"1\" height=\"1\">\
<script>alert('不会执行')</script></body></html>\r\n"
        .to_string()
}

fn with_attachment() -> String {
    "From: 韩梅梅 <hanmeimei@example.com>\r\n\
To: work@example.com\r\n\
Cc: 李雷 <lilei@example.com>\r\n\
Subject: Q4 预算表，请过目\r\n\
Date: Mon, 16 Nov 2026 08:12:00 +0800\r\n\
Message-ID: <budget-q4@example.com>\r\n\
Content-Type: multipart/mixed; boundary=\"B\"\r\n\
\r\n\
--B\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
你好，\r\n\
\r\n\
Q4 的预算表在附件里，主要变化：\r\n\
- 云服务费用下调 12%\r\n\
- 新增两台测试机\r\n\
\r\n\
细节见 https://wiki.example.com/budget/q4 。\r\n\
\r\n\
> 上次会议说周三前给初稿\r\n\
\r\n\
梅梅\r\n\
--B\r\n\
Content-Type: text/csv; name=\"budget-q4.csv\"\r\n\
Content-Disposition: attachment; filename=\"budget-q4.csv\"\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
6aG555uuLOmHkemine+8iOWFg++8iQrkupHmnI3liqEsMTIwMDAK5rWL6K+V5py6LDgwMDAK\r\n\
--B--\r\n"
        .to_string()
}

fn dated(message_id: &str, from: &str, subject: &str, date: &str, body: &str) -> String {
    mail(message_id, from, subject, body).replace("Mon, 16 Nov 2026 08:00:00 +0000", date)
}

#[tokio::main]
async fn main() {
    let work = FakeImap::start_on("work@example.com", "demo", 1143).await;
    let personal = FakeImap::start_on("me@example.org", "demo", 1144).await;

    work.deliver(
        "INBOX",
        &dated(
            "team-1@example.com",
            "李雷 <lilei@example.com>",
            "周五团建安排",
            "Fri, 13 Nov 2026 18:20:00 +0800",
            "大家好，\r\n\r\n周五下午三点出发，地点在西湖边，记得带外套。\r\n\r\n李雷",
        ),
        &["\\Seen"],
    );
    work.deliver(
        "INBOX",
        &dated(
            "review-7@example.com",
            "Design Review <review@example.com>",
            "Re: 设计评审结论",
            "Sat, 14 Nov 2026 10:05:00 +0800",
            "结论：方案 B 通过，下周一开始实现。\r\n\r\n> 方案 A 的交互太重了",
        ),
        &["\\Seen", "\\Flagged"],
    );
    work.deliver("INBOX", &with_attachment(), &[]);
    work.deliver("INBOX", &html_newsletter(), &[]);
    work.deliver(
        "INBOX",
        &dated(
            "ci-1201@example.com",
            "GitHub <noreply@github.example>",
            "[postflow] CI passed on main",
            "Mon, 16 Nov 2026 10:41:00 +0800",
            "All checks have passed.\r\n\r\nhttps://github.example/postflow/actions/runs/1201",
        ),
        &[],
    );

    personal.deliver(
        "INBOX",
        &dated("invoice-88@fastmail.example", "Fastmail <billing@fastmail.example>", "Your receipt for November", "Sun, 15 Nov 2026 07:00:00 +0000",
            "Thanks for your payment of $5.00.\r\n\r\nView it online: https://fastmail.example/billing"),
        &[],
    );
    personal.deliver(
        "INBOX",
        &dated(
            "mom-1@example.org",
            "妈妈 <mom@example.org>",
            "周末回家吃饭吗",
            "Mon, 16 Nov 2026 11:20:00 +0800",
            "周六炖了排骨，回来的话提前说一声。",
        ),
        &[],
    );

    println!("fake IMAP servers are running:");
    println!("  work@example.com / demo  on 127.0.0.1:1143");
    println!("  me@example.org   / demo  on 127.0.0.1:1144");

    let mut n = 0;
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        n += 1;
        let raw = mail(
            &format!("tick-{n}@example.com"),
            "王经理 <wang@example.com>",
            &format!("进度同步 #{n}"),
            "刚刚有新的进度，抽空看一下。",
        );
        work.deliver("INBOX", &raw, &[]);
        println!("delivered a new message (#{n})");
    }
}
