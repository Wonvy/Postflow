// 主题色的规则。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { ACCENT_PRESETS, PAPER_DEFAULT_ACCENT, accentKind, accentVars, contrast, parseHex } from "./accent.ts";

test("every preset reads well on all themes", () => {
  for (const p of ACCENT_PRESETS) {
    assert.ok(contrast(p.light, "#ffffff") >= 4.5, `${p.id} light ${contrast(p.light, "#ffffff")}`);
    assert.ok(contrast(p.dark, "#0f1012") >= 4.5, `${p.id} dark ${contrast(p.dark, "#0f1012")}`);
    const paper = accentVars(p.id, "paper").accent;
    assert.ok(
      contrast(paper, "#f1ece2") >= 4.5 && contrast(paper, "#eae3d7") >= 4.5,
      `${p.id} paper ${paper} (${contrast(paper, "#eae3d7")})`,
    );
  }
});

test("the default matches the logo", () => {
  assert.equal(accentVars("postflow", "light").accent, "#7533e8");
  assert.equal(accentVars("postflow", "dark").accent, "#a25bff");
  assert.equal(accentVars(PAPER_DEFAULT_ACCENT, "paper").accent, "#026a9f");
  // 认不出的当作默认
  assert.equal(accentVars("nonsense", "dark").accent, "#a25bff");
  assert.equal(accentKind("nonsense"), "postflow");
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
