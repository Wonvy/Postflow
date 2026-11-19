//! 朗读的本机声音。模型有几百兆，自动测试里只测没装模型时的表现；
//! 设了 `POSTFLOW_VOICE_SMOKE=<放 Kokoro v1.1-zh 文件的文件夹>` 时，再从那里导入、真的念一句。

use std::sync::Arc;

use postflow_core::Core;
use serde_json::{Value, json};

async fn call(core: &Arc<Core>, method: &str, params: Value) -> Value {
    core.rpc(method, params, None)
        .await
        .unwrap_or_else(|e| panic!("{method} failed: {e}"))
}

async fn error(core: &Arc<Core>, method: &str, params: Value) -> String {
    core.rpc(method, params, None).await.unwrap_err().code
}

#[tokio::test(flavor = "multi_thread")]
async fn voices_without_a_model() {
    let tmp = tempfile::tempdir().unwrap();
    let core = Core::open(tmp.path().join("local")).await.unwrap();

    // 不用先设置数据目录
    let overview = call(&core, "voices.overview", json!({})).await;
    let models = overview["models"].as_array().unwrap();
    assert!(models.len() >= 2);
    for m in models {
        assert_eq!(m["task"], "tts");
        assert_eq!(m["installed"], false);
        assert!(!m["speakers"].as_array().unwrap().is_empty());
    }
    let zh = models.iter().find(|m| m["id"] == "kokoro-v1.1-zh").unwrap();
    assert_eq!(zh["options"]["voice_zh"], "zf_094");

    assert_eq!(
        error(
            &core,
            "voices.speak",
            json!({ "model": "kokoro-v1.1-zh", "voice": "zf_001", "text": "你好" })
        )
        .await,
        "speech_not_installed"
    );
    assert_eq!(
        error(&core, "voices.download", json!({ "id": "no-such-model" })).await,
        "speech_unknown_model"
    );
    // 识别模型的 ID 在朗读这边不认
    assert_eq!(
        error(
            &core,
            "voices.download",
            json!({ "id": "sense-voice-small" })
        )
        .await,
        "speech_unknown_model"
    );
    assert_eq!(
        error(
            &core,
            "voices.import",
            json!({ "id": "kokoro-v1.1-zh", "path": tmp.path().join("nowhere").to_str().unwrap() })
        )
        .await,
        "speech_import_missing"
    );
    // 没装的删了也不出错
    call(&core, "voices.remove", json!({ "id": "kokoro-v1.0" })).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn speak_with_a_real_model() {
    let Ok(folder) = std::env::var("POSTFLOW_VOICE_SMOKE") else {
        eprintln!("POSTFLOW_VOICE_SMOKE is not set; skipping");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let core = Core::open(tmp.path().join("local")).await.unwrap();
    let overview = call(
        &core,
        "voices.import",
        json!({ "id": "kokoro-v1.1-zh", "path": folder }),
    )
    .await;
    assert!(
        overview["models"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == "kokoro-v1.1-zh" && m["installed"] == true)
    );

    let said = call(
        &core,
        "voices.speak",
        json!({ "model": "kokoro-v1.1-zh", "voice": "zf_001", "text": "今天的新闻：GPU 价格上涨了 5%。", "speed": 1.2 }),
    )
    .await;
    let duration = said["duration"].as_f64().unwrap();
    assert!(duration > 1.0 && duration < 10.0, "{duration}");
    // Base64 的 WAV：「RIFF」编出来是「UklGR」
    assert!(said["audio"].as_str().unwrap().starts_with("UklGR"));

    // 只有标点和网址：没什么可念的，返回空的
    let said = call(
        &core,
        "voices.speak",
        json!({ "model": "kokoro-v1.1-zh", "voice": "zf_001", "text": "—— https://example.com" }),
    )
    .await;
    assert_eq!(said["audio"], "");

    assert_eq!(
        error(
            &core,
            "voices.speak",
            json!({ "model": "kokoro-v1.1-zh", "voice": "nobody", "text": "你好" })
        )
        .await,
        "voice_failed"
    );
}
