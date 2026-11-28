//! 对着本机的假服务器走一遍：模型列表、一次返回、流式返回、JSON 任务、错误。

use shibapost_ai::{Client, Provider, prompts};
use shibapost_testkit::FakeAi;

fn provider(server: &FakeAi, model: &str) -> Provider {
    Provider {
        id: "fake".into(),
        preset: "custom".into(),
        name: "Fake".into(),
        base_url: server.base_url(),
        model: model.into(),
        proxy: None,
    }
}

const DOC: prompts::Doc<'static> = prompts::Doc {
    title: "Release notes",
    source: "Field Notes",
    text: "Version two ships today. It adds offline translation. Cards show cover images. Sync stays local.",
};

#[tokio::test]
async fn lists_models_and_completes() {
    let server = FakeAi::start(Some("test-key")).await;
    let client = Client::new();
    let p = provider(&server, "fake-large");
    assert_eq!(
        client.models(&p, Some("test-key")).await.unwrap(),
        vec!["fake-large", "fake-small"]
    );

    let reply = client
        .complete(&p, Some("test-key"), &prompts::summary(DOC, "en"))
        .await
        .unwrap();
    assert!(
        reply.text.starts_with("**Version two ships today.**"),
        "{}",
        reply.text
    );
    assert!(reply.usage.output > 0);

    let words = prompts::parse_keywords(
        &client
            .complete(&p, Some("test-key"), &prompts::keywords(DOC, "en"))
            .await
            .unwrap()
            .text,
    );
    assert!(words.contains(&"Version".to_string()), "{words:?}");
    // JSON 任务带上了 response_format
    let last = server.requests().pop().unwrap();
    assert_eq!(last["response_format"]["type"], "json_object");
}

/// 智谱等服务要求 temperature 最多两位小数、且大于 0。0.3 存成 f32 再转成 JSON 会变成
/// 0.30000001192092896，被这些服务以 400 拒绝。
#[tokio::test]
async fn sends_temperature_with_at_most_two_decimals() {
    let server = FakeAi::start(None).await;
    let client = Client::new();
    let p = provider(&server, "fake-large");
    for request in [prompts::summary(DOC, "en"), prompts::keywords(DOC, "en")] {
        client.complete(&p, None, &request).await.unwrap();
    }
    let sent: Vec<String> = server
        .requests()
        .iter()
        .map(|r| r["temperature"].to_string())
        .collect();
    assert_eq!(sent, vec!["0.3", "0.2"]);
}

#[test]
fn every_task_uses_a_temperature_all_services_accept() {
    let items = vec![("a".to_string(), DOC)];
    let categories = vec![prompts::Category {
        name: "News".into(),
        description: String::new(),
    }];
    let requests = [
        prompts::summary(DOC, "en"),
        prompts::summary_notes(DOC, "part", 1, 2, "en"),
        prompts::keywords(DOC, "en"),
        prompts::abstracts(&items, "en"),
        prompts::chat(DOC, &[], "Why?", "en"),
        prompts::translate(&["Hello".to_string()], "en", "zh"),
        prompts::classify(&items, &categories),
    ];
    for r in requests {
        let t = r.temperature.expect("every task sets a temperature");
        assert!(t > 0.0 && t <= 1.0, "{t}");
        assert_eq!((t * 100.0).round() / 100.0, t, "{t}");
    }
}

#[tokio::test]
async fn streams_in_pieces() {
    let server = FakeAi::start(None).await;
    let client = Client::new();
    let p = provider(&server, "fake-large");
    let mut pieces = Vec::new();
    let reply = client
        .stream(&p, None, &prompts::summary(DOC, "en"), |d| {
            pieces.push(d.to_string())
        })
        .await
        .unwrap();
    assert!(pieces.len() > 3);
    assert_eq!(pieces.concat(), reply.text);
    assert!(reply.usage.input > 0, "usage comes with the last chunk");
}

#[tokio::test]
async fn reports_bad_keys_and_missing_models() {
    let server = FakeAi::start(Some("test-key")).await;
    let client = Client::new();
    let err = client
        .complete(
            &provider(&server, "fake-large"),
            Some("wrong"),
            &prompts::summary(DOC, "en"),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "ai_auth");
    let err = client
        .complete(
            &provider(&server, ""),
            Some("test-key"),
            &prompts::summary(DOC, "en"),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "ai_no_model");
    // 连不上
    let mut gone = provider(&server, "m");
    gone.base_url = "http://127.0.0.1:9/v1".into();
    assert_eq!(
        client.models(&gone, None).await.unwrap_err().code(),
        "ai_network"
    );
}
