//! 没回复就提醒：记在同步日志里，到时间的能找出来；对方的回信能查到；
//! 到时间、标成未读的「已发送」邮件出现在「提醒」里。

use shibapost_store::{
    AccountConfig, FollowUp, ItemFilter, ItemKind, ItemPatch, MailFolder, MailSync, NewMail, Store,
    View, ids,
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

fn mail(
    account: &str,
    key: &str,
    message_id: &str,
    subject: &str,
    from: &str,
    date: i64,
) -> NewMail {
    NewMail {
        id: ids::mail_id(account, key),
        uid: date as u32,
        message_id: Some(message_id.into()),
        subject: subject.into(),
        from_name: None,
        from_address: Some(from.into()),
        snippet: String::new(),
        date,
        read: true,
        starred: false,
        size: 10,
        has_attachments: false,
        list_unsubscribe: None,
        raw: None,
        recipients: vec![],
        code: None,
    }
}

#[test]
fn follow_ups_come_due_and_show_in_reminders() {
    let (_tmp, store) = store();
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
    let sent_at = 1_000;
    store
        .apply_folder_sync(
            &account.id,
            MailFolder::Sent,
            "Sent",
            MailSync {
                uidvalidity: 1,
                started_at: 0,
                new: vec![mail(
                    &account.id,
                    "sent:abc@example.com",
                    "abc@example.com",
                    "Contract",
                    "me@example.com",
                    sent_at,
                )],
                flags: vec![],
                vanished: vec![],
            },
        )
        .unwrap();
    let now = shibapost_store::hlc::now_ms() as i64;
    let follow = FollowUp {
        message_id: "abc@example.com".into(),
        account_id: account.id.clone(),
        subject: "Contract".into(),
        to: vec!["lee@example.com".into()],
        sent_at,
        until: now - 1000,
    };
    store.set_follow_up(&follow).unwrap();
    assert_eq!(
        store.due_follow_ups(now - 5000, now).unwrap(),
        vec![follow.clone()]
    );
    assert!(store.due_follow_ups(now - 500, now).unwrap().is_empty());

    // 回信：从收件人那边来、在发信之后
    assert!(
        store
            .titles_from_since(&follow.to, sent_at)
            .unwrap()
            .is_empty()
    );
    store
        .apply_mail_sync(
            &account.id,
            MailSync {
                uidvalidity: 1,
                started_at: 0,
                new: vec![mail(
                    &account.id,
                    "reply@x",
                    "reply@x",
                    "Re: Contract",
                    "Lee@Example.com",
                    2_000,
                )],
                flags: vec![],
                vanished: vec![],
            },
        )
        .unwrap();
    assert_eq!(
        store.titles_from_since(&follow.to, sent_at).unwrap(),
        vec!["Re: Contract"]
    );

    // 没回复时提醒检查把「已发送」里那封标成未读，它就出现在「提醒」里
    let reminders = |s: &Store| {
        s.items(
            &ItemFilter {
                view: View::Reminders,
                kind: Some(ItemKind::Mail),
                ..Default::default()
            },
            None,
            10,
        )
        .unwrap()
        .into_iter()
        .map(|i| i.title)
        .collect::<Vec<_>>()
    };
    assert!(reminders(&store).is_empty());
    let sent_id = ids::mail_id(&account.id, "sent:abc@example.com");
    store
        .set_item_state(
            &[sent_id],
            &ItemPatch {
                read: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(reminders(&store), vec!["Contract"]);

    // 还没到时间的在「等回复」里
    let waiting = |s: &Store| {
        s.items(
            &ItemFilter {
                view: View::Waiting,
                ..Default::default()
            },
            None,
            10,
        )
        .unwrap()
        .len()
    };
    assert_eq!(waiting(&store), 0);
    store
        .set_follow_up(&FollowUp {
            until: now + 86_400_000,
            ..follow.clone()
        })
        .unwrap();
    assert_eq!(waiting(&store), 1);
    assert_eq!(store.counts(0).unwrap().waiting, 1);
    assert_eq!(
        store.follow_up("abc@example.com").unwrap().map(|f| f.until),
        Some(now + 86_400_000)
    );

    // 只取了信头的大邮件：从邮件结构认出来的附件先列在附件总览里，标着没下载
    let sent_id = ids::mail_id(&account.id, "sent:abc@example.com");
    store
        .set_structure_attachments(
            &sent_id,
            &[shibapost_store::AttachmentMeta {
                index: 0,
                name: "contract.pdf".into(),
                mime: "application/pdf".into(),
                size: 3_000_000,
            }],
        )
        .unwrap();
    let overview = store.attachments(None, 10).unwrap();
    assert_eq!(overview.files.len(), 1);
    assert!(!overview.files[0].downloaded);
    assert_eq!(overview.files[0].index, shibapost_store::STRUCTURE_INDEX);
    assert_eq!(
        store
            .structure_attachment_name(&sent_id, shibapost_store::STRUCTURE_INDEX)
            .unwrap()
            .as_deref(),
        Some("contract.pdf")
    );

    // 取消以后不再出现
    store.cancel_follow_up("abc@example.com").unwrap();
    assert!(reminders(&store).is_empty());
    assert_eq!(waiting(&store), 0);
    assert_eq!(store.follow_up("abc@example.com").unwrap(), None);
}
