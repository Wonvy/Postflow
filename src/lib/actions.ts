// 对条目的操作。工具栏按钮、快捷键和多选都调用这里，行为保持一致。
//
// 打开的是一个折叠着的组时，操作作用到组里的每一条：归档一个会话就是归档整个会话。

import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";
import type { InfiniteData } from "@tanstack/react-query";
import { api, type Item, type ItemDetail, type ItemPage, type ItemPatch, type Nav } from "./api";
import type { ComposeMode } from "./compose";
import { useListModel } from "./listModel";
import { openExternal } from "./platform";
import { keys, useMarkAllRead, useSetItems } from "./queries";
import { articleActions, canArchive, isArchiveLike, type Space, useUi } from "./store";

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
      // 草稿和已发送的信只在各自的文件夹里，不能归档
      const usable = flag === "archived" ? list.filter((i) => !i.draft && !i.sent) : list;
      if (usable.length === 0) return;
      // 订阅那边不显示「已归档」时文章不归档：E、往左滑改成标为已读
      if (flag === "archived" && !canArchive(usable, articleActions(useUi.getState().hiddenViews.feeds ?? []))) {
        apply(usable, { read: true });
        return;
      }
      apply(usable, { [flag]: !usable.every((i) => i[flag]) });
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
    const id =
      scope.kind === "feed"
        ? scope.feedId
        : scope.kind === "account" || scope.kind === "mailbox"
          ? scope.accountId
          : undefined;
    if (id) void api.refresh(id);
    else refreshSpace(useUi.getState().space);
  }, [scope]);

  /** 刷新当前空间：邮件那边收全部邮箱，订阅那边只抓订阅源。快捷键 R。 */
  const refreshAll = useCallback(() => refreshSpace(useUi.getState().space), []);

  /**
   * 写信。回复、全部回复、转发作用于 `item`，不给就是阅读面板里打开的那封邮件；写新邮件时
   * 默认从当前邮箱发。还没有邮箱时先去添加一个。
   */
  const compose = useCallback(
    (mode: ComposeMode, item?: Item) => {
      const ui = useUi.getState();
      if (!qc.getQueryData<Nav>(keys.nav)?.accounts.length) {
        ui.setDialog({ type: "add-account" });
        return;
      }
      const target = item ?? (ui.openId ? resolve([ui.openId])[0] : undefined);
      if (mode === "new") {
        const accountId =
          scope.kind === "account" ? scope.accountId : target?.kind === "mail" ? target.source_id : undefined;
        ui.setDialog({ type: "compose", mode, accountId });
        return;
      }
      if (target?.kind !== "mail") return;
      // 对草稿回复、转发没有意义：接着写它
      const m = target.draft ? "draft" : mode;
      ui.setDialog({ type: "compose", mode: m, itemId: target.id, accountId: target.source_id });
    },
    [qc, resolve, scope],
  );

  /** 这些条目马上从所有列表里拿掉，阅读区开着其中一条时关上。 */
  const dropFromLists = useCallback(
    (ids: string[]) => {
      const gone = new Set(ids);
      const ui = useUi.getState();
      if (ui.openId && gone.has(ui.openId)) ui.open(null);
      ui.setSelection(ui.selection.filter((id) => !gone.has(id)));
      qc.setQueriesData<InfiniteData<ItemPage>>({ queryKey: ["items"] }, (old) =>
        old
          ? { ...old, pages: old.pages.map((p) => ({ ...p, items: p.items.filter((i) => !gone.has(i.id)) })) }
          : old,
      );
    },
    [qc],
  );

  /**
   * 稍后提醒：选中的（或打开的）条目先从列表里拿掉，到 `until` 再回到收件箱。`null` 是取消，马上回来。
   * 在「稍后提醒」视图里改时间时留在列表里。
   */
  const snooze = useCallback(
    (until: number | null, list = targets()) => {
      if (list.length === 0) return;
      const ids = list.map((i) => i.id);
      const ui = useUi.getState();
      const inSnoozed = ui.scope.kind === "view" && ui.scope.view === "snoozed";
      if (until !== null && !inSnoozed) dropFromLists(ids);
      if (until === null && inSnoozed) dropFromLists(ids);
      api
        .snooze(ids, until)
        .catch(() => {})
        .finally(() => {
          void qc.invalidateQueries({ queryKey: ["items"] });
          void qc.invalidateQueries({ queryKey: ["item"] });
          void qc.invalidateQueries({ queryKey: keys.nav });
        });
    },
    [dropFromLists, qc, targets],
  );

  /** 删掉列表里的草稿：先从列表里拿掉，再让后台去邮箱里删。 */
  const deleteDrafts = useCallback(
    (list: Item[]) => {
      const drafts = list.filter((i) => i.draft);
      if (drafts.length === 0) return;
      dropFromLists(drafts.map((d) => d.id));
      for (const d of drafts) void api.deleteDraftItem(d.id).catch(() => void qc.invalidateQueries({ queryKey: ["items"] }));
    },
    [dropFromLists, qc],
  );

  /** 把邮件移到同一个邮箱的另一个文件夹。先从列表里拿掉，移成以后在那个文件夹里出现。 */
  const moveTo = useCallback(
    (list: Item[], mailbox: string) => {
      const mail = list.filter((i) => i.kind === "mail" && !i.draft);
      if (mail.length === 0) return;
      dropFromLists(mail.map((i) => i.id));
      void api
        .moveMail(
          mail.map((i) => i.id),
          mailbox,
        )
        .then(() => qc.invalidateQueries({ queryKey: keys.nav }))
        .catch(() => void qc.invalidateQueries({ queryKey: ["items"] }));
    },
    [dropFromLists, qc],
  );

  /** 列表换成这一条所在的订阅源或邮箱，阅读面板里还开着它。 */
  const goToSource = useCallback((item: Pick<Item, "id" | "kind" | "source_id">) => {
    const ui = useUi.getState();
    ui.setScope(
      item.kind === "mail" ? { kind: "account", accountId: item.source_id } : { kind: "feed", feedId: item.source_id },
    );
    // 换到另一边的订阅源或邮箱时空间跟着切换（见 setScope）；打开的那一条留着
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
    compose,
    deleteDrafts,
    moveTo,
    snooze,
  };
}

/** 刷新一个空间里的全部来源。 */
export function refreshSpace(space: Space) {
  if (space === "mail") void api.syncAccount();
  else void api.refresh(undefined, true);
}
