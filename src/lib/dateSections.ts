// 列表按日期分段：今天、昨天、本周、上周、本月，再往前按月。和 QQ 邮箱的分组一样一目了然。
//
// 都按本机时区的日历算，一周从星期一开始。这里只有纯函数，方便测试：npm run test:ui

import type { Lang } from "./time";

export type DateSection =
  | { kind: "today" | "yesterday" | "thisWeek" | "lastWeek" | "thisMonth"; key: string }
  | { kind: "month"; key: string; year: number; month: number };

function startOfDay(ms: number): Date {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d;
}

/** `ms` 这个时间在 `now` 看来属于哪一段。将来的时间（时钟不准的邮件）算今天。 */
export function dateSection(ms: number, now: number): DateSection {
  const today = startOfDay(now);
  if (ms >= today.getTime()) return { kind: "today", key: "today" };

  const yesterday = new Date(today);
  yesterday.setDate(today.getDate() - 1);
  if (ms >= yesterday.getTime()) return { kind: "yesterday", key: "yesterday" };

  // 星期一是一周的第一天：getDay() 里星期日是 0
  const week = new Date(today);
  week.setDate(today.getDate() - ((today.getDay() + 6) % 7));
  if (ms >= week.getTime()) return { kind: "thisWeek", key: "thisWeek" };

  const lastWeek = new Date(week);
  lastWeek.setDate(week.getDate() - 7);
  if (ms >= lastWeek.getTime()) return { kind: "lastWeek", key: "lastWeek" };

  const month = new Date(today.getFullYear(), today.getMonth(), 1);
  if (ms >= month.getTime()) return { kind: "thisMonth", key: "thisMonth" };

  const d = new Date(ms);
  return { kind: "month", key: `m:${d.getFullYear()}-${d.getMonth() + 1}`, year: d.getFullYear(), month: d.getMonth() + 1 };
}

const NAMES: Record<Lang, Record<Exclude<DateSection["kind"], "month">, string>> = {
  zh: { today: "今天", yesterday: "昨天", thisWeek: "本周", lastWeek: "上周", thisMonth: "本月" },
  en: { today: "Today", yesterday: "Yesterday", thisWeek: "This week", lastWeek: "Last week", thisMonth: "This month" },
};

/** 段的标题。往前的月份同一年只写月，不同年连年份一起写。 */
export function sectionLabel(section: DateSection, lang: Lang, now: number): string {
  if (section.kind !== "month") return NAMES[lang][section.kind];
  const sameYear = section.year === new Date(now).getFullYear();
  if (lang === "zh") return sameYear ? `${section.month} 月` : `${section.year} 年 ${section.month} 月`;
  return new Intl.DateTimeFormat("en-US", { month: "long", year: sameYear ? undefined : "numeric" }).format(
    new Date(section.year, section.month - 1, 1),
  );
}
