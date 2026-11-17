// 订阅统计页的计算。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import type { FeedStats } from "./api.ts";
import { flagsOf, hoursMinutes, isFinished, niceMax, perWeek, ratio, scrollDepth, sortFeeds } from "./stats.ts";

const DAY = 86_400_000;
const NOW = 1_800_000_000_000;

function feed(patch: Partial<FeedStats>): FeedStats {
  return {
    id: "f",
    title: "Feed",
    url: "https://x.example/feed",
    icon_url: null,
    folder: "",
    added_at: NOW - 400 * DAY,
    error: null,
    total: 10,
    last_published: NOW - DAY,
    published: 10,
    weekly: [],
    full_share: 1,
    read: 0,
    opened: 0,
    finished: 0,
    avg_depth: null,
    dwell_ms: 0,
    ...patch,
  };
}

const window30 = { now: NOW, since: NOW - 30 * DAY, tracking_since: NOW - 40 * DAY };

test("flags: errors, inactive feeds, summary-only feeds, feeds never read", () => {
  assert.deepEqual(flagsOf(feed({}), window30), ["unread"]);
  assert.deepEqual(flagsOf(feed({ opened: 1 }), window30), []);
  assert.deepEqual(flagsOf(feed({ error: "timeout", opened: 1 }), window30), ["error"]);
  assert.deepEqual(flagsOf(feed({ last_published: NOW - 90 * DAY, published: 0 }), window30), ["stale"]);
  assert.deepEqual(flagsOf(feed({ full_share: 0.1, opened: 1 }), window30), ["summary"]);
  // 阅读记录才开始几天：不说「一篇没读」
  assert.deepEqual(flagsOf(feed({}), { ...window30, tracking_since: NOW - 3 * DAY }), []);
  assert.deepEqual(flagsOf(feed({}), { ...window30, tracking_since: null }), []);
});

test("sorting by reading time, then by how much the feed posts", () => {
  const feeds = [
    feed({ id: "a", title: "A", dwell_ms: 0, published: 3 }),
    feed({ id: "b", title: "B", dwell_ms: 60_000 }),
    feed({ id: "c", title: "C", dwell_ms: 0, published: 9 }),
  ];
  assert.deepEqual(sortFeeds(feeds, "dwell", true).map((f) => f.id), ["b", "c", "a"]);
  assert.deepEqual(sortFeeds(feeds, "title", false).map((f) => f.id), ["a", "b", "c"]);
  // 没法算比例的排在最后
  const rates = [feed({ id: "x", published: 0 }), feed({ id: "y", published: 4, opened: 1 })];
  assert.deepEqual(sortFeeds(rates, "opened", true).map((f) => f.id), ["y", "x"]);
});

test("small helpers", () => {
  assert.equal(ratio(3, 0), null);
  assert.equal(ratio(5, 4), 1);
  assert.equal(perWeek(30, 30), 7);
  assert.deepEqual([0, 0.4, 3, 7, 12, 99, 101].map(niceMax), [1, 0.5, 5, 10, 20, 100, 200]);
  assert.deepEqual(hoursMinutes(95 * 60_000), { h: 1, m: 35 });
});

test("reading depth and when an article counts as finished", () => {
  assert.equal(scrollDepth(0, 800, 800), 1, "fits on the screen");
  assert.equal(scrollDepth(0, 500, 2000), 0.25);
  assert.equal(scrollDepth(1500, 500, 2000), 1);
  assert.equal(isFinished(0.95, 6000), true);
  assert.equal(isFinished(0.95, 2000), false, "too quick");
  assert.equal(isFinished(0.5, 60_000), false, "stopped halfway");
});
