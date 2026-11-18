//! 阅读区「原网页」模式用的网页视图。
//!
//! 很多网站不允许被嵌进 iframe，也没法给 iframe 换 User-Agent，所以桌面应用在主窗口里
//! 另放一个系统网页视图，叠在阅读区上面，位置和大小由界面随时告诉这里。
//! 它加载的是网上的页面，不在任何权限配置里，调不了应用的命令。

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::webview::{NewWindowResponse, PageLoadEvent, WebviewBuilder};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Runtime, State, Url, Webview,
    WebviewUrl,
};
use tauri_plugin_opener::OpenerExt;

/// 网页视图加载状态在界面一侧的事件名。
const EVENT_NAME: &str = "postflow:web";

/// 换页面时网页视图先收着，新页面加载完再显示，免得先闪一下空白页。加载太慢时最多等这么久。
const REVEAL_AFTER: Duration = Duration::from_millis(2500);

/// 阅读区里放网页的那块地方，单位是逻辑像素，相对窗口左上角。
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Frame {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    /// 页面缩放比例。模拟手机时视口比阅读区大，整体缩小放进去。
    zoom: f64,
}

#[derive(Clone, Serialize)]
struct LoadEvent {
    url: String,
    loading: bool,
}

/// 现在这个网页视图。User-Agent 只能在创建时设置，换了就重建一个。
struct Current {
    label: String,
    user_agent: Option<String>,
    /// 界面最后一次要求打开的地址。用户在网页里点进别的页面后，网页视图的地址和它不同。
    url: Url,
}

#[derive(Default)]
pub struct WebPageState {
    current: Mutex<Option<Current>>,
    serial: Mutex<u32>,
    /// 正在等新页面加载出来再显示：这一次等待的编号。收起时清掉，迟到的「加载完了」不会再把它显示出来。
    reveal: Mutex<Option<u64>>,
    reveals: AtomicU64,
}

/// 先收着网页视图，等新页面加载完（或者等了 `REVEAL_AFTER`）再显示。
fn reveal_when_loaded<R: Runtime>(app: &AppHandle<R>, state: &WebPageState) {
    let ticket = state.reveals.fetch_add(1, Ordering::Relaxed) + 1;
    if let Ok(mut pending) = state.reveal.lock() {
        *pending = Some(ticket);
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(REVEAL_AFTER);
        reveal(&app, Some(ticket));
    });
}

/// 把在等的网页视图显示出来。`ticket` 是超时那一路带的编号，只认这一次等待；页面加载完时不带。
fn reveal<R: Runtime>(app: &AppHandle<R>, ticket: Option<u64>) {
    let state = app.state::<WebPageState>();
    {
        let Ok(mut pending) = state.reveal.lock() else {
            return;
        };
        match (*pending, ticket) {
            (None, _) => return,
            (Some(waiting), Some(t)) if waiting != t => return,
            _ => *pending = None,
        }
    }
    if let Some(webview) = current(app, &state) {
        let _ = webview.show();
    }
}

/// 还在等新页面加载，先别显示。
fn revealing(state: &WebPageState) -> bool {
    state.reveal.lock().map(|p| p.is_some()).unwrap_or(false)
}

/// 不等了：收起时调用。
fn cancel_reveal(state: &WebPageState) {
    if let Ok(mut pending) = state.reveal.lock() {
        *pending = None;
    }
}

fn web_url(url: &str) -> Result<Url, String> {
    let url: Url = url.parse().map_err(|_| format!("invalid url: {url}"))?;
    if matches!(url.scheme(), "http" | "https") {
        Ok(url)
    } else {
        Err("only http and https pages can be shown".into())
    }
}

fn current<R: Runtime>(app: &AppHandle<R>, state: &WebPageState) -> Option<Webview<R>> {
    let label = state.current.lock().ok()?.as_ref()?.label.clone();
    app.get_webview(&label)
}

fn place<R: Runtime>(webview: &Webview<R>, frame: Frame) -> tauri::Result<()> {
    webview.set_position(LogicalPosition::new(frame.x, frame.y))?;
    webview.set_size(LogicalSize::new(
        frame.width.max(1.0),
        frame.height.max(1.0),
    ))?;
    webview.set_zoom(frame.zoom.clamp(0.1, 4.0))
}

/// 网页里要开新窗口、或者跳到 mailto: 之类的地址时，交给系统处理。
fn open_outside<R: Runtime>(app: &AppHandle<R>, url: &Url) {
    if matches!(url.scheme(), "http" | "https" | "mailto" | "tel") {
        let _ = app.opener().open_url(url.as_str(), None::<&str>);
    }
}

