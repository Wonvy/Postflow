// 手机布局：屏幕窄于 768 像素时（手机浏览器、竖着的小平板）一次只显示一栏。
// 列表占满屏幕，侧栏从左边滑出来；打开一条就只显示阅读区，左上角返回列表。

import { useSyncExternalStore } from "react";

export const MOBILE_QUERY = "(max-width: 767px)";

function subscribe(onChange: () => void): () => void {
  const media = window.matchMedia(MOBILE_QUERY);
  media.addEventListener("change", onChange);
  return () => media.removeEventListener("change", onChange);
}

/** 现在是不是手机布局。窗口拉窄、手机转屏时跟着变。 */
export function useIsMobile(): boolean {
  return useSyncExternalStore(
    subscribe,
    () => window.matchMedia(MOBILE_QUERY).matches,
    () => false,
  );
}
