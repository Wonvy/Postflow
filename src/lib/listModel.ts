// 当前列表的完整模型：查询到的条目、分组、显示的行、能停留的位置。
// 列表、快捷键和操作都从这里取，保证它们对「哪一行代表什么」的理解一致。

import { useMemo } from "react";
import { groupItems, rowsOf, type Stop, stopOf } from "./grouping";
import { useItems } from "./queries";
import { filterOf, searchFilterOf, useGroupMode, useUi } from "./store";

export function useListModel() {
  const scope = useUi((s) => s.scope);
  const space = useUi((s) => s.space);
  const groupMode = useGroupMode();
  const expandedGroups = useUi((s) => s.expandedGroups);

  // 搜索栏开着、输入了词时，列表换成搜索结果
  const search = useUi((s) => (s.searchOpen ? s.searchQuery.trim() : ""));
  const searchAll = useUi((s) => s.searchAll);
  const filter = useMemo(
    () => (search ? searchFilterOf(scope, space, search, searchAll) : filterOf(scope, space)),
    [scope, space, search, searchAll],
  );
  const query = useItems(filter);
  // 收件箱顶部：提醒时间刚到、还没读的稍后提醒，按日期本来排在后面，这里提到最前
  const pinReminders = !search && scope.kind === "view" && scope.view === "inbox";
  const reminders = useItems({ ...filterOf(scope, space), view: "reminders" }, pinReminders);
  const items = useMemo(() => {
    if (!pinReminders || reminders.items.length === 0) return query.items;
    const pinned = new Set(reminders.items.map((i) => i.id));
    return [...reminders.items, ...query.items.filter((i) => !pinned.has(i.id))];
  }, [pinReminders, reminders.items, query.items]);
  const groups = useMemo(() => groupItems(items, groupMode), [items, groupMode]);
  const rows = useMemo(() => rowsOf(groups, new Set(expandedGroups)), [groups, expandedGroups]);
  const stops = useMemo(() => rows.map(stopOf).filter((s): s is Stop => s !== null), [rows]);

  return { filter, query, items, groups, rows, stops, groupMode, search };
}
