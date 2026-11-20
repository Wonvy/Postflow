//! 托盘常驻：关掉窗口时只是藏起来，应用继续在后台收信；托盘图标上显示未读数，菜单里打开窗口或退出。
//!
//! 界面把未读数、菜单文字（界面语言）和「关窗口时留在后台」的设置交过来（`tray_update`）。
//! macOS 上数字直接显示在菜单栏图标旁边；Windows 和 Linux 显示在鼠标悬停的提示里。

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Deserialize;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow, WindowEvent};

const TRAY_ID: &str = "main";
/// 托盘菜单里要界面去做的事，发给界面的事件名。
const ACTION_EVENT: &str = "postflow://tray-action";

pub struct TrayState {
    /// 关窗口时留在后台。界面启动时按设置告诉这里；默认留着。
    keep_running: AtomicBool,
}

impl Default for TrayState {
    fn default() -> Self {
        Self {
            keep_running: AtomicBool::new(true),
        }
    }
}

/// 关窗口时是不是留在后台（托盘常驻）。
pub fn keeps_running<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.state::<TrayState>()
        .keep_running
        .load(Ordering::Relaxed)
}

/// 应用真的要退出了：先把托盘图标拿掉。Windows 上进程退出时不会自己拿掉，留下一个点了没反应的图标，
/// 鼠标划过才消失。
pub fn remove(app: &AppHandle<impl Runtime>) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_visible(false);
    }
}

/// 把主窗口拿出来放到最前面：关到后台藏起来的、最小化的、整个应用被隐藏（macOS ⌘H）的、
/// 靠边藏着的都行。主窗口要是已经没了（关窗口没被拦住，见 `on_window_event`），照配置重新建一个。
pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    #[cfg(target_os = "macos")]
    let _ = app.show();
    // 靠边藏着的窗口先拿出来
    crate::edge_hide::reveal(app);
    let Some(window) = app.get_webview_window("main") else {
        // Windows 上在事件处理里建窗口会卡死（WebView2 的限制），放到别的线程去建
        let app = app.clone();
        std::thread::spawn(move || {
            if let Some(window) = recreate_main(&app) {
                bring_up(&window);
            }
        });
        return;
    };
    bring_up(&window);
}

/// 把窗口显示出来、还原、放回屏幕上、拿到最前面。
fn bring_up<R: Runtime>(window: &WebviewWindow<R>) {
    let visible = window.is_visible().unwrap_or(false);
    let minimized = window.is_minimized().unwrap_or(false);
    tracing::info!(visible, minimized, "showing the main window");
    let _ = window.show();
    let _ = window.unminimize();
    keep_on_screen(window);
    let _ = window.set_focus();
    // Windows 不一定让后台的程序把窗口放到最前面：临时置顶一下再取消，窗口就到了别的窗口前面
    #[cfg(target_os = "windows")]
    {
        let _ = window.set_always_on_top(true);
        let _ = window.set_always_on_top(false);
    }
    if minimized {
        // 从最小化还原有动画，动画结束前系统还当它是最小化的，上面的 set_focus 不起作用，
        // 窗口会回到别的程序后面。等它真的还原了再聚焦一次
        let window = window.clone();
        std::thread::spawn(move || {
            for _ in 0..40 {
                std::thread::sleep(Duration::from_millis(50));
                if !window.is_minimized().unwrap_or(false) {
                    let _ = window.set_focus();
                    return;
                }
            }
        });
    }
}

/// 窗口不在任何一块屏幕上（拔掉了外接屏幕、靠边藏的时候被关掉之类）：挪回当前屏幕的正中。
fn keep_on_screen<R: Runtime>(window: &WebviewWindow<R>) {
    let (Ok(pos), Ok(size), Ok(monitors)) = (
        window.outer_position(),
        window.outer_size(),
        window.available_monitors(),
    ) else {
        return;
    };
    // 标题栏那一带（顶边往下 40 像素、左右中间）在某块屏幕上就算看得见
    let x = pos.x + size.width as i32 / 2;
    let y = pos.y + 40;
    let seen = monitors.iter().any(|m| {
        let a = m.work_area();
        x >= a.position.x
            && x < a.position.x + a.size.width as i32
            && y >= a.position.y
            && y < a.position.y + a.size.height as i32
    });
    if !seen {
        tracing::info!(
            x = pos.x,
            y = pos.y,
            "main window was off screen; centering it"
        );
        let _ = window.center();
    }
}

/// 照配置文件重新建主窗口。只在别的线程上调（见 `show_main`）。
fn recreate_main<R: Runtime>(app: &AppHandle<R>) -> Option<WebviewWindow<R>> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")?
        .clone();
    tracing::warn!("the main window was gone; creating it again");
    let window =
        match tauri::WebviewWindowBuilder::from_config(app, &config).and_then(|b| b.build()) {
            Ok(window) => window,
            Err(err) => {
                tracing::warn!("could not create the main window: {err}");
                return None;
            }
        };
    #[cfg(target_os = "windows")]
    let _ = window.set_decorations(false);
    Some(window)
}

