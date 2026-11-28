//! 订阅统计：更新频率、内容完整度、阅读记录按源和按天汇总。

use shibapost_store::{Article, NewFeed, Store, ids};
use tempfile::TempDir;

const DAY: i64 = 86_400_000;
const NOW: i64 = 1_800_000_000_000;

fn store() -> (TempDir, Store) {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(
        &tmp.path().join("data"),
        &tmp.path().join("index.db"),
        "aaaaaaaaaaaa",
    )
    .unwrap();
    (tmp, store)
}

fn feed(store: &Store, title: &str) -> String {
    store
        .put_feed(NewFeed {
            url: format!("https://{title}.example/feed"),
            title: title.into(),
            ..Default::default()
        })
        .unwrap()
        .id
}

/// 在 `days_ago` 天前发的一篇，正文长 `len` 个字节。
fn article(feed: &str, n: usize, days_ago: i64, len: usize) -> Article {
    Article {
        id: ids::item_id(feed, &n.to_string()),
        feed: feed.into(),
        guid: n.to_string(),
        url: None,
        title: format!("Post {n}"),
        author: None,
        published: NOW - days_ago * DAY,
        fetched: NOW,
        summary: String::new(),
        content: "x".repeat(len),
        image: None,
        media: None,
    }
}

#[test]
fn per_feed_frequency_completeness_and_reading() {
    let (_tmp, store) = store();
    let busy = feed(&store, "busy");
    let quiet = feed(&store, "quiet");
    // busy：最近 14 天里每天一篇全文；quiet：40 天前一篇摘要
    let mut articles: Vec<Article> = (0..14)
        .map(|d| article(&busy, d as usize, d, 3000))
        .collect();
    articles.push(article(&quiet, 100, 40, 200));
    store.add_articles(articles).unwrap();

    let id = |n: usize| ids::item_id(&busy, &n.to_string());
    // 第 1 篇看了两次，第二次看完；第 2 篇只看了一半
    assert!(
        store
            .record_reading(&id(1), NOW - DAY, 30_000, 0.4, false)
            .unwrap()
    );
    assert!(
        store
            .record_reading(&id(1), NOW - 1000, 20_000, 1.0, true)
            .unwrap()
    );
    assert!(
        store
            .record_reading(&id(2), NOW - 1000, 10_000, 0.5, false)
            .unwrap()
    );
    assert!(!store.record_reading("nope", NOW, 1000, 1.0, true).unwrap());

    let stats = store.feed_stats(NOW, 30, 0).unwrap();
    let busy_stats = stats.feeds.iter().find(|f| f.id == busy).unwrap();
    assert_eq!(busy_stats.total, 14);
    assert_eq!(busy_stats.published, 14);
    assert_eq!(busy_stats.weekly.len(), 5);
    assert_eq!(busy_stats.weekly.iter().sum::<i64>(), 14);
    assert_eq!(
        busy_stats.weekly[4], 7,
        "the last week holds the newest seven"
    );
    assert_eq!(busy_stats.full_share, Some(1.0));
    assert_eq!(busy_stats.opened, 2);
    assert_eq!(busy_stats.finished, 1);
    assert_eq!(busy_stats.avg_depth, Some(0.75));
    assert_eq!(busy_stats.dwell_ms, 60_000);

    let quiet_stats = stats.feeds.iter().find(|f| f.id == quiet).unwrap();
    assert_eq!(quiet_stats.total, 1);
    assert_eq!(quiet_stats.published, 0);
    assert_eq!(quiet_stats.full_share, Some(0.0));
    assert_eq!(quiet_stats.opened, 0);
    assert_eq!(quiet_stats.avg_depth, None);

    // 按天：31 天（含今天），今天打开了两篇
    assert_eq!(stats.days.len(), 31);
    let today = stats.days.last().unwrap();
    assert_eq!((today.opened, today.dwell_ms), (2, 30_000));
    assert_eq!(stats.days.iter().map(|d| d.published).sum::<i64>(), 14);
    assert_eq!(stats.tracking_since, Some(NOW - DAY));

    store.clear_reading().unwrap();
    let cleared = store.feed_stats(NOW, 30, 0).unwrap();
    assert_eq!(cleared.tracking_since, None);
    assert!(
        cleared
            .feeds
            .iter()
            .all(|f| f.opened == 0 && f.dwell_ms == 0)
    );
}

#[test]
fn days_follow_the_local_time_zone() {
    let (_tmp, store) = store();
    let f = feed(&store, "tz");
    store.add_articles(vec![article(&f, 1, 0, 10)]).unwrap();
    let id = ids::item_id(&f, "1");
    // 东八区：UTC 前一天 20:00 读的，本地是当天 04:00
    let local_midnight = (NOW + 8 * 3_600_000).div_euclid(DAY) * DAY - 8 * 3_600_000;
    store
        .record_reading(&id, local_midnight + 4 * 3_600_000, 5000, 1.0, true)
        .unwrap();
    let stats = store.feed_stats(NOW, 7, 8 * 3_600_000).unwrap();
    assert_eq!(stats.days.last().unwrap().opened, 1);
}
