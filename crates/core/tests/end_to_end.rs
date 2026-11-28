//! 起一个本地的假订阅服务器，从设置、订阅、阅读一路走到刷新，不依赖外网。

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;
use serde_json::{Value, json};
use shibapost_core::{Core, Event};

#[derive(Clone, Default)]
struct Site {
    /// 源里现在有几篇文章，测试里通过改它来模拟「发了新文章」。
    posts: Arc<AtomicUsize>,
    feed_hits: Arc<AtomicUsize>,
    not_modified: Arc<AtomicUsize>,
}

async fn feed(State(site): State<Site>, headers: HeaderMap) -> impl IntoResponse {
    site.feed_hits.fetch_add(1, Ordering::SeqCst);
    let posts = site.posts.load(Ordering::SeqCst);
    let etag = format!("\"v{posts}\"");
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        == Some(etag.as_str())
    {
        site.not_modified.fetch_add(1, Ordering::SeqCst);
        return (StatusCode::NOT_MODIFIED, HeaderMap::new(), String::new());
    }
    let items: String = (1..=posts)
        .map(|n| {
            format!(
                "<item><title>Post {n}</title><link>/posts/{n}</link><guid>post-{n}</guid>\
                 <pubDate>Mon, 0{n} Nov 2026 08:00:00 GMT</pubDate>\
                 <description>&lt;p&gt;Body of post {n} &lt;script&gt;alert(1)&lt;/script&gt;\
                 &lt;a href=\"/more\"&gt;more&lt;/a&gt;&lt;/p&gt;</description></item>"
            )
        })
        .collect();
    let body = format!(
        "<?xml version=\"1.0\"?><rss version=\"2.0\"><channel>\
         <title>Local Blog</title><link>/</link>{items}</channel></rss>"
    );
    let mut out = HeaderMap::new();
    out.insert(header::ETAG, etag.parse().unwrap());
    out.insert(header::CONTENT_TYPE, "application/rss+xml".parse().unwrap());
    (StatusCode::OK, out, body)
}

async fn home() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        "<html><head><title>Local Blog</title>\
         <link rel=\"alternate\" type=\"application/rss+xml\" title=\"Local feed\" href=\"/feed.xml\">\
         </head><body>hello</body></html>",
    )
}

