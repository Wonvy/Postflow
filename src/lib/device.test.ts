// 按宽度选布局，以及平板的默认字号。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { layoutFor } from "./device.ts";
import { isDefaultListStyle, LIST_STYLE_DEFAULT, listStyleDefault } from "./listStyle.ts";
import { isDefaultStyle, READER_DEFAULT, readerDefault } from "./readerStyle.ts";

test("computers and phones switch between one column and three at 768", () => {
  assert.equal(layoutFor(390, false), "phone");
  assert.equal(layoutFor(767, false), "phone");
  assert.equal(layoutFor(768, false), "desktop");
  assert.equal(layoutFor(1440, false), "desktop");
});

test("tablets show list and reader side by side until the screen is wide", () => {
  // iPad mini 竖屏、iPad 竖屏、iPad Pro 13 寸竖屏
  assert.equal(layoutFor(744, true), "tablet");
  assert.equal(layoutFor(820, true), "tablet");
  assert.equal(layoutFor(1024, true), "tablet");
  // 横屏的 11 寸、13 寸：三栏
  assert.equal(layoutFor(1180, true), "desktop");
  assert.equal(layoutFor(1366, true), "desktop");
  // 分屏后很窄：一次一栏
  assert.equal(layoutFor(507, true), "phone");
});

test("tablets get larger default text, other devices keep the usual defaults", () => {
  assert.equal(readerDefault(false), READER_DEFAULT);
  assert.ok(readerDefault(true).size > READER_DEFAULT.size);
  assert.ok(!isDefaultStyle(READER_DEFAULT, readerDefault(true)));
  assert.ok(isDefaultStyle(readerDefault(true), readerDefault(true)));
  assert.equal(listStyleDefault(false), LIST_STYLE_DEFAULT);
  assert.ok(listStyleDefault(true).titleSize > LIST_STYLE_DEFAULT.titleSize);
  assert.ok(isDefaultListStyle(listStyleDefault(true), listStyleDefault(true)));
  assert.ok(!isDefaultListStyle(listStyleDefault(true)));
});
