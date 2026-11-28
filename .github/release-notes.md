## Download

| System | File |
|---|---|
| macOS 10.15 or later (Apple silicon and Intel) | `ShibaPost___VERSION___universal.dmg` |
| Windows 10 / 11 | `ShibaPost___VERSION___x64-setup.exe`, or the `.msi` |
| Linux: Debian, Ubuntu | `.deb` |
| Linux: Fedora, openSUSE | `.rpm` |
| Other Linux | `.AppImage` (make it executable, then run it) |

## First launch

The installers are not yet signed by Apple or Microsoft, so your system will ask once:

- **macOS**: if it says the developer cannot be verified, open **System Settings → Privacy & Security** and click
  **Open Anyway** near the bottom. If it says the app is damaged, run
  `xattr -dr com.apple.quarantine /Applications/ShibaPost.app` in Terminal and open it again.
- **Windows**: when SmartScreen appears, click **More info**, then **Run anyway**.

<details>
<summary>中文说明</summary>

### 下载

| 系统 | 下载哪个文件 |
|---|---|
| macOS 10.15 及以上（Apple 芯片和 Intel 通用） | `ShibaPost___VERSION___universal.dmg` |
| Windows 10 / 11 | `ShibaPost___VERSION___x64-setup.exe`，或者 `.msi` |
| Linux：Debian、Ubuntu | `.deb` |
| Linux：Fedora、openSUSE | `.rpm` |
| 其他 Linux | `.AppImage`，加上可执行权限后直接运行 |

### 第一次打开

安装包还没有经过苹果和微软的开发者签名，系统会拦一下：

- **macOS**：提示无法验证开发者时，打开「系统设置 → 隐私与安全性」，在页面底部点「仍要打开」。
  如果提示「已损坏」，在终端运行 `xattr -dr com.apple.quarantine /Applications/ShibaPost.app` 后再打开。
- **Windows**：出现 SmartScreen 提示时，点「更多信息」，再点「仍要运行」。

</details>

