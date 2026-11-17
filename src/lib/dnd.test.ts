// 拖到一行上时落在哪：上方、下方还是放进去。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { positionAt } from "./dnd.ts";

test("rows that take items inside split into equal thirds", () => {
  assert.equal(positionAt(0, true), "before");
  assert.equal(positionAt(0.3, true), "before");
  assert.equal(positionAt(0.5, true), "inside");
  assert.equal(positionAt(0.7, true), "after");
  assert.equal(positionAt(1, true), "after");
});

test("other rows split in half", () => {
  assert.equal(positionAt(0.49), "before");
  assert.equal(positionAt(0.51), "after");
});
