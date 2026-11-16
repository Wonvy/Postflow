//! 邮箱账户与邮件的存储：账户设置跨设备同步，邮件只在本机，和文章混在同一个信息流里。

use std::fs;

use postflow_store::{
    AccountConfig, Article, ItemFilter, ItemKind, ItemPatch, MailSync, NewFeed, NewMail, Store,
    View, ids,
};

fn open(dir: &std::path::Path, data: &std::path::Path, device: &str) -> Store {
    Store::open(data, &dir.join(format!("{device}.db")), device).unwrap()
}

fn account(email: &str) -> AccountConfig {
    AccountConfig {
        email: email.into(),
        name: "Work".into(),
        imap_host: "imap.example.com".into(),
        imap_port: 993,
        imap_security: "tls".into(),
        username: email.into(),
    }
}

fn mail(account_id: &str, uid: u32, date: i64) -> NewMail {
    let message_id = format!("m{uid}@example.com");
    NewMail {
        id: ids::mail_id(account_id, &message_id),
        uid,
        message_id: Some(message_id),
        subject: format!("Subject {uid}"),
        from_name: Some("Ada".into()),
        from_address: Some("ada@example.com".into()),
        snippet: "hello".into(),
        date,
        size: 100,
        raw: Some(format!("Subject: Subject {uid}\r\n\r\nhello").into_bytes()),
        ..Default::default()
    }
}

fn sync(uidvalidity: u32, started_at: i64) -> MailSync {
    MailSync {
        uidvalidity,
        started_at,
        ..Default::default()
    }
}

#[test]
fn account_settings_sync_to_other_devices_without_secrets() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    let a = open(tmp.path(), &data, "aaaaaaaaaaaa");
    let acc = a.put_account(account("Me@Example.com")).unwrap();
    assert_eq!(acc.id, ids::account_id("me@example.com"));
    assert_eq!(acc.imap_host, "imap.example.com");

    // 第二台设备打开同一个数据目录，看到同样的账户
    let b = open(tmp.path(), &data, "bbbbbbbbbbbb");
    let accounts = b.accounts().unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].email, "Me@Example.com");
    assert_eq!(accounts[0].name, "Work");

    // 日志里绝不能出现密码之类的字段
    for entry in fs::read_dir(data.join("oplog/aaaaaaaaaaaa")).unwrap() {
        let text = fs::read_to_string(entry.unwrap().path()).unwrap();
        assert!(!text.to_lowercase().contains("password"));
    }

    // 再加一次同一个地址是更新，不是新建
    let mut changed = account("me@example.com");
    changed.name = "Personal".into();
    a.put_account(changed).unwrap();
    assert_eq!(a.accounts().unwrap().len(), 1);
    assert_eq!(a.accounts().unwrap()[0].name, "Personal");
}

#[test]
fn sync_adds_updates_and_removes_mail() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open(tmp.path(), &tmp.path().join("data"), "aaaaaaaaaaaa");
    let acc = store.put_account(account("me@example.com")).unwrap().id;

    let mut first = sync(7, 1_000);
    first.new = vec![
        mail(&acc, 1, 1_000),
        mail(&acc, 2, 2_000),
        mail(&acc, 3, 3_000),
    ];
    first.new[2].read = true;
    let stats = store.apply_mail_sync(&acc, first).unwrap();
    assert_eq!(stats.added, 3);
    assert_eq!(store.mail_uidvalidity(&acc).unwrap(), Some(7));

    let inbox = store.items(&ItemFilter::default(), None, 50).unwrap();
    assert_eq!(inbox.len(), 3);
    assert_eq!(inbox[0].kind, "mail");
    assert_eq!(inbox[0].title, "Subject 3");
    assert_eq!(inbox[0].author.as_deref(), Some("Ada"));
    assert_eq!(inbox[0].source_title, "Work");
    assert_eq!(inbox[0].sender_address.as_deref(), Some("ada@example.com"));
    assert_eq!(store.accounts().unwrap()[0].unread, 2);
    assert!(store.mail_body(&inbox[0].id).unwrap().is_some());

    // 服务器上：1 被别的客户端标为已读并加星，2 被删了
    let mut second = sync(7, 5_000);
    second.flags = vec![(1, true, true), (3, true, false)];
    second.vanished = vec![2];
    let stats = store.apply_mail_sync(&acc, second).unwrap();
    assert_eq!(
        stats,
        postflow_store::MailSyncStats {
            added: 0,
            updated: 1,
            removed: 1
        }
    );
    let inbox = store.items(&ItemFilter::default(), None, 50).unwrap();
    assert_eq!(inbox.len(), 2);
    let one = inbox.iter().find(|i| i.title == "Subject 1").unwrap();
    assert!(one.read && one.starred);
    assert_eq!(store.accounts().unwrap()[0].unread, 0);
}

