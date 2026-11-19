//! 会「思考」的模型：思考也算在输出上限里，短任务的回答会被截断甚至是空的。
//! 对着假服务（按模型名模拟几类服务，见 testkit 的 ai.rs）确认总结和关键词照样拿得到完整的回答。

use postflow_ai::{Client, Provider, prompts};
use postflow_testkit::FakeAi;

fn provider(server: &FakeAi, preset: &str, model: &str) -> Provider {
    Provider {
        id: "fake".into(),
        preset: preset.into(),
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

const SUMMARY: &str = "**Version two ships today.**\n\n- It adds offline translation.\n- Cards show cover images.\n- Sync stays local.";

async fn summary(client: &Client, p: &Provider) -> (String, String) {
    let mut streamed = String::new();
    let reply = client
        .stream(p, None, &prompts::summary(DOC, "en"), |d| {
            streamed.push_str(d)
        })
        .await
        .unwrap();
    (reply.text, streamed)
}

async fn keywords(client: &Client, p: &Provider) -> Vec<String> {
    let reply = client
        .complete(p, None, &prompts::keywords(DOC, "en"))
        .await
        .unwrap();
    prompts::parse_keywords(&reply.text)
}

/// 智谱：短任务让它别思考，一次请求就拿到完整的回答。
#[tokio::test]
async fn zhipu_is_asked_not_to_think_for_short_tasks() {
    let server = FakeAi::start(None).await;
    let client = Client::new();
    let p = provider(&server, "zhipu", "glm-thinker");

    let (text, streamed) = summary(&client, &p).await;
    assert_eq!(text, SUMMARY);
    assert_eq!(streamed, SUMMARY);
    assert!(keywords(&client, &p).await.contains(&"Version".to_string()));

    let requests = server.requests();
    assert_eq!(requests.len(), 2, "one request per task, no retries");
    for r in &requests {
        assert_eq!(r["thinking"]["type"], "disabled");
        assert!(r["max_tokens"].is_number());
    }

    // 对话不带输出上限，让模型自己决定要不要思考
    let chat = postflow_ai::Request {
        messages: vec![postflow_ai::Message::user("What changed?")],
        ..Default::default()
    };
    client.complete(&p, None, &chat).await.unwrap();
    let last = server.requests().pop().unwrap();
    assert!(last.get("thinking").is_none());
    assert!(last.get("max_tokens").is_none());
}

/// 关不掉思考的服务（这里是自定义服务，不发那个参数）：发现在思考就不带上限重来，并且记住这个模型。
#[tokio::test]
async fn a_thinking_model_gets_no_output_limit() {
    let server = FakeAi::start(None).await;
    let client = Client::new();
    let p = provider(&server, "custom", "some-thinker");

    // 流式：思考一开始就重来，回答一个字都不会重复给出去
    let (text, streamed) = summary(&client, &p).await;
    assert_eq!(text, SUMMARY);
    assert_eq!(
        streamed, SUMMARY,
        "nothing from the abandoned attempt leaks out"
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["max_tokens"], 900);
    assert!(requests[0].get("thinking").is_none());
    assert!(requests[1].get("max_tokens").is_none());

    // 记住了：后面的请求直接不带上限，不用再试一次
    let words = keywords(&client, &p).await;
    assert!(words.contains(&"Version".to_string()), "{words:?}");
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert!(requests[2].get("max_tokens").is_none());

    // 换一个客户端（相当于重启应用）：一次性的请求先被截断，再不带上限重来；两次的用量都算
    let fresh = Client::new();
    let reply = fresh
        .complete(&p, None, &prompts::keywords(DOC, "en"))
        .await
        .unwrap();
    assert!(!prompts::parse_keywords(&reply.text).is_empty());
    let requests = server.requests();
    assert_eq!(requests.len(), 5);
    assert_eq!(requests[3]["max_tokens"], 300);
    assert!(requests[4].get("max_tokens").is_none());
    assert!(
        reply.usage.output > 300,
        "both attempts are counted: {:?}",
        reply.usage
    );

    // 「测试连接」只给 16 个 token，全被思考用掉了，也要能拿到回答
    let ping = postflow_ai::Request {
        messages: vec![postflow_ai::Message::user("Reply with the single word OK.")],
        max_tokens: Some(16),
        ..Default::default()
    };
    let reply = Client::new().complete(&p, None, &ping).await.unwrap();
    assert!(reply.text.contains("OK"), "{}", reply.text);
}

/// 智谱里不让关掉思考的模型：去掉那个参数再试，之后按会思考的模型处理，并且不再发那个参数。
#[tokio::test]
async fn a_model_that_refuses_to_stop_thinking_still_answers() {
    let server = FakeAi::start(None).await;
    let client = Client::new();
    let p = provider(&server, "zhipu", "glm-thinker-forced");

    let (text, streamed) = summary(&client, &p).await;
    assert_eq!(text, SUMMARY);
    assert_eq!(streamed, SUMMARY);
    let requests = server.requests();
    // 带着「别思考」被拒；去掉后模型开始思考；不带上限重来
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0]["thinking"]["type"], "disabled");
    assert!(requests[1].get("thinking").is_none());
    assert_eq!(requests[1]["max_tokens"], 900);
    assert!(requests[2].get("max_tokens").is_none());

    assert!(!keywords(&client, &p).await.is_empty());
    let requests = server.requests();
    assert_eq!(requests.len(), 4, "the model is remembered");
    assert!(requests[3].get("thinking").is_none());
    assert!(requests[3].get("max_tokens").is_none());
}

