// 新手引导什么时候自动弹出。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { welcomeOnStart } from "./welcome.ts";

test("a first install with nothing added opens the guide", () => {
  assert.equal(welcomeOnStart(false, 0, 0), "open");
});

test("people who already have mail or feeds don't get the guide", () => {
  assert.equal(welcomeOnStart(false, 1, 0), "mark-done");
  assert.equal(welcomeOnStart(false, 0, 3), "mark-done");
});

test("once seen or skipped it doesn't open again, even with nothing added", () => {
  assert.equal(welcomeOnStart(true, 0, 0), "nothing");
});
