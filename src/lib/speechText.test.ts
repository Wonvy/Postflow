// 朗读的分段和语言判断。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { chunks, guessLang } from "./speechText.ts";

test("text is split into sentences", () => {
  assert.deepEqual(chunks("你好。今天天气不错！要出门吗？\n\nHello there. How are you?  Fine."), [
    "你好。",
    "今天天气不错！",
    "要出门吗？",
    "Hello there.",
    "How are you?",
    "Fine.",
  ]);
});

test("long sentences are cut at commas or spaces", () => {
  const long = `${"word ".repeat(80)}end`;
  const parts = chunks(long, 100);
  assert.ok(parts.every((p) => p.length <= 100));
  assert.equal(parts.join(" "), long.trim());
  assert.deepEqual(chunks("", 10), []);
});

test("language is guessed from the script", () => {
  assert.equal(guessLang("本地优先的软件把数据放在你自己的设备上。"), "zh");
  assert.equal(guessLang("Local-first software keeps your data on your own devices."), "en");
  assert.equal(guessLang("ローカルファーストのソフトウェアはデータを自分のデバイスに保存します。".repeat(2)), "ja");
});
