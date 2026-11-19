import assert from "node:assert/strict";
import { test } from "node:test";
import { feedErrorText } from "./feedError.ts";

// 把词条名和参数原样拼出来，看选的是哪一条
const t = (key: string, vars?: Record<string, string | number>) => (vars ? `${key} ${JSON.stringify(vars)}` : key);

test("feed errors are explained by their kind", () => {
  // 小宇宙的订阅源回了一个「403 · 无法访问」的网页
  assert.equal(
    feedErrorText(t, 'the server refused to send the feed and returned a page saying "403 · 无法访问"', "feed_refused"),
    'feedError.refused {"title":"403 · 无法访问"}',
  );
  assert.equal(
    feedErrorText(t, 'the server refused to send the feed and returned a page saying ""', "feed_refused"),
    "feedError.refusedPage",
  );
  assert.equal(feedErrorText(t, "server returned HTTP 404", "http_status"), 'feedError.gone {"status":404}');
  assert.equal(feedErrorText(t, "server returned HTTP 403", "http_status"), 'feedError.denied {"status":403}');
  assert.equal(feedErrorText(t, "server returned HTTP 429", "http_status"), "feedError.tooMany");
  assert.equal(feedErrorText(t, "server returned HTTP 502", "http_status"), 'feedError.server {"status":502}');
  assert.equal(feedErrorText(t, "server returned HTTP 418", "http_status"), 'feedError.status {"status":418}');
  assert.equal(feedErrorText(t, "request failed: timed out", "network"), "feedError.timeout");
  assert.equal(feedErrorText(t, "request failed: connection refused", "network"), "feedError.network");
});

test("errors recorded before the kind was kept are recognized from the message", () => {
  assert.equal(feedErrorText(t, "not a feed: unable to parse feed: no root element"), "feedError.notAFeed");
  assert.equal(feedErrorText(t, "server returned HTTP 410", null), 'feedError.gone {"status":410}');
  assert.equal(feedErrorText(t, "request failed: dns error"), "feedError.network");
  // 认不出来的照原样给
  assert.equal(feedErrorText(t, "something else entirely"), "something else entirely");
});
