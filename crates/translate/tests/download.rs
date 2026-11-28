//! 下载、校验、安装和删除模型。用本机的假服务器，不联网。

use std::io::Write;
use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use sha2::{Digest, Sha256};
use shibapost_translate::Translator;

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(data).unwrap();
    gz.finish().unwrap()
}

fn hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 起一个服务器，提供模型目录和 en-zh、en-de 两个模型。en-de 的哈希故意写错。
async fn serve() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let model = b"pretend this is a model".to_vec();
    let catalog = serde_json::json!({
        "baseUrl": base,
        "models": {
            "en-zh": [{
                "releaseStatus": "Release", "sourceLanguage": "en", "targetLanguage": "zh",
                "files": {
                    "model": { "path": "files/model.gz", "uncompressedSize": model.len(), "uncompressedHash": hex(&model) },
                    "srcVocab": { "path": "files/vocab.gz" },
                    "trgVocab": { "path": "files/vocab.gz" },
                    "lexicalShortlist": { "path": "files/lex.gz" }
                }
            }],
            "en-de": [{
                "releaseStatus": "Release", "sourceLanguage": "en", "targetLanguage": "de",
                "files": {
                    "model": { "path": "files/model.gz", "uncompressedSize": model.len(), "uncompressedHash": "00" },
                    "vocab": { "path": "files/vocab.gz" },
                    "lexicalShortlist": { "path": "files/lex.gz" }
                }
            }]
        }
    })
    .to_string();
    let packed = Arc::new(gzip(&model));
    let app = Router::new()
        .route("/models.json", get(move || async move { catalog }))
        .route(
            "/files/model.gz",
            get(move || async move { packed.to_vec() }),
        )
        .route("/files/vocab.gz", get(|| async { gzip(b"vocab") }))
        .route("/files/lex.gz", get(|| async { gzip(b"lex") }));
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    base
}

#[tokio::test]
async fn downloads_and_installs_a_model() {
    let base = serve().await;
    let dir = tempfile::tempdir().unwrap();
    let t = Translator::with_catalog_url(dir.path(), &format!("{base}/models.json")).unwrap();

    let langs = t.languages().await;
    assert!(langs.catalog);
    assert_eq!(langs.targets, vec!["de", "en", "zh"]);
    assert!(langs.installed.is_empty());

    let plan = t
        .plan(
            "Could you take a look at the budget before Friday?",
            None,
            "zh",
        )
        .await
        .unwrap();
    assert_eq!(plan.from.as_deref(), Some("en"));
    assert_eq!(plan.missing.len(), 1);
    assert_eq!(plan.missing[0].pair, "en-zh");

    assert!(t.begin_download("en-zh").unwrap());
    // 同一个语言对不会同时下两份
    assert!(!t.begin_download("en-zh").unwrap());
    let mut reports = 0;
    let installed = t.download("en-zh", |_| reports += 1).await.unwrap();
    assert!(reports >= 2);
    assert_eq!(installed.pair, "en-zh");
    assert_eq!(
        std::fs::read(dir.path().join("en-zh/model.bin")).unwrap(),
        b"pretend this is a model"
    );
    assert_eq!(
        std::fs::read(dir.path().join("en-zh/srcvocab.spm")).unwrap(),
        b"vocab"
    );

    let plan = t
        .plan("Thanks for the notes, see you then.", None, "zh")
        .await
        .unwrap();
    assert!(plan.missing.is_empty());
    assert_eq!(t.installed().len(), 1);
    assert!(t.languages().await.downloads.is_empty());

    // 假模型加载不了，报错而不是崩溃
    let err = t
        .translate(
            "en",
            "zh",
            vec![shibapost_translate::Segment::plain("Hello there, friend.")],
        )
        .unwrap_err();
    assert_eq!(err.code(), "translate_load_failed");

    t.remove("en-zh").unwrap();
    assert!(t.installed().is_empty());
}

#[tokio::test]
async fn rejects_a_corrupt_download() {
    let base = serve().await;
    let dir = tempfile::tempdir().unwrap();
    let t = Translator::with_catalog_url(dir.path(), &format!("{base}/models.json")).unwrap();
    assert!(t.begin_download("en-de").unwrap());
    let err = t.download("en-de", |_| {}).await.unwrap_err();
    assert_eq!(err.code(), "translate_download_failed");
    assert!(t.installed().is_empty());
    // 半截的文件不留在磁盘上
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name() != "catalog.json")
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[tokio::test]
async fn works_offline_with_a_cached_catalog() {
    let base = serve().await;
    let dir = tempfile::tempdir().unwrap();
    let url = format!("{base}/models.json");
    Translator::with_catalog_url(dir.path(), &url)
        .unwrap()
        .languages()
        .await;

    // 换一个连不上的地址：目录缓存还在，照样能用
    let t = Translator::with_catalog_url(dir.path(), "http://127.0.0.1:9/models.json").unwrap();
    assert!(t.languages().await.catalog);

    // 什么都没有缓存、也连不上网
    let empty = tempfile::tempdir().unwrap();
    let t = Translator::with_catalog_url(empty.path(), "http://127.0.0.1:9/models.json").unwrap();
    let langs = t.languages().await;
    assert!(!langs.catalog);
    let err = t
        .plan("Hello there, how are you doing?", None, "zh")
        .await
        .unwrap_err();
    assert_eq!(err.code(), "translate_offline");
}
