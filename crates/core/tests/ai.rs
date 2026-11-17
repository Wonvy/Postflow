//! 对着本机的假 AI 服务把 AI 功能走一遍：配置服务、总结、关键词、列表摘要、对话，以及邮件的隐私限制。

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::routing::get;
use postflow_core::{Core, Event, MemorySecrets};
use postflow_testkit::{FakeAi, FakeImap, mail};
use serde_json::{Value, json};
use tokio::sync::broadcast::Receiver;

async fn call(core: &Arc<Core>, method: &str, params: Value) -> Value {
    core.rpc(method, params, None)
        .await
        .unwrap_or_else(|e| panic!("{method} failed: {e}"))
}

async fn error(core: &Arc<Core>, method: &str, params: Value) -> String {
    core.rpc(method, params, None).await.unwrap_err().code
}

const FEED: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom"><title>Field Notes</title><id>urn:notes</id><updated>2026-11-16T08:00:00Z</updated>
<entry><id>urn:notes:1</id><title>Version two</title><updated>2026-11-16T08:00:00Z</updated>
<content type="html">&lt;p&gt;Version two ships today.&lt;/p&gt;&lt;p&gt;It adds offline translation. Cards show cover images. Sync stays local.&lt;/p&gt;</content></entry>
</feed>"#;

async fn feed_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new().route(
        "/feed.xml",
        get(|| async { ([("content-type", "application/atom+xml")], FEED) }),
    );
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}/feed.xml")
}

/// 等这个请求结束，返回流式收到的文字和最后那个事件里的结果或错误。
async fn finish(events: &mut Receiver<Event>, id: &str) -> (String, Result<Value, String>) {
    let mut text = String::new();
    let wait = async {
        loop {
            if let Ok(Event::Ai {
                request_id,
                delta,
                done,
                result,
                error,
                ..
            }) = events.recv().await
            {
                if request_id != id {
                    continue;
                }
                if let Some(d) = delta {
                    text.push_str(&d);
                }
                if done {
                    return match error {
                        Some(code) => Err(code),
                        None => Ok(result.unwrap_or(Value::Null)),
                    };
                }
            }
        }
    };
    let result = tokio::time::timeout(Duration::from_secs(20), wait)
        .await
        .expect("the AI request did not finish");
    (text, result)
}

