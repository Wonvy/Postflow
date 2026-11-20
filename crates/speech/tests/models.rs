//! 对着本机的假下载服务把模型管理走一遍：换地址重试、断点续传、校验、测速选下载源、从文件夹导入、
//! 选用和删除。
//! 不加载真的模型（几百兆，不适合放进自动测试）；真模型用 examples/transcribe.rs 手动试。

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;
use postflow_speech::{Error, Speech};
use serde_json::json;
use sha2::{Digest, Sha256};

/// 假模型的两个文件。
fn model_bytes() -> Vec<u8> {
    (0..300_000u32).flat_map(|i| i.to_le_bytes()).collect()
}
const TOKENS: &[u8] = b"<blk> 0\nhello 1\n";

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 一次请求：路径，和它带的 Range 头。
type Request = (String, Option<String>);

#[derive(Clone, Default)]
struct Seen(Arc<Mutex<Vec<Request>>>);

impl Seen {
    fn list(&self) -> Vec<Request> {
        self.0.lock().unwrap().clone()
    }
}

/// `Range: bytes=起点-[终点]`。
fn parse_range(range: Option<&str>, len: usize) -> Option<(usize, usize)> {
    let (start, end) = range?.strip_prefix("bytes=")?.split_once('-')?;
    let start: usize = start.parse().ok()?;
    let end = if end.is_empty() {
        len - 1
    } else {
        end.parse::<usize>().ok()?.min(len - 1)
    };
    (start <= end).then_some((start, end))
}