/// 智谱的老模型不认「别思考」这个参数，报错里也不提它：去掉再试，之后不再发。
#[tokio::test]
async fn an_older_model_that_rejects_the_parameter_still_works() {
    let server = FakeAi::start(None).await;
    let client = Client::new();
    let p = provider(&server, "zhipu", "glm-legacy");

    let (text, _) = summary(&client, &p).await;
    assert_eq!(text, SUMMARY);
    assert!(!keywords(&client, &p).await.is_empty());
    let requests = server.requests();
    assert_eq!(requests.len(), 3, "one rejected request, then one per task");
    assert_eq!(requests[0]["thinking"]["type"], "disabled");
    assert!(requests[1].get("thinking").is_none());
    assert_eq!(
        requests[1]["max_tokens"], 900,
        "an ordinary model keeps its limit"
    );
    assert!(requests[2].get("thinking").is_none());

    // 真正的错误（密钥不对）不会因为多试一次就被吞掉，也不会让它记错
    let locked = FakeAi::start(Some("right-key")).await;
    let p = provider(&locked, "zhipu", "glm-thinker");
    let err = client
        .complete(&p, Some("wrong"), &prompts::keywords(DOC, "en"))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "ai_auth");
    assert_eq!(
        locked.requests().len(),
        0,
        "rejected before reaching the model"
    );
    client
        .complete(&p, Some("right-key"), &prompts::keywords(DOC, "en"))
        .await
        .unwrap();
    assert_eq!(locked.requests()[0]["thinking"]["type"], "disabled");
}

/// 思考写在正文开头 `<think>…</think>` 里的模型：思考不算回答。
#[tokio::test]
async fn thinking_written_into_the_answer_is_left_out() {
    let server = FakeAi::start(None).await;
    let client = Client::new();
    let p = provider(&server, "ollama", "local-think-tags");

    let (text, streamed) = summary(&client, &p).await;
    assert_eq!(text, SUMMARY);
    assert_eq!(streamed, SUMMARY);
    assert!(!streamed.contains("think"));

    let fresh = Client::new();
    let reply = fresh
        .complete(&p, None, &prompts::keywords(DOC, "en"))
        .await
        .unwrap();
    assert!(!reply.text.contains("<think>"), "{}", reply.text);
    assert!(prompts::parse_keywords(&reply.text).contains(&"Version".to_string()));
}

/// 不认 `max_tokens` 的模型：去掉它再试。
#[tokio::test]
async fn a_model_that_rejects_max_tokens_is_asked_without_it() {
    let server = FakeAi::start(None).await;
    let client = Client::new();
    let p = provider(&server, "openai", "o-strict-cap");

    let (text, _) = summary(&client, &p).await;
    assert_eq!(text, SUMMARY);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["max_tokens"], 900);
    assert!(requests[1].get("max_tokens").is_none());
    assert!(!keywords(&client, &p).await.is_empty());
    assert_eq!(server.requests().len(), 3);
}

/// 普通模型不受影响：带着上限，一次请求。
#[tokio::test]
async fn ordinary_models_are_untouched() {
    let server = FakeAi::start(None).await;
    let client = Client::new();
    let p = provider(&server, "custom", "fake-large");
    let (text, streamed) = summary(&client, &p).await;
    assert_eq!(text, SUMMARY);
    assert_eq!(streamed, SUMMARY);
    assert!(!keywords(&client, &p).await.is_empty());
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["max_tokens"], 900);
    assert_eq!(requests[1]["max_tokens"], 300);
    assert!(requests.iter().all(|r| r.get("thinking").is_none()));
}
