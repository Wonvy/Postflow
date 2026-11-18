// 折叠相同邮件的分组规则。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import type { Item } from "./api.ts";
import { articleUrlKey, groupItems, rowsOf, stopOf, threadSubject } from "./grouping.ts";

const base = {
  url: null,
  author: null,
  summary: "",
  published_at: 0,
  read: true,
  starred: false,
  archived: false,
  source_icon: null,
  sender_address: null,
  has_attachments: false,
};

function mail(id: string, title: string, from: string, account = "acc", extra: Partial<Item> = {}): Item {
  return {
    ...base,
    id,
    kind: "mail",
    source_id: account,
    source_title: "Work",
    title,
    author: from.split("@")[0] ?? null,
    sender_address: from,
    ...extra,
  };
}

function article(id: string, feed: string): Item {
  return { ...base, id, kind: "article", source_id: feed, source_title: feed, title: `Post ${id}` };
}

const items = [
  mail("1", "Re: Design review", "ana@x", "acc", { read: false }),
  article("2", "feedA"),
  mail("3", "Build #3 passed", "ci@x"),
  mail("4", "Design review", "bob@x", "acc", { starred: true }),
  mail("5", "Build #2 failed", "ci@x", "acc", { read: false }),
  article("6", "feedA"),
  mail("7", "Design review", "ana@x", "other-account"),
  mail("8", "Re:", "x@x"),
  mail("9", "Re:", "y@x"),
];

test("reply and forward prefixes are ignored when matching conversations", () => {
  assert.equal(threadSubject("Re: RE:  Design review"), "design review");
  assert.equal(threadSubject("回复：转发: 周五团建安排"), "周五团建安排");
  assert.equal(threadSubject("Fwd[2]: Budget"), "budget");
  assert.equal(threadSubject("Re: "), "");
  // 不能把以 Re 开头的普通单词当成前缀
  assert.equal(threadSubject("Resume of candidate"), "resume of candidate");
});

test("by conversation: same account and subject fold together, articles never do", () => {
  const groups = groupItems(items, "thread");
  assert.deepEqual(
    groups.map((g) => g.items.map((i) => i.id)),
    [["1", "4"], ["2"], ["3"], ["5"], ["6"], ["7"], ["8"], ["9"]],
  );
  const [first] = groups;
  assert.ok(first);
  assert.equal(first.lead.id, "1");
  assert.equal(first.unread, 1);
  assert.equal(first.starred, true);
  assert.deepEqual(first.people, ["ana", "bob"]);
});

test("by sender: mail folds by address across accounts, articles by feed", () => {
  const groups = groupItems(items, "sender");
  assert.deepEqual(
    groups.map((g) => g.items.map((i) => i.id)),
    [["1", "7"], ["2", "6"], ["3", "5"], ["4"], ["8"], ["9"]],
  );
  assert.equal(groupItems(items, "none").length, items.length);
});

test("collapsed and expanded rows, and where the cursor can stop", () => {
  const groups = groupItems(items, "sender");
  const collapsed = rowsOf(groups, new Set());
  assert.deepEqual(
    collapsed.map((r) => r.kind),
    ["group", "group", "group", "item", "item", "item"],
  );
  const third = collapsed[2];
  assert.ok(third);
  assert.deepEqual(stopOf(third), { id: "3", members: ["3", "5"] });

  const key = groups[2]?.key ?? "";
  const expanded = rowsOf(groups, new Set([key]));
  assert.deepEqual(
    expanded.map((r) => (r.kind === "item" ? `${r.item.id}${r.child ? "*" : ""}` : `G${r.expanded ? "+" : ""}`)),
    ["G", "G", "G+", "3*", "5*", "4", "8", "9"],
  );
  const header = expanded[2];
  assert.ok(header);
  assert.equal(stopOf(header), null);
});

test("the same article from several feeds shows once", () => {
  const post = (id: string, feed: string, url: string | null, title: string): Item => ({
    ...article(id, feed),
    url,
    title,
  });
  const list = [
    post("a", "feedA", "https://www.example.com/2026/story/?utm_source=rss#top", "A story"),
    post("b", "feedB", "http://example.com/2026/story", "A story (repost)"),
    post("c", "feedC", null, "一模一样的长标题，转载到了好几个地方"),
    post("d", "feedD", null, "一模一样的长标题，转载到了好几个地方"),
    post("e", "feedA", null, "Weekly"),
    post("f", "feedB", null, "Weekly"),
    post("g", "feedA", "https://example.com/2026/other", "Another"),
  ];
  for (const mode of ["none", "thread", "mailbox"] as const) {
    assert.deepEqual(
      groupItems(list, mode).map((g) => g.items.map((i) => i.id)),
      [["a", "b"], ["c", "d"], ["e"], ["f"], ["g"]],
      mode,
    );
  }
  // 按来源分组时还是按订阅源
  assert.deepEqual(
    groupItems(list, "sender").map((g) => g.items.map((i) => i.id)),
    [["a", "e", "g"], ["b", "f"], ["c"], ["d"]],
  );
  assert.equal(articleUrlKey("https://Example.com/a/?ref=hn&id=3"), "example.com/a?id=3");
  assert.equal(articleUrlKey("mailto:x@example.com"), null);
});
