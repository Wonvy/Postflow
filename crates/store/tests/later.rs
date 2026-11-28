//! 稍后读：存下的网页只在「稍后读」里，不混进订阅的文章流，订阅源列表和刷新也看不到它。

use shibapost_store::{Article, ItemFilter, ItemKind, NewFeed, Store, View, ids, later_feed_id};

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

fn article(feed_id: &str, url: &str, published: i64) -> Article {
    Article {
        id: ids::item_id(feed_id, url),
        feed: feed_id.to_string(),
        guid: url.to_string(),
        url: Some(url.to_string()),
        title: format!("Title of {url}"),
        author: None,
        published,
        fetched: published,
        summary: String::new(),
        content: String::new(),
        image: None,
        media: None,
    }
}

fn titles(store: &Store, filter: ItemFilter) -> Vec<String> {
    store
        .items(&filter, None, 50)
        .unwrap()
        .into_iter()
        .map(|i| i.title)
        .collect()
}

#[test]
fn saved_pages_stay_in_read_later() {
    let (_tmp, store) = store();
    let feed = store
        .put_feed(NewFeed {
            url: "https://example.com/feed.xml".into(),
            title: "Example".into(),
            ..Default::default()
        })
        .unwrap();
    let later = store.ensure_later_feed().unwrap();
    assert_eq!(later, later_feed_id());
    // 第二次不会再建一个
    assert_eq!(store.ensure_later_feed().unwrap(), later);
    store
        .add_articles(vec![
            article(&feed.id, "https://example.com/post", 1_000),
            article(&later, "https://www.blog.example/saved?x=1", 2_000),
        ])
        .unwrap();

    let articles = |view| ItemFilter {
        view,
        kind: Some(ItemKind::Article),
        ..Default::default()
    };
    assert_eq!(
        titles(&store, articles(View::Inbox)),
        ["Title of https://example.com/post"]
    );
    assert_eq!(titles(&store, articles(View::Unread)).len(), 1);
    assert_eq!(
        titles(&store, articles(View::Later)),
        ["Title of https://www.blog.example/saved?x=1"]
    );
    // 搜索「全部」和星标照样找得到
    assert_eq!(titles(&store, articles(View::All)).len(), 2);

    // 来源显示网站的域名，没有订阅源的图标
    let saved = &store.items(&articles(View::Later), None, 1).unwrap()[0];
    assert_eq!(saved.source_title, "blog.example");
    assert_eq!(saved.source_icon, None);

    // 不在订阅源列表、不刷新，未读数单独算
    assert!(store.feeds().unwrap().iter().all(|f| f.id != later));
    assert!(
        store
            .all_fetch_targets()
            .unwrap()
            .iter()
            .all(|t| t.feed_id != later)
    );
    let counts = store.counts(0).unwrap();
    assert_eq!((counts.unread_article, counts.later), (1, 1));
}

#[test]
fn highlights_are_kept_per_article() {
    use shibapost_store::Highlight;
    let (_tmp, store) = store();
    let later = store.ensure_later_feed().unwrap();
    store
        .add_articles(vec![
            article(&later, "https://example.com/a", 1),
            article(&later, "https://example.com/b", 2),
        ])
        .unwrap();
    let a = ids::item_id(&later, "https://example.com/a");
    let b = ids::item_id(&later, "https://example.com/b");
    let h = |id: &str, item: &str, quote: &str, at: i64| Highlight {
        id: id.into(),
        item: item.into(),
        quote: quote.into(),
        prefix: "before ".into(),
        suffix: " after".into(),
        note: String::new(),
        created_at: at,
    };
    store.put_highlight(&h("h1", &a, "first", 10)).unwrap();
    store.put_highlight(&h("h2", &a, "second", 20)).unwrap();
    store.put_highlight(&h("h3", &b, "other", 30)).unwrap();
    store.set_highlight_note("h2", "remember this").unwrap();
    let on_a = store.highlights(Some(&a)).unwrap();
    assert_eq!(
        on_a.iter().map(|h| h.quote.as_str()).collect::<Vec<_>>(),
        ["first", "second"]
    );
    assert_eq!(on_a[1].note, "remember this");
    store.remove_highlight("h1").unwrap();
    assert_eq!(store.highlights(Some(&a)).unwrap().len(), 1);
    let all = store.all_highlights().unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].highlight.quote, "other", "newest first");
    assert_eq!(all[0].title, "Title of https://example.com/b");
}
