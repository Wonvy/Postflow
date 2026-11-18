// ⌘K 命令面板的搜索。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { type Command, rankCommands } from "./commands.ts";

const cmd = (id: string, label: string, keywords?: string): Command => ({ id, label, group: "", keywords, run: () => {} });
const list = [
  cmd("compose", "写新邮件", "compose new message"),
  cmd("inbox", "收件箱", "inbox"),
  cmd("feed", "Rust Blog", "blog.rust-lang.org"),
  cmd("settings", "设置：外观", "settings appearance"),
  cmd("blogs", "博客"),
];

test("an empty query keeps every command in order", () => {
  assert.deepEqual(rankCommands(list, "  ").map((c) => c.id), list.map((c) => c.id));
});

test("names that start with the query come first, then words, contains, keywords and loose matches", () => {
  assert.deepEqual(rankCommands(list, "rust").map((c) => c.id), ["feed"]);
  assert.deepEqual(rankCommands(list, "blog").map((c) => c.id), ["feed"], "a word in the name, then the url keyword");
  assert.deepEqual(rankCommands(list, "外观").map((c) => c.id), ["settings"]);
  assert.deepEqual(rankCommands(list, "compose").map((c) => c.id), ["compose"]);
  assert.deepEqual(rankCommands(list, "新件").map((c) => c.id), ["compose"], "characters in order");
  assert.deepEqual(rankCommands(list, "zzz"), []);
});

test("Chinese names are found by their pinyin initials", () => {
  const zh = [cmd("sspai", "少数派"), cmd("inbox", "收件箱"), cmd("settings", "设置 · 外观")];
  assert.deepEqual(rankCommands(zh, "ssp").map((c) => c.id), ["sspai"]);
  assert.deepEqual(rankCommands(zh, "sjx").map((c) => c.id), ["inbox"]);
  assert.deepEqual(rankCommands(zh, "wg").map((c) => c.id), ["settings"], "inside the name too");
});

test("the case of the query doesn't matter", () => {
  assert.deepEqual(rankCommands(list, "RUST b").map((c) => c.id), ["feed"]);
});
