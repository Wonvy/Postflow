// 播完一期后接着放哪一期。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { nextEntry } from "./playerQueue.ts";

// 列表新的在前：第 4 期、第 3 期（听过）、第 2 期、第 1 期
const list = [
  { id: "ep4", published: 400, read: false },
  { id: "ep3", published: 300, read: true },
  { id: "ep2", published: 200, read: false },
  { id: "ep1", published: 100, read: false },
];

test("normally the next row in the list plays", () => {
  assert.equal(nextEntry(list, "ep4", false)?.id, "ep3");
  assert.equal(nextEntry(list, "ep1", false), undefined);
  assert.equal(nextEntry(list, "gone", false), undefined);
});

test("catching up goes oldest to newest and skips played ones", () => {
  assert.equal(nextEntry(list, "ep1", true)?.id, "ep2");
  assert.equal(nextEntry(list, "ep2", true)?.id, "ep4", "ep3 was already played");
  assert.equal(nextEntry(list, "ep2", true, new Set(["ep4"])), undefined);
  assert.equal(nextEntry(list, "ep4", true), undefined, "nothing newer");
});

test("episodes published at the same moment keep the list order", () => {
  const same = [
    { id: "b", published: 100, read: false },
    { id: "a", published: 100, read: false },
  ];
  assert.equal(nextEntry(same, "a", true)?.id, "b");
  assert.equal(nextEntry(same, "b", true), undefined);
});
