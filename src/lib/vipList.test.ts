// 重要联系人：名单整理和只留他们的来信。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { vipList, vipOnly } from "./vipList.ts";

test("the list is lowercased, trimmed and without duplicates", () => {
  assert.deepEqual(vipList({ vip_senders: [" Boss@Example.com", "boss@example.com", 3, "mom@example.org"] }), [
    "boss@example.com",
    "mom@example.org",
  ]);
  assert.deepEqual(vipList(undefined), []);
  assert.deepEqual(vipList({ vip_senders: "nope" }), []);
});

test("only mail from important senders is kept", () => {
  const mails = [
    { id: "1", address: "boss@example.com" },
    { id: "2", address: "news@example.com" },
    { id: "3" },
    { id: "4", address: "BOSS@example.com" },
  ];
  assert.deepEqual(
    vipOnly(mails, ["boss@example.com"]).map((m) => m.id),
    ["1", "4"],
  );
});
