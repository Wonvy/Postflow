// 字数和阅读时间。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { readingMinutes, totalWords, wordCount } from "./wordCount.ts";

test("Chinese counts characters, other languages count words", () => {
  assert.deepEqual(wordCount("今天天气很好"), { cjk: 6, words: 0 });
  assert.deepEqual(wordCount("Hello, world! It's 2026."), { cjk: 0, words: 4 });
  assert.deepEqual(wordCount("用 Rust 写的 ShibaPost 很快"), { cjk: 5, words: 2 });
  assert.equal(totalWords(wordCount("  ")), 0);
});

test("reading time rounds and never shows zero for real text", () => {
  assert.equal(readingMinutes({ cjk: 0, words: 0 }), 0);
  assert.equal(readingMinutes({ cjk: 10, words: 0 }), 1);
  assert.equal(readingMinutes({ cjk: 1200, words: 0 }), 3);
  assert.equal(readingMinutes({ cjk: 0, words: 1100 }), 5);
});
