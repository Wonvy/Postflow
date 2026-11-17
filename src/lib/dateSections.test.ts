// 列表按日期分段的规则。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { dateSection, sectionLabel } from "./dateSections.ts";

// 2026 年 11 月 19 日，星期四，下午三点（本机时区）
const now = new Date(2026, 10, 19, 15, 0).getTime();
const at = (month: number, day: number, hour = 9, year = 2026) => new Date(year, month - 1, day, hour).getTime();
const kind = (ms: number) => dateSection(ms, now).kind;

test("recent days and weeks", () => {
  assert.equal(kind(at(11, 19, 0)), "today");
  assert.equal(kind(now + 3_600_000), "today");
  assert.equal(kind(at(11, 18, 23)), "yesterday");
  // 本周从星期一（16 日）开始
  assert.equal(kind(at(11, 17)), "thisWeek");
  assert.equal(kind(at(11, 16, 0)), "thisWeek");
  assert.equal(kind(at(11, 15, 23)), "lastWeek");
  assert.equal(kind(at(11, 9)), "lastWeek");
  assert.equal(kind(at(11, 8)), "thisMonth");
  assert.equal(kind(at(11, 1, 0)), "thisMonth");
});

test("older items fall into months", () => {
  const october = dateSection(at(10, 31), now);
  assert.equal(october.kind, "month");
  assert.equal(october.key, "m:2026-10");
  assert.equal(sectionLabel(october, "zh", now), "10 月");
  assert.equal(sectionLabel(october, "en", now), "October");
  const old = dateSection(at(12, 5, 9, 2025), now);
  assert.equal(sectionLabel(old, "zh", now), "2025 年 12 月");
  assert.equal(sectionLabel(old, "en", now), "December 2025");
});

test("on a Monday yesterday is last week's Sunday, not this week", () => {
  const monday = new Date(2026, 10, 16, 10).getTime();
  assert.equal(dateSection(new Date(2026, 10, 15, 20).getTime(), monday).kind, "yesterday");
  assert.equal(dateSection(new Date(2026, 10, 14, 20).getTime(), monday).kind, "lastWeek");
  assert.equal(sectionLabel(dateSection(monday, monday), "zh", monday), "今天");
});
