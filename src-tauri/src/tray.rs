//! 托盘常驻：关掉窗口时只是藏起来，应用继续在后台收信；托盘图标上显示未读数，菜单里打开窗口或退出。
//!
//! 界面把未读数、菜单文字（界面语言）和「关窗口时留在后台」的设置交过来（`tray_update`）。
//! macOS 上数字直接显示在菜单栏图标旁边；Windows 和 Linux 显示在鼠标悬停的提示里。

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Deserialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime, WindowEvent};

const TRAY_ID: &str = "main";

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

/// 把主窗口拿出来放到最前面：关到后台藏起来的、最小化的、整个应用被隐藏（macOS ⌘H）的都行。
pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    #[cfg(target_os = "macos")]
    let _ = app.show();
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let minimized = window.is_minimized().unwrap_or(false);
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
    if minimized {
        // 从最小化还原有动画，动画结束前系统还当它是最小化的，上面的 set_focus 不起作用，
        // 窗口会回到别的程序后面。等它真的还原了再聚焦一次
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

/// 关窗口：设置了留在后台就只是藏起来。
pub fn on_window_event<R: Runtime>(window: &tauri::Window<R>, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event
        && window.label() == "main"
        && window
            .state::<TrayState>()
            .keep_running
            .load(Ordering::Relaxed)
    {
        api.prevent_close();
        let _ = window.hide();
    }
}

#[derive(Deserialize)]
pub struct TrayUpdate {
    unread: u32,
    /// 鼠标悬停的提示，比如「Postflow · 3 封未读邮件」。
    tooltip: String,
    show_label: String,
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
    let show = MenuItem::with_id(&app, "show", &update.show_label, true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let quit = MenuItem::with_id(&app, "quit", &update.quit_label, true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let separator = PredefinedMenuItem::separator(&app).map_err(|e| e.to_string())?;
    let menu = Menu::with_items(&app, &[&show, &separator, &quit]).map_err(|e| e.to_string())?;
    tray.set_menu(Some(menu)).map_err(|e| e.to_string())?;
    Ok(())
}
