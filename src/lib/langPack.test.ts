// 语言文件的导入导出。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { exportFile, importFile, LangFileError, packCodeOf, placeholders } from "./langPack.ts";

const reference = {
  "nav.inbox": "Inbox",
  "group.count": "{n} messages",
  "update.available": "Postflow {version} is available",
};

const file = (messages: Record<string, unknown>, extra: Record<string, unknown> = {}) =>
  JSON.stringify({ postflow_language: 1, code: "ja", name: "日本語", base: "en", messages, ...extra });

test("an exported file imports back unchanged", () => {
  const text = exportFile({ code: "ja", name: "日本語", base: "en" }, { "nav.inbox": "受信トレイ" }, "0.1.12");
  const report = importFile(text, reference);
  assert.deepEqual(report.pack, { code: "ja", name: "日本語", base: "en", messages: { "nav.inbox": "受信トレイ" } });
  assert.equal(report.used, 1);
  assert.equal(report.total, 3);
});

test("unknown keys are skipped and listed", () => {
  const report = importFile(file({ "nav.inbox": "受信トレイ", "old.key": "x" }), reference);
  assert.deepEqual(report.unknown, ["old.key"]);
  assert.deepEqual(Object.keys(report.pack.messages), ["nav.inbox"]);
});

test("a translation that loses or invents a placeholder is dropped", () => {
  const report = importFile(
    file({ "group.count": "メッセージ", "update.available": "Postflow {version} が公開されました", "nav.inbox": "{x}" }),
    reference,
  );
  assert.deepEqual(report.badPlaceholders.sort(), ["group.count", "nav.inbox"]);
  assert.deepEqual(Object.keys(report.pack.messages), ["update.available"]);
});

test("broken files explain why", () => {
  const reason = (text: string) => {
    try {
      importFile(text, reference);
      return "ok";
    } catch (e) {
      return e instanceof LangFileError ? e.reason : "other";
    }
  };
  assert.equal(reason("{not json"), "json");
  assert.equal(reason("[]"), "format");
  assert.equal(reason(file({ "nav.inbox": "x" }, { postflow_language: 99 })), "version");
  assert.equal(reason(file({ "nav.inbox": "x" }, { code: "日本" })), "code");
  assert.equal(reason(file({ "nav.inbox": "x" }, { code: "EN" })), "builtin");
  assert.equal(reason(file({ "nav.inbox": "x" }, { name: " " })), "name");
  assert.equal(reason(file({ "nav.inbox": "  ", "old.key": "x" })), "empty");
  assert.equal(reason(`﻿${file({ "nav.inbox": "x" })}`), "ok");
});

test("base falls back to English and placeholders are compared as sets", () => {
  assert.equal(importFile(file({ "nav.inbox": "x" }, { base: "fr" }), reference).pack.base, "en");
  assert.deepEqual(placeholders("{b} and {a} and {a}"), ["{a}", "{b}"]);
  assert.equal(packCodeOf("pack:zh-TW"), "zh-TW");
  assert.equal(packCodeOf("zh"), null);
});