/// `/good/<名字>` 给对的内容（认 Range），`/bad/<名字>` 给一样长但不对的内容，
/// `/slow/<名字>` 和 good 一样但每次先等一会儿，`/quirky/<名字>` 认 Range 但状态码给 200（像魔搭的小文件），
/// 别的都是 404。
async fn file(
    State(seen): State<Seen>,
    Path((kind, name)): Path<(String, String)>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    seen.0
        .lock()
        .unwrap()
        .push((format!("{kind}/{name}"), range.clone()));
    let mut body = match name.as_str() {
        "model.onnx" => model_bytes(),
        "tokens.txt" => TOKENS.to_vec(),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    match kind.as_str() {
        "good" | "quirky" => {}
        "slow" => tokio::time::sleep(std::time::Duration::from_millis(700)).await,
        "bad" => body.iter_mut().for_each(|b| *b = b.wrapping_add(1)),
        _ => return StatusCode::NOT_FOUND.into_response(),
    }
    if let Some((start, end)) = parse_range(range.as_deref(), body.len()) {
        let len = body.len();
        let part = body[start..=end].to_vec();
        let status = if kind == "quirky" {
            StatusCode::OK
        } else {
            StatusCode::PARTIAL_CONTENT
        };
        let content_range = format!("bytes {start}-{end}/{len}");
        return (status, [(header::CONTENT_RANGE, content_range)], part).into_response();
    }
    body.into_response()
}

/// 起一个假服务，返回它的地址和它见过的请求。
async fn server() -> (String, Seen) {
    let seen = Seen::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let catalog_base = base.clone();
    let app = Router::new()
        .route("/{kind}/{name}", get(file))
        .route(
            "/catalog.json",
            get(move || {
                let base = catalog_base.clone();
                async move { catalog(&base).to_string() }
            }),
        )
        .with_state(seen.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (base, seen)
}

/// 目录里三个模型：`first` 第一个地址是坏的、第二个是好的；`second` 只有好地址；`corrupt` 只有内容不对的地址。
fn catalog(base: &str) -> serde_json::Value {
    let files = |kinds: &[&str]| {
        json!([
            {
                "name": "model.onnx", "role": "model",
                "size": model_bytes().len(), "sha256": sha(&model_bytes()),
                "urls": kinds.iter().map(|k| format!("{base}/{k}/model.onnx")).collect::<Vec<_>>(),
            },
            {
                "name": "tokens.txt", "role": "tokens",
                "size": TOKENS.len(), "sha256": sha(TOKENS),
                "urls": kinds.iter().map(|k| format!("{base}/{k}/tokens.txt")).collect::<Vec<_>>(),
            },
        ])
    };
    let model = |id: &str, kinds: &[&str]| {
        json!({
            "id": id, "name": id, "kind": "sense_voice", "languages": ["zh"],
            "license": "test", "files": files(kinds),
        })
    };
    json!({
        "schema": 1,
        "updated": "2999-01-01T00:00:00Z",
        "models": [
            model("first", &["missing", "good"]),
            model("second", &["good"]),
            model("corrupt", &["bad"]),
        ]
    })
}

async fn speech(root: &std::path::Path, base: &str) -> Speech {
    let speech = Speech::with_catalog_urls(root, vec![format!("{base}/catalog.json")]).unwrap();
    // 还没联网时用的是随程序带的目录
    assert!(speech.catalog().get("sense-voice-small").is_some());
    assert!(speech.catalog_due());
    assert!(speech.refresh_catalog().await);
    assert!(!speech.catalog_due());
    speech
}

async fn download(speech: &Speech, id: &str) -> Result<Vec<(u64, u64)>, Error> {
    assert!(speech.begin_download(id)?);
    let mut progress = Vec::new();
    speech
        .download(id, |p| progress.push((p.received, p.total)))
        .await?;
    Ok(progress)
}

#[tokio::test]
async fn downloads_with_fallback_and_keeps_the_first_choice() {
    let (base, seen) = server().await;
    let tmp = tempfile::tempdir().unwrap();
    let speech = speech(tmp.path(), &base).await;

    let overview = speech.overview();
    assert_eq!(overview.models.len(), 3);
    assert!(overview.models.iter().all(|m| !m.installed && m.supported));
    assert_eq!(overview.selected, None);

    // 第一个地址 404，换第二个
    let progress = download(&speech, "first").await.unwrap();
    let total = (model_bytes().len() + TOKENS.len()) as u64;
    assert_eq!(progress.first(), Some(&(0, total)));
    assert_eq!(progress.last(), Some(&(total, total)));
    assert!(progress.windows(2).all(|w| w[0].0 <= w[1].0));
    let paths: Vec<String> = seen.list().into_iter().map(|(p, _)| p).collect();
    assert_eq!(
        paths,
        [
            "missing/model.onnx",
            "good/model.onnx",
            "missing/tokens.txt",
            "good/tokens.txt"
        ]
    );
    let dir = tmp.path().join("models/first");
    assert_eq!(
        std::fs::read(dir.join("model.onnx")).unwrap(),
        model_bytes()
    );
    assert_eq!(std::fs::read(dir.join("tokens.txt")).unwrap(), TOKENS);
    assert!(!tmp.path().join("models/.first.partial").exists());

    // 第一个装的就是现在用的；再装别的不会悄悄换掉
    assert_eq!(speech.selected().as_deref(), Some("first"));
    download(&speech, "second").await.unwrap();
    assert_eq!(speech.selected().as_deref(), Some("first"));
    speech.select("second").unwrap();
    assert_eq!(speech.selected().as_deref(), Some("second"));
    assert!(matches!(
        speech.select("corrupt"),
        Err(Error::NotInstalled(_))
    ));

    let overview = speech.overview();
    let installed: Vec<&str> = overview
        .models
        .iter()
        .filter(|m| m.installed)
        .map(|m| m.model.id.as_str())
        .collect();
    assert_eq!(installed, ["first", "second"]);

    // 删掉正在用的，换成剩下的那个
    speech.remove("second").unwrap();
    assert_eq!(speech.selected().as_deref(), Some("first"));
    speech.remove("first").unwrap();
    assert_eq!(speech.selected(), None);
    assert!(speech.installed().is_empty());
}

#[tokio::test]
async fn resumes_a_partial_download_and_rejects_corrupt_files() {
    let (base, seen) = server().await;
    let tmp = tempfile::tempdir().unwrap();
    let speech = speech(tmp.path(), &base).await;

    // 上次下到一半
    let partial = tmp.path().join("models/.second.partial");
    std::fs::create_dir_all(&partial).unwrap();
    let half = model_bytes().len() / 2;
    std::fs::write(partial.join("model.onnx"), &model_bytes()[..half]).unwrap();
    download(&speech, "second").await.unwrap();
    assert_eq!(
        seen.list()[0],
        (
            "good/model.onnx".to_string(),
            Some(format!("bytes={half}-"))
        )
    );
    assert_eq!(
        std::fs::read(tmp.path().join("models/second/model.onnx")).unwrap(),
        model_bytes()
    );

    // 内容和校验值对不上的不装
    let err = download(&speech, "corrupt").await.unwrap_err();
    assert!(matches!(err, Error::Checksum(_)), "{err}");
    assert_eq!(err.code(), "speech_download_failed");
    assert!(speech.installed_model("corrupt").is_none());
    // 占位清掉了，可以再试
    assert!(speech.begin_download("corrupt").unwrap());

    assert!(matches!(
        speech.begin_download("nope"),
        Err(Error::UnknownModel(_))
    ));
}

#[tokio::test]
async fn a_partial_file_from_another_mirror_version_is_not_resumed() {
    // 几个镜像上的同名文件不一定一样：上次从别的版本下了一半的，不能接着下（接出来的文件是坏的）
    let (base, seen) = server().await;
    let tmp = tempfile::tempdir().unwrap();
    let speech = speech(tmp.path(), &base).await;
    let partial = tmp.path().join("models/.second.partial");
    std::fs::create_dir_all(&partial).unwrap();
    let half = model_bytes().len() / 2;
    std::fs::write(partial.join("model.onnx"), &model_bytes()[..half]).unwrap();
    std::fs::write(partial.join("model.onnx.variant"), "0".repeat(64)).unwrap();
    download(&speech, "second").await.unwrap();
    // 从头下的，没带 Range
    assert_eq!(seen.list()[0], ("good/model.onnx".to_string(), None));
    assert_eq!(
        std::fs::read(tmp.path().join("models/second/model.onnx")).unwrap(),
        model_bytes()
    );
    // 下完以后记录也清掉了
    assert!(!tmp.path().join("models/second/model.onnx.variant").exists());
}

#[tokio::test]
async fn imports_from_a_folder() {
    let (base, _) = server().await;
    let tmp = tempfile::tempdir().unwrap();
    let speech = speech(&tmp.path().join("speech"), &base).await;
    let folder = tmp.path().join("downloads");
    // 解压出来的压缩包常常多一层目录
    let inner = folder.join("some-model-dir");
    std::fs::create_dir_all(&inner).unwrap();
    std::fs::write(inner.join("tokens.txt"), TOKENS).unwrap();

    let err = speech.import("first", &folder).await.unwrap_err();
    assert!(
        matches!(&err, Error::MissingFile(name) if name == "model.onnx"),
        "{err}"
    );

    std::fs::write(inner.join("model.onnx"), b"not the model").unwrap();
    let err = speech.import("first", &folder).await.unwrap_err();
    assert!(matches!(err, Error::Checksum(_)), "{err}");
    assert!(speech.installed().is_empty());

    std::fs::write(inner.join("model.onnx"), model_bytes()).unwrap();
    speech.import("first", &folder).await.unwrap();
    assert_eq!(speech.selected().as_deref(), Some("first"));
    assert_eq!(
        std::fs::read(tmp.path().join("speech/models/first/tokens.txt")).unwrap(),
        TOKENS
    );
    // 原来的文件夹不动
    assert!(inner.join("model.onnx").exists());
}

#[tokio::test]
async fn an_older_online_catalog_does_not_replace_the_builtin_one() {
    // 刚升级了应用、网上的目录（CDN 缓存）还是旧的：用随程序带的
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let old = catalog(&base)
        .to_string()
        .replace("2999-01-01", "2020-01-01");
    let app = Router::new().route("/catalog.json", get(move || async move { old.clone() }));
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let tmp = tempfile::tempdir().unwrap();
    let speech =
        Speech::with_catalog_urls(tmp.path(), vec![format!("{base}/catalog.json")]).unwrap();
    assert!(!speech.refresh_catalog().await);
    assert!(speech.catalog().get("sense-voice-small").is_some());
    assert!(speech.catalog().get("first").is_none());
    // 下次启动（存下的副本还是旧的）也一样，而且一天之内不用再问
    let again =
        Speech::with_catalog_urls(tmp.path(), vec![format!("{base}/catalog.json")]).unwrap();
    assert!(again.catalog().get("sense-voice-small").is_some());
    assert!(!again.catalog_due());
}

#[tokio::test]
async fn uses_the_builtin_catalog_when_offline_and_writes_the_vad_model() {
    let tmp = tempfile::tempdir().unwrap();
    // 一个没人听的端口
    let speech =
        Speech::with_catalog_urls(tmp.path(), vec!["http://127.0.0.1:9/catalog.json".into()])
            .unwrap();
    assert!(!speech.refresh_catalog().await);
    let overview = speech.overview();
    assert!(
        overview
            .models
            .iter()
            .any(|m| m.model.id == "sense-voice-small")
    );
    assert!(overview.models.iter().all(|m| !m.installed));

    let vad = speech.vad_path().unwrap();
    assert!(std::fs::metadata(&vad).unwrap().len() > 100_000);
    // 被截断了会重新写出来
    std::fs::write(&vad, b"broken").unwrap();
    let again = speech.vad_path().unwrap();
    assert!(std::fs::metadata(&again).unwrap().len() > 100_000);

    // 没装的模型加载不了
    assert!(matches!(
        speech.load("sense-voice-small"),
        Err(Error::NotInstalled(_))
    ));
    assert_eq!(speech.disk_usage().1, 0);
}

/// 测速用的请求：只要开头一兆。
const PROBE: &str = "bytes=0-1048575";

/// 一个文件放在好几个地方（`(网址, 种类)`）的模型，直接写进本机缓存的目录，不用再联网取。
fn write_catalog(root: &std::path::Path, sources: &[(&str, &str)]) {
    let file = |name: &str, bytes: &[u8], role: &str| {
        json!({
            "name": name, "role": role, "size": bytes.len(), "sha256": sha(bytes),
            "urls": sources.iter().map(|(base, kind)| format!("{base}/{kind}/{name}")).collect::<Vec<_>>(),
        })
    };
    let doc = json!({
        "schema": 1,
        "updated": "2999-01-01T00:00:00Z",
        "models": [{
            "id": "multi", "name": "Multi", "kind": "sense_voice", "languages": ["zh"], "license": "test",
            "files": [file("model.onnx", &model_bytes(), "model"), file("tokens.txt", TOKENS, "tokens")],
        }],
    });
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(root.join("catalog.json"), doc.to_string()).unwrap();
}

/// 国内连不上 Hugging Face、国外从魔搭下又慢：先测速，从最快的那里下，连不上的根本不去试。
#[tokio::test]
async fn downloads_from_the_fastest_source() {
    let (fast, fast_seen) = server().await;
    let (slow, slow_seen) = server().await;
    let tmp = tempfile::tempdir().unwrap();
    // 目录里排在前面的一个连不上（没人听的端口），一个慢，最快的排在最后
    write_catalog(
        tmp.path(),
        &[
            ("http://127.0.0.1:9", "good"),
            (&slow, "slow"),
            (&fast, "good"),
        ],
    );
    let speech = Speech::with_catalog_urls(tmp.path(), Vec::new()).unwrap();
    download(&speech, "multi").await.unwrap();
    assert_eq!(
        std::fs::read(tmp.path().join("models/multi/model.onnx")).unwrap(),
        model_bytes()
    );

    // 慢的只被测了一次速，两个文件都从快的那里下
    assert_eq!(
        slow_seen.list(),
        [("slow/model.onnx".to_string(), Some(PROBE.to_string()))]
    );
    assert_eq!(
        fast_seen.list(),
        [
            ("good/model.onnx".to_string(), Some(PROBE.to_string())),
            ("good/model.onnx".to_string(), None),
            ("good/tokens.txt".to_string(), None),
        ]
    );
}

/// 有的服务器续传时只给了要的那一段，状态码却是 200（魔搭的小文件）：照 Content-Range 接着写，不能从头覆盖。
#[tokio::test]
async fn resumes_when_a_range_comes_back_with_status_200() {
    let (base, seen) = server().await;
    let tmp = tempfile::tempdir().unwrap();
    write_catalog(tmp.path(), &[(&base, "quirky")]);
    let partial = tmp.path().join("models/.multi.partial");
    std::fs::create_dir_all(&partial).unwrap();
    let half = model_bytes().len() / 2;
    std::fs::write(partial.join("model.onnx"), &model_bytes()[..half]).unwrap();

    let speech = Speech::with_catalog_urls(tmp.path(), Vec::new()).unwrap();
    download(&speech, "multi").await.unwrap();
    assert_eq!(
        std::fs::read(tmp.path().join("models/multi/model.onnx")).unwrap(),
        model_bytes()
    );
    // 只请求了后一半，没有重下
    assert_eq!(
        seen.list(),
        [
            (
                "quirky/model.onnx".to_string(),
                Some(format!("bytes={half}-"))
            ),
            ("quirky/tokens.txt".to_string(), None),
        ]
    );
}