#[test]
fn local_changes_are_not_overwritten_by_an_older_sync() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open(tmp.path(), &tmp.path().join("data"), "aaaaaaaaaaaa");
    let acc = store.put_account(account("me@example.com")).unwrap().id;
    let mut first = sync(7, 0);
    first.new = vec![mail(&acc, 1, 1_000), mail(&acc, 2, 2_000)];
    store.apply_mail_sync(&acc, first).unwrap();
    let id1 = ids::mail_id(&acc, "m1@example.com");
    let id2 = ids::mail_id(&acc, "m2@example.com");

    // 一次同步开始后，用户在本机标了已读、归档了另一封
    let started = 1;
    let refs = store
        .set_item_state(
            std::slice::from_ref(&id1),
            &ItemPatch {
                read: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].uid, Some(1));
    assert_eq!(refs[0].message_id.as_deref(), Some("m1@example.com"));
    store
        .set_item_state(
            std::slice::from_ref(&id2),
            &ItemPatch {
                archived: Some(true),
                ..Default::default()
            },
        )
        .unwrap();

    // 这次同步看到的是改动前的服务器状态，不能覆盖本机的改动，也不能把归档的那封当成被删了
    let mut stale = sync(7, started);
    stale.flags = vec![(1, false, false)];
    stale.vanished = vec![2];
    store.apply_mail_sync(&acc, stale).unwrap();
    let all = store
        .items(
            &ItemFilter {
                view: View::Inbox,
                ..Default::default()
            },
            None,
            50,
        )
        .unwrap();
    assert!(all.iter().find(|i| i.id == id1).unwrap().read);
    let archived = store
        .items(
            &ItemFilter {
                view: View::Archived,
                ..Default::default()
            },
            None,
            50,
        )
        .unwrap();
    assert_eq!(archived.len(), 1);
    assert_eq!(archived[0].id, id2);
}

#[test]
fn mail_state_stays_local_and_survives_index_rebuild() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    let store = open(tmp.path(), &data, "aaaaaaaaaaaa");
    let acc = store.put_account(account("me@example.com")).unwrap().id;
    let mut first = sync(7, 0);
    first.new = vec![mail(&acc, 1, 1_000)];
    store.apply_mail_sync(&acc, first).unwrap();
    let id = ids::mail_id(&acc, "m1@example.com");

    let feed = store
        .put_feed(NewFeed {
            url: "https://x.example/feed".into(),
            title: "X".into(),
            ..Default::default()
        })
        .unwrap();
    store
        .add_articles(vec![Article {
            id: ids::item_id(&feed.id, "a"),
            feed: feed.id.clone(),
            guid: "a".into(),
            url: None,
            title: "Article".into(),
            author: None,
            published: 5_000,
            fetched: 5_000,
            summary: String::new(),
            content: String::new(),
            image: None,
        }])
        .unwrap();

    // 混在一起的「全部标为已读」：文章进日志，邮件只在本机
    let (count, refs) = store.mark_all_read(&ItemFilter::default()).unwrap();
    assert_eq!(count, 2);
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].id, id);
    let log = fs::read_to_string(
        fs::read_dir(data.join("oplog/aaaaaaaaaaaa"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path(),
    )
    .unwrap();
    assert!(
        !log.contains(&id),
        "mail state must not be written to the synced log"
    );

    // 只看邮件 / 只看文章 / 只看某个账户
    let only_mail = ItemFilter {
        kind: Some(ItemKind::Mail),
        ..Default::default()
    };
    let only_articles = ItemFilter {
        kind: Some(ItemKind::Article),
        ..Default::default()
    };
    let by_account = ItemFilter {
        source_id: Some(acc.clone()),
        ..Default::default()
    };
    assert_eq!(store.items(&only_mail, None, 10).unwrap().len(), 1);
    assert_eq!(store.items(&only_articles, None, 10).unwrap().len(), 1);
    assert_eq!(store.items(&by_account, None, 10).unwrap()[0].id, id);

    // 重建索引：文章从日志恢复，邮件原样保留
    let stats = store.rebuild_index().unwrap();
    assert_eq!(stats.articles, 1);
    let all = store.items(&ItemFilter::default(), None, 10).unwrap();
    assert_eq!(all.len(), 2);
    assert!(all.iter().all(|i| i.read));
    assert!(store.mail_body(&id).unwrap().is_some());
}

#[test]
fn removing_an_account_removes_its_mail_everywhere() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    let a = open(tmp.path(), &data, "aaaaaaaaaaaa");
    let b = open(tmp.path(), &data, "bbbbbbbbbbbb");
    let acc = a.put_account(account("me@example.com")).unwrap().id;
    b.import_all().unwrap();

    // 两台设备各自同步了邮件
    for store in [&a, &b] {
        let mut s = sync(7, 0);
        s.new = vec![mail(&acc, 1, 1_000)];
        store.apply_mail_sync(&acc, s).unwrap();
        assert_eq!(
            store.items(&ItemFilter::default(), None, 10).unwrap().len(),
            1
        );
    }

    a.remove_account(&acc).unwrap();
    assert!(a.accounts().unwrap().is_empty());
    assert!(
        a.items(&ItemFilter::default(), None, 10)
            .unwrap()
            .is_empty()
    );

    // 另一台设备读到移除操作后也清掉
    b.import_all().unwrap();
    assert!(b.accounts().unwrap().is_empty());
    assert!(
        b.items(&ItemFilter::default(), None, 10)
            .unwrap()
            .is_empty()
    );
    assert!(
        b.mail_body(&ids::mail_id(&acc, "m1@example.com"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn uidvalidity_reset_drops_stale_uids() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open(tmp.path(), &tmp.path().join("data"), "aaaaaaaaaaaa");
    let acc = store.put_account(account("me@example.com")).unwrap().id;
    let mut s = sync(7, 0);
    s.new = vec![mail(&acc, 1, 1_000), mail(&acc, 2, 2_000)];
    store.apply_mail_sync(&acc, s).unwrap();
    store
        .set_item_state(
            &[ids::mail_id(&acc, "m2@example.com")],
            &ItemPatch {
                archived: Some(true),
                ..Default::default()
            },
        )
        .unwrap();

    store.reset_inbox(&acc).unwrap();
    assert!(store.known_mail(&acc).unwrap().iter().all(|m| m.archived));
    assert_eq!(
        store.items(&ItemFilter::default(), None, 10).unwrap().len(),
        0
    );
    assert_eq!(
        store
            .items(
                &ItemFilter {
                    view: View::Archived,
                    ..Default::default()
                },
                None,
                10
            )
            .unwrap()
            .len(),
        1
    );
}
