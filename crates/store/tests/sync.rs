//! 模拟两台设备各有一份数据目录，由文件同步工具在两边之间拷贝文件。

use std::fs;
use std::io::Write;
use std::path::Path;

use shibapost_store::{Article, FeedPatch, ItemFilter, ItemPatch, NewFeed, Store, View, ids};
use tempfile::TempDir;

struct Device {
    _tmp: TempDir,
    data: std::path::PathBuf,
    index: std::path::PathBuf,
    id: &'static str,
    store: Store,
}

fn device(id: &'static str) -> Device {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    let index = tmp.path().join("local/index.db");
    let store = Store::open(&data, &index, id).unwrap();
    Device {
        _tmp: tmp,
        data,
        index,
        id,
        store,
    }
}

/// 把 `from` 里比 `to` 新的文件拷过去，等价于文件同步工具做的事。
/// 每个文件只有一台设备在追加，所以「更长的那份」就是更新的那份。
fn copy_newer(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let dest = to.join(path.file_name().unwrap());
        if path.is_dir() {
            fs::create_dir_all(&dest).unwrap();
            copy_newer(&path, &dest);
        } else {
            let src_len = fs::metadata(&path).unwrap().len();
            let dst_len = fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
            if src_len > dst_len {
                fs::copy(&path, &dest).unwrap();
            }
        }
    }
}

fn sync(a: &Device, b: &Device) {
    copy_newer(&a.data, &b.data);
    copy_newer(&b.data, &a.data);
    a.store.import_all().unwrap();
    b.store.import_all().unwrap();
}

fn article(feed_id: &str, guid: &str, published: i64) -> Article {
    Article {
        id: ids::item_id(feed_id, guid),
        feed: feed_id.to_string(),
        guid: guid.to_string(),
        url: Some(format!("https://example.com/{guid}")),
        title: format!("Post {guid}"),
        author: None,
        published,
        fetched: published,
        summary: "summary".into(),
        content: "<p>content</p>".into(),
        image: None,
        media: None,
    }
}

fn subscribe(dev: &Device, url: &str) -> String {
    dev.store
        .put_feed(NewFeed {
            url: url.into(),
            title: "Example".into(),
            ..Default::default()
        })
        .unwrap()
        .id
}

fn inbox(dev: &Device) -> Vec<shibapost_store::ItemSummary> {
    dev.store.items(&ItemFilter::default(), None, 100).unwrap()
}

