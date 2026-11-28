//! 字幕的方法在没装识别模型时的表现。真的转写要几百兆的模型，不放进自动测试
//! （用 `cargo run -p shibapost-speech --example transcribe` 手动试）。

use std::sync::Arc;

use serde_json::{Value, json};
use shibapost_core::Core;

async fn call(core: &Arc<Core>, method: &str, params: Value) -> Value {
    core.rpc(method, params, None)
        .await
        .unwrap_or_else(|e| panic!("{method} failed: {e}"))
}

async fn error(core: &Arc<Core>, method: &str, params: Value) -> String {
    core.rpc(method, params, None).await.unwrap_err().code
}

#[tokio::test(flavor = "multi_thread")]
async fn subtitles_without_a_model() {
    let tmp = tempfile::tempdir().unwrap();
    let local = tmp.path().join("local");
    let core = Core::open(&local).await.unwrap();

    // 字幕要读条目，得先设置好数据目录
    assert_eq!(
        error(&core, "subtitles.get", json!({ "id": "abc123" })).await,
        "not_configured"
    );
    call(
        &core,
        "app.setup",
        json!({ "data_dir": tmp.path().join("data").to_str().unwrap(), "device_name": "Test" }),
    )
    .await;

    // 没转过的条目：空的，不是错误
    let snap = call(&core, "subtitles.get", json!({ "id": "abc123" })).await;
    assert_eq!(snap["state"], "idle");
    assert_eq!(snap["complete"], false);
    assert_eq!(snap["cues"], json!([]));
    assert_eq!(snap["covered"], json!([]));
    assert_eq!(
        call(&core, "subtitles.list", json!({})).await["ids"],
        json!([])
    );

    // 一个模型都没装：开始转写时明说，界面据此引导去下载
    assert_eq!(
        error(
            &core,
            "subtitles.start",
            json!({ "id": "abc123", "position": 12.5 })
        )
        .await,
        "speech_not_installed"
    );
    assert_eq!(
        error(&core, "speech.select", json!({ "id": "sense-voice-small" })).await,
        "speech_not_installed"
    );
    assert_eq!(
        error(
            &core,
            "speech.import",
            json!({ "id": "sense-voice-small", "path": "/no/such/folder" })
        )
        .await,
        "speech_import_missing"
    );

    // 以前转过的字幕，模型删了以后照样能看
    let saved = json!({
        "model": "sense-voice-small",
        "duration": 60.0,
        "covered": [[0.0, 30.0]],
        "cues": [{ "start": 1.0, "end": 3.5, "text": "你好，欢迎收听。" }],
    });
    let dir = local.join("speech/transcripts");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("abc123.json"), saved.to_string()).unwrap();
    let snap = call(
        &core,
        "subtitles.start",
        json!({ "id": "abc123", "position": 0 }),
    )
    .await;
    assert_eq!(snap["state"], "idle");
    assert_eq!(snap["complete"], false);
    assert_eq!(snap["model"], "sense-voice-small");
    assert_eq!(snap["cues"][0]["text"], "你好，欢迎收听。");
    assert_eq!(snap["covered"], json!([[0.0, 30.0]]));
    assert_eq!(
        call(&core, "subtitles.list", json!({})).await["ids"],
        json!(["abc123"])
    );

    // 删掉这一期的，再全部清掉
    let snap = call(&core, "subtitles.remove", json!({ "id": "abc123" })).await;
    assert_eq!(snap["cues"], json!([]));
    assert!(!dir.join("abc123.json").exists());
    std::fs::write(dir.join("def456.json"), saved.to_string()).unwrap();
    call(&core, "subtitles.clear", json!({})).await;
    assert_eq!(
        call(&core, "subtitles.list", json!({})).await["ids"],
        json!([])
    );

    // 条目 ID 不会被拿去拼别的路径
    let snap = call(&core, "subtitles.get", json!({ "id": "../../config" })).await;
    assert_eq!(snap["cues"], json!([]));
    core.shutdown();
}