async fn start_site() -> (Site, SocketAddr) {
    let site = Site::default();
    site.posts.store(2, Ordering::SeqCst);
    let app = Router::new()
        .route("/", get(home))
        .route("/feed.xml", get(feed))
        .with_state(site.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (site, addr)
}

async fn call(core: &Arc<Core>, method: &str, params: Value) -> Value {
    core.rpc(method, params, Some("test".into()))
        .await
        .unwrap_or_else(|e| panic!("{method} failed: {e}"))
}

async fn list(core: &Arc<Core>, view: &str) -> Vec<Value> {
    call(core, "items.list", json!({ "filter": { "view": view } })).await["items"]
        .as_array()
        .unwrap()
        .clone()
}

/// 等到刷新结束的事件，超时就失败。
async fn wait_for_refresh(rx: &mut tokio::sync::broadcast::Receiver<Event>) {
    let wait = async {
        loop {
            if let Ok(Event::Refresh { active: 0, .. }) = rx.recv().await {
                break;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(10), wait)
        .await
        .expect("refresh did not finish");
}

#[tokio::test(flavor = "multi_thread")]
async fn setup_subscribe_read_refresh() {
    let (site, addr) = start_site().await;
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let core = Core::open(tmp.path().join("local")).await.unwrap();

    // 还没设置时，除了状态和设置以外的方法都应该被拒绝
    let status = call(&core, "app.status", json!({})).await;
    assert_eq!(status["configured"], false);
    assert!(!status["suggested_data_dir"].as_str().unwrap().is_empty());
    let err = core.rpc("nav.get", json!({}), None).await.unwrap_err();
    assert_eq!(err.code, "not_configured");

    let status = call(
        &core,
        "app.setup",
        json!({ "data_dir": data_dir.to_str().unwrap(), "device_name": "Test Mac" }),
    )
    .await;
    assert_eq!(status["configured"], true);
    assert_eq!(status["device_name"], "Test Mac");
    assert!(data_dir.join("meta.json").exists());

    // 贴网站首页，应当发现它声明的订阅源
    let home = format!("http://{addr}/");
    let candidates = call(&core, "feeds.discover", json!({ "url": home })).await;
    assert_eq!(candidates[0]["url"], format!("http://{addr}/feed.xml"));
    assert_eq!(candidates[0]["title"], "Local feed");

    // 直接拿首页地址去订阅也行，会自动采用唯一的候选
    let feed = call(
        &core,
        "feeds.add",
        json!({ "url": home, "folder": "Blogs" }),
    )
    .await;
    assert_eq!(feed["title"], "Local Blog");
    assert_eq!(feed["folder"], "Blogs");
    assert_eq!(feed["unread"], 2);
    assert_eq!(feed["icon_url"], format!("http://{addr}/favicon.ico"));
    let feed_id = feed["id"].as_str().unwrap().to_string();

    let nav = call(&core, "nav.get", json!({ "today_since": 0 })).await;
    assert_eq!(nav["counts"]["unread"], 2);
    assert_eq!(nav["feeds"].as_array().unwrap().len(), 1);

    // 新的在前；相对链接补全成绝对地址
    let items = list(&core, "inbox").await;
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["title"], "Post 2");
    assert_eq!(items[0]["url"], format!("http://{addr}/posts/2"));
    assert_eq!(items[0]["source_title"], "Local Blog");
    assert_eq!(items[0]["read"], false);

    // 正文经过净化：脚本没了，相对链接补全并在新窗口打开
    let id = items[0]["id"].as_str().unwrap().to_string();
    let detail = call(&core, "items.get", json!({ "id": id })).await;
    let content = detail["content"].as_str().unwrap();
    assert!(content.contains("Body of post 2"), "{content}");
    assert!(!content.contains("script"), "{content}");
    assert!(
        content.contains(&format!("href=\"http://{addr}/more\"")),
        "{content}"
    );
    assert!(content.contains("target=\"_blank\""), "{content}");

    // 已读、星标、归档
    call(
        &core,
        "items.set",
        json!({ "ids": [id], "read": true, "starred": true }),
    )
    .await;
    assert_eq!(list(&core, "unread").await.len(), 1);
    assert_eq!(list(&core, "starred").await.len(), 1);
    call(&core, "items.set", json!({ "ids": [id], "archived": true })).await;
    assert_eq!(list(&core, "inbox").await.len(), 1);
    assert_eq!(list(&core, "archived").await.len(), 1);

    // 刷新：内容没变时服务器回 304，不产生新文章
    let mut events = core.subscribe();
    call(&core, "feeds.refresh", json!({})).await;
    wait_for_refresh(&mut events).await;
    assert_eq!(site.not_modified.load(Ordering::SeqCst), 1);
    assert_eq!(list(&core, "inbox").await.len(), 1);

    // 发了一篇新文章，刷新后出现，旧文章的状态不受影响
    site.posts.store(3, Ordering::SeqCst);
    call(&core, "feeds.refresh", json!({ "id": feed_id })).await;
    wait_for_refresh(&mut events).await;
    let items = list(&core, "inbox").await;
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["title"], "Post 3");
    assert_eq!(list(&core, "archived").await.len(), 1);

    // 全部标为已读
    let marked = call(
        &core,
        "items.mark_all_read",
        json!({ "filter": { "view": "inbox" } }),
    )
    .await;
    assert_eq!(marked["count"], 2);
    assert_eq!(
        call(&core, "nav.get", json!({})).await["counts"]["unread"],
        0
    );

    // 改名、移动文件夹、退订
    call(
        &core,
        "feeds.update",
        json!({ "id": feed_id, "title": "Renamed", "folder": "" }),
    )
    .await;
    let nav = call(&core, "nav.get", json!({})).await;
    assert_eq!(nav["feeds"][0]["title"], "Renamed");
    assert_eq!(nav["feeds"][0]["folder"], "");
    call(&core, "feeds.remove", json!({ "id": feed_id })).await;
    let nav = call(&core, "nav.get", json!({})).await;
    assert!(nav["feeds"].as_array().unwrap().is_empty());
    // 星标的那篇在退订后仍然保留
    assert_eq!(list(&core, "starred").await.len(), 1);

    core.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn opml_roundtrip_and_restart() {
    let (_site, addr) = start_site().await;
    let tmp = tempfile::tempdir().unwrap();
    let local = tmp.path().join("local");
    let data_dir = tmp.path().join("data");
    let core = Core::open(&local).await.unwrap();
    call(
        &core,
        "app.setup",
        json!({ "data_dir": data_dir.to_str().unwrap() }),
    )
    .await;

    let opml = format!(
        "<?xml version=\"1.0\"?><opml version=\"2.0\"><head><title>x</title></head><body>\
         <outline text=\"News\"><outline type=\"rss\" text=\"Local\" xmlUrl=\"http://{addr}/feed.xml\"/></outline>\
         <outline type=\"rss\" text=\"Dead\" xmlUrl=\"http://127.0.0.1:9/nothing.xml\"/>\
         </body></opml>"
    );
    let mut events = core.subscribe();
    let result = call(&core, "feeds.import_opml", json!({ "content": opml })).await;
    assert_eq!(result["added"], 2);
    wait_for_refresh(&mut events).await;

    let nav = call(&core, "nav.get", json!({})).await;
    let feeds = nav["feeds"].as_array().unwrap();
    let local_feed = feeds.iter().find(|f| f["title"] == "Local").unwrap();
    let dead_feed = feeds.iter().find(|f| f["title"] == "Dead").unwrap();
    assert_eq!(local_feed["folder"], "News");
    assert_eq!(local_feed["unread"], 2);
    assert!(local_feed["error"].is_null());
    // 抓不到的源要把原因记下来，界面据此标出失效源
    assert!(
        dead_feed["error"].as_str().is_some_and(|e| !e.is_empty()),
        "{dead_feed}"
    );

    // 导出再解析，订阅和文件夹都还在
    let exported = call(&core, "feeds.export_opml", json!({})).await;
    let content = exported["content"].as_str().unwrap();
    assert!(content.contains(&format!("http://{addr}/feed.xml")));
    assert!(content.contains("News"));

    // 重启：配置、订阅、文章都从磁盘恢复，不需要重新设置
    core.shutdown();
    drop(core);
    let core = Core::open(&local).await.unwrap();
    let status = call(&core, "app.status", json!({})).await;
    assert_eq!(status["configured"], true);
    assert_eq!(list(&core, "inbox").await.len(), 2);

    // 把索引删了也能从数据目录重建出来
    let stats = call(&core, "index.rebuild", json!({})).await;
    assert_eq!(stats["articles"], 2);
    assert_eq!(list(&core, "inbox").await.len(), 2);
    core.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn changes_written_by_another_device_are_picked_up() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let core = Core::open(tmp.path().join("local")).await.unwrap();
    call(
        &core,
        "app.setup",
        json!({ "data_dir": data_dir.to_str().unwrap() }),
    )
    .await;
    let mut events = core.subscribe();

    // 另一台设备（另一个索引、另一个设备 ID）往同一个数据目录里写了一条订阅，
    // 相当于同步工具把它的日志文件放了进来
    let other_index = tmp.path().join("other/index.db");
    let other = shibapost_store::Store::open(&data_dir, &other_index, "ffffffffffff").unwrap();
    other
        .put_feed(shibapost_store::NewFeed {
            url: "https://elsewhere.example/feed.xml".into(),
            title: "From the other device".into(),
            ..Default::default()
        })
        .unwrap();

    let wait = async {
        loop {
            if let Ok(Event::Changed { source: None, .. }) = events.recv().await {
                break;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(15), wait)
        .await
        .expect("the directory watcher did not notice the new log file");

    let nav = call(&core, "nav.get", json!({})).await;
    assert_eq!(nav["feeds"][0]["title"], "From the other device");
    core.shutdown();
}

#[tokio::test]
async fn translation_works_before_setup_without_network() {
    let tmp = tempfile::tempdir().unwrap();
    let core = Core::open(tmp.path().join("local")).await.unwrap();

    // 还没设置数据目录也能调用；原文已经是目标语言时不需要任何模型
    let plan = call(
        &core,
        "translate.plan",
        json!({ "text": "今天天气很好，我们去散步吧。", "to": "zh" }),
    )
    .await;
    assert_eq!(plan["from"], "zh");
    assert_eq!(plan["same"], true);
    assert_eq!(plan["missing"], json!([]));

    let out = call(
        &core,
        "translate.text",
        json!({ "from": "zh_hant", "to": "zh", "segments": [{ "text": "你好", "marks": [[0, 2]] }] }),
    )
    .await;
    assert_eq!(out["segments"][0]["text"], "你好");
    assert_eq!(out["segments"][0]["marks"], json!([[0, 2, 0]]));

    let err = core
        .rpc("translate.remove", json!({ "pair": "../x-en" }), None)
        .await
        .unwrap_err();
    assert_eq!(err.code, "bad_params");
    assert!(tmp.path().join("local/translation").is_dir());
}

/// 源搬了家（比如 DeepMind 把博客从 /discover/blog/ 挪到 /blog/）：换地址后同一个订阅源接着用，
/// 已读状态和以前的文章都在；新地址不能用、或者已经订阅过时不换。
#[tokio::test(flavor = "multi_thread")]
async fn changing_a_feed_address_keeps_its_history() {
    let site = Site::default();
    site.posts.store(2, Ordering::SeqCst);
    let app = Router::new()
        .route("/old.xml", get(feed))
        .route("/new.xml", get(feed))
        .route("/other.xml", get(feed))
        .with_state(site.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let tmp = tempfile::tempdir().unwrap();
    let core = Core::open(tmp.path().join("local")).await.unwrap();
    let data_dir = tmp.path().join("data");
    call(
        &core,
        "app.setup",
        json!({ "data_dir": data_dir.to_str().unwrap(), "device_name": "Test" }),
    )
    .await;

    let feed = call(
        &core,
        "feeds.add",
        json!({ "url": format!("http://{addr}/old.xml") }),
    )
    .await;
    let id = feed["id"].as_str().unwrap().to_string();
    let first = list(&core, "inbox").await[0]["id"].clone();
    call(&core, "items.set", json!({ "ids": [first], "read": true })).await;

    call(
        &core,
        "feeds.update",
        json!({ "id": id, "url": format!("http://{addr}/new.xml") }),
    )
    .await;
    let nav = call(&core, "nav.get", json!({})).await;
    assert_eq!(nav["feeds"][0]["id"], id.as_str());
    assert_eq!(nav["feeds"][0]["url"], format!("http://{addr}/new.xml"));
    let items = list(&core, "inbox").await;
    assert_eq!(items.len(), 2, "no duplicates after moving");
    assert_eq!(list(&core, "unread").await.len(), 1, "read state kept");

    // 新地址抓不到源：报错，地址不变
    let err = core
        .rpc(
            "feeds.update",
            json!({ "id": id, "url": format!("http://{addr}/missing.xml") }),
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, "http_status");

    // 已经有另一个订阅源用这个地址
    call(
        &core,
        "feeds.add",
        json!({ "url": format!("http://{addr}/other.xml") }),
    )
    .await;
    let err = core
        .rpc(
            "feeds.update",
            json!({ "id": id, "url": format!("http://{addr}/other.xml") }),
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, "feed_exists");
    let nav = call(&core, "nav.get", json!({})).await;
    let url_of = |id: &str| {
        nav["feeds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["id"] == id)
            .unwrap()["url"]
            .clone()
    };
    assert_eq!(url_of(&id), format!("http://{addr}/new.xml"));
}

/// 全文搜索：items.list 带上 query，刚收进来的文章马上能搜到，标题没命中时给出正文片段。
#[tokio::test(flavor = "multi_thread")]
async fn search_finds_new_articles_right_away() {
    let (_site, addr) = start_site().await;
    let tmp = tempfile::tempdir().unwrap();
    let core = Core::open(tmp.path().join("local")).await.unwrap();
    let data_dir = tmp.path().join("data");
    call(
        &core,
        "app.setup",
        json!({ "data_dir": data_dir.to_str().unwrap(), "device_name": "Test" }),
    )
    .await;
    call(
        &core,
        "feeds.add",
        json!({ "url": format!("http://{addr}/feed.xml") }),
    )
    .await;

    let search = |q: &'static str, view: &'static str| {
        let core = core.clone();
        async move {
            call(
                &core,
                "items.list",
                json!({ "filter": { "view": view, "query": q } }),
            )
            .await["items"]
                .as_array()
                .unwrap()
                .clone()
        }
    };
    let hits = search("post 2", "all").await;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["title"], "Post 2");
    assert!(
        hits[0].get("snippet").is_none(),
        "the title already matches"
    );

    let hits = search("body", "inbox").await;
    assert_eq!(hits.len(), 2);
    assert!(
        hits[0]["snippet"]
            .as_str()
            .unwrap()
            .contains("Body of post")
    );
    // script 里的内容不算，标点搜不到东西
    assert!(search("alert", "all").await.is_empty());
    assert!(search("???", "all").await.is_empty());
}

/// 源永久搬家（301 / 308）时订阅地址跟着换；临时跳转（302）不换。
#[tokio::test(flavor = "multi_thread")]
async fn permanent_redirects_update_the_feed_address() {
    use axum::response::Redirect;
    use std::sync::atomic::AtomicBool;

    let site = Site::default();
    site.posts.store(2, Ordering::SeqCst);
    let moving = Arc::new(AtomicBool::new(false));
    let flag = moving.clone();
    let app = Router::new()
        .route("/final.xml", get(feed))
        .route("/new-home.xml", get(feed))
        .route("/temp-target.xml", get(feed))
        .route(
            "/moved.xml",
            get(|| async { Redirect::permanent("/final.xml") }),
        )
        .route(
            "/temp.xml",
            get(|| async { Redirect::temporary("/temp-target.xml") }),
        )
        .route(
            "/moving.xml",
            get(move |state: State<Site>, headers: HeaderMap| {
                let flag = flag.clone();
                async move {
                    if flag.load(Ordering::SeqCst) {
                        Redirect::permanent("/new-home.xml").into_response()
                    } else {
                        feed(state, headers).await.into_response()
                    }
                }
            }),
        )
        .with_state(site.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let tmp = tempfile::tempdir().unwrap();
    let core = Core::open(tmp.path().join("local")).await.unwrap();
    let data_dir = tmp.path().join("data");
    call(
        &core,
        "app.setup",
        json!({ "data_dir": data_dir.to_str().unwrap(), "device_name": "Test" }),
    )
    .await;
    let url_of = |feed: &Value| feed["url"].as_str().unwrap().to_string();

    // 订阅时就已经搬了家：直接存新地址
    let moved = call(
        &core,
        "feeds.add",
        json!({ "url": format!("http://{addr}/moved.xml") }),
    )
    .await;
    assert_eq!(url_of(&moved), format!("http://{addr}/final.xml"));
    // 临时跳转：还用原地址
    let temp = call(
        &core,
        "feeds.add",
        json!({ "url": format!("http://{addr}/temp.xml") }),
    )
    .await;
    assert_eq!(url_of(&temp), format!("http://{addr}/temp.xml"));

    // 订阅之后才搬家：下次刷新时换成新地址
    let later = call(
        &core,
        "feeds.add",
        json!({ "url": format!("http://{addr}/moving.xml") }),
    )
    .await;
    let id = later["id"].as_str().unwrap().to_string();
    moving.store(true, Ordering::SeqCst);
    let mut events = core.subscribe();
    call(&core, "feeds.refresh", json!({ "id": id })).await;
    wait_for_refresh(&mut events).await;
    let nav = call(&core, "nav.get", json!({})).await;
    let now = nav["feeds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == id.as_str())
        .unwrap()
        .clone();
    assert_eq!(url_of(&now), format!("http://{addr}/new-home.xml"));
    assert!(now["error"].is_null());
}
