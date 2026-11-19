// 划线的位置：记下来、找回来、导出。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { locate, quoteAt, toMarkdown } from "./highlightsText.ts";

const text = "The cat sat. The dog sat. The cat ran away from the dog.";

test("a quote is found again by its context", () => {
  const second = text.indexOf("The cat ran");
  const q = quoteAt(text, second, second + 7);
  assert.equal(q.quote, "The cat");
  assert.equal(locate(text, q.quote, q.prefix, q.suffix), second);
  const first = quoteAt(text, 0, 7);
  assert.equal(locate(text, first.quote, first.prefix, first.suffix), 0);
});

test("changed whitespace still matches", () => {
  const reflowed = "The cat sat.\n  The dog\nsat.";
  assert.equal(locate(reflowed, "The dog sat."), reflowed.indexOf("The dog"));
  assert.equal(locate(text, "not there"), -1);
  assert.equal(locate(text, ""), -1);
});

test("markdown export", () => {
  const md = toMarkdown([
    { title: "Quiet", url: "https://example.com/q", source: "example.com", highlights: [{ quote: "a\nb", note: "my note" }] },
    { title: "Loud", url: null, source: "", highlights: [{ quote: "c", note: "" }] },
  ]);
  assert.equal(md, "## [Quiet](https://example.com/q)\n*example.com*\n\n> a\n> b\n\nmy note\n\n## Loud\n\n> c");
});
