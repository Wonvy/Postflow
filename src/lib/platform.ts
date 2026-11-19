// 只有桌面应用才有的能力：系统对话框、用默认浏览器打开链接。
// 在普通浏览器里退回到网页能做的等价操作。

import { api } from "./api";
import { isTauri } from "./transport";

export const isMac = /Mac|iPhone|iPad/i.test(navigator.userAgent);

/** macOS 桌面应用用的是内嵌在窗口里的红绿灯，左上角要给它们留位置。 */
export const hasInsetTitlebar = isTauri && isMac;

export const isWindows = /Windows/i.test(navigator.userAgent);

/**
 * Windows 桌面应用没有系统标题栏，右上角由界面自己画最小化、最大化和关闭三个按钮（WindowControls）。
 * 最右边那一栏的顶栏要给它们留出 WINDOW_CONTROLS_WIDTH 的位置。
 * 开发时可以在浏览器里加上 `?wincontrols` 看效果（按钮不起作用）。
 */
export const hasWindowControls =
  (isTauri && isWindows) || (import.meta.env.DEV && new URLSearchParams(location.search).has("wincontrols"));
export const WINDOW_CONTROLS_WIDTH = 138;

export async function openExternal(url: string): Promise<void> {
  if (!/^(https?:\/\/|mailto:)/i.test(url)) return;
  if (isTauri) {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
  } else {
    window.open(url, "_blank", "noopener,noreferrer");
  }
}

/** 在系统的文件管理器里显示这个目录。只有桌面应用能做到。 */
export async function revealFolder(path: string): Promise<boolean> {
  if (!isTauri) return false;
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
  await revealItemInDir(path);
  return true;
}

/** 让用户选一个目录。浏览器里拿不到真实路径，返回 null，由调用方让用户手动输入。 */
export async function pickDirectory(defaultPath?: string): Promise<string | null> {
  if (!isTauri) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ directory: true, multiple: false, defaultPath, canCreateDirectories: true });
  return typeof picked === "string" ? picked : null;
}

/** 导出订阅列表，返回导出的订阅数；用户取消时返回 null。 */
export async function exportOpml(): Promise<number | null> {
  const filename = "postflow-subscriptions.opml";
  if (isTauri) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const path = await save({
      defaultPath: filename,
      filters: [{ name: "OPML", extensions: ["opml", "xml"] }],
    });
    if (!path) return null;
    return (await api.exportOpml(path)).count;
  }
  const { content, count } = await api.exportOpml();
  const blob = new Blob([content ?? ""], { type: "text/x-opml;charset=utf-8" });
  const href = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = href;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(href);
  return count;
}

export async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    // 剪贴板不可用时静默失败，不值得打断用户
  }
}

/** 用系统的日历应用打开会议邀请；网页里下载 .ics，浏览器会交给日历应用。 */
export async function openInvite(id: string): Promise<void> {
  if (isTauri) {
    const { path } = await api.inviteFile(id, true);
    if (!path) return;
    const { openPath } = await import("@tauri-apps/plugin-opener");
    await openPath(path);
    return;
  }
  const { ics } = await api.inviteFile(id, false);
  const href = URL.createObjectURL(new Blob([ics ?? ""], { type: "text/calendar" }));
  const a = document.createElement("a");
  a.href = href;
  a.download = "invite.ics";
  a.click();
  URL.revokeObjectURL(href);
}

/** 保存邮件附件。返回 false 表示用户取消了。 */
export async function saveAttachment(id: string, index: number, name: string): Promise<boolean> {
  if (isTauri) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const path = await save({ defaultPath: name });
    if (!path) return false;
    await api.attachment(id, index, path);
    return true;
  }
  const file = await api.attachment(id, index);
  const binary = atob(file.data ?? "");
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  const blob = new Blob([bytes], { type: file.mime || "application/octet-stream" });
  const href = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = href;
  a.download = file.name || name;
  a.click();
  URL.revokeObjectURL(href);
  return true;
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

/**
 * 把一段文字存成文件。桌面应用弹保存对话框；浏览器里直接下载。
 * 返回 false 表示用户取消了。
 */
export async function saveTextFile(filename: string, contents: string, filter: { name: string; extensions: string[] }): Promise<boolean> {
  if (isTauri) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const path = await save({ defaultPath: filename, filters: [filter] });
    if (!path) return false;
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("save_text_file", { path, contents });
    return true;
  }
  const blob = new Blob([contents], { type: "application/json;charset=utf-8" });
  const href = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = href;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(href);
  return true;
}
