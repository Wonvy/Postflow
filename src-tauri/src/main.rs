// 在 Windows 的发布构建里不弹出额外的控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    postflow_desktop_lib::run();
}