/// 建托盘图标。菜单文字先用英文，界面加载后换成界面语言。
pub fn create<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Open Postflow", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Postflow", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &PredefinedMenuItem::separator(app)?, &quit])?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Postflow")
        .menu(&menu)
        // 左键点图标打开窗口，右键出菜单
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "quit" => app.exit(0),
            // 写邮件、检查新内容、设置：把窗口拿出来，交给界面去做
            id @ ("compose" | "refresh" | "settings") => {
                show_main(app);
                let _ = app.emit(ACTION_EVENT, id);
            }
            // 靠边隐藏的开关：界面改设置，再告诉这里
            "edge" => {
                let _ = app.emit(ACTION_EVENT, "edge");
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// 关窗口：设置了留在后台就只是藏起来。万一主窗口还是被销毁了（有报告说 Windows 上最小化、
/// 贴靠以后关窗口会拦不住），应用不跟着退出（见 lib.rs 的 `ExitRequested`），从托盘打开时重新建。
pub fn on_window_event<R: Runtime>(window: &tauri::Window<R>, event: &WindowEvent) {
    if window.label() != "main" {
        return;
    }
    match event {
        WindowEvent::CloseRequested { api, .. } if keeps_running(window.app_handle()) => {
            api.prevent_close();
            let _ = window.hide();
            tracing::info!("main window closed to the tray");
        }
        WindowEvent::Destroyed => tracing::info!("main window destroyed"),
        _ => {}
    }
}

#[derive(Deserialize)]
pub struct TrayUpdate {
    unread: u32,
    /// 鼠标悬停的提示，比如「Postflow · 3 封未读邮件」。
    tooltip: String,
    /// 菜单最上面一行（不能点）：「3 封未读邮件」。没有未读时不显示。
    #[serde(default)]
    unread_label: Option<String>,
    show_label: String,
    #[serde(default)]
    compose_label: Option<String>,
    #[serde(default)]
    refresh_label: Option<String>,
    #[serde(default)]
    settings_label: Option<String>,
    /// 「靠边自动隐藏」勾选项的文字和现在是否打开。
    #[serde(default)]
    edge_label: Option<String>,
    #[serde(default)]
    edge_hide: bool,
    quit_label: String,
    keep_running: bool,
}

#[tauri::command]
pub fn tray_update<R: Runtime>(app: AppHandle<R>, update: TrayUpdate) -> Result<(), String> {
    app.state::<TrayState>()
        .keep_running
        .store(update.keep_running, Ordering::Relaxed);
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Ok(());
    };
    let _ = tray.set_tooltip(Some(&update.tooltip));
    // macOS：数字显示在菜单栏图标旁边。其他系统没有这个位置，数字已经写在悬停提示里
    #[cfg(target_os = "macos")]
    let _ = tray.set_title(if update.unread > 0 {
        Some(update.unread.to_string())
    } else {
        None
    });
    #[cfg(not(target_os = "macos"))]
    let _ = update.unread;
    let menu = build_menu(&app, &update).map_err(|e| e.to_string())?;
    tray.set_menu(Some(menu)).map_err(|e| e.to_string())?;
    crate::edge_hide::set_enabled(&app, update.edge_hide);
    Ok(())
}

/// 托盘菜单：未读数、打开、写邮件、检查新内容、设置、靠边自动隐藏、退出。
fn build_menu<R: Runtime>(app: &AppHandle<R>, update: &TrayUpdate) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    if let Some(label) = &update.unread_label {
        menu.append(&MenuItem::with_id(
            app,
            "unread",
            label,
            false,
            None::<&str>,
        )?)?;
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }
    menu.append(&MenuItem::with_id(
        app,
        "show",
        &update.show_label,
        true,
        None::<&str>,
    )?)?;
    for (id, label) in [
        ("compose", &update.compose_label),
        ("refresh", &update.refresh_label),
    ] {
        if let Some(label) = label {
            menu.append(&MenuItem::with_id(app, id, label, true, None::<&str>)?)?;
        }
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    if let Some(label) = &update.edge_label {
        menu.append(&CheckMenuItem::with_id(
            app,
            "edge",
            label,
            true,
            update.edge_hide,
            None::<&str>,
        )?)?;
    }
    if let Some(label) = &update.settings_label {
        menu.append(&MenuItem::with_id(
            app,
            "settings",
            label,
            true,
            None::<&str>,
        )?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "quit",
        &update.quit_label,
        true,
        None::<&str>,
    )?)?;
    Ok(menu)
}
