// 订阅源的文件夹和顺序。
//
// 文件夹只是订阅源上的一个名字。文件夹之间的顺序、以及还没放进订阅源的空文件夹，
// 记在同步的设置 `folder_order` 里。订阅源在文件夹里的顺序由核心记在每个源上。

import { type QueryClient, useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { api, type Feed, type Nav, type Settings } from "./api";
import { keys, useNav, useSettings } from "./queries";
import { useUi } from "./store";

const ORDER_KEY = "folder_order";

/** 文件夹的显示顺序：设置里记着的在前（包括空文件夹），其余按名称排在后面。 */
export function orderFolders(feeds: Feed[], saved: unknown): string[] {
  const listed = Array.isArray(saved)
    ? saved.filter((x): x is string => typeof x === "string" && x.trim() !== "")
    : [];
  const used = [...new Set(feeds.map((f) => f.folder).filter(Boolean))].sort((a, b) => a.localeCompare(b));
  return [...new Set([...listed, ...used])];
}

export function useFolders(): string[] {
  const nav = useNav();
  const settings = useSettings();
  return orderFolders(nav.data?.feeds ?? [], settings.data?.[ORDER_KEY]);
}

/** 先在本地缓存里把订阅源排好，界面不等核心。 */
function placeLocally(qc: QueryClient, folder: string, ids: string[]) {
  qc.setQueryData<Nav>(keys.nav, (old) => {
    if (!old) return old;
    const byId = new Map(old.feeds.map((f) => [f.id, f]));
    const moved = ids.map((id) => byId.get(id)).filter((f): f is Feed => f !== undefined);
    const rest = old.feeds.filter((f) => !ids.includes(f.id));
    return { ...old, feeds: [...rest, ...moved.map((f) => ({ ...f, folder }))] };
  });
}

function saveOrderLocally(qc: QueryClient, names: string[]) {
  qc.setQueryData<Settings>(keys.settings, (old) => ({ ...old, [ORDER_KEY]: names }));
  void api.setSetting(ORDER_KEY, names);
}

/** 侧栏里移动订阅源、排列文件夹、建文件夹、改名和删除。 */
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

  /** 把一组订阅源按这个顺序放进文件夹（空字符串是文件夹外）。 */
  const place = useCallback(
    (folder: string, ids: string[]) => {
      // 把现有的文件夹都记下来：源被拖空的文件夹留着，等用户自己删
      const names = folder && !folders.includes(folder) ? [...folders, folder] : folders;
      const saved = qc.getQueryData<Settings>(keys.settings)?.[ORDER_KEY];
      if (JSON.stringify(saved) !== JSON.stringify(names)) saveOrderLocally(qc, names);
      placeLocally(qc, folder, ids);
      api.reorderFeeds(folder, ids).then(
        // 文件夹变了，按文件夹筛选的列表要重新查
        () => void qc.invalidateQueries({ queryKey: ["items"] }),
        refresh,
      );
    },
    [folders, qc, refresh],
  );

  const orderFolders = useCallback((names: string[]) => saveOrderLocally(qc, names), [qc]);

  const create = useCallback(
    (name: string) => {
      const clean = name.trim();
      if (clean && !folders.includes(clean)) orderFolders([...folders, clean]);
    },
    [folders, orderFolders],
  );

  /** 改名。新名字已经有了就合并进去。 */
  const rename = useCallback(
    (from: string, to: string) => {
      const clean = to.trim();
      if (!clean || clean === from) return;
      const merged = folders.includes(clean);
      place(clean, [...(merged ? feedsIn(clean) : []), ...feedsIn(from)]);
      orderFolders(merged ? folders.filter((f) => f !== from) : folders.map((f) => (f === from ? clean : f)));
      const ui = useUi.getState();
      if (ui.scope.kind === "folder" && ui.scope.folder === from) ui.setScope({ kind: "folder", folder: clean });
    },
    [feedsIn, folders, orderFolders, place],
  );

  /** 删掉文件夹，里面的订阅源移到文件夹外，不会退订。 */
  const remove = useCallback(
    (name: string) => {
      const inside = feedsIn(name);
      if (inside.length > 0) place("", [...feedsIn(""), ...inside]);
      orderFolders(folders.filter((f) => f !== name));
      const ui = useUi.getState();
      if (ui.scope.kind === "folder" && ui.scope.folder === name) ui.setScope({ kind: "view", view: "inbox" });
    },
    [feedsIn, folders, orderFolders, place],
  );

  return { folders, feedsIn, place, orderFolders, create, rename, remove };
}
