//! 桌面与移动端外壳。这里几乎没有逻辑：把界面的调用转给核心，把核心的事件转给界面。

use std::sync::Arc;

use postflow_core::{Core, Event, RpcError, Scope};
use serde_json::Value;
use tauri::utils::config::BundleType;
use tauri::{Emitter, Manager};
use tokio::sync::broadcast::error::RecvError;

#[cfg(desktop)]
mod deep_link;
#[cfg(desktop)]
mod edge_hide;
#[cfg(desktop)]
mod elevated_update;
#[cfg(desktop)]
mod tray;
#[cfg(desktop)]
mod web_access;
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

/// 把界面生成的文字存到用户在保存对话框里选的位置（导出语言文件）。
/// 只有桌面应用的界面能调用；网页访问时界面改用浏览器下载。
#[tauri::command]
async fn save_text_file(path: String, contents: String) -> Result<(), String> {
    // 文件很小，直接写
    std::fs::write(&path, contents).map_err(|e| e.to_string())
}

/// 打包时写进程序的安装包类型（Tauri 打包工具按 deb、rpm、AppImage 分别改写）。
/// 直接 `cargo run` 的开发版没有，是 None。
fn linux_package() -> Option<BundleType> {
    if cfg!(target_os = "linux") {
        tauri::utils::platform::bundle_type()
    } else {
        None
    }
}

/// 这个安装方式能不能在应用里直接更新。Linux 上 AppImage 原地替换自己；deb、rpm 由更新插件
/// 交给 dpkg、rpm 安装（系统会弹窗要管理员密码）。认不出安装方式时界面引导去下载页。
#[tauri::command]
fn updater_supported() -> bool {
    if cfg!(target_os = "linux") {
        std::env::var_os("APPIMAGE").is_some()
            || matches!(
                linux_package(),
                Some(BundleType::AppImage | BundleType::Deb | BundleType::Rpm)
            )
    } else {
        cfg!(desktop)
    }
}

/// 安装更新时系统会不会要管理员密码：Linux 上用 deb、rpm 装的。更新窗口提前说一声。
#[cfg(desktop)]
#[tauri::command]
fn update_needs_password() -> bool {
    matches!(linux_package(), Some(BundleType::Deb | BundleType::Rpm))
}

/// 界面能调用的命令。原网页模式的网页视图只有桌面端有，手机上界面改用 iframe。
#[cfg(desktop)]
fn handlers<R: tauri::Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        rpc,
        save_text_file,
        updater_supported,
        update_needs_password,
        deep_link::pending_saves,
        elevated_update::install_dir_writable,
        elevated_update::install_update_elevated,
        web_page::web_show,
        web_page::web_frame,
        web_page::web_hide,
        web_page::web_reload,
        web_access::web_access_status,
        web_access::web_access_update,
        tray::tray_update
    ]
}

#[cfg(mobile)]
fn handlers<R: tauri::Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![rpc, save_text_file, updater_supported]
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    // 只运行一个：再次打开 Postflow 时，把已经开着的窗口拿到最前面，新启动的那个直接退出。
    // 两个进程同时写同一份数据和索引会出问题。这个插件要第一个注册
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        tray::show_main(app)
    }));
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_deep_link::init());
    let builder = builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init());
    // 在线更新只有桌面端有；手机上走应用商店
    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(web_page::WebPageState::default())
        .manage(web_access::WebAccess::default())
        .manage(tray::TrayState::default())
        .manage(edge_hide::EdgeHide::default())
        .manage(deep_link::PendingSaves::default())
        // 关窗口时留在后台（托盘常驻）
        .on_window_event(tray::on_window_event);

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

            // 上次开着「在浏览器中使用」就接着开
            #[cfg(desktop)]
            web_access::start(app.handle(), core.clone());
            // 托盘图标：关窗口后还能从这里打开，也显示未读数
            #[cfg(desktop)]
            if let Err(err) = tray::create(app.handle()) {
                tracing::warn!("tray icon: {err}");
            }
            // 靠边自动隐藏：后台看着窗口，设置打开后才动它
            #[cfg(desktop)]
            edge_hide::start(app.handle());
            // postflow://save?url=… ：浏览器书签按钮存网页
            #[cfg(desktop)]
            deep_link::setup(app.handle());

            app.manage(AppState { core });
            Ok(())
        })
        .invoke_handler(handlers())
        .build(tauri::generate_context!())
        .expect("error while building Postflow")
        .run(|app, event| match event {
            tauri::RunEvent::Exit => {
                if let Some(state) = app.try_state::<AppState>() {
                    state.core.shutdown();
                }
            }
            // macOS：窗口藏起来以后点程序坞图标，把它拿回来
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => tray::show_main(app),
            _ => {}
        });
}
