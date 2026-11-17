// 阅读区排版设置的规则。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  clampSize,
  isDefaultStyle,
  mailZoom,
  normalizeReaderStyle,
  READER_DEFAULT,
  readerVars,
  readerWidth,
  titleSize,
} from "./readerStyle.ts";

test("missing or broken saved values fall back to the defaults", () => {
  assert.deepEqual(normalizeReaderStyle(undefined), READER_DEFAULT);
  assert.deepEqual(normalizeReaderStyle({ size: "big", leading: "huge", font: 3 }), READER_DEFAULT);
  assert.deepEqual(normalizeReaderStyle({ size: 18, font: "serif" }), { ...READER_DEFAULT, size: 18, font: "serif" });
});

test("font size stays between 12 and 24", () => {
  assert.equal(clampSize(3), 12);
  assert.equal(clampSize(99), 24);
  assert.equal(clampSize(16.4), 16);
  assert.equal(normalizeReaderStyle({ size: 40 }).size, 24);
});

test("the defaults look the same as before reading settings existed", () => {
  assert.ok(isDefaultStyle(READER_DEFAULT));
  assert.deepEqual(readerVars(READER_DEFAULT), {
    "--reader-size": "15px",
    "--reader-leading": "1.72",
    "--reader-font": "var(--font-sans)",
  });
  assert.equal(readerWidth(READER_DEFAULT, "article"), 720);
  assert.equal(readerWidth(READER_DEFAULT, "mail"), 760);
  assert.equal(titleSize(READER_DEFAULT, "article"), 26);
  assert.equal(titleSize(READER_DEFAULT, "mail"), 22);
  assert.equal(mailZoom(READER_DEFAULT), 1);
});

test("titles and HTML mail grow with the text", () => {
  const big = { ...READER_DEFAULT, size: 18 };
  assert.equal(titleSize(big, "article"), 31);
  assert.equal(mailZoom(big), 1.2);
  assert.ok(!isDefaultStyle(big));
});
