//! 桌面与移动端外壳。这里几乎没有逻辑：把界面的调用转给核心，把核心的事件转给界面。

use std::sync::Arc;

use postflow_core::{Core, Event, RpcError, Scope};
use serde_json::Value;
use tauri::{Emitter, Manager};
use tokio::sync::broadcast::error::RecvError;

#[cfg(desktop)]
mod web_page;

/// 核心事件在界面一侧的事件名。
const EVENT_NAME: &str = "postflow:event";

struct AppState {
    core: Arc<Core>,
}

#[tauri::command]
async fn rpc(
    state: tauri::State<'_, AppState>,
    method: String,
    params: Option<Value>,
    client: Option<String>,
) -> Result<Value, RpcError> {
    tracing::debug!(%method, "rpc");
    let result = state
        .core
        .rpc(&method, params.unwrap_or(Value::Null), client)
        .await;
    // 只记方法名和错误，不记参数：参数里可能有密码
    if let Err(e) = &result {
        tracing::warn!(%method, code = %e.code, "rpc failed: {}", e.message);
    }
    result
}

/// 这个安装方式能不能在应用里直接更新。Linux 上只有 AppImage 可以原地替换自己；
/// 用 deb、rpm 装的版本由系统的包管理器管，界面会改成引导去下载页。
#[tauri::command]
fn updater_supported() -> bool {
    if cfg!(target_os = "linux") {
        std::env::var_os("APPIMAGE").is_some()
    } else {
        cfg!(desktop)
    }
}

/// 界面能调用的命令。原网页模式的网页视图只有桌面端有，手机上界面改用 iframe。
#[cfg(desktop)]
fn handlers<R: tauri::Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        rpc,
        updater_supported,
        web_page::web_show,
        web_page::web_frame,
        web_page::web_hide,
        web_page::web_reload
    ]
}

#[cfg(mobile)]
fn handlers<R: tauri::Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![rpc, updater_supported]
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init());
    // 在线更新只有桌面端有；手机上走应用商店
    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(web_page::WebPageState::default());

    builder
        .setup(|app| {
            // Windows 上不用系统的标题栏，由界面自己画最小化、最大化和关闭（见 WindowControls.tsx）。
            // 去掉边框后 Tauri 仍保留窗口阴影，边缘也还能拖动改大小。
            #[cfg(target_os = "windows")]
            if let Some(window) = app.get_window("main") {
                let _ = window.set_decorations(false);
            }
            // 索引和本机配置放在系统给应用分配的私有目录里，不参与同步
            let local_dir = app.path().app_local_data_dir()?;
            // 日志写到本机目录的 logs 下，设置里能查看；开发时同时打到终端
            postflow_core::logging::init(&local_dir, cfg!(debug_assertions));
            let core = tauri::async_runtime::block_on(Core::open(local_dir))?;
            tracing::info!("core ready");

            let handle = app.handle().clone();
            let mut events = core.subscribe();
            tauri::async_runtime::spawn(async move {
                loop {
                    let event = match events.recv().await {
                        Ok(event) => event,
                        // 漏了事件：让界面把所有东西重新查一遍
                        Err(RecvError::Lagged(_)) => Event::Changed {
                            scope: vec![
                                Scope::Feeds,
                                Scope::Items,
                                Scope::Settings,
                                Scope::Devices,
                            ],
                            source: None,
                        },
                        Err(RecvError::Closed) => break,
                    };
                    let _ = handle.emit(EVENT_NAME, &event);
                }
            });

            app.manage(AppState { core });
            Ok(())
        })
        .invoke_handler(handlers())
        .build(tauri::generate_context!())
        .expect("error while building Postflow")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event
                && let Some(state) = app.try_state::<AppState>()
            {
                state.core.shutdown();
            }
        });
}