#[tokio::test(flavor = "multi_thread")]
async fn ai_features_end_to_end() {
    let ai = FakeAi::start(Some("test-key")).await;
    let imap = FakeImap::start("me@example.com", "pw").await;
    imap.deliver(
        "INBOX",
        &mail(
            "m1@example.com",
            "Ada <ada@example.com>",
            "Lunch?",
            "Noon at the usual place. Bring the slides.",
        ),
        &[],
    );
    let tmp = tempfile::tempdir().unwrap();
    let core =
        Core::open_with_secrets(tmp.path().join("local"), Arc::new(MemorySecrets::default()))
            .await
            .unwrap();
    let mut events = core.subscribe();
    call(
        &core,
        "app.setup",
        json!({ "data_dir": tmp.path().join("data") }),
    )
    .await;
    call(&core, "feeds.add", json!({ "url": feed_server().await })).await;
    let items = call(
        &core,
        "items.list",
        json!({ "filter": { "view": "inbox" } }),
    )
    .await;
    let article = items["items"][0]["id"].as_str().unwrap().to_string();

    // 还没配置服务
    assert_eq!(
        error(
            &core,
            "ai.run",
            json!({ "id": article, "task": "summary", "lang": "en" })
        )
        .await,
        "ai_not_configured"
    );

    // 配置一个服务；密钥进钥匙串，不出现在设置里
    let saved = call(
        &core,
        "ai.providers.save",
        json!({
            "provider": { "preset": "custom", "name": "Fake", "base_url": format!("{}/", ai.base_url()), "model": "fake-large" },
            "api_key": "test-key",
        }),
    )
    .await;
    let fake_id = saved["id"].as_str().unwrap().to_string();
    assert_eq!(
        saved["base_url"],
        ai.base_url(),
        "trailing slash is trimmed"
    );
    let listed = call(&core, "ai.providers", json!({})).await;
    assert_eq!(listed["default"], fake_id);
    assert_eq!(listed["providers"][0]["has_key"], true);
    assert_eq!(listed["providers"][0]["local"], true);
    let settings = call(&core, "settings.get", json!({})).await;
    assert!(
        !settings.to_string().contains("test-key"),
        "the key must never reach the synced settings"
    );

    let models = call(&core, "ai.models", json!({ "provider": saved })).await;
    assert_eq!(models["models"], json!(["fake-large", "fake-small"]));
    assert_eq!(
        call(&core, "ai.providers.test", json!({ "provider": saved })).await["ok"],
        true
    );
    assert_eq!(
        error(
            &core,
            "ai.providers.test",
            json!({ "provider": saved, "api_key": "wrong" })
        )
        .await,
        "ai_auth"
    );

    // 总结：流式输出，然后缓存
    let started = call(
        &core,
        "ai.run",
        json!({ "id": article, "task": "summary", "lang": "en" }),
    )
    .await;
    let (streamed, done) = finish(&mut events, started["request_id"].as_str().unwrap()).await;
    let done = done.unwrap();
    assert!(
        streamed.starts_with("**Version two ships today.**"),
        "{streamed}"
    );
    assert_eq!(done["result"]["content"], streamed);
    let again = call(
        &core,
        "ai.run",
        json!({ "id": article, "task": "summary", "lang": "en" }),
    )
    .await;
    assert_eq!(again["cached"]["content"], streamed);

    // 关键词
    let started = call(
        &core,
        "ai.run",
        json!({ "id": article, "task": "keywords", "lang": "en" }),
    )
    .await;
    let (_, done) = finish(&mut events, started["request_id"].as_str().unwrap()).await;
    let words: Vec<String> =
        serde_json::from_str(done.unwrap()["result"]["content"].as_str().unwrap()).unwrap();
    assert!(words.contains(&"Version".to_string()), "{words:?}");
    let results = call(&core, "ai.results", json!({ "id": article, "lang": "en" })).await;
    assert!(results["summary"].is_object() && results["keywords"].is_object());

    // 列表摘要
    let abstracts = call(
        &core,
        "ai.abstracts",
        json!({ "ids": [article], "lang": "zh" }),
    )
    .await;
    assert_eq!(
        abstracts["abstracts"][&article],
        "关于「Version two」的简短介绍。"
    );

    // 对话
    let asked = call(
        &core,
        "ai.chat",
        json!({ "id": article, "message": "What ships?", "lang": "en" }),
    )
    .await;
    assert_eq!(asked["message"]["role"], "user");
    let (reply, done) = finish(&mut events, asked["request_id"].as_str().unwrap()).await;
    assert!(reply.contains("What ships?"), "{reply}");
    assert_eq!(done.unwrap()["message"]["role"], "assistant");
    let history = call(&core, "ai.chat.history", json!({ "id": article })).await;
    assert_eq!(history["messages"].as_array().unwrap().len(), 2);

    // 邮件：默认不发给云端服务，本机的服务可以
    call(
        &core,
        "accounts.add",
        json!({
            "email": "me@example.com", "password": "pw", "imap_host": "127.0.0.1",
            "imap_port": imap.port, "imap_security": "none", "username": "me@example.com",
        }),
    )
    .await;
    let mut mail_id = None;
    for _ in 0..100 {
        let list = call(
            &core,
            "items.list",
            json!({ "filter": { "view": "inbox", "kind": "mail" } }),
        )
        .await;
        if let Some(id) = list["items"][0]["id"].as_str() {
            mail_id = Some(id.to_string());
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let mail_id = mail_id.expect("mail synced");
    let started = call(
        &core,
        "ai.run",
        json!({ "id": mail_id, "task": "summary", "lang": "en" }),
    )
    .await;
    let (_, done) = finish(&mut events, started["request_id"].as_str().unwrap()).await;
    assert!(done.is_ok(), "a local service may read mail");

    let cloud = call(
        &core,
        "ai.providers.save",
        json!({ "provider": { "preset": "deepseek", "base_url": "https://api.example.invalid", "model": "m" }, "api_key": "k" }),
    )
    .await;
    assert_eq!(cloud["name"], "DeepSeek", "name comes from the preset");
    call(
        &core,
        "settings.set",
        json!({ "key": "ai.default", "value": cloud["id"] }),
    )
    .await;
    assert_eq!(
        error(
            &core,
            "ai.chat",
            json!({ "id": mail_id, "message": "hi", "lang": "en" })
        )
        .await,
        "ai_mail_blocked"
    );
    // 文章不受影响
    let started = call(
        &core,
        "ai.run",
        json!({ "id": article, "task": "summary", "lang": "fr" }),
    )
    .await;
    assert!(started["request_id"].is_string());
    let _ = finish(&mut events, started["request_id"].as_str().unwrap()).await;
    // 打开开关以后邮件也可以发
    call(
        &core,
        "settings.set",
        json!({ "key": "ai.allow_mail", "value": true }),
    )
    .await;
    let started = call(
        &core,
        "ai.chat",
        json!({ "id": mail_id, "message": "hi", "lang": "en" }),
    )
    .await;
    let (_, done) = finish(&mut events, started["request_id"].as_str().unwrap()).await;
    assert_eq!(done.unwrap_err(), "ai_network");

    // 用量
    let usage = call(&core, "ai.usage", json!({})).await;
    let fake = usage["last30"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["provider"] == fake_id)
        .unwrap();
    assert!(fake["requests"].as_i64().unwrap() >= 5, "{usage}");

    // 删掉默认的服务，默认改成剩下的那个
    call(&core, "ai.providers.remove", json!({ "id": cloud["id"] })).await;
    let listed = call(&core, "ai.providers", json!({})).await;
    assert_eq!(listed["providers"].as_array().unwrap().len(), 1);
    assert_eq!(listed["default"], fake_id);
    core.shutdown();
}
