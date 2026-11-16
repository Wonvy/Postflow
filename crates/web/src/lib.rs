//! 通过 HTTP 暴露核心，让浏览器里的界面也能用。
//!
//! - `POST /api/rpc`    调用一个方法
//! - `GET  /api/events` 事件流（SSE）
//! - 其余路径           界面的静态文件
//!
//! 目前只面向本机：只接受发往回环地址的请求，没有登录。
//! 开放给局域网需要先加上访问密码，在那之前不要把它绑定到其他网卡。

use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::{Stream, StreamExt};
use postflow_core::{Core, Event, Scope};
use serde::Deserialize;
use serde_json::{Value, json};
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

pub fn router(core: Arc<Core>, static_dir: Option<PathBuf>) -> Router {
    let api = Router::new()
        .route("/api/rpc", post(rpc))
        .route("/api/events", get(events))
        .with_state(core);
    let app = match static_dir {
        Some(dir) => {
            // 界面是单页应用，找不到的路径一律回到首页
            let index = ServeFile::new(dir.join("index.html"));
            api.fallback_service(ServeDir::new(dir).fallback(index))
        }
        None => api,
    };
    app.layer(middleware::from_fn(loopback_only))
}

pub async fn serve(
    core: Arc<Core>,
    listener: tokio::net::TcpListener,
    static_dir: Option<PathBuf>,
) -> std::io::Result<()> {
    axum::serve(listener, router(core, static_dir)).await
}

async fn rpc(State(core): State<Arc<Core>>, Json(req): Json<RpcRequest>) -> Response {
    match core.rpc(&req.method, req.params, req.client).await {
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
    State(core): State<Arc<Core>>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let stream = BroadcastStream::new(core.subscribe()).map(|item| {
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
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(20)))
}

/// 只放行发往回环地址、且来自本机页面的请求。
///
/// 检查 Host 是为了挡住 DNS 重绑定：恶意网站把自己的域名解析到 127.0.0.1，
/// 浏览器就会把它当成同源去读本机服务。检查 Origin 是为了挡住其他网站直接往本机发请求。
async fn loopback_only(req: Request, next: Next) -> Response {
    let headers = req.headers();
    let host_ok = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .is_some_and(is_loopback_authority);
    let origin_ok = match headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        None => true,
        Some(origin) => origin
            .strip_prefix("http://")
            .or_else(|| origin.strip_prefix("https://"))
            .is_some_and(is_loopback_authority),
    };
    if host_ok && origin_ok {
        next.run(req).await
    } else {
        (
            StatusCode::FORBIDDEN,
            "this server only accepts requests from this machine",
        )
            .into_response()
    }
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
