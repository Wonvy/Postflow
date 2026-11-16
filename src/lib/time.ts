export type Lang = "zh" | "en";

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** 今天零点的时间戳，按本机时区。 */
export function startOfToday(): number {
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

const locale = (lang: Lang) => (lang === "zh" ? "zh-CN" : "en-US");

/** 列表里用的短时间：越近越粗略，占的宽度尽量小。 */
export function shortTime(ms: number, lang: Lang, now = Date.now()): string {
  const diff = Math.max(0, now - ms);
  if (diff < MINUTE) return lang === "zh" ? "刚刚" : "now";
  if (diff < HOUR) {
    const m = Math.floor(diff / MINUTE);
    return lang === "zh" ? `${m} 分钟` : `${m}m`;
  }
  if (diff < DAY) {
    const h = Math.floor(diff / HOUR);
    return lang === "zh" ? `${h} 小时` : `${h}h`;
  }
  if (diff < 7 * DAY) {
    const d = Math.floor(diff / DAY);
    return lang === "zh" ? `${d} 天` : `${d}d`;
  }
  const date = new Date(ms);
  const sameYear = date.getFullYear() === new Date(now).getFullYear();
  return new Intl.DateTimeFormat(locale(lang), {
    year: sameYear ? undefined : "numeric",
    month: "short",
    day: "numeric",
  }).format(date);
}

/** 阅读面板里用的完整时间。 */
export function fullTime(ms: number, lang: Lang): string {
  return new Intl.DateTimeFormat(locale(lang), {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(ms));
}
