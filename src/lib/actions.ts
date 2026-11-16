// 对条目的操作。工具栏按钮、快捷键和多选面板都调用这里，行为保持一致。

import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";
import { api, type Item, type ItemDetail, type ItemPatch } from "./api";
import { openExternal } from "./platform";
import { keys, useItems, useMarkAllRead, useSetItems } from "./queries";
import { filterOf, useUi } from "./store";

type Flag = "read" | "starred" | "archived";

export function useActions() {
  const qc = useQueryClient();
  const scope = useUi((s) => s.scope);
  const kindFilter = useUi((s) => s.kindFilter);
  const filter = useMemo(() => filterOf(scope, kindFilter), [scope, kindFilter]);
  const { items } = useItems(filter);
  const setItems = useSetItems();
  const markAll = useMarkAllRead();

  /** 当前操作的对象：有勾选时是勾选的那些，否则是阅读面板里打开的那一条。 */
  const targets = useCallback((): Item[] => {
    const { selection, openId } = useUi.getState();
    const ids = selection.length > 0 ? selection : openId ? [openId] : [];
    return ids
      .map((id) => items.find((i) => i.id === id) ?? qc.getQueryData<ItemDetail>(keys.item(id)))
      .filter((i): i is Item => i !== undefined);
  }, [items, qc]);

  const apply = useCallback(
    (list: Item[], patch: ItemPatch) => {
      if (list.length === 0) return;
      const ids = list.map((i) => i.id);

      // 归档会让条目离开当前列表，先想好接下来打开哪一条，阅读不被打断
      const leaves =
        patch.archived !== undefined &&
        filter.view !== "starred" &&
        (filter.view === "archived" ? !patch.archived : patch.archived);
      if (leaves) {
        const gone = new Set(ids);
        const first = items.findIndex((i) => gone.has(i.id));
        const after = items.slice(first + 1).find((i) => !gone.has(i.id));
        const before = items.slice(0, Math.max(first, 0)).reverse().find((i) => !gone.has(i.id));
        const ui = useUi.getState();
        // 打开的那条还留在列表里就不动它
        if (ui.openId === null || gone.has(ui.openId)) ui.open(after?.id ?? before?.id ?? null);
        ui.setSelection(ui.selection.filter((id) => !gone.has(id)));
      }
      setItems.mutate({ ids, patch });
    },
    [filter.view, items, setItems],
  );

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
      if (items.length === 0) return;
      const { openId, open } = useUi.getState();
      const current = items.findIndex((i) => i.id === openId);
      const next =
        current === -1
          ? delta === 1
            ? 0
            : items.length - 1
          : Math.min(items.length - 1, Math.max(0, current + delta));
      const item = items[next];
      if (item && item.id !== openId) open(item.id);
    },
    [items],
  );

  const openOriginal = useCallback(() => {
    const url = targets()[0]?.url;
    if (url) void openExternal(url);
  }, [targets]);

  const markAllRead = useCallback(() => markAll.mutate(filter), [filter, markAll]);

  const refresh = useCallback(() => {
    const id = scope.kind === "feed" ? scope.feedId : scope.kind === "account" ? scope.accountId : undefined;
    void api.refresh(id);
  }, [scope]);

  const refreshAll = useCallback(() => void api.refresh(), []);

  return { items, filter, targets, apply, toggle, move, openOriginal, markAllRead, refresh, refreshAll };
}
