// 阅读区排版设置的规则。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  clampSize,
  clampTo,
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
    "--reader-tracking": "0em",
    "--reader-spacing": "1.15em",
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

test("leading and width saved as the old three choices carry over", () => {
  const old = normalizeReaderStyle({ size: 16, leading: "relaxed", width: "wide", font: "serif" });
  assert.equal(old.leading, 1.95);
  assert.equal(old.width, 900);
  assert.equal(old.tracking, 0);
  assert.equal(normalizeReaderStyle({ leading: "normal", width: "normal" }).width, 720);
  assert.ok(isDefaultStyle(normalizeReaderStyle({ leading: "normal", width: "normal" })));
});

test("slider values stay in range and snap to the step", () => {
  assert.equal(clampTo("leading", 9), 2.4);
  assert.equal(clampTo("leading", 1.7231), 1.72);
  assert.equal(clampTo("tracking", 0.0124), 0.01);
  assert.equal(clampTo("width", 726), 720);
  assert.equal(clampTo("spacing", 0), 0.4);
});

test("every default sits on its slider's steps", () => {
  assert.deepEqual(normalizeReaderStyle(READER_DEFAULT), READER_DEFAULT);
  for (const key of ["size", "leading", "tracking", "spacing", "width"] as const) {
    assert.equal(clampTo(key, READER_DEFAULT[key]), READER_DEFAULT[key], key);
  }
});
