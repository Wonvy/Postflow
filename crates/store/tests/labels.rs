//! 分类：AI 分的、用户改的、按分类筛选和计数、找出还没分类的。

use postflow_store::{
    AccountConfig, Article, ItemFilter, ItemPatch, MailSync, NewFeed, NewMail, Store, View, ids,
};
use tempfile::TempDir;

fn setup() -> (TempDir, Store, Vec<String>) {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(
        &tmp.path().join("data"),
        &tmp.path().join("index.db"),
        "aaaaaaaaaaaa",
    )
    .unwrap();
    let feed = store
        .put_feed(NewFeed {
            url: "https://x.example/feed".into(),
            title: "X".into(),
            ..Default::default()
        })
        .unwrap();
    let articles: Vec<Article> = (1..=4)
        .map(|n| Article {
            id: ids::item_id(&feed.id, &n.to_string()),
            feed: feed.id.clone(),
            guid: n.to_string(),
            url: None,
            title: format!("Post {n}"),
            author: None,
            published: 1_000 * n,
            fetched: 1_000 * n,
            summary: String::new(),
            content: String::new(),
            image: None,
            media: None,
        })
        .collect();
    let ids: Vec<String> = articles.iter().map(|a| a.id.clone()).collect();
    store.add_articles(articles).unwrap();
    (tmp, store, ids)
}

#[test]
fn ai_labels_never_override_the_user() {
    let (_tmp, store, ids) = setup();
    store
        .set_labels(
            &[
                (ids[0].clone(), Some("工作".into())),
                (ids[1].clone(), Some("资讯".into())),
                (ids[2].clone(), None),
            ],
            false,
        )
        .unwrap();
    store
        .set_labels(&[(ids[1].clone(), Some("工作".into()))], true)
        .unwrap();
    // AI 再分一次：用户改过的那条不动
    store
        .set_labels(
            &[
                (ids[1].clone(), Some("推广".into())),
                (ids[0].clone(), Some("资讯".into())),
            ],
            false,
        )
        .unwrap();

    let items = store.items(&ItemFilter::default(), None, 10).unwrap();
    let label = |id: &str| items.iter().find(|i| i.id == id).unwrap().label.clone();
    assert_eq!(label(&ids[0]).as_deref(), Some("资讯"));
    assert_eq!(label(&ids[1]).as_deref(), Some("工作"));
    assert_eq!(label(&ids[2]), None, "\"none of them\" shows as no label");

    let only = ItemFilter {
        view: View::Inbox,
        label: Some("工作".into()),
        ..Default::default()
    };
    let work: Vec<String> = store
        .items(&only, None, 10)
        .unwrap()
        .into_iter()
        .map(|i| i.id)
        .collect();
    assert_eq!(work, vec![ids[1].clone()]);

    store
        .set_item_state(
            &[ids[1].clone()],
            &ItemPatch {
                read: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
    let counts = store.label_counts().unwrap();
    let work = counts.iter().find(|c| c.label == "工作").unwrap();
    assert_eq!((work.unread, work.total), (0, 1));
    assert!(counts.iter().all(|c| !c.label.is_empty()));

    // 「哪一类都不是」也算分过；只剩第四条没分
    let pending: Vec<String> = store
        .unlabeled(0, true, 10)
        .unwrap()
        .into_iter()
        .map(|i| i.id)
        .collect();
    assert_eq!(pending, vec![ids[3].clone()]);
    assert!(
        store.unlabeled(5_000, true, 10).unwrap().is_empty(),
        "older than `since` is skipped"
    );
}

#[test]
fn counts_split_mail_from_articles() {
    let (_tmp, store, ids) = setup();
    let acc = store
        .put_account(AccountConfig {
            email: "me@example.com".into(),
            imap_host: "imap.example.com".into(),
            imap_port: 993,
            imap_security: "tls".into(),
            username: "me@example.com".into(),
            ..Default::default()
        })
        .unwrap();
    let mail_id = ids::mail_id(&acc.id, "m1@example.com");
    store
        .apply_mail_sync(
            &acc.id,
            MailSync {
                uidvalidity: 1,
                started_at: 1_000,
                new: vec![NewMail {
                    id: mail_id.clone(),
                    uid: 1,
                    message_id: Some("m1@example.com".into()),
                    subject: "Invoice".into(),
                    date: 1_000,
                    ..Default::default()
                }],
                ..Default::default()
            },
        )
        .unwrap();
    store
        .set_labels(
            &[
                (mail_id, Some("账单".into())),
                (ids[0].clone(), Some("账单".into())),
                (ids[1].clone(), Some("资讯".into())),
            ],
            false,
        )
        .unwrap();

    let counts = store.label_counts().unwrap();
    let bills = counts.iter().find(|c| c.label == "账单").unwrap();
    assert_eq!((bills.unread, bills.total), (2, 2));
    assert_eq!((bills.unread_mail, bills.total_mail), (1, 1));
    let reading = counts.iter().find(|c| c.label == "资讯").unwrap();
    assert_eq!((reading.unread_mail, reading.total_mail), (0, 0));
}
