//! 朗读的本机声音。模型有几百兆，自动测试里只测没装模型时的表现；
//! 设了 `SHIBAPOST_VOICE_SMOKE=<放 Kokoro v1.1-zh 文件的文件夹>` 时，再从那里导入、真的念一句。

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
        // ZipVoice 没有自带的声音，用声音样本
        if m["kind"] == "kokoro" {
            assert!(!m["speakers"].as_array().unwrap().is_empty());
        }
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
    let Ok(folder) = std::env::var("SHIBAPOST_VOICE_SMOKE") else {
        eprintln!("SHIBAPOST_VOICE_SMOKE is not set; skipping");
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

/// 一段测试用的录音：`seconds` 秒的正弦波，16 位单声道 WAV，Base64。
fn tone(seconds: f64, rate: u32) -> String {
    let n = (seconds * f64::from(rate)) as usize;
    let samples: Vec<f32> = (0..n)
        .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 0.3)
        .collect();
    shibapost_mail::base64(&shibapost_speech::tts::wav(&samples, rate))
}

#[tokio::test(flavor = "multi_thread")]
async fn voice_samples() {
    let tmp = tempfile::tempdir().unwrap();
    let core = Core::open(tmp.path().join("local")).await.unwrap();
    assert_eq!(
        call(&core, "voices.samples", json!({})).await["samples"],
        json!([])
    );

    // 太短、读不出来的录音不收
    assert_eq!(
        error(
            &core,
            "voices.samples.add",
            json!({ "name": "short", "audio": tone(1.0, 16_000), "ext": "wav", "text": "你好" })
        )
        .await,
        "voice_sample_short"
    );
    assert_eq!(
        error(
            &core,
            "voices.samples.add",
            json!({ "name": "junk", "audio": shibapost_mail::base64(b"not audio at all"), "ext": "mp3" })
        )
        .await,
        "voice_sample_unreadable"
    );

    // 44.1 kHz 的录音存成 24 kHz；没装识别模型时文字是空的，让用户自己填
    let added = call(
        &core,
        "voices.samples.add",
        json!({ "name": "  My voice  ", "audio": tone(4.0, 44_100), "ext": "wav" }),
    )
    .await;
    let id = added["id"].as_str().unwrap().to_string();
    assert_eq!(added["name"], "My voice");
    assert_eq!(added["text"], "");
    let seconds = added["seconds"].as_f64().unwrap();
    assert!((3.8..=4.3).contains(&seconds), "{seconds}");

    let updated = call(
        &core,
        "voices.samples.update",
        json!({ "id": id, "text": " 今天天气不错。 ", "name": "" }),
    )
    .await;
    assert_eq!(updated["text"], "今天天气不错。");
    assert_eq!(updated["name"], "My voice");
    let list = call(&core, "voices.samples", json!({})).await;
    assert_eq!(list["samples"].as_array().unwrap().len(), 1);

    let audio = call(&core, "voices.samples.audio", json!({ "id": id })).await;
    assert!(audio["audio"].as_str().unwrap().starts_with("UklGR"));

    // 没装 ZipVoice：念不了
    assert_eq!(
        error(
            &core,
            "voices.speak",
            json!({ "model": "zipvoice-distill-zh-en", "voice": id, "text": "你好" })
        )
        .await,
        "speech_not_installed"
    );

    // 不是我们生成的 ID 一律不认
    assert_eq!(
        error(&core, "voices.samples.audio", json!({ "id": "../../etc" })).await,
        "voice_sample_missing"
    );
    let left = call(&core, "voices.samples.remove", json!({ "id": id })).await;
    assert_eq!(left["samples"], json!([]));
}

#[tokio::test(flavor = "multi_thread")]
async fn speak_with_a_voice_sample() {
    // SHIBAPOST_ZIPVOICE_SMOKE=<放 ZipVoice 文件的文件夹>|<一段录音>|<录音里说的话>
    let Ok(spec) = std::env::var("SHIBAPOST_ZIPVOICE_SMOKE") else {
        eprintln!("SHIBAPOST_ZIPVOICE_SMOKE is not set; skipping");
        return;
    };
    let mut parts = spec.splitn(3, '|');
    let (folder, recording, text) = (
        parts.next().unwrap(),
        parts.next().unwrap(),
        parts.next().unwrap(),
    );
    let tmp = tempfile::tempdir().unwrap();
    let core = Core::open(tmp.path().join("local")).await.unwrap();
    call(
        &core,
        "voices.import",
        json!({ "id": "zipvoice-distill-zh-en", "path": folder }),
    )
    .await;
    let bytes = std::fs::read(recording).unwrap();
    let sample = call(
        &core,
        "voices.samples.add",
        json!({ "name": "Anchor", "audio": shibapost_mail::base64(&bytes), "ext": "wav", "text": text }),
    )
    .await;
    let said = call(
        &core,
        "voices.speak",
        json!({ "model": "zipvoice-distill-zh-en", "voice": sample["id"], "text": "售价5,999元，同比增长5.2%。" }),
    )
    .await;
    let duration = said["duration"].as_f64().unwrap();
    assert!(duration > 2.0 && duration < 15.0, "{duration}");
    assert!(said["audio"].as_str().unwrap().starts_with("UklGR"));
}

#[tokio::test(flavor = "multi_thread")]
async fn sentences_the_interface_no_longer_wants_are_skipped() {
    // 跳过、换篇、停止时，界面把还没念的句子撤回：轮到它们时直接跳过，不白占着模型
    let tmp = tempfile::tempdir().unwrap();
    let core = Core::open(tmp.path().join("local")).await.unwrap();
    call(
        &core,
        "voices.cancel",
        json!({ "requests": ["tab-1", "tab-2"] }),
    )
    .await;
    let speak = |request: &'static str| json!({ "model": "kokoro-v1.1-zh", "voice": "zf_094", "text": "你好", "request": request });
    assert_eq!(
        error(&core, "voices.speak", speak("tab-1")).await,
        "voice_cancelled"
    );
    // 撤回只管一次；没撤回的照常去念（这里没装模型）
    assert_ne!(
        error(&core, "voices.speak", speak("tab-1")).await,
        "voice_cancelled"
    );
    assert_ne!(
        error(&core, "voices.speak", speak("tab-3")).await,
        "voice_cancelled"
    );
}
