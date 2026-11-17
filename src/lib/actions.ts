// 对条目的操作。工具栏按钮、快捷键和多选都调用这里，行为保持一致。
//
// 打开的是一个折叠着的组时，操作作用到组里的每一条：归档一个会话就是归档整个会话。

import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";
import { api, type Item, type ItemDetail, type ItemPatch } from "./api";
import { useListModel } from "./listModel";
import { openExternal } from "./platform";
import { keys, useMarkAllRead, useSetItems } from "./queries";
import { isArchiveLike, useUi } from "./store";

type Flag = "read" | "starred" | "archived";

export function useActions() {
  const qc = useQueryClient();
  const scope = useUi((s) => s.scope);
  const { items, groups, stops, filter } = useListModel();
  const setItems = useSetItems();
  const markAll = useMarkAllRead();

  const byId = useMemo(() => new Map(items.map((i) => [i.id, i])), [items]);

  const resolve = useCallback(
    (ids: string[]): Item[] =>
      ids
        .map((id) => byId.get(id) ?? qc.getQueryData<ItemDetail>(keys.item(id)))
        .filter((i): i is Item => i !== undefined),
    [byId, qc],
  );

  /** 列表里一个位置代表的所有条目：折叠的组是组里每一条，其他就是它自己。 */
  const membersOf = useCallback(
    (id: string): string[] => stops.find((s) => s.id === id)?.members ?? [id],
    [stops],
  );

  /** 阅读面板里打开的那一行代表的条目，不管有没有勾选。 */
  const openTargets = useCallback((): Item[] => {
    const { openId } = useUi.getState();
    return openId ? resolve(membersOf(openId)) : [];
  }, [membersOf, resolve]);

  /** 当前操作的对象：有勾选时是勾选的那些，否则是打开的那一行。 */
  const targets = useCallback((): Item[] => {
    const { selection } = useUi.getState();
    return selection.length > 0 ? resolve(selection) : openTargets();
  }, [openTargets, resolve]);

  const apply = useCallback(
    (list: Item[], patch: ItemPatch) => {
      if (list.length === 0) return;
      const ids = list.map((i) => i.id);

      // 归档会让条目离开当前列表，先想好接下来打开哪一行，阅读不被打断
      const leaves =
        patch.archived !== undefined &&
        filter.view !== "starred" &&
        filter.view !== "all" &&
        (isArchiveLike(filter.view) ? !patch.archived : patch.archived);
      if (leaves) {
        const gone = new Set(ids);
        const remaining = (members: string[]) => members.some((m) => !gone.has(m));
        const first = stops.findIndex((s) => s.members.some((m) => gone.has(m)));
        const after = stops.slice(first + 1).find((s) => remaining(s.members));
        const before = stops
          .slice(0, Math.max(first, 0))
          .reverse()
          .find((s) => remaining(s.members));
        const target = after ?? before;
        // 组里只走了一部分时，打开它剩下的第一条
        const nextId = target ? (gone.has(target.id) ? target.members.find((m) => !gone.has(m)) : target.id) : null;
        const ui = useUi.getState();
        // 打开的那条还留在列表里就不动它
        if (ui.openId === null || gone.has(ui.openId)) ui.open(nextId ?? null);
        ui.setSelection(ui.selection.filter((id) => !gone.has(id)));
      }
      setItems.mutate({ ids, patch });
    },
    [filter.view, stops, setItems],
  );

  /** 按 ID 修改条目，拖到侧栏的视图上时用。 */
  const applyTo = useCallback((ids: string[], patch: ItemPatch) => apply(resolve(ids), patch), [apply, resolve]);

  /** 全都已经是这个状态就取消，否则全部设上。 */
  const toggle = useCallback(
    (flag: Flag, list = targets()) => {
      if (list.length === 0) return;
      apply(list, { [flag]: !list.every((i) => i[flag]) });
    },
    [apply, targets],
  );

  const move = useCallback(
    (delta: 1 | -1) => {
      if (stops.length === 0) return;
      const { openId, open } = useUi.getState();
      const current = stops.findIndex((s) => s.id === openId);
      const next =
        current === -1
          ? delta === 1
            ? 0
            : stops.length - 1
          : Math.min(stops.length - 1, Math.max(0, current + delta));
      const stop = stops[next];
      if (stop && stop.id !== openId) open(stop.id);
    },
    [stops],
  );

  /** 勾选或取消勾选一行。折叠的组整组进出。 */
  const toggleSelected = useCallback(
    (id: string) => {
      const ui = useUi.getState();
      const members = membersOf(id);
      const all = members.every((m) => ui.selection.includes(m));
      ui.setSelection(
        all ? ui.selection.filter((m) => !members.includes(m)) : [...new Set([...ui.selection, ...members])],
      );
    },
    [membersOf],
  );

  /** 展开打开着的那个组（→ 键）。 */
  const expandOpen = useCallback(() => {
    const { openId, setGroupExpanded } = useUi.getState();
    const group = groups.find((g) => g.items.length > 1 && g.lead.id === openId);
    if (group) setGroupExpanded(group.key, true);
  }, [groups]);

  /** 收起打开的条目所在的组，停在组上（← 键）。 */
  const collapseOpen = useCallback(() => {
    const { openId, open, setGroupExpanded, expandedGroups } = useUi.getState();
    const group = groups.find(
      (g) => g.items.length > 1 && expandedGroups.includes(g.key) && g.items.some((i) => i.id === openId),
    );
    if (group) {
      setGroupExpanded(group.key, false);
      open(group.lead.id);
    }
  }, [groups]);

  const openOriginal = useCallback(() => {
    const url = targets()[0]?.url;
    if (url) void openExternal(url);
  }, [targets]);

  const markAllRead = useCallback(() => markAll.mutate(filter), [filter, markAll]);

  /** 全部标为已读之前先问一下：工具栏按钮和 Shift+A 都走这里。 */
  const confirmMarkAllRead = useCallback(() => useUi.getState().setDialog({ type: "mark-all-read" }), []);

  const refresh = useCallback(() => {
    const id = scope.kind === "feed" ? scope.feedId : scope.kind === "account" ? scope.accountId : undefined;
    void api.refresh(id);
  }, [scope]);

  const refreshAll = useCallback(() => void api.refresh(), []);

  /** 列表换成这一条所在的订阅源或邮箱，阅读面板里还开着它。 */
  const goToSource = useCallback((item: Pick<Item, "id" | "kind" | "source_id">) => {
    const ui = useUi.getState();
    ui.setScope(
      item.kind === "mail" ? { kind: "account", accountId: item.source_id } : { kind: "feed", feedId: item.source_id },
    );
    // 内置视图里只看邮件或订阅时，换到订阅源或邮箱不受影响；打开的那一条留着
    ui.open(item.id);
  }, []);

  return {
    items,
    filter,
    membersOf,
    itemsOf: resolve,
    targets,
    openTargets,
    apply,
    applyTo,
    toggle,
    move,
    toggleSelected,
    expandOpen,
    collapseOpen,
    openOriginal,
    markAllRead,
    confirmMarkAllRead,
    goToSource,
    refresh,
    refreshAll,
  };
}
