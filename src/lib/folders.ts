// 订阅源的文件夹和顺序。
//
// 文件夹只是订阅源上的一个名字。顶层的顺序（文件夹和文件夹外的源交错排列）、以及还没放进
// 订阅源的空文件夹，记在同步的设置 `folder_order` 里，规则见 feedTree.ts。
// 订阅源在文件夹里（包括文件夹外）的顺序由核心记在每个源上。

import { type QueryClient, useQueryClient } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";
import { api, type Nav, type Settings } from "./api";
import {
  insertFolder,
  orderFolders,
  removeFolder,
  renameFolder,
  serializeTop,
  type TopEntry,
  topLevel,
} from "./feedTree";
import { keys, useNav, useSettings } from "./queries";
import { useUi } from "./store";

const ORDER_KEY = "folder_order";

export function useFolders(): string[] {
  const nav = useNav();
  const settings = useSettings();
  return orderFolders(nav.data?.feeds ?? [], settings.data?.[ORDER_KEY]);
}

/** 侧栏「订阅」一组的顶层顺序。 */
export function useTopLevel(): TopEntry[] {
  const nav = useNav();
  const settings = useSettings();
  return useMemo(
    () => topLevel(nav.data?.feeds ?? [], settings.data?.[ORDER_KEY]),
    [nav.data?.feeds, settings.data],
  );
}

/** 按缓存里最新的数据算顶层顺序。拖放的回调里用它，不会拿到上一次渲染时的旧值。 */
function currentTop(qc: QueryClient): TopEntry[] {
  return topLevel(qc.getQueryData<Nav>(keys.nav)?.feeds ?? [], qc.getQueryData<Settings>(keys.settings)?.[ORDER_KEY]);
}

/** 先在本地缓存里把订阅源排好，界面不等核心。 */
function placeLocally(qc: QueryClient, folder: string, ids: string[]) {
  qc.setQueryData<Nav>(keys.nav, (old) => {
    if (!old) return old;
    const byId = new Map(old.feeds.map((f) => [f.id, f]));
    const moved = ids.map((id) => byId.get(id)).filter((f) => f !== undefined);
    const rest = old.feeds.filter((f) => !ids.includes(f.id));
    return { ...old, feeds: [...rest, ...moved.map((f) => ({ ...f, folder }))] };
  });
}

function saveTop(qc: QueryClient, entries: TopEntry[]) {
  const value = serializeTop(entries);
  const saved = qc.getQueryData<Settings>(keys.settings)?.[ORDER_KEY];
  if (JSON.stringify(saved) === JSON.stringify(value)) return;
  qc.setQueryData<Settings>(keys.settings, (old) => ({ ...old, [ORDER_KEY]: value }));
  void api.setSetting(ORDER_KEY, value);
}

/** 侧栏里移动订阅源、排列顶层、建文件夹、改名和删除。 */
export function useFolderActions() {
  const qc = useQueryClient();
  const folders = useFolders();

  const feedsIn = useCallback(
    (folder: string) => (qc.getQueryData<Nav>(keys.nav)?.feeds ?? []).filter((f) => f.folder === folder).map((f) => f.id),
    [qc],
  );

  const refresh = useCallback(() => {
    void qc.invalidateQueries({ queryKey: keys.nav });
    void qc.invalidateQueries({ queryKey: ["items"] });
  }, [qc]);

  /** 把一组订阅源按这个顺序放进文件夹（空字符串是文件夹外），只动源本身。 */
  const moveFeeds = useCallback(
    (folder: string, ids: string[]) => {
      placeLocally(qc, folder, ids);
      api.reorderFeeds(folder, ids).then(
        // 文件夹变了，按文件夹筛选的列表要重新查
        () => void qc.invalidateQueries({ queryKey: ["items"] }),
        refresh,
      );
    },
    [qc, refresh],
  );

  /** 把一组订阅源按这个顺序放进文件夹，并记下顶层顺序：源被拖空的文件夹留着，等用户自己删。 */
  const place = useCallback(
    (folder: string, ids: string[]) => {
      moveFeeds(folder, ids);
      const top = currentTop(qc);
      saveTop(qc, folder ? insertFolder(top, folder) : top);
    },
    [moveFeeds, qc],
  );

  /** 按新的顶层顺序排好。文件夹外的源变了（拖出文件夹、换了位置）时一并告诉核心。 */
  const arrangeTop = useCallback(
    (entries: TopEntry[]) => {
      const loose = entries.flatMap((e) => (e.kind === "feed" ? [e.id] : []));
      if (JSON.stringify(feedsIn("")) !== JSON.stringify(loose)) moveFeeds("", loose);
      saveTop(qc, entries);
    },
    [feedsIn, moveFeeds, qc],
  );

  const create = useCallback(
    (name: string) => {
      const clean = name.trim();
      if (clean) saveTop(qc, insertFolder(currentTop(qc), clean));
    },
    [qc],
  );

  /** 改名。新名字已经有了就合并进去。 */
  const rename = useCallback(
    (from: string, to: string) => {
      const clean = to.trim();
      if (!clean || clean === from) return;
      const ids = [...feedsIn(clean), ...feedsIn(from)];
      saveTop(qc, renameFolder(currentTop(qc), from, clean));
      if (ids.length > 0) moveFeeds(clean, ids);
      const ui = useUi.getState();
      if (ui.scope.kind === "folder" && ui.scope.folder === from) ui.setScope({ kind: "folder", folder: clean });
    },
    [feedsIn, moveFeeds, qc],
  );

  /** 删掉文件夹，里面的订阅源留在文件夹原来的位置，不会退订。 */
  const remove = useCallback(
    (name: string) => {
      arrangeTop(removeFolder(currentTop(qc), name, feedsIn(name)));
      const ui = useUi.getState();
      if (ui.scope.kind === "folder" && ui.scope.folder === name) ui.setScope({ kind: "view", view: "inbox" });
    },
    [arrangeTop, feedsIn, qc],
  );

  return {
    folders,
    feedsIn,
    place,
    arrangeTop,
    currentTop: () => currentTop(qc),
    create,
    rename,
    remove,
  };
}
