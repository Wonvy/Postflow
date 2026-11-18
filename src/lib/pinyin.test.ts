// 拼音首字母。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { initials } from "./pinyin.ts";

test("common Chinese words give their pinyin initials", () => {
  assert.equal(initials("少数派"), "ssp");
  assert.equal(initials("阮一峰的网络日志"), "ryfdwlrz");
  assert.equal(initials("收件箱"), "sjx");
  assert.equal(initials("稍后提醒"), "shtx");
  assert.equal(initials("写邮件"), "xyj");
  assert.equal(initials("垃圾邮件"), "ljyj");
  assert.equal(initials("哈哈"), "hh");
  assert.equal(initials("如果日历"), "rgrl");
});

test("letters and digits are kept, punctuation is dropped", () => {
  assert.equal(initials("IT之家"), "itzj");
  assert.equal(initials("设置 · 外观"), "szwg");
  assert.equal(initials("36氪"), "36k");
});
