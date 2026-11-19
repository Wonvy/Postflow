//! 全文搜索：文章和邮件写进索引后，按标题、发件人和正文都能搜到，中文按词也能搜到。

use postflow_store::{
    AccountConfig, Article, AttachmentMeta, ItemFilter, MailSync, MailText, NewFeed, NewMail,
    Store, View, ids,
};

fn store() -> (tempfile::TempDir, Store) {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::open(
        &tmp.path().join("data"),
        &tmp.path().join("local/index.db"),
        "aaaaaaaaaaaa",
    )
    .unwrap();
    (tmp, store)
}

fn article(feed_id: &str, guid: &str, title: &str, content: &str, published: i64) -> Article {
    Article {
        id: ids::item_id(feed_id, guid),
        feed: feed_id.to_string(),
        guid: guid.to_string(),
        url: Some(format!("https://example.com/{guid}")),
        title: title.into(),
        author: Some("Ada".into()),
        published,
        fetched: published,
        summary: String::new(),
        content: content.into(),
        image: None,
        media: None,
    }
}

/// 邮件原文变纯文字：测试里直接当 UTF-8 文字用，`To:` 开头的行算收件人。
fn mail_text(raw: &[u8]) -> MailText {
    let text = String::from_utf8_lossy(raw);
    let (to, body): (Vec<&str>, Vec<&str>) = text.lines().partition(|l| l.starts_with("To:"));
    let (files, body): (Vec<&str>, Vec<&str>) =
        body.into_iter().partition(|l| l.starts_with("File:"));
    MailText {
        body: body.join("\n"),
        recipients: to
            .iter()
            .map(|l| l.trim_start_matches("To:"))
            .collect::<Vec<_>>()
            .join(" "),
        attachments: files
            .iter()
            .enumerate()
            .map(|(i, l)| AttachmentMeta {
                index: i as u32,
                name: l.trim_start_matches("File:").trim().to_string(),
                mime: "application/pdf".into(),
                size: 1000,
            })
            .collect(),
    }
}

fn index(store: &Store) {
    while store.index_search(100, &mail_text).unwrap() > 0 {}
    assert_eq!(store.search_pending().unwrap(), 0);
}

fn search(store: &Store, query: &str) -> Vec<(String, Option<String>)> {
    let filter = ItemFilter {
        view: View::All,
        query: Some(query.into()),
        ..Default::default()
    };
    store
        .items(&filter, None, 50)
        .unwrap()
        .into_iter()
        .map(|i| (i.title, i.snippet))
        .collect()
}

