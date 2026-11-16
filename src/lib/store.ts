import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { ItemFilter, ItemKind, View } from "./api";
import { startOfToday } from "./time";

/** 列表当前显示的范围：一个内置视图、一个订阅源，或一个文件夹。 */
export type ListScope =
  | { kind: "view"; view: View }
  | { kind: "feed"; feedId: string }
  | { kind: "account"; accountId: string }
  | { kind: "folder"; folder: string };

export type ThemePref = "system" | "dark" | "light";
export type Density = "compact" | "comfortable";
export type LangPref = "system" | "zh" | "en";

export type Dialog =
  | { type: "add-feed" }
  | { type: "settings" }
  | { type: "edit-feed"; feedId: string }
  | { type: "add-account" }
  | { type: "edit-account"; accountId: string }
  | { type: "remove-account"; accountId: string }
  | null;

interface UiState {
  scope: ListScope;
  /** 阅读面板里打开的条目。 */
  openId: string | null;
  /** 明确勾选的条目。非空时操作作用于它们，否则作用于打开的那一条。 */
  selection: string[];
  dialog: Dialog;
  /** 正在刷新的订阅源和邮箱数量。 */
  refreshing: number;
  /** 内置视图里只看邮件或只看文章。 */
  kindFilter: ItemKind | null;

  // 以下是这台设备上的界面偏好，存在浏览器本地
  theme: ThemePref;
  density: Density;
  lang: LangPref;
  sidebarWidth: number;
  listWidth: number;
  collapsedFolders: string[];

  setScope: (scope: ListScope) => void;
  open: (id: string | null) => void;
  setSelection: (ids: string[]) => void;
  setDialog: (dialog: Dialog) => void;
  setRefreshing: (n: number) => void;
  setKindFilter: (kind: ItemKind | null) => void;
  setTheme: (theme: ThemePref) => void;
  setDensity: (density: Density) => void;
  setLang: (lang: LangPref) => void;
  setSidebarWidth: (w: number) => void;
  setListWidth: (w: number) => void;
  toggleFolder: (folder: string) => void;
}

export const SIDEBAR_MIN = 200;
export const SIDEBAR_MAX = 360;
export const LIST_MIN = 280;
export const LIST_MAX = 640;

const clamp = (n: number, min: number, max: number) => Math.round(Math.min(max, Math.max(min, n)));

export const useUi = create<UiState>()(
  persist(
    (set) => ({
      scope: { kind: "view", view: "inbox" },
      openId: null,
      selection: [],
      dialog: null,
      refreshing: 0,
      kindFilter: null,

      theme: "system",
      density: "compact",
      lang: "system",
      sidebarWidth: 244,
      listWidth: 380,
      collapsedFolders: [],

      setScope: (scope) => set({ scope, openId: null, selection: [] }),
      open: (id) => set({ openId: id }),
      setSelection: (ids) => set({ selection: ids }),
      setDialog: (dialog) => set({ dialog }),
      setRefreshing: (refreshing) => set({ refreshing }),
      setKindFilter: (kindFilter) => set({ kindFilter, openId: null, selection: [] }),
      setTheme: (theme) => set({ theme }),
      setDensity: (density) => set({ density }),
      setLang: (lang) => set({ lang }),
      setSidebarWidth: (w) => set({ sidebarWidth: clamp(w, SIDEBAR_MIN, SIDEBAR_MAX) }),
      setListWidth: (w) => set({ listWidth: clamp(w, LIST_MIN, LIST_MAX) }),
      toggleFolder: (folder) =>
        set((s) => ({
          collapsedFolders: s.collapsedFolders.includes(folder)
            ? s.collapsedFolders.filter((f) => f !== folder)
            : [...s.collapsedFolders, folder],
        })),
    }),
    {
      name: "postflow.ui",
      partialize: (s) => ({
        theme: s.theme,
        density: s.density,
        lang: s.lang,
        sidebarWidth: s.sidebarWidth,
        listWidth: s.listWidth,
        collapsedFolders: s.collapsedFolders,
        kindFilter: s.kindFilter,
      }),
    },
  ),
);

/** 把列表范围换算成查询条件。订阅源、文件夹和邮箱里显示的是未归档的条目。 */
export function filterOf(scope: ListScope, kind: ItemKind | null = null): ItemFilter {
  switch (scope.kind) {
    case "feed":
      return { view: "inbox", source_id: scope.feedId };
    case "account":
      return { view: "inbox", source_id: scope.accountId };
    case "folder":
      return { view: "inbox", folder: scope.folder };
    case "view": {
      const filter: ItemFilter =
        scope.view === "today" ? { view: "today", since: startOfToday() } : { view: scope.view };
      if (kind) filter.kind = kind;
      return filter;
    }
  }
}

export function sameScope(a: ListScope, b: ListScope): boolean {
  if (a.kind === "view" && b.kind === "view") return a.view === b.view;
  if (a.kind === "feed" && b.kind === "feed") return a.feedId === b.feedId;
  if (a.kind === "account" && b.kind === "account") return a.accountId === b.accountId;
  if (a.kind === "folder" && b.kind === "folder") return a.folder === b.folder;
  return false;
}
