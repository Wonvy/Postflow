// 播放时长的写法。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { formatLength, formatTime } from "./duration.ts";

test("player clock", () => {
  assert.equal(formatTime(0), "0:00");
  assert.equal(formatTime(65.9), "1:05");
  assert.equal(formatTime(3723), "1:02:03");
  assert.equal(formatTime(Number.NaN), "0:00");
  assert.equal(formatTime(-3), "0:00");
});

test("episode length", () => {
  assert.equal(formatLength(45 * 60, "zh"), "45 分钟");
  assert.equal(formatLength(62 * 60 + 10, "zh"), "1 小时 2 分");
  assert.equal(formatLength(3600, "en"), "1 h");
  assert.equal(formatLength(20, "en"), "1 min");
});