#[test]
fn subscriptions_articles_and_state_reach_the_other_device() {
    let a = device("aaaaaaaaaaaa");
    let b = device("bbbbbbbbbbbb");

    let feed = subscribe(&a, "https://example.com/feed.xml");
    let added = a
        .store
        .add_articles(vec![article(&feed, "1", 1_000), article(&feed, "2", 2_000)])
        .unwrap();
    assert_eq!(added, 2);
    assert!(b.store.feeds().unwrap().is_empty());

    sync(&a, &b);
    let feeds = b.store.feeds().unwrap();
    assert_eq!(feeds.len(), 1);
    assert_eq!(feeds[0].unread, 2);
    assert_eq!(inbox(&b).len(), 2);

    // B 上读一篇、星标一篇，同步回 A
    let first = ids::item_id(&feed, "1");
    let second = ids::item_id(&feed, "2");
    b.store
        .set_item_state(
            std::slice::from_ref(&first),
            &ItemPatch {
                read: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
    b.store
        .set_item_state(
            std::slice::from_ref(&second),
            &ItemPatch {
                starred: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
    sync(&a, &b);

    let items = inbox(&a);
    let get = |id: &str| items.iter().find(|i| i.id == id).unwrap();
    assert!(get(&first).read && !get(&first).starred);
    assert!(!get(&second).read && get(&second).starred);
    assert_eq!(a.store.feeds().unwrap()[0].unread, 1);
    assert_eq!(a.store.counts(0).unwrap().starred, 1);
}

#[test]
fn both_devices_fetching_the_same_article_do_not_duplicate_it() {
    let a = device("aaaaaaaaaaaa");
    let b = device("bbbbbbbbbbbb");
    let feed = subscribe(&a, "https://example.com/feed.xml");
    sync(&a, &b);

    // 两边在同步之前各自抓到了同一篇
    assert_eq!(
        a.store
            .add_articles(vec![article(&feed, "1", 1_000)])
            .unwrap(),
        1
    );
    assert_eq!(
        b.store
            .add_articles(vec![article(&feed, "1", 1_000)])
            .unwrap(),
        1
    );
    sync(&a, &b);
    assert_eq!(inbox(&a).len(), 1);
    assert_eq!(inbox(&b).len(), 1);

    // 同步之后再抓到就不会重复写入存档
    assert_eq!(
        a.store
            .add_articles(vec![article(&feed, "1", 1_000)])
            .unwrap(),
        0
    );
}

#[test]
fn concurrent_edits_converge_to_the_latest_write_per_field() {
    let a = device("aaaaaaaaaaaa");
    let b = device("bbbbbbbbbbbb");
    let feed = subscribe(&a, "https://example.com/feed.xml");
    sync(&a, &b);

    // 断网期间：A 改标题，B 改文件夹，随后 B 又改了标题
    a.store
        .update_feed(
            &feed,
            FeedPatch {
                title: Some("From A".into()),
                ..Default::default()
            },
        )
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    b.store
        .update_feed(
            &feed,
            FeedPatch {
                folder: Some("Tech".into()),
                ..Default::default()
            },
        )
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    b.store
        .update_feed(
            &feed,
            FeedPatch {
                title: Some("From B".into()),
                ..Default::default()
            },
        )
        .unwrap();

    sync(&a, &b);
    for dev in [&a, &b] {
        let f = &dev.store.feeds().unwrap()[0];
        assert_eq!(f.title, "From B", "device {}", dev.id);
        assert_eq!(f.folder, "Tech", "device {}", dev.id);
    }
}

#[test]
fn state_that_arrives_before_its_article_is_applied_when_the_article_shows_up() {
    let a = device("aaaaaaaaaaaa");
    let b = device("bbbbbbbbbbbb");
    let feed = subscribe(&a, "https://example.com/feed.xml");
    a.store
        .add_articles(vec![article(&feed, "1", 1_000)])
        .unwrap();
    let id = ids::item_id(&feed, "1");
    a.store
        .set_item_state(
            std::slice::from_ref(&id),
            &ItemPatch {
                read: Some(true),
                ..Default::default()
            },
        )
        .unwrap();

    // 同步工具先传完了操作日志，文章存档还在路上
    copy_newer(&a.data.join("oplog"), &b.data.join("oplog"));
    b.store.import_all().unwrap();
    assert_eq!(b.store.feeds().unwrap().len(), 1);
    assert!(inbox(&b).is_empty());

    copy_newer(&a.data.join("articles"), &b.data.join("articles"));
    b.store.import_all().unwrap();
    let items = inbox(&b);
    assert_eq!(items.len(), 1);
    assert!(items[0].read);
}

#[test]
fn rebuilding_the_index_reproduces_the_same_state() {
    let a = device("aaaaaaaaaaaa");
    let feed = subscribe(&a, "https://example.com/feed.xml");
    a.store
        .add_articles(
            (0..50)
                .map(|n| article(&feed, &n.to_string(), n * 1_000))
                .collect(),
        )
        .unwrap();
    assert_eq!(
        a.store
            .mark_all_read(&ItemFilter {
                view: View::Inbox,
                ..Default::default()
            })
            .unwrap()
            .0,
        50
    );
    let starred = ids::item_id(&feed, "7");
    a.store
        .set_item_state(
            std::slice::from_ref(&starred),
            &ItemPatch {
                starred: Some(true),
                read: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
    a.store
        .set_setting("refresh_interval_min", serde_json::json!(15))
        .unwrap();
    a.store.set_device_name("MacBook").unwrap();

    let snapshot = |dev: &Device| {
        let items: Vec<_> = inbox(dev)
            .into_iter()
            .map(|i| (i.id, i.read, i.starred, i.archived))
            .collect();
        let feeds: Vec<_> = dev
            .store
            .feeds()
            .unwrap()
            .into_iter()
            .map(|f| (f.id, f.title, f.unread))
            .collect();
        (
            items,
            feeds,
            dev.store.settings().unwrap(),
            dev.store.devices().unwrap().len(),
        )
    };
    let before = snapshot(&a);
    assert_eq!(before.1[0].2, 1);

    let stats = a.store.rebuild_index().unwrap();
    assert_eq!(stats.articles, 50);
    assert_eq!(snapshot(&a), before);

    // 把索引文件整个删掉重开，结果也一样
    drop(a.store);
    fs::remove_file(&a.index).unwrap();
    let reopened = Device {
        store: Store::open(&a.data, &a.index, a.id).unwrap(),
        ..a
    };
    assert_eq!(snapshot(&reopened), before);
}

#[test]
fn a_new_device_pointed_at_an_existing_data_dir_gets_everything() {
    let a = device("aaaaaaaaaaaa");
    let feed = subscribe(&a, "https://example.com/feed.xml");
    a.store
        .add_articles(vec![article(&feed, "1", 1_000)])
        .unwrap();

    let tmp = tempfile::tempdir().unwrap();
    let c = Store::open(&a.data, &tmp.path().join("index.db"), "cccccccccccc").unwrap();
    assert_eq!(c.feeds().unwrap().len(), 1);
    assert_eq!(c.items(&ItemFilter::default(), None, 10).unwrap().len(), 1);
}

#[test]
fn conflict_copies_and_half_synced_files_are_handled() {
    let a = device("aaaaaaaaaaaa");
    let b = device("bbbbbbbbbbbb");
    let feed = subscribe(&a, "https://example.com/feed.xml");
    a.store
        .add_articles(vec![article(&feed, "1", 1_000), article(&feed, "2", 2_000)])
        .unwrap();
    sync(&a, &b);

    // B 标记已读，但文件只同步了一半到 A
    b.store.mark_all_read(&ItemFilter::default()).unwrap();
    let b_log_dir = b.data.join("oplog").join(b.id);
    let log = fs::read_dir(&b_log_dir)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let bytes = fs::read(&log).unwrap();
    let a_copy_dir = a.data.join("oplog").join(b.id);
    fs::create_dir_all(&a_copy_dir).unwrap();
    let a_copy = a_copy_dir.join(log.file_name().unwrap());
    fs::write(&a_copy, &bytes[..bytes.len() - 10]).unwrap();
    a.store.import_all().unwrap();
    assert_eq!(
        a.store.feeds().unwrap()[0].unread,
        2,
        "half a line must not be applied"
    );

    // 剩下的部分到了
    fs::write(&a_copy, &bytes).unwrap();
    a.store.import_all().unwrap();
    assert_eq!(a.store.feeds().unwrap()[0].unread, 0);

    // 同步工具生成了一个冲突副本，内容是同一批操作外加一行垃圾
    let conflict = a_copy_dir.join("2026-01.sync-conflict-20260101-000000-ABCDEFG.jsonl");
    let mut f = fs::File::create(&conflict).unwrap();
    f.write_all(&bytes).unwrap();
    f.write_all(b"this is not json\n").unwrap();
    let stats = a.store.import_all().unwrap();
    assert_eq!(stats.skipped, 1);
    assert_eq!(stats.ops, 0, "replaying the same ops must change nothing");
    assert_eq!(a.store.feeds().unwrap()[0].unread, 0);
}

#[test]
fn unsubscribing_hides_the_feed_everywhere_but_keeps_starred_items() {
    let a = device("aaaaaaaaaaaa");
    let b = device("bbbbbbbbbbbb");
    let feed = subscribe(&a, "https://example.com/feed.xml");
    a.store
        .add_articles(vec![article(&feed, "1", 1_000), article(&feed, "2", 2_000)])
        .unwrap();
    let kept = ids::item_id(&feed, "2");
    a.store
        .set_item_state(
            std::slice::from_ref(&kept),
            &ItemPatch {
                starred: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
    a.store.remove_feed(&feed).unwrap();
    sync(&a, &b);

    assert!(b.store.feeds().unwrap().is_empty());
    assert!(inbox(&b).is_empty());
    let starred = b
        .store
        .items(
            &ItemFilter {
                view: View::Starred,
                ..Default::default()
            },
            None,
            10,
        )
        .unwrap();
    assert_eq!(starred.len(), 1);
    assert_eq!(starred[0].id, kept);

    // 重新订阅后文章还在
    subscribe(&b, "https://example.com/feed.xml");
    assert_eq!(inbox(&b).len(), 2);
}

#[test]
fn pagination_walks_every_item_once() {
    let a = device("aaaaaaaaaaaa");
    let feed = subscribe(&a, "https://example.com/feed.xml");
    // 故意让很多条目时间相同，检验游标在并列时不丢不重
    a.store
        .add_articles(
            (0..25)
                .map(|n| article(&feed, &n.to_string(), (n / 5) * 1_000))
                .collect(),
        )
        .unwrap();

    let mut seen = Vec::new();
    let mut cursor = None;
    loop {
        let page = a
            .store
            .items(&ItemFilter::default(), cursor.as_ref(), 7)
            .unwrap();
        if page.is_empty() {
            break;
        }
        let last = page.last().unwrap();
        cursor = Some(shibapost_store::Cursor {
            published_at: last.published_at,
            id: last.id.clone(),
        });
        seen.extend(page.into_iter().map(|i| i.id));
    }
    assert_eq!(seen.len(), 25);
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 25);
}

#[test]
fn changing_a_feed_address_keeps_its_id_and_syncs() {
    let a = device("aaaaaaaaaaaa");
    let b = device("bbbbbbbbbbbb");
    let feed = subscribe(&a, "https://example.com/old.xml");
    sync(&a, &b);

    a.store
        .update_feed(
            &feed,
            FeedPatch {
                url: Some(" https://example.com/new.xml ".into()),
                ..Default::default()
            },
        )
        .unwrap();
    sync(&a, &b);
    for dev in [&a, &b] {
        let feeds = dev.store.feeds().unwrap();
        assert_eq!(feeds.len(), 1, "device {}", dev.id);
        assert_eq!(feeds[0].id, feed, "device {}", dev.id);
        assert_eq!(
            feeds[0].url, "https://example.com/new.xml",
            "device {}",
            dev.id
        );
    }
}
