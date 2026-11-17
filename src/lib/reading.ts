// 阅读统计：打开一篇文章以后，实际看了多久、往下看到了哪里、看没看完。
//
// 只在窗口在前台、最近一分钟里有过操作（滚轮、滚动、按键、移动鼠标）时计时，
// 放着不管不算在读。记录只存在本机，用在订阅统计里。

import { type RefObject, useEffect, useRef } from "react";
import { api } from "./api";
import { isFinished, scrollDepth } from "./stats";

/** 这么久没有任何操作，就不再算在读。 */
const IDLE_MS = 60_000;
const TICK_MS = 1_000;
/** 停留不到这么久的不记：多半是用键盘翻过去的。 */
const MIN_DWELL_MS = 1_000;
/** 刚打开时正文可能还没排好，等这么久之后才按页面高度算看到了哪里。 */
const SETTLE_MS = 3_000;

/**
 * 记下这一篇的阅读。`container` 是阅读区可以滚动的那一层；`ready` 为假（正文还没加载出来）时不算深度。
 * 换到别的条目、窗口切到后台、关掉页面时把这一段记下来。
 */
export function useReadingTracker(itemId: string, container: RefObject<HTMLElement | null>, ready: boolean, enabled: boolean) {
  const readyRef = useRef(ready);
  readyRef.current = ready;

  useEffect(() => {
    if (!enabled) return;
    let dwell = 0;
    let total = 0;
    let depth = 0;
    let lastActive = Date.now();
    const node = container.current;

    const measure = (force: boolean) => {
      if (!node || !readyRef.current || (!force && total < SETTLE_MS)) return;
      depth = Math.max(depth, scrollDepth(node.scrollTop, node.clientHeight, node.scrollHeight));
    };
    const active = () => {
      lastActive = Date.now();
    };
    const onScroll = () => {
      active();
      measure(true);
    };
    const tick = window.setInterval(() => {
      if (document.visibilityState !== "visible" || Date.now() - lastActive > IDLE_MS) return;
      dwell += TICK_MS;
      total += TICK_MS;
      measure(false);
    }, TICK_MS);
    const flush = () => {
      if (dwell < MIN_DWELL_MS) return;
      const finished = isFinished(depth, total);
      void api.recordReading({ item_id: itemId, dwell_ms: dwell, depth, finished }).catch(() => {});
      dwell = 0;
    };
    const onVisibility = () => {
      if (document.visibilityState === "hidden") flush();
      else active();
    };

    node?.addEventListener("scroll", onScroll, { passive: true });
    node?.addEventListener("wheel", active, { passive: true });
    node?.addEventListener("pointermove", active, { passive: true });
    node?.addEventListener("pointerdown", active);
    window.addEventListener("keydown", active);
    document.addEventListener("visibilitychange", onVisibility);
    window.addEventListener("pagehide", flush);
    return () => {
      window.clearInterval(tick);
      node?.removeEventListener("scroll", onScroll);
      node?.removeEventListener("wheel", active);
      node?.removeEventListener("pointermove", active);
      node?.removeEventListener("pointerdown", active);
      window.removeEventListener("keydown", active);
      document.removeEventListener("visibilitychange", onVisibility);
      window.removeEventListener("pagehide", flush);
      flush();
    };
  }, [container, enabled, itemId]);
}
