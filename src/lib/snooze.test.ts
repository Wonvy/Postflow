// 稍后提醒的预设时间。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { snoozePresets, toLocalInput } from "./snooze.ts";

const local = (d: Date) => toLocalInput(d);

test("a weekday morning offers every preset", () => {
  // 2026-11-18 是周三
  const presets = snoozePresets(new Date(2026, 10, 18, 9, 20));
  assert.deepEqual(
    presets.map((p) => [p.key, local(p.at)]),
    [
      ["later", "2026-11-18T12:00"],
      ["tonight", "2026-11-18T19:00"],
      ["tomorrow", "2026-11-19T08:00"],
      ["weekend", "2026-11-21T09:00"],
      ["nextWeek", "2026-11-23T08:00"],
    ],
  );
});

test("late in the day there's no 'tonight', and 'later' is at least a few hours away", () => {
  const presets = snoozePresets(new Date(2026, 10, 18, 18, 50));
  assert.deepEqual(presets.map((p) => p.key), ["later", "tomorrow", "weekend", "nextWeek"]);
  assert.equal(local(presets[0]!.at), "2026-11-18T22:00");
});

test("on Friday and Saturday the weekend is skipped; on Sunday next week is tomorrow's Monday", () => {
  assert.ok(!snoozePresets(new Date(2026, 10, 20, 10, 0)).some((p) => p.key === "weekend"), "Friday");
  assert.ok(!snoozePresets(new Date(2026, 10, 21, 10, 0)).some((p) => p.key === "weekend"), "Saturday");
  const sunday = snoozePresets(new Date(2026, 10, 22, 10, 0));
  assert.equal(local(sunday.find((p) => p.key === "nextWeek")!.at), "2026-11-23T08:00");
  const monday = snoozePresets(new Date(2026, 10, 23, 10, 0));
  assert.equal(local(monday.find((p) => p.key === "nextWeek")!.at), "2026-11-30T08:00");
});
