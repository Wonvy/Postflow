// 侧栏订阅源顶层顺序的规则。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  insertFolder,
  folderParent,
  folderName,
  inFolder,
  moveTop,
  orderFolders,
  removeFolder,
  renameFolder,
  serializeTop,
  type TopEntry,
  topLevel,
} from "./feedTree.ts";

const feeds = [
  { id: "a", folder: "" },
  { id: "b", folder: "News" },
  { id: "c", folder: "" },
  { id: "d", folder: "Blogs" },
];
const F = (name: string): TopEntry => ({ kind: "folder", name });
const S = (id: string): TopEntry => ({ kind: "feed", id });

test("nested folders retain ancestors and distinct boundaries", () => {
  assert.equal(folderParent("Tools / Code / Rust"), "Tools / Code");
  assert.equal(folderName("Tools / Code / Rust"), "Rust");
  assert.equal(inFolder("Tools / Code", "Tools"), true);
  assert.equal(inFolder("Toolshed", "Tools"), false);
  assert.deepEqual(orderFolders([{ id: "x", folder: "Tools / Code / Rust" }], []),
    ["Tools", "Tools / Code", "Tools / Code / Rust"]);
});

test("old settings (folder names only): folders first, then loose feeds in their order", () => {
  assert.deepEqual(topLevel(feeds, ["News"]), [F("News"), F("Blogs"), S("a"), S("c")]);
  assert.deepEqual(topLevel(feeds, undefined), [F("Blogs"), F("News"), S("a"), S("c")]);
});

test("saved mixed order is kept; stale and foldered feeds are skipped; new feeds go last", () => {
  const saved = [{ feed: "c" }, "News", { feed: "b" }, { feed: "gone" }, "Empty", { feed: "c" }];
  assert.deepEqual(topLevel(feeds, saved), [S("c"), F("News"), F("Empty"), F("Blogs"), S("a")]);
});

test("older versions still read the folder names out of a mixed list", () => {
  const saved = serializeTop([S("c"), F("News"), S("a"), F("Blogs")]);
  assert.deepEqual(saved, [{ feed: "c" }, "News", { feed: "a" }, "Blogs"]);
  assert.deepEqual(orderFolders(feeds, saved), ["News", "Blogs"]);
});

test("a feed can move above a folder, and out of a folder to the top", () => {
  const top = [F("News"), F("Blogs"), S("a"), S("c")];
  assert.deepEqual(moveTop(top, S("a"), F("News"), "before"), [S("a"), F("News"), F("Blogs"), S("c")]);
  assert.deepEqual(moveTop(top, S("b"), null, "before"), [S("b"), F("News"), F("Blogs"), S("a"), S("c")]);
  assert.deepEqual(moveTop(top, F("News"), S("c"), "after"), [F("Blogs"), S("a"), S("c"), F("News")]);
  assert.deepEqual(moveTop(top, S("a"), null, "after"), [F("News"), F("Blogs"), S("c"), S("a")]);
});

test("new folders go after the last folder; deleting one leaves its feeds in its place", () => {
  assert.deepEqual(insertFolder([S("a"), F("News"), S("c")], "New"), [S("a"), F("News"), F("New"), S("c")]);
  assert.deepEqual(insertFolder([S("a")], "New"), [F("New"), S("a")]);
  assert.deepEqual(insertFolder([F("New")], "New"), [F("New")]);
  assert.deepEqual(removeFolder([S("a"), F("News"), S("c")], "News", ["b", "x"]), [S("a"), S("b"), S("x"), S("c")]);
});

test("renaming keeps the position, or merges into an existing folder", () => {
  assert.deepEqual(renameFolder([S("a"), F("News"), F("Blogs")], "News", "Press"), [S("a"), F("Press"), F("Blogs")]);
  assert.deepEqual(renameFolder([S("a"), F("News"), F("Blogs")], "News", "Blogs"), [S("a"), F("Blogs")]);
});
