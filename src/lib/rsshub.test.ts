// RSSHub 订阅地址的拼法。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { buildFeedUrl, initialValues, normalizeInstance, plainDescription, type RsshubRoute } from "./rsshub.ts";

const issues = { namespace: "github", path: "/issue/:user/:repo/:state?" };

test("parameters fill the path; optional ones can be left out", () => {
  const base = "https://rsshub.example.com";
  assert.equal(buildFeedUrl(base, issues, { user: "DIYgod", repo: "RSSHub" }), `${base}/github/issue/DIYgod/RSSHub`);
  assert.equal(
    buildFeedUrl(base, issues, { user: "DIYgod", repo: "RSSHub", state: "closed" }),
    `${base}/github/issue/DIYgod/RSSHub/closed`,
  );
  assert.equal(buildFeedUrl(base, issues, { user: "DIYgod" }), null);
  assert.equal(buildFeedUrl(base, { namespace: "hn", path: "/" }, {}), `${base}/hn`);
});

test("values are escaped, except slashes in multi-segment parameters", () => {
  const base = "https://r.example";
  assert.equal(buildFeedUrl(base, { namespace: "x", path: "/search/:q" }, { q: "a b/c" }), `${base}/x/search/a%20b%2Fc`);
  assert.equal(
    buildFeedUrl(base, { namespace: "81", path: "/81rc/:category{.+}?" }, { category: "sy/gzdt 1" }),
    `${base}/81/81rc/sy/gzdt%201`,
  );
});

test("instance addresses are tidied up", () => {
  assert.equal(normalizeInstance(""), "https://rsshub.app");
  assert.equal(normalizeInstance("rsshub.example.com/"), "https://rsshub.example.com");
  assert.equal(normalizeInstance("http://localhost:1200"), "http://localhost:1200");
  assert.equal(normalizeInstance("https://example.com/rsshub/"), "https://example.com/rsshub");
  assert.equal(normalizeInstance("not a host"), null);
});

test("form starts with values found in the page address, then defaults", () => {
  const route = {
    params: [
      { name: "user", default: null },
      { name: "state", default: "open" },
    ],
  } as unknown as RsshubRoute;
  assert.deepEqual(initialValues(route, { user: "DIYgod" }), { user: "DIYgod", state: "open" });
});

test("markdown in descriptions becomes plain text", () => {
  assert.equal(plainDescription("See [the docs](https://x.y) and `code`\n\n\n::: tip\n**bold**"), "See the docs and code\n\n\nbold".replace("\n\n\n", "\n\n"));
});
