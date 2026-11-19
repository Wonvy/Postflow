// 快速回复模板：读设置、起名字、插到光标处。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { fillTemplate, insertAt, templateList, templateName } from "./templatesList.ts";

test("broken entries are dropped", () => {
  const list = templateList({
    mail_templates: [{ id: "a", name: "Thanks", body: "Thanks!" }, { id: "b" }, null, "x"],
  });
  assert.deepEqual(list, [{ id: "a", name: "Thanks", body: "Thanks!" }]);
  assert.deepEqual(templateList(undefined), []);
});

test("a new template is named after its first line", () => {
  assert.equal(templateName("  收到，谢谢！\n下周一前回复你。"), "收到，谢谢！");
  assert.equal(templateName("x".repeat(30), 10), `${"x".repeat(10)}…`);
});

test("text goes in at the caret", () => {
  assert.deepEqual(insertAt("Hi ,", 3, "Mira"), { text: "Hi Mira,", caret: 7 });
  assert.deepEqual(insertAt("abc", 99, "!"), { text: "abc!", caret: 4 });
});

test("placeholders are filled from the message", () => {
  const now = new Date(2026, 10, 19);
  const ctx = { to: ["Mira Chen <mira@example.com>", "bo@example.com"], me: "Sam", now, lang: "en" as const };
  assert.equal(fillTemplate("Hi {first}, {Name} — {me}, {date}. {unknown}", ctx), "Hi Mira, Mira Chen — Sam, November 19, 2026. {unknown}");
  assert.equal(fillTemplate("{名字}你好", { ...ctx, to: ["李雷 <lilei@example.com>"], lang: "zh" }), "李雷你好");
  assert.equal(fillTemplate("Hi {name}", { ...ctx, to: ["only@example.com"] }), "Hi only");
  assert.equal(fillTemplate("Hi {name}", { ...ctx, to: [] }), "Hi ");
});
