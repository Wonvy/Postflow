// 这台设备是什么：手机、平板，还是用鼠标的电脑。打开页面时判断一次，用来选默认的字号和布局。
// index.html 里放大手机页面用的是同一个判断（触屏，屏幕短边小于 600）。

const coarse = typeof matchMedia === "function" && matchMedia("(pointer: coarse)").matches;
const shortSide = typeof screen === "object" ? Math.min(screen.width, screen.height) : 0;

/** 手机：触屏，屏幕短边不到 600。页面整体放大过，字号不用再加。 */
export const IS_PHONE = coarse && shortSide > 0 && shortSide < 600;

/** 平板（iPad、安卓平板）：触屏，屏幕大。界面不放大，阅读和列表的默认字号大一点，手指好点。 */
export const IS_TABLET = coarse && shortSide >= 600;

export type Layout = "phone" | "tablet" | "desktop";

/** 平板窄于这个宽度时侧栏收起来（iPad 竖屏都在这以下，横屏的 11 寸以上不收）。 */
const TABLET_DRAWER_BELOW = 1100;

/** 这个宽度用哪种布局，见 mobile.ts。 */
export function layoutFor(width: number, tablet: boolean): Layout {
  if (tablet) {
    if (width < 600) return "phone";
    return width < TABLET_DRAWER_BELOW ? "tablet" : "desktop";
  }
  return width < 768 ? "phone" : "desktop";
}