/// 显示 `url`。第一次调用、或者换了 User-Agent 时新建网页视图，否则在原来的里面打开。
#[tauri::command]
pub async fn web_show<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, WebPageState>,
    url: String,
    user_agent: Option<String>,
    frame: Frame,
) -> Result<(), String> {
    let url = web_url(&url)?;
    let existing = current(&app, &state);
    let (same_agent, same_url) = {
        let current = state.current.lock().map_err(|e| e.to_string())?;
        match current.as_ref() {
            Some(c) => (c.user_agent == user_agent, c.url == url),
            None => (false, false),
        }
    };

    if let (Some(webview), true) = (&existing, same_agent) {
        place(webview, frame).map_err(|e| e.to_string())?;
        // 同一个地址再要一次（比如回到原网页模式）时不重新加载，留在用户点进去的页面上
        if same_url {
            if revealing(&state) {
                return Ok(());
            }
            return webview.show().map_err(|e| e.to_string());
        }
        // 换了地址：收着等它加载出来，不显示上一页或者空白页
        webview.hide().map_err(|e| e.to_string())?;
        webview.navigate(url.clone()).map_err(|e| e.to_string())?;
        if let Some(c) = state.current.lock().map_err(|e| e.to_string())?.as_mut() {
            c.url = url;
        }
        reveal_when_loaded(&app, &state);
        return Ok(());
    }

    if let Some(old) = existing {
        let _ = old.close();
    }
    let window = app.get_window("main").ok_or("main window is gone")?;
    // 标签不能重复，旧的关闭是异步的，所以每次用新的标签
    let label = {
        let mut serial = state.serial.lock().map_err(|e| e.to_string())?;
        *serial += 1;
        format!("web-page-{serial}")
    };
    let opener = app.clone();
    let navigator = app.clone();
    let mut builder = WebviewBuilder::new(&label, WebviewUrl::External(url.clone()))
        .on_new_window(move |url, _| {
            open_outside(&opener, &url);
            NewWindowResponse::Deny
        })
        .on_navigation(move |url| {
            let inside = matches!(url.scheme(), "http" | "https" | "about" | "blob" | "data");
            if !inside {
                open_outside(&navigator, url);
            }
            inside
        })
        .on_page_load(|webview, payload| {
            let finished = matches!(payload.event(), PageLoadEvent::Finished);
            if finished && payload.url().scheme() != "about" {
                reveal(webview.app_handle(), None);
            }
            let _ = webview.app_handle().emit_to(
                "main",
                EVENT_NAME,
                LoadEvent {
                    url: payload.url().to_string(),
                    loading: matches!(payload.event(), PageLoadEvent::Started),
                },
            );
        });
    if let Some(agent) = &user_agent {
        builder = builder.user_agent(agent);
    }
    let webview = window
        .add_child(
            builder,
            LogicalPosition::new(frame.x, frame.y),
            LogicalSize::new(frame.width.max(1.0), frame.height.max(1.0)),
        )
        .map_err(|e| e.to_string())?;
    let _ = webview.set_zoom(frame.zoom.clamp(0.1, 4.0));
    // 新建的网页视图一开始是白的，同样等页面加载出来再显示
    let _ = webview.hide();
    *state.current.lock().map_err(|e| e.to_string())? = Some(Current {
        label,
        user_agent,
        url,
    });
    reveal_when_loaded(&app, &state);
    Ok(())
}

/// 阅读区挪了位置或者变了大小。`show` 时把收起的网页视图重新显示出来。
#[tauri::command]
pub async fn web_frame<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, WebPageState>,
    frame: Frame,
    show: Option<bool>,
) -> Result<(), String> {
    let Some(webview) = current(&app, &state) else {
        return Ok(());
    };
    place(&webview, frame).map_err(|e| e.to_string())?;
    // 还在等新页面加载时不显示，加载完会自己出来
    if show == Some(true) && !revealing(&state) {
        webview.show().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 收起网页视图：上面要弹出菜单和对话框（`clear` 为否，菜单关了原样显示回来），或者关掉了原网页模式、
/// 换到没有链接的条目（`clear`：换成空白页，正在播的视频、音频随之停下，不会在后台接着放）。
/// 网页视图本身不销毁，下次显示更快，登录状态之类也还在。
#[tauri::command]
pub async fn web_hide<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, WebPageState>,
    clear: Option<bool>,
) -> Result<(), String> {
    cancel_reveal(&state);
    let Some(webview) = current(&app, &state) else {
        return Ok(());
    };
    webview.hide().map_err(|e| e.to_string())?;
    if clear == Some(true) {
        let blank: Url = "about:blank".parse().map_err(|_| "invalid blank url")?;
        webview.navigate(blank.clone()).map_err(|e| e.to_string())?;
        // 下次要同一个地址时重新加载
        if let Some(c) = state.current.lock().map_err(|e| e.to_string())?.as_mut() {
            c.url = blank;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn web_reload<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, WebPageState>,
) -> Result<(), String> {
    match current(&app, &state) {
        Some(webview) => webview.reload().map_err(|e| e.to_string()),
        None => Ok(()),
    }
}
