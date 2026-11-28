//! `shibapost://` 链接：浏览器里的「存到 ShibaPost」书签按钮打开 `shibapost://save?url=<网页地址>`，
//! 系统把它交给 ShibaPost（已经开着的话由单实例插件转给开着的那个），这里把地址交给界面存进稍后读。
//!
//! 应用还没启动、被链接拉起来时，界面还没准备好收事件，所以地址先记着，界面起来后用
//! `pending_saves` 取走。

use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, Url};
use tauri_plugin_deep_link::DeepLinkExt;

/// 发给界面的事件，内容是要存的网页地址。
const SAVE_EVENT: &str = "shibapost://save-later";

#[derive(Default)]
pub struct PendingSaves(Mutex<Vec<String>>);

/// `shibapost://save?url=…` 里要存的地址。只收 http 和 https。
fn save_target(url: &Url) -> Option<String> {
    if url.scheme() != "shibapost" || url.host_str() != Some("save") {
        return None;
    }
    let target = url.query_pairs().find(|(k, _)| k == "url")?.1.into_owned();
    (target.starts_with("https://") || target.starts_with("http://")).then_some(target)
}

fn handle(app: &AppHandle, url: &Url) {
    let Some(target) = save_target(url) else {
        tracing::info!("ignoring an unknown shibapost:// link");
        return;
    };
    crate::tray::show_main(app);
    if let Some(state) = app.try_state::<PendingSaves>() {
        state
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(target.clone());
    }
    let _ = app.emit(SAVE_EVENT, target);
}

/// 注册 `shibapost://`，开始接收链接。启动时就带着的链接也处理。
pub fn setup(app: &AppHandle) {
    // Windows 和 Linux 的安装包会登记链接；直接运行（开发、便携）时这里再登记一次
    #[cfg(any(windows, target_os = "linux"))]
    if let Err(err) = app.deep_link().register_all() {
        tracing::info!("could not register shibapost:// links: {err}");
    }
    let handle_app = app.clone();
    app.deep_link().on_open_url(move |event| {
        for url in event.urls() {
            handle(&handle_app, &url);
        }
    });
    if let Ok(Some(urls)) = app.deep_link().get_current() {
        for url in urls {
            handle(app, &url);
        }
    }
}

/// 界面起来以后取走还没存的地址（应用被链接拉起来时，事件比界面早）。
#[tauri::command]
pub fn pending_saves(state: tauri::State<'_, PendingSaves>) -> Vec<String> {
    std::mem::take(&mut *state.0.lock().unwrap_or_else(|e| e.into_inner()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_save_links_with_web_addresses() {
        let url = |s: &str| Url::parse(s).unwrap();
        assert_eq!(
            save_target(&url(
                "shibapost://save?url=https%3A%2F%2Fexample.com%2Fa%3Fb%3D1"
            ))
            .as_deref(),
            Some("https://example.com/a?b=1")
        );
        assert_eq!(
            save_target(&url("shibapost://save?url=javascript%3Aalert(1)")),
            None
        );
        assert_eq!(
            save_target(&url("shibapost://open?url=https%3A%2F%2Fexample.com")),
            None
        );
        assert_eq!(
            save_target(&url("other://save?url=https%3A%2F%2Fexample.com")),
            None
        );
    }
}
