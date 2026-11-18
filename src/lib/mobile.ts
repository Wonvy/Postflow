// 按屏幕宽度和设备选布局：
// - 手机布局：一次只显示一栏。列表占满屏幕，侧栏从左边滑出来；打开一条就只显示阅读区，左上角返回列表。
//   窗口窄于 768 像素（手机、电脑上拉窄的窗口），或者平板分屏后窄于 600 像素时用。
// - 平板布局：列表和阅读区并排，侧栏从左边滑出来，给阅读留地方。平板竖着拿、或者横着但屏幕不够宽时用。
// - 电脑布局：侧栏、列表、阅读区三栏。

import { useSyncExternalStore } from "react";
import { IS_TABLET, type Layout, layoutFor } from "./device";
import { useUi } from "./store";

function subscribeResize(onChange: () => void): () => void {
  window.addEventListener("resize", onChange);
  return () => window.removeEventListener("resize", onChange);
}

/** 现在用哪种布局。窗口拉窄、转屏、平板分屏时跟着变。 */
export function useLayout(): Layout {
  return useSyncExternalStore(
    subscribeResize,
    () => layoutFor(window.innerWidth, IS_TABLET),
    () => "desktop" as Layout,
  );
}

/** 现在是不是手机布局（一次一栏）。 */
export function useIsMobile(): boolean {
  return useLayout() === "phone";
}

/** 阅读区是不是弹出来的（设置里选了「弹出」，手机布局除外）。这时窗口里只有侧栏和列表两栏。 */
export function useReaderOverlay(): boolean {
  const overlay = useUi((s) => s.readerLayout === "overlay");
  const layout = useLayout();
  return overlay && layout !== "phone";
}

/** 侧栏是不是收成从左边滑出来的抽屉（手机和平板布局）。 */
export function useDrawerSidebar(): boolean {
  return useLayout() !== "desktop";
}

function useMedia(query: string): boolean {
  return useSyncExternalStore(
    (onChange) => {
      const media = window.matchMedia(query);
      media.addEventListener("change", onChange);
      return () => media.removeEventListener("change", onChange);
    },
    () => window.matchMedia(query).matches,
    () => false,
  );
}

/** 主要用手指操作（手机、平板）：行和按钮要加高。 */
export function useIsTouch(): boolean {
  return useMedia("(pointer: coarse)");
}

/** 有鼠标或触控板，能「停在」某处（悬停效果只在这时有意义）。 */
export function useCanHover(): boolean {
  return useMedia("(hover: hover) and (pointer: fine)");
}
