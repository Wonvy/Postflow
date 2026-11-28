//! 稍后提醒：没到时间时从收件箱里藏起来，到了时间回来；提醒时间同步到其他设备。

use shibapost_store::{Article, ItemFilter, NewFeed, Store, View, ids};
use tempfile::TempDir;

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

fn open(tmp: &TempDir, device: &str) -> Store {
    Store::open(
        &tmp.path().join("data"),
        &tmp.path().join(format!("{device}.db")),
        device,
    )
    .unwrap()
}

fn setup() -> (TempDir, Store, Vec<String>) {
    let tmp = TempDir::new().unwrap();
    let store = open(&tmp, "aaaaaaaaaaaa");
    let feed = store
        .put_feed(NewFeed {
            url: "https://x.example/feed".into(),
            title: "X".into(),
            ..Default::default()
        })
        .unwrap();
    let articles: Vec<Article> = (1..=3)
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

fn listed(store: &Store, view: View) -> Vec<String> {
    let filter = ItemFilter {
        view,
        ..Default::default()
    };
    let mut ids: Vec<String> = store
        .items(&filter, None, 50)
        .unwrap()
        .into_iter()
        .map(|i| i.id)
        .collect();
    ids.sort();
    ids
}

#[test]
fn snoozed_items_leave_the_inbox_until_their_time() {
    let (_tmp, store, ids) = setup();
    let later = now() + 3_600_000;
    store.set_snooze(&ids[..1], Some(later)).unwrap();

    let inbox = listed(&store, View::Inbox);
    assert!(!inbox.contains(&ids[0]));
    assert_eq!(inbox.len(), 2);
    assert!(!listed(&store, View::Unread).contains(&ids[0]));
    assert_eq!(listed(&store, View::Snoozed), vec![ids[0].clone()]);
    assert_eq!(
        store.counts(0).unwrap().unread,
        2,
        "the badge doesn't count it either"
    );
    let feed = &store.feeds().unwrap()[0];
    assert_eq!(feed.unread, 2, "nor does the feed's count in the sidebar");
    let snoozed = store
        .items(
            &ItemFilter {
                view: View::Snoozed,
                ..Default::default()
            },
            None,
            5,
        )
        .unwrap();
    assert_eq!(snoozed[0].snoozed_until, Some(later));
    // 打开这一条（读正文）也带着提醒时间
    let detail = store.item(&ids[0]).unwrap().unwrap();
    assert_eq!(detail.item.snoozed_until, Some(later));

    // 取消：马上回来
    store.set_snooze(&ids[..1], None).unwrap();
    assert_eq!(listed(&store, View::Inbox).len(), 3);
    assert!(listed(&store, View::Snoozed).is_empty());
}

#[test]
fn reminders_that_came_due_are_listed_and_found() {
    let (_tmp, store, ids) = setup();
    let past = now() - 60_000;
    store.set_snooze(&ids[1..2], Some(past)).unwrap();
    // 时间已经到了：回到收件箱，也在「提醒」里
    assert!(listed(&store, View::Inbox).contains(&ids[1]));
    assert_eq!(listed(&store, View::Reminders), vec![ids[1].clone()]);

    let due = store.due_reminders(past - 1, now()).unwrap();
    assert_eq!(
        due.iter().map(|i| i.id.clone()).collect::<Vec<_>>(),
        vec![ids[1].clone()]
    );
    assert!(
        store.due_reminders(past, now()).unwrap().is_empty(),
        "already reported"
    );
}

#[test]
fn snoozes_sync_to_other_devices() {
    let (tmp, store, ids) = setup();
    store
        .set_snooze(&ids[2..], Some(now() + 3_600_000))
        .unwrap();
    let other = open(&tmp, "bbbbbbbbbbbb");
    assert_eq!(listed(&other, View::Snoozed), vec![ids[2].clone()]);
    assert!(!listed(&other, View::Inbox).contains(&ids[2]));
}
