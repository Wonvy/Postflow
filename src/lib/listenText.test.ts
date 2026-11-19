// 听新闻的纯函数。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { firstSentences, heading, nextInQueue, spokenSummary } from "./listenText.ts";

test("AI summaries lose their Markdown", () => {
  const md = "**央行宣布降息 0.25 个百分点**\n\n- 新利率从下周一起执行\n- 市场普遍预期年内还会再降一次\n1. 房贷利率随之下调";
  assert.equal(spokenSummary(md), "央行宣布降息 0.25 个百分点。\n新利率从下周一起执行。\n市场普遍预期年内还会再降一次。\n房贷利率随之下调。");
  assert.equal(
    spokenSummary("**The Fed held rates.**\n* Two cuts are *expected* this year\n- See [the statement](https://x.y)"),
    "The Fed held rates.\nTwo cuts are expected this year.\nSee the statement.",
  );
  assert.equal(spokenSummary("## 要点\n- `snake_case` 名字不动"), "要点。\nsnake_case 名字不动。");
});

test("the start of an article stands in for a missing summary", () => {
  assert.equal(firstSentences("第一句。第二句！第三句？第四句。"), "第一句。第二句！第三句？");
  assert.equal(firstSentences("One. Two.  Three. Four.", 2), "One. Two.");
  assert.equal(firstSentences("长".repeat(500), 3, 100), `${"长".repeat(100)}…`);
  assert.equal(firstSentences(""), "");
});

test("the next article follows the list", () => {
  const q = [{ id: "a" }, { id: "b" }, { id: "c" }];
  assert.equal(nextInQueue(q, "a")?.id, "b");
  assert.equal(nextInQueue(q, "c"), undefined);
  assert.equal(nextInQueue(q, "x"), undefined);
});

test("headings name the source when moving on", () => {
  assert.equal(heading("降息了", "财经日报", true), "财经日报。降息了");
  assert.equal(heading("Rates held", "Daily News", true), "Daily News. Rates held");
  assert.equal(heading("Rates held", "Daily News", false), "Rates held");
  assert.equal(heading("Same", "Same", true), "Same");
  assert.equal(heading("No source", "", true), "No source");
});
