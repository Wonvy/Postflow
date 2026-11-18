//! 侧栏里订阅源和邮箱的顺序、文件夹，以及卡片视图的封面图。

use postflow_store::{AccountConfig, Article, ItemFilter, MailSync, NewFeed, NewMail, Store, ids};
use tempfile::TempDir;

fn open(tmp: &TempDir, device: &str) -> Store {
    Store::open(
        &tmp.path().join("data"),
        &tmp.path().join(format!("{device}.db")),
        device,
    )
    .unwrap()
}

fn feed(store: &Store, url: &str, title: &str) -> String {
    store
        .put_feed(NewFeed {
            url: url.into(),
            title: title.into(),
            ..Default::default()
        })
        .unwrap()
        .id
}

fn titles(store: &Store) -> Vec<(String, String)> {
    store
        .feeds()
        .unwrap()
        .into_iter()
        .map(|f| (f.folder, f.title))
        .collect()
}

#[test]
fn feeds_keep_the_order_they_were_dragged_into() {
    let tmp = TempDir::new().unwrap();
    let a = open(&tmp, "aaaaaaaaaaaa");
    let c = feed(&a, "https://c.example/feed", "Charlie");
    let x = feed(&a, "https://a.example/feed", "Alpha");
    let b = feed(&a, "https://b.example/feed", "Bravo");
    // 没拖动过的按名称排
    assert_eq!(
        titles(&a),
        vec![
            ("".into(), "Alpha".into()),
            ("".into(), "Bravo".into()),
            ("".into(), "Charlie".into())
        ]
    );

    a.reorder_feeds("", &[c.clone(), x.clone()]).unwrap();
    // 拖动过的排在前面，没拖动过的跟在后面
    assert_eq!(
        titles(&a),
        vec![
            ("".into(), "Charlie".into()),
            ("".into(), "Alpha".into()),
            ("".into(), "Bravo".into())
        ]
    );

    // 拖进文件夹
    a.reorder_feeds("News", std::slice::from_ref(&b)).unwrap();
    assert_eq!(a.feed(&b).unwrap().unwrap().folder, "News");

    // 另一台设备看到同样的顺序和文件夹
    let other = open(&tmp, "bbbbbbbbbbbb");
    assert_eq!(titles(&other), titles(&a));
    assert_eq!(
        titles(&other),
        vec![
            ("".into(), "Charlie".into()),
            ("".into(), "Alpha".into()),
            ("News".into(), "Bravo".into())
        ]
    );

    // 重建索引后顺序还在
    a.rebuild_index().unwrap();
    assert_eq!(titles(&a), titles(&other));
}

#[test]
fn accounts_keep_their_order() {
    let tmp = TempDir::new().unwrap();
    let store = open(&tmp, "aaaaaaaaaaaa");
    let add = |email: &str| {
        store
            .put_account(AccountConfig {
                email: email.into(),
                imap_host: "imap.example.com".into(),
                imap_port: 993,
                imap_security: "tls".into(),
                username: email.into(),
                ..Default::default()
            })
            .unwrap()
            .id
    };
    let first = add("first@example.com");
    let second = add("second@example.com");
    let third = add("third@example.com");
    store
        .reorder_accounts(&[third.clone(), first.clone(), second.clone()])
        .unwrap();
    let order: Vec<String> = store
        .accounts()
        .unwrap()
        .into_iter()
        .map(|a| a.id)
        .collect();
    assert_eq!(order, vec![third, first, second]);
}

#[test]
fn articles_get_a_cover_image() {
    let tmp = TempDir::new().unwrap();
    let store = open(&tmp, "aaaaaaaaaaaa");
    let id = feed(&store, "https://x.example/feed", "X");
    let article = |guid: &str, content: &str, image: Option<&str>| Article {
        id: ids::item_id(&id, guid),
        feed: id.clone(),
        guid: guid.into(),
        url: Some(format!("https://x.example/posts/{guid}")),
        title: guid.into(),
        author: None,
        published: 1_000,
        fetched: 1_000,
        summary: String::new(),
        content: content.into(),
        image: image.map(String::from),
        media: None,
    };
    store
        .add_articles(vec![
            article("from-content", r#"<p><img src="/img/cover.jpg"></p>"#, None),
            article(
                "from-feed",
                r#"<img src="https://x.example/inline.png">"#,
                Some("https://x.example/thumb.jpg"),
            ),
            article("none", "<p>text only</p>", None),
        ])
        .unwrap();
    let items = store.items(&ItemFilter::default(), None, 10).unwrap();
    let image = |guid: &str| {
        items
            .iter()
            .find(|i| i.title == guid)
            .unwrap()
            .image
            .clone()
    };
    assert_eq!(
        image("from-content").as_deref(),
        Some("https://x.example/img/cover.jpg")
    );
    assert_eq!(
        image("from-feed").as_deref(),
        Some("https://x.example/thumb.jpg")
    );
    assert_eq!(image("none"), None);
}

#[test]
fn an_old_index_gains_the_new_columns_without_losing_mail() {
    let tmp = TempDir::new().unwrap();
    let index = tmp.path().join("aaaaaaaaaaaa.db");
    {
        let store = open(&tmp, "aaaaaaaaaaaa");
        let id = feed(&store, "https://x.example/feed", "X");
        store
            .add_articles(vec![Article {
                id: ids::item_id(&id, "a"),
                feed: id.clone(),
                guid: "a".into(),
                url: None,
                title: "With picture".into(),
                author: None,
                published: 1_000,
                fetched: 1_000,
                summary: String::new(),
                content: r#"<img src="https://x.example/a.jpg">"#.into(),
                image: None,
                media: None,
            }])
            .unwrap();
        let account = store
            .put_account(AccountConfig {
                email: "me@example.com".into(),
                imap_host: "imap.example.com".into(),
                imap_port: 993,
                imap_security: "tls".into(),
                username: "me@example.com".into(),
                ..Default::default()
            })
            .unwrap();
        let message_id = "m1@example.com".to_string();
        store
            .apply_mail_sync(
                &account.id,
                MailSync {
                    uidvalidity: 1,
                    started_at: 1,
                    new: vec![NewMail {
                        id: ids::mail_id(&account.id, &message_id),
                        uid: 1,
                        message_id: Some(message_id),
                        subject: "Hello".into(),
                        date: 2_000,
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            )
            .unwrap();
    }
    // 做成加列之前的样子
    {
        let conn = rusqlite::Connection::open(&index).unwrap();
        conn.execute_batch(
            "ALTER TABLE items DROP COLUMN image;
             ALTER TABLE feeds DROP COLUMN position;
             ALTER TABLE accounts DROP COLUMN position;",
        )
        .unwrap();
    }
    let store = open(&tmp, "aaaaaaaaaaaa");
    let items = store.items(&ItemFilter::default(), None, 10).unwrap();
    assert_eq!(
        items.len(),
        2,
        "mail synced before the upgrade is still there"
    );
    let article = items.iter().find(|i| i.kind == "article").unwrap();
    assert_eq!(article.image.as_deref(), Some("https://x.example/a.jpg"));
    assert_eq!(store.accounts().unwrap().len(), 1);
}
