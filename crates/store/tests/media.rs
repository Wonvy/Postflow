//! 播客和视频：附件存进索引、按内容判断订阅源的类型、用户改类型、按类型筛选，以及给早先入库的单集补附件。

use shibapost_store::{Article, FeedPatch, ItemFilter, Media, NewFeed, Store, View, ids};
use tempfile::TempDir;

fn open(tmp: &TempDir) -> Store {
    Store::open(
        &tmp.path().join("data"),
        &tmp.path().join("index.db"),
        "aaaaaaaaaaaa",
    )
    .unwrap()
}

fn feed(store: &Store, url: &str) -> String {
    store
        .put_feed(NewFeed {
            url: url.into(),
            title: url.into(),
            ..Default::default()
        })
        .unwrap()
        .id
}

fn episode(feed: &str, n: i64, media: Option<Media>) -> Article {
    Article {
        id: ids::item_id(feed, &n.to_string()),
        feed: feed.into(),
        guid: n.to_string(),
        url: None,
        title: format!("Episode {n}"),
        author: None,
        published: 1_000 * n,
        fetched: 1_000 * n,
        summary: String::new(),
        content: String::new(),
        image: None,
        media,
    }
}

fn audio(n: i64) -> Option<Media> {
    Some(Media {
        url: format!("https://cdn.example.com/{n}.mp3"),
        mime: "audio/mpeg".into(),
        duration: Some(60 * n as u64),
        size: None,
        chapters: None,
        transcript: None,
    })
}

fn kind_of(store: &Store, id: &str) -> (String, Option<String>) {
    let f = store
        .feeds()
        .unwrap()
        .into_iter()
        .find(|f| f.id == id)
        .unwrap();
    (f.kind, f.kind_set)
}

fn titles(store: &Store, feed_kind: &str) -> Vec<String> {
    let filter = ItemFilter {
        view: View::All,
        feed_kind: Some(feed_kind.into()),
        ..Default::default()
    };
    store
        .items(&filter, None, 50)
        .unwrap()
        .into_iter()
        .map(|i| i.title)
        .collect()
}

#[test]
fn feeds_are_sorted_into_articles_videos_and_podcasts() {
    let tmp = TempDir::new().unwrap();
    let store = open(&tmp);
    let blog = feed(&store, "https://blog.example.com/feed");
    let show = feed(&store, "https://show.example.com/podcast.xml");
    let tube = feed(
        &store,
        "https://www.youtube.com/feeds/videos.xml?channel_id=UC123",
    );

    store
        .add_articles(vec![
            episode(&blog, 1, None),
            episode(&show, 2, audio(2)),
            episode(&show, 3, audio(3)),
            episode(&tube, 4, None),
        ])
        .unwrap();

    assert_eq!(kind_of(&store, &blog), ("article".into(), None));
    assert_eq!(kind_of(&store, &show), ("podcast".into(), None));
    assert_eq!(kind_of(&store, &tube), ("video".into(), None));

    // 附件跟着条目一起出来
    let list = store
        .items(
            &ItemFilter {
                view: View::All,
                source_id: Some(show.clone()),
                ..Default::default()
            },
            None,
            10,
        )
        .unwrap();
    assert_eq!(list[0].media, audio(3));

    assert_eq!(titles(&store, "podcast"), vec!["Episode 3", "Episode 2"]);
    assert_eq!(titles(&store, "article"), vec!["Episode 1"]);
    assert_eq!(titles(&store, "video"), vec!["Episode 4"]);

    // 用户把博客改成视频，再改回自动
    store
        .update_feed(
            &blog,
            FeedPatch {
                kind: Some(Some("video".into())),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        kind_of(&store, &blog),
        ("video".into(), Some("video".into()))
    );
    assert_eq!(titles(&store, "video"), vec!["Episode 4", "Episode 1"]);
    store
        .update_feed(
            &blog,
            FeedPatch {
                kind: Some(None),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(kind_of(&store, &blog), ("article".into(), None));
    // 不认识的类型当作自动
    store
        .update_feed(
            &blog,
            FeedPatch {
                kind: Some(Some("music".into())),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(kind_of(&store, &blog), ("article".into(), None));
}

#[test]
fn old_episodes_get_their_audio_on_the_next_fetch() {
    let tmp = TempDir::new().unwrap();
    let store = open(&tmp);
    let show = feed(&store, "https://show.example.com/podcast.xml");
    // 以前的版本入库时没有附件
    store.add_articles(vec![episode(&show, 1, None)]).unwrap();
    assert_eq!(kind_of(&store, &show).0, "article");

    // 再抓到同一条，这次带着附件：补上，但不算新文章
    let added = store
        .add_articles(vec![episode(&show, 1, audio(1))])
        .unwrap();
    assert_eq!(added, 0);
    assert_eq!(kind_of(&store, &show).0, "podcast");

    // 重新打开（从同步日志重建索引）也一样
    drop(store);
    std::fs::remove_file(tmp.path().join("index.db")).unwrap();
    let store = open(&tmp);
    assert_eq!(kind_of(&store, &show).0, "podcast");
    let item = store.item(&ids::item_id(&show, "1")).unwrap().unwrap();
    assert_eq!(item.item.media, audio(1));
}

#[test]
fn feeds_can_always_open_full_text() {
    let tmp = TempDir::new().unwrap();
    let store = open(&tmp);
    let blog = feed(&store, "https://blog.example.com/feed");
    let full = |store: &Store| {
        store
            .feeds()
            .unwrap()
            .into_iter()
            .find(|f| f.id == blog)
            .unwrap()
            .full_text
    };
    assert!(!full(&store));
    store
        .update_feed(
            &blog,
            FeedPatch {
                full_text: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(full(&store));
    // 同步日志里有，重建索引后还在
    drop(store);
    std::fs::remove_file(tmp.path().join("index.db")).unwrap();
    let store = open(&tmp);
    assert!(full(&store));
}

#[test]
fn chapters_are_added_once_but_new_links_alone_are_not() {
    let tmp = TempDir::new().unwrap();
    let store = open(&tmp);
    let show = feed(&store, "https://show.example.com/podcast.xml");
    store
        .add_articles(vec![episode(&show, 1, audio(1))])
        .unwrap();
    let item = |store: &Store| {
        store
            .item(&ids::item_id(&show, "1"))
            .unwrap()
            .unwrap()
            .item
            .media
            .unwrap()
    };

    // 只是下载地址变了（带签名）：不记
    let mut signed = audio(1).unwrap();
    signed.url = "https://cdn.example.com/1.mp3?sig=abc".into();
    let mut again = episode(&show, 1, Some(signed));
    store.add_articles(vec![again.clone()]).unwrap();
    assert_eq!(item(&store).url, "https://cdn.example.com/1.mp3");

    // 这次带了章节：补上
    if let Some(m) = again.media.as_mut() {
        m.chapters = Some("https://show.example.com/1.json".into());
    }
    store.add_articles(vec![again]).unwrap();
    assert_eq!(
        item(&store).chapters.as_deref(),
        Some("https://show.example.com/1.json")
    );
}
