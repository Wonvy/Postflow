// 订阅统计页的计算：排序、提醒标记、坐标轴刻度。只有纯函数，方便测试：npm run test:ui

import type { FeedStats, Stats } from "./api";

const DAY = 86_400_000;
/** 这么多天没发新文章，算很久没更新。 */
export const STALE_DAYS = 60;
/** 带全文的比例低于这个，算只给摘要。 */
const SUMMARY_SHARE = 0.3;

export type SortKey = "title" | "rate" | "last" | "full" | "opened" | "finished" | "dwell";

/** 比例。分母为 0 时没有意义，返回空。 */
export function ratio(n: number, of: number): number | null {
  return of > 0 ? Math.min(1, n / of) : null;
}

/** 平均每周几篇。 */
export function perWeek(published: number, days: number): number {
  return days > 0 ? (published * 7) / days : 0;
}

export type Flag = "error" | "stale" | "summary" | "unread";

/** 值得注意的地方：抓取出错、很久没更新、只给摘要、这段时间一篇都没读。 */
export function flagsOf(feed: FeedStats, stats: Pick<Stats, "now" | "since" | "tracking_since">): Flag[] {
  const flags: Flag[] = [];
  if (feed.error) flags.push("error");
  const lastSeen = feed.last_published ?? feed.added_at;
  if (lastSeen < stats.now - STALE_DAYS * DAY) flags.push("stale");
  if (feed.full_share !== null && feed.total >= 3 && feed.full_share < SUMMARY_SHARE) flags.push("summary");
  // 只有整段时间都有阅读记录时才说「一篇没读」，否则只是还没开始记
  const tracked = stats.tracking_since !== null && stats.tracking_since <= stats.since;
  if (tracked && feed.published >= 5 && feed.opened === 0) flags.push("unread");
  return flags;
}

function sortValue(feed: FeedStats, key: SortKey): number | string {
  switch (key) {
    case "title":
      return feed.title.toLocaleLowerCase();
    case "rate":
      return feed.published;
    case "last":
      return feed.last_published ?? -1;
    case "full":
      return feed.full_share ?? -1;
    case "opened":
      return ratio(feed.opened, feed.published) ?? -1;
    case "finished":
      return ratio(feed.finished, feed.opened) ?? -1;
    case "dwell":
      return feed.dwell_ms;
  }
}

/** 按某一列排序。一样的按发文数、再按名称排，顺序稳定。 */
export function sortFeeds(feeds: FeedStats[], key: SortKey, desc: boolean): FeedStats[] {
  const dir = desc ? -1 : 1;
  return [...feeds].sort((a, b) => {
    const x = sortValue(a, key);
    const y = sortValue(b, key);
    if (x !== y) return (x < y ? -1 : 1) * dir;
    if (a.published !== b.published) return b.published - a.published;
    return a.title.localeCompare(b.title);
  });
}

/** 坐标轴的上限：1、2、5 乘以 10 的整数次方里不小于最大值的最小那个。 */
export function niceMax(max: number): number {
  if (!(max > 0)) return 1;
  const power = 10 ** Math.floor(Math.log10(max));
  for (const step of [1, 2, 5, 10]) {
    if (step * power >= max) return step * power;
  }
  return 10 * power;
}

/** 看到这么深、又停留了这么久，算看完了。 */
const FINISHED_DEPTH = 0.9;
const MIN_FINISH_MS = 5_000;

/** 看到了哪里：可见部分的底边在整篇里的位置。整篇都放得下时算看到了底。 */
export function scrollDepth(scrollTop: number, clientHeight: number, scrollHeight: number): number {
  if (scrollHeight - clientHeight <= 4) return 1;
  return Math.min(1, Math.max(0, (scrollTop + clientHeight) / scrollHeight));
}

export function isFinished(depth: number, dwellMs: number): boolean {
  return depth >= FINISHED_DEPTH && dwellMs >= MIN_FINISH_MS;
}

/** 毫秒换成小时和分钟（四舍五入到分钟）。 */
export function hoursMinutes(ms: number): { h: number; m: number } {
  const minutes = Math.round(ms / 60_000);
  return { h: Math.floor(minutes / 60), m: minutes % 60 };
}
