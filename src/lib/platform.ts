// 只有桌面应用才有的能力：系统对话框、用默认浏览器打开链接。
// 在普通浏览器里退回到网页能做的等价操作。

import { api } from "./api";
import { isTauri } from "./transport";

export const isMac = /Mac|iPhone|iPad/i.test(navigator.userAgent);

/** macOS 桌面应用用的是内嵌在窗口里的红绿灯，左上角要给它们留位置。 */
export const hasInsetTitlebar = isTauri && isMac;

export async function openExternal(url: string): Promise<void> {
  if (!/^(https?:\/\/|mailto:)/i.test(url)) return;
  if (isTauri) {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
  } else {
    window.open(url, "_blank", "noopener,noreferrer");
  }
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
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