#[test]
fn finds_articles_and_mail_by_title_people_and_body() {
    let (_tmp, store) = store();
    let feed = store
        .put_feed(NewFeed {
            url: "https://example.com/feed.xml".into(),
            title: "Example".into(),
            ..Default::default()
        })
        .unwrap();
    store
        .add_articles(vec![
            article(
                &feed.id,
                "a",
                "本地优先的软件",
                "<p>数据保存在用户自己设备上，<b>多台设备</b>之间同步。</p>",
                2,
            ),
            article(
                &feed.id,
                "b",
                "Quiet interfaces",
                "<p>Fewer borders, more alignment.</p><script>budget()</script>",
                1,
            ),
        ])
        .unwrap();

    let account = store
        .put_account(AccountConfig {
            email: "me@example.com".into(),
            name: "Me".into(),
            imap_host: "127.0.0.1".into(),
            imap_port: 1143,
            imap_security: "none".into(),
            username: "me@example.com".into(),
            ..Default::default()
        })
        .unwrap();
    store
        .apply_mail_sync(
            &account.id,
            MailSync {
                uidvalidity: 1,
                started_at: 0,
                new: vec![NewMail {
                    id: ids::mail_id(&account.id, "<q4@example.com>"),
                    uid: 1,
                    message_id: Some("<q4@example.com>".into()),
                    subject: "Q4 budget for review".into(),
                    from_name: Some("Mei Han".into()),
                    from_address: Some("mei@example.com".into()),
                    snippet: "The Q4 budget".into(),
                    date: 3,
                    read: false,
                    starred: false,
                    size: 100,
                    has_attachments: false,
                    list_unsubscribe: None,
                    raw: Some(
                        b"To: Alex Rivera <alex@example.com>\nFile: budget-q4.pdf\nCloud spend went down 12% after the storage migration."
                            .to_vec(),
                    ),
                    recipients: vec![],
                }],
                flags: vec![],
                vanished: vec![],
            },
        )
        .unwrap();
    index(&store);

    let titles = |q: &str| {
        search(&store, q)
            .into_iter()
            .map(|(t, _)| t)
            .collect::<Vec<_>>()
    };
    // 中文按词、按单字、跨越标签
    assert_eq!(titles("同步"), vec!["本地优先的软件"]);
    assert_eq!(titles("多台设备"), vec!["本地优先的软件"]);
    assert_eq!(titles("步"), vec!["本地优先的软件"]);
    assert_eq!(titles("软件 设备"), vec!["本地优先的软件"]);
    assert!(titles("设备 budget").is_empty(), "all words must match");
    // 发件人、正文、前缀、大小写
    assert_eq!(titles("mei"), vec!["Q4 budget for review"]);
    assert_eq!(titles("STORAGE migr"), vec!["Q4 budget for review"]);
    assert_eq!(titles("align"), vec!["Quiet interfaces"]);
    // script 里的字不算
    assert_eq!(titles("budget"), vec!["Q4 budget for review"]);
    assert!(titles("!!!").is_empty());

    // 搜索语法
    assert_eq!(titles("from:mei"), vec!["Q4 budget for review"]);
    assert_eq!(titles("发件人:mei"), vec!["Q4 budget for review"]);
    assert!(
        titles("from:quiet").is_empty(),
        "from: only looks at senders and authors"
    );
    assert_eq!(titles("to:alex"), vec!["Q4 budget for review"]);
    assert_eq!(titles("to:\"Alex Rivera\""), vec!["Q4 budget for review"]);
    assert!(titles("to:mei").is_empty());
    assert_eq!(titles("subject:budget"), vec!["Q4 budget for review"]);
    assert!(
        titles("subject:storage").is_empty(),
        "storage is only in the body"
    );
    assert_eq!(titles("-from:mei").len(), 2);
    assert_eq!(titles("budget -storage"), Vec::<String>::new());
    assert_eq!(titles("is:unread").len(), 3);
    assert_eq!(titles("is:unread from:mei"), vec!["Q4 budget for review"]);
    assert!(titles("has:attachment").is_empty());
    assert_eq!(titles("-has:attachment").len(), 3);
    assert_eq!(titles("before:1970-01-02").len(), 3);
    assert!(titles("after:2000").is_empty());
    assert!(
        titles("after:2000-13").is_empty(),
        "a date we can't read finds nothing"
    );
    // 网址里的冒号不是条件
    assert!(titles("https://example.com").is_empty());

    // 附件总览
    let files = store.attachments(None, 10).unwrap();
    assert_eq!(files.files.len(), 1);
    assert_eq!(files.files[0].name, "budget-q4.pdf");
    assert_eq!(files.files[0].from_name.as_deref(), Some("Mei Han"));
    assert!(files.not_downloaded.is_empty());
    assert_eq!(
        store.attachments(Some("BUDGET"), 10).unwrap().files.len(),
        1
    );
    assert_eq!(store.attachments(Some("mei@"), 10).unwrap().files.len(), 1);
    assert!(
        store
            .attachments(Some("100%"), 10)
            .unwrap()
            .files
            .is_empty()
    );


    // 标题没命中时给出正文里的片段
    let hit = search(&store, "storage");
    assert_eq!(
        hit[0].1.as_deref(),
        Some("Cloud spend went down 12% after the storage migration.")
    );
    assert_eq!(
        search(&store, "budget")[0].1,
        None,
        "title already shows the match"
    );
}

#[test]
fn later_writes_are_picked_up() {
    let (_tmp, store) = store();
    let feed = store
        .put_feed(NewFeed {
            url: "https://example.com/feed.xml".into(),
            title: "Example".into(),
            ..Default::default()
        })
        .unwrap();
    let a = article(&feed.id, "a", "Short", "<p>only the opening</p>", 1);
    let id = a.id.clone();
    store.add_articles(vec![a]).unwrap();
    index(&store);
    assert!(search(&store, "conclusion").is_empty());

    // 抓到全文后能搜到全文里的字
    store
        .set_full_text(&id, "<p>only the opening, then the conclusion</p>")
        .unwrap();
    assert_eq!(store.search_pending().unwrap(), 1);
    index(&store);
    assert_eq!(search(&store, "conclusion").len(), 1);

    // 重建索引后条目重新收录，不重复
    store.rebuild_index().unwrap();
    index(&store);
    assert_eq!(search(&store, "short").len(), 1);
}
