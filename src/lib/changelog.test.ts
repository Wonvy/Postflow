// 更新说明的格式和取法。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { compareVersions, entriesBetween, entryText, parseChangelog, unwrap } from "./changelog.ts";

const SAMPLE = `# Changelog

## Unreleased

### English

- Not out yet

## 0.2.0 — 2026-12-01

### English

- Big thing

### 中文

- 大改动

## 0.1.10

- Same text in both languages

## v0.1.9 - 2026-11-17

### English

- Older
`;

test("reads versions, dates and both languages, skipping Unreleased", () => {
  const entries = parseChangelog(SAMPLE);
  assert.deepEqual(
    entries.map((e) => [e.version, e.date]),
    [
      ["0.2.0", "2026-12-01"],
      ["0.1.10", null],
      ["0.1.9", "2026-11-17"],
    ],
  );
  const [big, same, older] = entries;
  assert.equal(entryText(big!, "en"), "- Big thing");
  assert.equal(entryText(big!, "zh"), "- 大改动");
  assert.equal(entryText(same!, "zh"), "- Same text in both languages");
  // 没写中文时用英文
  assert.equal(entryText(older!, "zh"), "- Older");
});

test("wrapped bullets are joined back into one line", () => {
  assert.equal(unwrap("- A long line that\n  goes on\n- Next"), "- A long line that goes on\n- Next");
  assert.equal(unwrap("- 列表分成\n  今天、昨天"), "- 列表分成今天、昨天");
  assert.equal(unwrap("- 订阅 `youtube.com/@名字`\n  这样的地址"), "- 订阅 `youtube.com/@名字` 这样的地址");
});

test("versions compare by number, prereleases first", () => {
  assert.ok(compareVersions("0.1.10", "0.1.9") > 0);
  assert.ok(compareVersions("v1.0.0", "0.9.9") > 0);
  assert.equal(compareVersions("0.1.9", "v0.1.9"), 0);
  assert.ok(compareVersions("0.2.0-beta.1", "0.2.0") < 0);
  assert.ok(compareVersions("0.2.0-beta.2", "0.2.0-beta.1") > 0);
});

test("updating across several versions lists all of them, newest first", () => {
  const entries = parseChangelog(SAMPLE);
  assert.deepEqual(
    entriesBetween(entries, "0.1.9", "0.2.0").map((e) => e.version),
    ["0.2.0", "0.1.10"],
  );
  assert.deepEqual(entriesBetween(entries, "0.1.10", "0.1.10").map((e) => e.version), []);
  assert.deepEqual(entriesBetween(entries, null, "0.1.9").map((e) => e.version), ["0.1.9"]);
});

test("the repository's CHANGELOG.md can be read and every version has both languages", () => {
  const text = readFileSync(new URL("../../CHANGELOG.md", import.meta.url), "utf8");
  assert.match(text, /^## Unreleased$/m, "new entries go under Unreleased");
  const entries = parseChangelog(text);
  assert.ok(entries.length > 0);
  for (const entry of entries) {
    assert.ok(entry.en.startsWith("- "), `${entry.version} has English notes`);
    assert.ok(entry.zh.startsWith("- "), `${entry.version} has Chinese notes`);
    assert.notEqual(entry.en, entry.zh, `${entry.version} has two different languages`);
  }
  const versions = entries.map((e) => e.version);
  assert.deepEqual([...versions].sort((a, b) => compareVersions(b, a)), versions, "newest first");
});
