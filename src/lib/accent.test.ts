// 主题色的规则。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { ACCENT_PRESETS, DEFAULT_ACCENT, PAPER_DEFAULT_ACCENT, accentKind, accentVars, contrast, parseHex } from "./accent.ts";

test("every preset keeps its exact configured colour across themes", () => {
  for (const p of ACCENT_PRESETS) {
    assert.equal(accentVars(p.id, "light").accent, p.light);
    assert.equal(accentVars(p.id, "dark").accent, p.dark);
    assert.equal(accentVars(p.id, "paper").accent, p.light);
  }
});

test("defaults and legacy preset ids resolve properly", () => {
  assert.equal(accentVars(DEFAULT_ACCENT, "light").accent, "#5e6ad2");
  assert.equal(accentVars(DEFAULT_ACCENT, "dark").accent, "#5e6ad2");
  assert.equal(accentVars(PAPER_DEFAULT_ACCENT, "paper").accent, "#0284c7");
  // 旧预设 ID 平滑映射
  assert.equal(accentKind("shibapost"), "violet");
  assert.equal(accentKind("graphite"), "slate");
  // 认不出的当作默认
  assert.equal(accentVars("nonsense", "dark").accent, "#5e6ad2");
  assert.equal(accentKind("nonsense"), DEFAULT_ACCENT);
});

test("a custom colour is adjusted until text is readable", () => {
  // 很浅的黄：浅色界面要压暗，深色界面原样就够亮
  const light = accentVars("#ffe066", "light");
  assert.ok(contrast(light.accent, "#ffffff") >= 4.5 && contrast(light.accent, "#f7f7f8") >= 4.5, light.accent);
  const dark = accentVars("#ffe066", "dark");
  assert.equal(dark.accent, "#ffe066");
  // 很深的蓝：深色界面要提亮
  const deep = accentVars("#0a1a6e", "dark");
  assert.ok(contrast(deep.accent, "#0f1012") >= 4.5, deep.accent);
  assert.equal(accentKind("#0A1A6E"), "custom");
  assert.match(accentVars("#0a1a6e", "light").soft, /^rgb\(\d+ \d+ \d+ \/ 0\.1\)$/);
  assert.equal(parseHex("#xyz"), null);
});
