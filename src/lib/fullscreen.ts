// 全屏：桌面应用让整个窗口全屏，浏览器里让页面全屏。
// 从 ShibaPost 的全屏按钮（或 Shift+F）进入时，阅读区同时最大化，退出全屏后回到原来的样子；
// 用户用系统的方式让窗口全屏（macOS 的绿色按钮）时只是窗口变大，布局不动。

import { create } from "zustand";
import { useUi } from "./store";
import { isTauri } from "./transport";

interface FullscreenState {
  on: boolean;
  /** 这次全屏是不是 ShibaPost 自己开的（开的时候顺便最大化了阅读区）。 */
  ours: boolean;
  /** 进全屏前阅读区是不是已经最大化了，退出时恢复。 */
  wasMaximized: boolean;
}

export const useFullscreen = create<FullscreenState>()(() => ({ on: false, ours: false, wasMaximized: false }));

/** 下一次进入全屏是 ShibaPost 的按钮触发的。 */
let requested = false;

async function tauriWindow() {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}

/** 全屏状态变了（包括用户用系统的方式进出全屏，比如 macOS 的绿色按钮、浏览器里按 Esc）。 */
function changed(on: boolean) {
  const state = useFullscreen.getState();
  if (on === state.on) return;
  const ui = useUi.getState();
  if (on) {
    const ours = requested;
    requested = false;
    useFullscreen.setState({ on: true, ours, wasMaximized: ui.readerMaximized });
    if (ours) ui.setReaderMaximized(true);
  } else {
    useFullscreen.setState({ on: false, ours: false });
    if (state.ours) ui.setReaderMaximized(state.wasMaximized);
  }
}

let watching = false;
/** 应用启动时调用一次：跟上系统那边的全屏状态。 */
export function watchFullscreen(): void {
  if (watching) return;
  watching = true;
  if (isTauri) {
    void tauriWindow().then((win) => {
      const update = () => void win.isFullscreen().then(changed);
      void win.onResized(update);
    });
  } else {
    document.addEventListener("fullscreenchange", () => changed(document.fullscreenElement !== null));
  }
}

export async function setFullscreen(on: boolean): Promise<void> {
  if (on) requested = true;
  if (isTauri) {
    const win = await tauriWindow();
    await win.setFullscreen(on);
    changed(on);
    return;
  }
  try {
    if (on && !document.fullscreenElement) await document.documentElement.requestFullscreen?.();
    if (!on && document.fullscreenElement) await document.exitFullscreen?.();
  } catch (err) {
    // 浏览器不让全屏（比如嵌在别的页面里）：至少把阅读区最大化
    console.warn("full screen was refused", err);
    if (on) useUi.getState().setReaderMaximized(true);
  }
}

export function toggleFullscreen(): Promise<void> {
  return setFullscreen(!useFullscreen.getState().on);
}
