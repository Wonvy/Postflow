//! Windows：装在受保护目录里时的在线更新。
//!
//! 安装程序默认按用户安装，更新时也以普通权限运行。但有人在安装时把位置改到了
//! `D:\Program Files\...` 这类需要管理员权限才能写的目录，普通权限的更新就写不进去，安装失败。
//!
//! 所以更新前先看一眼应用所在的目录能不能写：能写就照常更新（界面用更新插件）；
//! 不能写就走这里：下载并校验更新包（签名不对的不会运行），再通过系统的管理员授权窗口启动安装程序，
//! 参数和更新插件一样（/P 只显示进度，/R 装完重新打开，/UPDATE 更新模式）。用户在授权窗口点「否」时报错，应用照常开着。

use tauri::{AppHandle, Runtime};

/// 下载进度，发给界面的更新窗口。
#[cfg(windows)]
#[derive(Clone, serde::Serialize)]
struct Progress {
    downloaded: u64,
    total: Option<u64>,
}

/// 应用所在的目录能不能写（试着建一个临时文件再删掉）。不是 Windows 时总是 true。
///
/// 用 MSI 装的也总是 true：MSI 装在 Program Files、本来就写不进去，但更新插件交给 msiexec 装，
/// msiexec 自己会弹管理员授权窗口。这里的提权办法只会运行 NSIS 的 setup.exe，MSI 走它必然失败。
#[tauri::command]
pub fn install_dir_writable() -> bool {
    #[cfg(windows)]
    {
        if matches!(
            tauri::utils::platform::bundle_type(),
            Some(tauri::utils::config::BundleType::Msi)
        ) {
            return true;
        }
        let Some(dir) = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        else {
            return true;
        };
        let probe = dir.join(format!(".shibapost-write-test-{}", std::process::id()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe)
        {
            Ok(_) => {
                let _ = std::fs::remove_file(&probe);
                true
            }
            Err(e) => {
                tracing::info!("install folder {} is not writable: {e}", dir.display());
                false
            }
        }
    }
    #[cfg(not(windows))]
    {
        true
    }
}

/// 下载更新，然后以管理员权限运行安装程序。成功启动安装程序后应用退出，由安装程序装完再打开。
#[tauri::command]
pub async fn install_update_elevated<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("only needed on Windows".into())
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use tauri::Emitter;
        use tauri_plugin_updater::UpdaterExt;

        /// 不弹出命令行窗口。
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        let update = app
            .updater()
            .map_err(|e| e.to_string())?
            .check()
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "no update is available".to_string())?;

        let mut downloaded = 0u64;
        let bytes = update
            .download(
                |chunk, total| {
                    downloaded += chunk as u64;
                    let _ = app.emit("shibapost:update-progress", Progress { downloaded, total });
                },
                || {},
            )
            .await
            .map_err(|e| e.to_string())?;

        // 更新包就是签过名的安装程序本身，download 已经校验过签名。这里只会运行 setup.exe：
        // 不是 Windows 程序（比如 MSI）就说清楚，别当成用户在授权窗口点了「否」
        if !bytes.starts_with(b"MZ") {
            return Err("the update package is not an installer program".into());
        }
        // 放进一个名字不好猜的新文件夹，别的程序不容易抢先换掉它
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir =
            std::env::temp_dir().join(format!("shibapost-update-{}-{nonce:x}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!("ShibaPost_{}_x64-setup.exe", update.version));
        std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;

        // Start-Process -Verb RunAs 弹出系统的管理员授权窗口；用户点「否」时它报错，退出码不为 0。
        // PowerShell 把弯引号也当单引号，路径里的都要双写
        let quoted: String = path
            .to_string_lossy()
            .chars()
            .flat_map(|c| {
                let twice = matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}');
                std::iter::repeat_n(c, if twice { 2 } else { 1 })
            })
            .collect();
        let script = format!(
            "Start-Process -FilePath '{quoted}' -ArgumentList '/P','/R','/UPDATE' -Verb RunAs -ErrorAction Stop"
        );
        let status = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &script,
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            let _ = std::fs::remove_dir_all(&dir);
            return Err("elevation_declined".into());
        }
        tracing::info!("started the installer with administrator rights; exiting");
        app.exit(0);
        Ok(())
    }
}
