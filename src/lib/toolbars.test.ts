// 工具栏布局。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { isDefaultLayout, moveTo, normalizeLayout, setPlace, TOOL_DEFAULTS, toolsAt } from "./toolbars.ts";

test("nothing saved gives the defaults", () => {
  assert.deepEqual(normalizeLayout("list", undefined), TOOL_DEFAULTS.list);
  assert.ok(isDefaultLayout("reader", normalizeLayout("reader", null)));
});

test("saved order is kept; unknown, duplicate and broken entries are dropped", () => {
  const layout = normalizeLayout("list", [
    { id: "refresh", place: "status" },
    { id: "gone", place: "top" },
    { id: "refresh", place: "top" },
    { id: "search", place: "nowhere" },
    "junk",
  ]);
  const saved = layout.filter((s) => s.id === "refresh" || s.id === "search");
  assert.deepEqual(saved, [
    { id: "refresh", place: "status" },
    { id: "search", place: "top" },
  ]);
  assert.equal(layout.length, TOOL_DEFAULTS.list.length);
});

test("buttons added in a newer version slot in after their default neighbour", () => {
  // 保存的时候还没有 markAllRead
  const layout = normalizeLayout("list", [
    { id: "search", place: "top" },
    { id: "kindFilter", place: "top" },
    { id: "refresh", place: "top" },
  ]);
  const ids = layout.map((s) => s.id);
  assert.equal(ids.indexOf("markAllRead"), ids.indexOf("search") + 1);
  assert.equal(layout.find((s) => s.id === "view")?.place, "status");
});

test("moving and placing buttons", () => {
  const base = TOOL_DEFAULTS.list;
  const moved = moveTo(base, "refresh", 0);
  assert.equal(moved[0]?.id, "refresh");
  assert.equal(moved.length, base.length);
  assert.deepEqual(moveTo(base, "missing", 0), base);
  const hidden = setPlace(base, "search", "hidden");
  assert.ok(!toolsAt(hidden, "top").includes("search"));
  assert.deepEqual(toolsAt(base, "status"), ["view", "group", "translate"]);
  assert.ok(!isDefaultLayout("list", hidden));
});

test("plugin buttons join at the end and keep their place while the plugin is off", () => {
  const withPlugin = normalizeLayout("reader", undefined, ["plugin:copy-code:copy"]);
  assert.deepEqual(withPlugin.at(-1), { id: "plugin:copy-code:copy", place: "top" });
  assert.ok(isDefaultLayout("reader", withPlugin));
  const saved = setPlace(withPlugin, "plugin:copy-code:copy", "status");
  const off = normalizeLayout("reader", saved, []);
  assert.equal(off.find((s) => s.id === "plugin:copy-code:copy")?.place, "status");
});
