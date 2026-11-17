// 列表样式的规则。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { isDefaultListStyle, LIST_STYLE_DEFAULT, listMetrics, normalizeListStyle } from "./listStyle.ts";

test("missing or broken saved values fall back to the defaults", () => {
  assert.deepEqual(normalizeListStyle(undefined), LIST_STYLE_DEFAULT);
  assert.deepEqual(normalizeListStyle({ titleSize: "big", showIcon: 1, gap: Number.NaN }), LIST_STYLE_DEFAULT);
  assert.deepEqual(normalizeListStyle({ titleSize: 15, showSummary: false }), {
    ...LIST_STYLE_DEFAULT,
    titleSize: 15,
    showSummary: false,
  });
});

test("numbers are kept inside their ranges", () => {
  const style = normalizeListStyle({ titleSize: 40, summarySize: 2, gap: -3, padding: 99, summaryLines: 2.6 });
  assert.equal(style.titleSize, 18);
  assert.equal(style.summarySize, 10);
  assert.equal(style.gap, 0);
  assert.equal(style.padding, 16);
  assert.equal(style.summaryLines, 3);
  assert.ok(isDefaultListStyle(LIST_STYLE_DEFAULT));
  assert.ok(!isDefaultListStyle(style));
});

test("default rows keep roughly the heights the list had before", () => {
  const m = listMetrics(LIST_STYLE_DEFAULT);
  assert.equal(m.compact, 34);
  assert.ok(Math.abs(m.comfortable - 57) <= 2);
  assert.ok(Math.abs(m.summaryRow - 98) <= 4);
  assert.ok(Math.abs(m.cardText - 128) <= 4);
  assert.equal(m.cardGap, 10);
  assert.equal(m.padX, 10);
});

test("rows grow with the text and shrink when parts are hidden", () => {
  const base = listMetrics(LIST_STYLE_DEFAULT);
  const bigger = listMetrics({ ...LIST_STYLE_DEFAULT, titleSize: 16, padding: 12 });
  assert.ok(bigger.compact > base.compact);
  assert.ok(bigger.comfortable > base.comfortable);

  const noSummary = listMetrics({ ...LIST_STYLE_DEFAULT, showSummary: false, showImage: false });
  assert.ok(noSummary.summaryRow < base.summaryRow);
  assert.ok(noSummary.cardText < base.cardText);

  const moreLines = listMetrics({ ...LIST_STYLE_DEFAULT, summaryLines: 4, showImage: false });
  assert.ok(moreLines.summaryRow > base.summaryRow);
});
