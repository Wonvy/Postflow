// 当前列表的完整模型：查询到的条目、分组、显示的行、能停留的位置。
// 列表、快捷键和操作都从这里取，保证它们对「哪一行代表什么」的理解一致。

import { useMemo } from "react";
import { groupItems, rowsOf, type Stop, stopOf } from "./grouping";
import { useItems } from "./queries";
import { filterOf, searchFilterOf, useGroupMode, useUi } from "./store";

export function useListModel() {
  const scope = useUi((s) => s.scope);
  const kindFilter = useUi((s) => s.kindFilter);
  const groupMode = useGroupMode();
  const expandedGroups = useUi((s) => s.expandedGroups);

  // 搜索栏开着、输入了词时，列表换成搜索结果
  const search = useUi((s) => (s.searchOpen ? s.searchQuery.trim() : ""));
  const searchAll = useUi((s) => s.searchAll);
  const filter = useMemo(
    () => (search ? searchFilterOf(scope, kindFilter, search, searchAll) : filterOf(scope, kindFilter)),
    [scope, kindFilter, search, searchAll],
  );
  const query = useItems(filter);
  const { items } = query;
  const groups = useMemo(() => groupItems(items, groupMode), [items, groupMode]);
  const rows = useMemo(() => rowsOf(groups, new Set(expandedGroups)), [groups, expandedGroups]);
  const stops = useMemo(() => rows.map(stopOf).filter((s): s is Stop => s !== null), [rows]);

  return { filter, query, items, groups, rows, stops, groupMode, search };
}
