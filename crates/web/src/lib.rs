//! 通过 HTTP 暴露核心，让浏览器里的界面也能用。
//!
//! - `POST /api/rpc`    调用一个方法
//! - `GET  /api/events` 事件流（SSE）
//! - 其余路径           界面的静态文件
//!
//! 两种用法：
//! - 开发时单独运行（`npm run server`）：只接受发往回环地址的请求，没有访问密钥。
//! - 桌面应用里打开「在浏览器中使用」：读写数据的接口（`/api/…`）一定要访问密钥（[`Access::key`]）。
//!   链接里带一次 `?key=`，之后浏览器用 Cookie 记住它。界面本身的文件（开源的界面代码、图标、
//!   推荐订阅源目录）不要密钥，没有密钥时界面会请用户贴上访问链接——手机主屏幕上的网页应用和浏览器
//!   不共用 Cookie，这样第一次打开也有路走。打开「允许局域网访问」时才绑定到所有网卡（[`Access::lan`]）。

use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::{Stream, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use shibapost_core::{Core, Event, Scope};
use tokio::sync::watch;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tower_http::services::{ServeDir, ServeFile};

#[derive(Deserialize)]
struct RpcRequest {
    method: String,
    #[serde(default)]
    params: Value,
    #[serde(default)]
    client: Option<String>,
}

/// 一次请求最多这么大：50 MB 的附件转成 Base64 以后，再留些余量。
const RPC_BODY_LIMIT: usize = 72 * 1024 * 1024;

/// 界面文件从哪里来。
#[derive(Clone)]
pub enum Ui {
    /// 磁盘上构建好的目录（开发时的 dist）。
    Dir(PathBuf),
    /// 桌面应用内置的文件：给路径，返回内容、类型和要带的 Content-Security-Policy。
    /// 找不到的路径应该返回首页（单页应用）。
    Assets(AssetFn),
    /// 只有接口，没有界面。
    None,
}

/// 按路径取界面文件。
pub type AssetFn = Arc<dyn Fn(&str) -> Option<UiFile> + Send + Sync>;

/// 一个界面文件。
pub struct UiFile {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub csp: Option<String>,
}

/// 谁能访问。
#[derive(Clone, Default)]
pub struct Access {
    /// 访问密钥。为空时不检查（只给开发时单独运行用，那时只绑定回环地址）。
    pub key: Option<String>,
    /// 接受发往其他地址（局域网 IP）的请求。必须同时有 `key`。
    pub lan: bool,
}

/// 浏览器记住访问密钥用的 Cookie。
const KEY_COOKIE: &str = "shibapost_key";

#[derive(Clone)]
struct ApiState {
    core: Arc<Core>,
    /// 变成 true 时服务要停：事件流也随之结束，不然浏览器开着就停不下来。
    stop: watch::Receiver<bool>,
    /// 桌面应用里的网页访问（有访问密钥）：只给浏览器用得上的那些方法，见 [`desktop_only`]。
    restricted: bool,
}

/// 只能在桌面应用里调的：参数里带着这台电脑上的路径（把附件、OPML 写到哪里，从哪个文件夹导入模型，
/// 数据目录放哪里），或者往插件目录写代码（桌面应用启用了的插件以应用自己的权限运行）。
/// 网页界面本来就不传路径（附件、OPML 由浏览器自己下载），拿到访问链接的人也不该能读写这台电脑上的
/// 任意文件。
fn desktop_only(method: &str, params: &Value) -> bool {
    matches!(method, "app.setup" | "plugins.install")
        || ["path", "data_dir"]
            .iter()
            .any(|k| params.get(k).is_some_and(|v| !v.is_null()))
}

/// 下好的一期。文件在核心的本机目录里，ID 由核心检查过只有字母数字。
async fn episode(
    State(state): State<ApiState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    request: Request,
) -> Response {
    let Some(file) = state.core.episode_file(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match ServeFile::new(file).try_call(request).await {
        Ok(resp) => resp.map(Body::new),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub fn router(core: Arc<Core>, ui: Ui, access: Access, stop: watch::Receiver<bool>) -> Router {
    let api = Router::new()
        // 写信时附件随请求一起传，默认的 2 MB 不够
        .route(
            "/api/rpc",
            post(rpc).layer(DefaultBodyLimit::max(RPC_BODY_LIMIT)),
        )
        .route("/api/events", get(events))
        // 下载下来离线听的播客单集，支持拖动进度（Range）
        .route("/api/episodes/{id}", get(episode))
        .with_state(ApiState {
            core,
            stop,
            restricted: access.key.is_some(),
        });
    let app = match ui {
        Ui::Dir(dir) => {
            // 界面是单页应用，找不到的路径一律回到首页
            let index = ServeFile::new(dir.join("index.html"));
            api.fallback_service(ServeDir::new(dir).fallback(index))
        }
        Ui::Assets(get) => api.fallback(move |req: Request| {
            let get = get.clone();
            async move { asset(&get, req.uri().path()) }
        }),
        Ui::None => api,
    };
    // 密钥比本机地址更严：有密钥时不用再限定回环地址（局域网访问靠它）
    let access = Arc::new(access);
    app.layer(middleware::from_fn_with_state(access, guard))
        .layer(middleware::from_fn(no_framing))
}

/// 不许别的网页用 iframe 把界面嵌进去（点击劫持）：本机别的端口上的网页也算别的网页。
async fn no_framing(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    res.headers_mut()
        .insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    res
}

/// 运行服务，直到 `stop` 变成 true。
pub async fn serve(
    core: Arc<Core>,
    listener: tokio::net::TcpListener,
    ui: Ui,
    access: Access,
    stop: watch::Receiver<bool>,
) -> std::io::Result<()> {
    let mut until = stop.clone();
    axum::serve(listener, router(core, ui, access, stop))
        .with_graceful_shutdown(async move {
            let _ = until.wait_for(|stopped| *stopped).await;
        })
        .await
}

fn asset(get: &AssetFn, path: &str) -> Response {
    // 单页应用：不是具体文件的路径（包括 /）都给首页
    let Some(file) = get(path).or_else(|| get("/index.html")) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mut res = Response::new(Body::from(file.bytes));
    if let Ok(mime) = HeaderValue::from_str(&file.mime) {
        res.headers_mut().insert(header::CONTENT_TYPE, mime);
    }
    if let Some(csp) = file.csp.and_then(|c| HeaderValue::from_str(&c).ok()) {
        res.headers_mut()
            .insert(header::CONTENT_SECURITY_POLICY, csp);
    }
    res
}

async fn rpc(State(state): State<ApiState>, Json(req): Json<RpcRequest>) -> Response {
    if state.restricted && desktop_only(&req.method, &req.params) {
        tracing::warn!(method = %req.method, "rpc refused: desktop only");
        let error = json!({ "code": "desktop_only", "message": "this can only be done in the desktop app" });
        return (StatusCode::FORBIDDEN, Json(json!({ "error": error }))).into_response();
    }
    match state.core.rpc(&req.method, req.params, req.client).await {
        Ok(result) => Json(json!({ "result": result })).into_response(),
        Err(error) => {
            // 只记方法名和错误，不记参数：参数里可能有密码
            tracing::warn!(method = %req.method, code = %error.code, "rpc failed: {}", error.message);
            let status = match error.code.as_str() {
                "unknown_method" => StatusCode::NOT_FOUND,
                "internal" | "store" | "io" => StatusCode::INTERNAL_SERVER_ERROR,
                _ => StatusCode::BAD_REQUEST,
            };
            (status, Json(json!({ "error": error }))).into_response()
        }
    }
}

async fn events(
    State(state): State<ApiState>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let mut stop = state.stop.clone();
    let stopped = async move {
        let _ = stop.wait_for(|stopped| *stopped).await;
    };
    let stream = BroadcastStream::new(state.core.subscribe()).map(|item| {
        let event = match item {
            Ok(event) => event,
            // 客户端太慢、漏了事件：让它把所有东西重新查一遍
            Err(BroadcastStreamRecvError::Lagged(_)) => Event::Changed {
                scope: vec![Scope::Feeds, Scope::Items, Scope::Settings, Scope::Devices],
                source: None,
            },
        };
        Ok(SseEvent::default().json_data(&event).unwrap_or_default())
    });
    Sse::new(stream.take_until(Box::pin(stopped)))
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(20)))
}

/// 放行规则：
///
/// - 没有局域网访问时，只放行发往回环地址的请求。检查 Host 是为了挡住 DNS 重绑定：恶意网站把自己的
///   域名解析到 127.0.0.1，浏览器就会把它当成同源去读本机服务。
/// - 带 Origin 的请求必须来自同一个地方（没有局域网访问时：本机的任何端口，开发时界面在另一个端口上），
///   挡住其他网站直接往这里发请求。
/// - 有访问密钥时，请求要带着它：Cookie 里，或者网址的 `?key=`（这时种下 Cookie，再跳回不带密钥的地址）。
async fn guard(State(access): State<Arc<Access>>, req: Request, next: Next) -> Response {
    let headers = req.headers();
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(|o| {
            o.strip_prefix("http://")
                .or_else(|| o.strip_prefix("https://"))
                .unwrap_or("")
        });
    let host_ok = access.lan || host.is_some_and(is_loopback_authority);
    let origin_ok = match origin {
        None => true,
        Some(origin) if access.lan => host.is_some_and(|h| h.eq_ignore_ascii_case(origin)),
        Some(origin) => is_loopback_authority(origin),
    };
    if !host_ok || !origin_ok {
        return (
            StatusCode::FORBIDDEN,
            "this server doesn't accept requests from here",
        )
            .into_response();
    }

    let Some(key) = access.key.as_deref() else {
        if req.uri().path() == "/manifest.webmanifest" {
            return manifest("/");
        }
        return next.run(req).await;
    };
    let signed_in = cookie_value(req.headers(), KEY_COOKIE).is_some_and(|v| same(v, key));
    // 链接或二维码里带着密钥：记进 Cookie，跳回不带密钥的地址，免得它留在地址栏和历史记录里
    let (with_key, rest) = split_key(req.uri().query());
    if req.method() == Method::GET && with_key.is_some_and(|v| same(&v, key)) {
        let path = req.uri().path();
        let location = if rest.is_empty() {
            path.to_string()
        } else {
            format!("{path}?{rest}")
        };
        return Response::builder()
            .status(StatusCode::SEE_OTHER)
            .header(header::LOCATION, location)
            // Lax：扫码、点链接这样从别处打开时也带上；别的网站发来的 POST 不带
            .header(
                header::SET_COOKIE,
                format!("{KEY_COOKIE}={key}; Path=/; Max-Age=31536000; HttpOnly; SameSite=Lax"),
            )
            .body(Body::empty())
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }
    // 「添加到主屏幕」用的清单：已经带着密钥的浏览器，主屏幕上的图标直接带着密钥打开
    if req.uri().path() == "/manifest.webmanifest" {
        return manifest(&if signed_in {
            format!("/?key={key}")
        } else {
            "/".to_string()
        });
    }
    if signed_in || !req.uri().path().starts_with("/api/") {
        return next.run(req).await;
    }
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": { "code": "unauthorized", "message": "open ShibaPost with the link from the desktop app" } })),
    )
        .into_response()
}

/// 网页应用清单：手机「添加到主屏幕」后像应用一样全屏打开。
fn manifest(start_url: &str) -> Response {
    let body = json!({
        "name": "ShibaPost",
        "short_name": "ShibaPost",
        "start_url": start_url,
        "scope": "/",
        "display": "standalone",
        "background_color": "#16161a",
        "theme_color": "#16161a",
        "icons": [
            { "src": "/icons/icon-192.png", "sizes": "192x192", "type": "image/png" },
            { "src": "/icons/icon-512.png", "sizes": "512x512", "type": "image/png" },
        ],
    });
    (
        [
            (header::CONTENT_TYPE, "application/manifest+json"),
            // 带着密钥的清单不让中间的缓存留着
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        body.to_string(),
    )
        .into_response()
}

/// 请求里某个 Cookie 的值。
fn cookie_value<'a>(headers: &'a axum::http::HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

/// 从查询字符串里取出 `key`，返回它和剩下的部分。
fn split_key(query: Option<&str>) -> (Option<String>, String) {
    let mut key = None;
    let mut rest = Vec::new();
    for pair in query.unwrap_or("").split('&').filter(|p| !p.is_empty()) {
        match pair.strip_prefix("key=") {
            Some(v) => key = Some(v.to_string()),
            None => rest.push(pair),
        }
    }
    (key, rest.join("&"))
}

/// 比较密钥，耗时和内容无关，免得从响应时间一个字一个字猜出来。
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// 新的访问密钥：32 个字母和数字。
pub fn new_key() -> String {
    const CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    (0..32)
        .map(|_| CHARS[rand::random_range(0..CHARS.len())] as char)
        .collect()
}

fn is_loopback_authority(authority: &str) -> bool {
    let host = if let Some(rest) = authority.strip_prefix('[') {
        rest.split(']').next().unwrap_or_default()
    } else {
        authority
            .rsplit_once(':')
            .map_or(authority, |(host, port)| {
                if port.chars().all(|c| c.is_ascii_digit()) {
                    host
                } else {
                    authority
                }
            })
    };
    let host = host.to_ascii_lowercase();
    host == "localhost" || host == "127.0.0.1" || host == "::1" || host.ends_with(".localhost")
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn app(access: Access) -> (tempfile::TempDir, Router) {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Arc::new(shibapost_core::MemorySecrets::default());
        let core = Core::open_with_secrets(dir.path(), secrets).await.unwrap();
        let ui = Ui::Assets(Arc::new(|path: &str| {
            Some(UiFile {
                bytes: format!("ui:{path}").into_bytes(),
                mime: "text/html".into(),
                csp: None,
            })
        }));
        let (_stop, stopped) = watch::channel(false);
        (dir, router(core, ui, access, stopped))
    }

    async fn send(app: &Router, req: axum::http::Request<Body>) -> Response {
        use tower::ServiceExt;
        app.clone().oneshot(req).await.unwrap()
    }

    fn get(uri: &str, host: &str) -> axum::http::request::Builder {
        axum::http::Request::get(uri).header(header::HOST, host)
    }

    #[tokio::test]
    async fn the_access_key_is_required_and_remembered() {
        let key = new_key();
        let (_dir, app) = app(Access {
            key: Some(key.clone()),
            lan: false,
        })
        .await;

        // 界面本身不要密钥（没有密钥时界面会请用户贴上链接），读写数据的接口要
        let res = send(
            &app,
            get("/", "127.0.0.1:4319").body(Body::empty()).unwrap(),
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let wrong = send(
            &app,
            get("/?key=nope", "127.0.0.1:4319")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert!(
            wrong.headers().get(header::SET_COOKIE).is_none(),
            "a wrong key sets nothing"
        );

        // 链接里的密钥换成 Cookie，跳回不带密钥的地址
        let res = send(
            &app,
            get(&format!("/inbox?key={key}&a=1"), "127.0.0.1:4319")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(res.headers()[header::LOCATION], "/inbox?a=1");
        let cookie = res.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .to_string();
        assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax"));
        let cookie = cookie.split(';').next().unwrap().to_string();

        let res = send(
            &app,
            get("/", "127.0.0.1:4319")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let rpc = axum::http::Request::post("/api/rpc")
            .header(header::HOST, "127.0.0.1:4319")
            .header(header::COOKIE, &cookie)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"method":"app.status"}"#))
            .unwrap();
        assert_eq!(send(&app, rpc).await.status(), StatusCode::OK);
        let no_cookie = axum::http::Request::post("/api/rpc")
            .header(header::HOST, "127.0.0.1:4319")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"method":"app.status"}"#))
            .unwrap();
        assert_eq!(
            send(&app, no_cookie).await.status(),
            StatusCode::UNAUTHORIZED
        );

        // 带着这台电脑上的路径、往插件目录写代码的，网页访问一律不给
        let call = |body: &str| {
            axum::http::Request::post("/api/rpc")
                .header(header::HOST, "127.0.0.1:4319")
                .header(header::COOKIE, &cookie)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap()
        };
        for body in [
            r#"{"method":"mail.attachment","params":{"id":"x","index":0,"path":"/tmp/evil.sh"}}"#,
            r#"{"method":"feeds.export_opml","params":{"path":"/tmp/x.opml"}}"#,
            r#"{"method":"voices.import","params":{"id":"x","path":"//host/share"}}"#,
            r#"{"method":"app.setup","params":{"data_dir":"/tmp/elsewhere"}}"#,
            r#"{"method":"plugins.install","params":{"manifest":"{}","source":""}}"#,
        ] {
            let res = send(&app, call(body)).await;
            assert_eq!(res.status(), StatusCode::FORBIDDEN, "{body}");
            assert_eq!(res.headers()[header::X_FRAME_OPTIONS], "DENY");
        }
        // 不带路径的照常（OPML 的内容交给浏览器自己下载）
        let res = send(
            &app,
            call(r#"{"method":"feeds.export_opml","params":{"path":null}}"#),
        )
        .await;
        assert_ne!(res.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn only_paths_and_plugin_code_are_desktop_only() {
        assert!(desktop_only(
            "mail.attachment",
            &json!({ "id": "x", "index": 0, "path": "/tmp/a" })
        ));
        assert!(!desktop_only(
            "mail.attachment",
            &json!({ "id": "x", "index": 0 })
        ));
        assert!(!desktop_only("feeds.export_opml", &json!({ "path": null })));
        assert!(desktop_only("plugins.install", &json!({})));
        assert!(!desktop_only("plugins.list", &Value::Null));
    }

    #[tokio::test]
    async fn lan_addresses_only_when_allowed_and_never_from_other_sites() {
        let key = new_key();
        let cookie = format!("{KEY_COOKIE}={key}");
        let (_dir, local) = app(Access {
            key: Some(key.clone()),
            lan: false,
        })
        .await;
        let res = send(
            &local,
            get("/", "192.168.1.5:4319")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);

        let (_dir, lan) = app(Access {
            key: Some(key.clone()),
            lan: true,
        })
        .await;
        let res = send(
            &lan,
            get("/", "192.168.1.5:4319")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let api = axum::http::Request::post("/api/rpc")
            .header(header::HOST, "192.168.1.5:4319")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"method":"app.status"}"#))
            .unwrap();
        assert_eq!(
            send(&lan, api).await.status(),
            StatusCode::UNAUTHORIZED,
            "the network alone isn't enough"
        );
        // 别的网站往这里发请求：即使碰巧带着 Cookie 也不行
        let cross = axum::http::Request::post("/api/rpc")
            .header(header::HOST, "192.168.1.5:4319")
            .header(header::ORIGIN, "http://evil.example")
            .header(header::COOKIE, &cookie)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"method":"app.status"}"#))
            .unwrap();
        assert_eq!(send(&lan, cross).await.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn the_manifest_carries_the_key_only_for_a_signed_in_browser() {
        let key = new_key();
        let (_dir, app) = app(Access {
            key: Some(key.clone()),
            lan: false,
        })
        .await;
        async fn read(res: Response) -> Value {
            let bytes = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
            serde_json::from_slice(&bytes).unwrap()
        }
        let anonymous = send(
            &app,
            get("/manifest.webmanifest", "127.0.0.1:4319")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(read(anonymous).await["start_url"], "/");
        let signed_in = send(
            &app,
            get("/manifest.webmanifest", "127.0.0.1:4319")
                .header(header::COOKIE, format!("{KEY_COOKIE}={key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(
            signed_in.headers()[header::CACHE_CONTROL],
            "private, no-store"
        );
        let manifest = read(signed_in).await;
        assert_eq!(manifest["start_url"], format!("/?key={key}"));
        assert_eq!(manifest["display"], "standalone");
    }

    #[tokio::test]
    async fn paths_without_a_file_get_the_index_page() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Arc::new(shibapost_core::MemorySecrets::default());
        let core = Core::open_with_secrets(dir.path(), secrets).await.unwrap();
        // 像开发时的 Tauri 一样：只认得具体的文件名，不会自己回到首页
        let ui = Ui::Assets(Arc::new(|path: &str| {
            (path == "/index.html" || path == "/app.js").then(|| UiFile {
                bytes: path.as_bytes().to_vec(),
                mime: "text/html".into(),
                csp: Some("default-src 'self'".into()),
            })
        }));
        let (_stop, stopped) = watch::channel(false);
        let app = router(core, ui, Access::default(), stopped);
        for (path, body) in [
            ("/", "/index.html"),
            ("/inbox/today", "/index.html"),
            ("/app.js", "/app.js"),
        ] {
            let res = send(
                &app,
                get(path, "127.0.0.1:4319").body(Body::empty()).unwrap(),
            )
            .await;
            assert_eq!(res.status(), StatusCode::OK, "{path}");
            assert_eq!(
                res.headers()[header::CONTENT_SECURITY_POLICY],
                "default-src 'self'"
            );
            let bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
            assert_eq!(&bytes[..], body.as_bytes(), "{path}");
        }
    }

    #[test]
    fn key_helpers() {
        assert_eq!(
            split_key(Some("key=abc&x=1")),
            (Some("abc".into()), "x=1".into())
        );
        assert_eq!(split_key(None), (None, String::new()));
        assert!(same("abc", "abc") && !same("abc", "abd") && !same("abc", "ab"));
        let key = new_key();
        assert_eq!(key.len(), 32);
        assert!(key.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(key, new_key());
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("a=1; shibapost_key=xyz"),
        );
        assert_eq!(cookie_value(&headers, KEY_COOKIE), Some("xyz"));
    }

    #[test]
    fn loopback_detection() {
        for ok in [
            "localhost",
            "localhost:4319",
            "127.0.0.1:4319",
            "[::1]:4319",
            "app.localhost:80",
        ] {
            assert!(is_loopback_authority(ok), "{ok}");
        }
        for bad in [
            "evil.example",
            "evil.example:4319",
            "localhost.evil.example",
            "192.168.1.5:4319",
            "",
        ] {
            assert!(!is_loopback_authority(bad), "{bad}");
        }
    }
}
