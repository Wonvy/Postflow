import { useMemo } from "react";
import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { ItemFilter, ItemKind, View } from "./api";
import type { ComposeMode } from "./compose";
import type { GroupMode } from "./grouping";
import { LIST_STYLE_DEFAULT, type ListStyle, normalizeListStyle } from "./listStyle";
import { normalizeReaderStyle, READER_DEFAULT, type ReaderStyle } from "./readerStyle";
import { startOfToday } from "./time";
import { devicePreset, type WebDevice } from "./webDevice";

/** 列表当前显示的范围：一个内置视图、一个订阅源，或一个文件夹。 */
export type ListScope =
  | { kind: "view"; view: View }
  | { kind: "feed"; feedId: string }
  | { kind: "account"; accountId: string }
  | { kind: "folder"; folder: string }
  | { kind: "label"; label: string };

export type ThemePref = "system" | "dark" | "light";
/** 列表的显示方式：一行一条、带摘要和缩略图、卡片。 */
export type ListView = "list" | "summary" | "cards";

/** 侧栏布局：邮箱和订阅上下排列（可折叠），或者分成选项卡。 */
export type SidebarLayout = "groups" | "tabs";
export type SidebarTab = "all" | "mail" | "feeds";
export type SidebarSection = "mail" | "feeds" | "labels";
/** 每个选项卡只看哪一类条目。 */
export const TAB_KIND: Record<SidebarTab, ItemKind | null> = { all: null, mail: "mail", feeds: "article" };

export const CARD_MIN = 160;
export const CARD_MAX = 480;
export type Density = "compact" | "comfortable";
export type LangPref = "system" | "zh" | "en";

/** 设置面板的各页。 */
export type SettingsPage =
  | "appearance"
  | "feeds"
  | "translation"
  | "ai"
  | "categories"
  | "data"
  | "diagnostics"
  | "about";

export type Dialog =
  /** 添加订阅。从文件夹的菜单打开时带上 `folder`，默认放进那个文件夹。 */
  | { type: "add-feed"; folder?: string }
  /** 设置。给了 `page` 就直接打开那一页，否则停在上次看的那一页。 */
  | { type: "settings"; page?: SettingsPage }
  | { type: "edit-feed"; feedId: string }
  | { type: "add-account" }
  | { type: "edit-account"; accountId: string }
  | { type: "remove-account"; accountId: string }
  /** 新建文件夹，或者给 `folder` 改名。 */
  | { type: "folder"; folder?: string }
  /** 确认把当前列表全部标为已读。 */
  | { type: "mark-all-read" }
  /** 写信。回复、转发时带上原信 `itemId`；写新邮件时可以指定从哪个邮箱发。 */
  | { type: "compose"; mode: ComposeMode; itemId?: string; accountId?: string }
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
  /** 当前列表里展开着的组。换列表时清空。 */
  expandedGroups: string[];
  /** 按 T 键时加一，阅读面板据此切换译文。 */
  translateRequest: number;
  /** 列表上方的搜索栏开着。 */
  searchOpen: boolean;
  /** 正在搜的词（输入停顿一下才更新）。为空时列表照常显示。 */
  searchQuery: string;
  /** 在全部内容里搜，而不是只在当前列表里。 */
  searchAll: boolean;
  /** 按 / 时加一，搜索框据此获得焦点。 */
  searchFocus: number;

  // 以下是这台设备上的界面偏好，存在浏览器本地
  theme: ThemePref;
  density: Density;
  lang: LangPref;
  sidebarWidth: number;
  listWidth: number;
  collapsedFolders: string[];
  /** 翻译成哪种语言，模型目录里的语言代码。为空时跟界面语言一致。 */
  translateTo: string | null;
  /** 每个列表各自的显示方式，键见 `listKey`。没选过的是列表。改一个列表不影响别的。 */
  listViews: Record<string, ListView>;
  /** 每个列表各自的折叠方式，键同上。没选过的不折叠。 */
  groupModes: Record<string, GroupMode>;
  /** 卡片的最小宽度。按住 Ctrl 滚动滚轮调整。 */
  cardSize: number;
  sidebarLayout: SidebarLayout;
  collapsedSections: SidebarSection[];
  /** 选项卡模式下当前的选项卡。 */
  tab: SidebarTab;
  /** 每个选项卡上次停在的列表，切回来时回到那里。 */
  tabScopes: Partial<Record<SidebarTab, ListScope>>;
  /** 列表里显示译文。 */
  listTranslate: boolean;
  /** 阅读面板的翻译开关：开着时打开的每一条都自动翻译，直到关掉。 */
  readerTranslate: boolean;
  /** 阅读面板的全文开关：开着时打开的每篇文章都自动抓取原网页全文，直到关掉。 */
  readerFullText: boolean;
  /** 阅读区的字号、行距、字体和宽度。读的时候用 `useReaderStyle`，它会补齐旧版本缺的字段。 */
  readerStyle: ReaderStyle;
  /** 列表的字号、间距和显示哪些部分。读的时候用 `useListStyle`。 */
  listStyle: ListStyle;
  /** 阅读面板的原网页开关：开着时有链接的条目直接显示原网页，直到关掉。 */
  readerWeb: boolean;
  /** 原网页模拟的设备，以及是否横过来。 */
  webDevice: WebDevice;
  webLandscape: boolean;
  /** 最近放进去过订阅的文件夹，新的在前。添加订阅时排在最前面。 */
  recentFolders: string[];
  /** 应用不在前台时，新邮件弹系统通知。 */
  notifyMail: boolean;
  /** 图标上显示收件箱的未读邮件数。 */
  badgeUnread: boolean;
  /** 右边两栏显示什么：列表和阅读区，还是订阅统计。不记住，重开应用回到列表。 */
  mainView: "list" | "stats";
  /** 阅读面板右侧的 AI 栏开着。 */
  aiPanel: boolean;
  /** 用户是在哪一条上打开 AI 栏的。只有这一条会自动开始总结。 */
  aiPanelFor: string | null;

  setScope: (scope: ListScope) => void;
  setMainView: (view: "list" | "stats") => void;
  open: (id: string | null) => void;
  setSelection: (ids: string[]) => void;
  setDialog: (dialog: Dialog) => void;
  setRefreshing: (n: number) => void;
  setKindFilter: (kind: ItemKind | null) => void;
  setGroupMode: (mode: GroupMode) => void;
  setGroupExpanded: (key: string, expanded: boolean) => void;
  setTheme: (theme: ThemePref) => void;
  setDensity: (density: Density) => void;
  setLang: (lang: LangPref) => void;
  setSidebarWidth: (w: number) => void;
  setListWidth: (w: number) => void;
  toggleFolder: (folder: string) => void;
  setTranslateTo: (code: string | null) => void;
  setListView: (scope: ListScope, view: ListView) => void;
  setCardSize: (size: number) => void;
  setSidebarLayout: (layout: SidebarLayout) => void;
  toggleSection: (section: SidebarSection) => void;
  setTab: (tab: SidebarTab) => void;
  toggleAiPanel: () => void;
  toggleListTranslate: () => void;
  setReaderTranslate: (on: boolean) => void;
  setReaderFullText: (on: boolean) => void;
  setReaderStyle: (patch: Partial<ReaderStyle>) => void;
  setListStyle: (patch: Partial<ListStyle>) => void;
  setReaderWeb: (on: boolean) => void;
  setWebDevice: (device: WebDevice) => void;
  setWebLandscape: (landscape: boolean) => void;
  rememberFolder: (folder: string) => void;
  setNotifyMail: (on: boolean) => void;
  setBadgeUnread: (on: boolean) => void;
  requestTranslate: () => void;
  openSearch: () => void;
  closeSearch: () => void;
  setSearchQuery: (query: string) => void;
  setSearchAll: (all: boolean) => void;
}

export const SIDEBAR_MIN = 200;
export const SIDEBAR_MAX = 360;
export const LIST_MIN = 280;
export const LIST_MAX = 1400;
/** 阅读面板至少留这么宽，列表再宽也不挤掉它。 */
export const READER_MIN = 380;

/** 记住几个最近用过的文件夹。 */
const RECENT_FOLDERS = 6;

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
      expandedGroups: [],
      translateRequest: 0,
      searchOpen: false,
      searchQuery: "",
      searchAll: false,
      searchFocus: 0,

      theme: "system",
      density: "compact",
      lang: "system",
      sidebarWidth: 244,
      listWidth: 380,
      collapsedFolders: [],
      translateTo: null,
      listViews: {},
      groupModes: {},
      cardSize: 260,
      sidebarLayout: "groups",
      collapsedSections: [],
      tab: "all",
      tabScopes: {},
      aiPanel: false,
      aiPanelFor: null,
      mainView: "list",
      listTranslate: false,
      readerTranslate: false,
      readerFullText: false,
      readerStyle: READER_DEFAULT,
      listStyle: LIST_STYLE_DEFAULT,
      readerWeb: false,
      webDevice: "desktop",
      webLandscape: false,
      recentFolders: [],
      notifyMail: true,
      badgeUnread: true,

      // 换列表时收起搜索
      setScope: (scope) =>
        set({
          scope,
          mainView: "list",
          openId: null,
          selection: [],
          expandedGroups: [],
          searchOpen: false,
          searchQuery: "",
        }),
      setMainView: (mainView) => set({ mainView }),
      open: (id) => set({ openId: id }),
      setSelection: (ids) => set({ selection: ids }),
      setDialog: (dialog) => set({ dialog }),
      setRefreshing: (refreshing) => set({ refreshing }),
      setKindFilter: (kindFilter) => set({ kindFilter, openId: null, selection: [], expandedGroups: [] }),
      setGroupMode: (mode) =>
        set((s) => ({
          groupModes: { ...s.groupModes, [listKey(s.scope, s.kindFilter)]: mode },
          selection: [],
          expandedGroups: [],
        })),
      setGroupExpanded: (key, expanded) =>
        set((s) => ({
          expandedGroups: expanded
            ? [...new Set([...s.expandedGroups, key])]
            : s.expandedGroups.filter((k) => k !== key),
        })),
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
      setTranslateTo: (translateTo) => set({ translateTo }),
      setListView: (scope, view) =>
        set((s) => ({ listViews: { ...s.listViews, [listKey(scope, s.kindFilter)]: view } })),
      setSidebarLayout: (sidebarLayout) =>
        set((s) => {
          if (sidebarLayout === "groups") return { sidebarLayout };
          // 进入选项卡模式：按当前选项卡筛选，停在不属于它的列表上就回到收件箱
          const kind = TAB_KIND[s.tab];
          const fits =
            s.tab === "all" ||
            (s.scope.kind === "view" && !(s.tab === "feeds" && isMailOnlyView(s.scope.view))) ||
            (s.tab === "mail" ? s.scope.kind === "account" : s.scope.kind !== "account");
          return {
            sidebarLayout,
            kindFilter: kind,
            ...(fits ? {} : { scope: { kind: "view", view: "inbox" } as ListScope, openId: null, selection: [] }),
          };
        }),
      toggleListTranslate: () => set((s) => ({ listTranslate: !s.listTranslate })),
      setReaderTranslate: (readerTranslate) => set({ readerTranslate }),
      setReaderFullText: (readerFullText) => set({ readerFullText }),
      setReaderStyle: (patch) =>
        set((s) => ({ readerStyle: normalizeReaderStyle({ ...normalizeReaderStyle(s.readerStyle), ...patch }) })),
      setListStyle: (patch) =>
        set((s) => ({ listStyle: normalizeListStyle({ ...normalizeListStyle(s.listStyle), ...patch }) })),
      setReaderWeb: (readerWeb) => set({ readerWeb }),
      setWebDevice: (webDevice) => set({ webDevice: devicePreset(webDevice).id }),
      setWebLandscape: (webLandscape) => set({ webLandscape }),
      setNotifyMail: (notifyMail) => set({ notifyMail }),
      setBadgeUnread: (badgeUnread) => set({ badgeUnread }),
      rememberFolder: (folder) =>
        set((s) => {
          const name = folder.trim();
          if (!name) return {};
          return { recentFolders: [name, ...s.recentFolders.filter((f) => f !== name)].slice(0, RECENT_FOLDERS) };
        }),
      toggleAiPanel: () => set((s) => ({ aiPanel: !s.aiPanel, aiPanelFor: s.aiPanel ? null : s.openId })),
      toggleSection: (section) =>
        set((s) => ({
          collapsedSections: s.collapsedSections.includes(section)
            ? s.collapsedSections.filter((x) => x !== section)
            : [...s.collapsedSections, section],
        })),
      setTab: (tab) =>
        set((s) => {
          if (tab === s.tab) return {};
          const tabScopes = { ...s.tabScopes, [s.tab]: s.scope };
          return {
            tab,
            tabScopes,
            scope: tabScopes[tab] ?? { kind: "view", view: "inbox" },
            kindFilter: TAB_KIND[tab],
            openId: null,
            selection: [],
            expandedGroups: [],
          };
        }),
      setCardSize: (size) => set({ cardSize: clamp(size, CARD_MIN, CARD_MAX) }),
      requestTranslate: () => set((s) => ({ translateRequest: s.translateRequest + 1 })),
      openSearch: () => set((s) => ({ searchOpen: true, searchFocus: s.searchFocus + 1, mainView: "list" })),
      closeSearch: () => set({ searchOpen: false, searchQuery: "", selection: [] }),
      setSearchQuery: (searchQuery) => set({ searchQuery, selection: [], expandedGroups: [] }),
      setSearchAll: (searchAll) => set({ searchAll, selection: [] }),
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
        translateTo: s.translateTo,
        listViews: s.listViews,
        groupModes: s.groupModes,
        cardSize: s.cardSize,
        sidebarLayout: s.sidebarLayout,
        collapsedSections: s.collapsedSections,
        tab: s.tab,
        aiPanel: s.aiPanel,
        listTranslate: s.listTranslate,
        readerTranslate: s.readerTranslate,
        readerFullText: s.readerFullText,
        readerStyle: s.readerStyle,
        listStyle: s.listStyle,
        readerWeb: s.readerWeb,
        webDevice: s.webDevice,
        webLandscape: s.webLandscape,
        recentFolders: s.recentFolders,
        searchAll: s.searchAll,
        notifyMail: s.notifyMail,
        badgeUnread: s.badgeUnread,
      }),
    },
  ),
);

/** 列表范围的标识，用来分别记住每个列表的显示方式。 */
/** 这些视图里的条目都不在收件箱，取消归档（移回收件箱）会让它们离开列表。 */
export function isArchiveLike(view: View | "all" | undefined): boolean {
  return view === "archived" || view === "junk" || view === "trash";
}

/** 只有邮件才有的视图：垃圾邮件和已删除。 */
export function isMailOnlyView(view: View): boolean {
  return view === "junk" || view === "trash";
}

export function scopeKey(scope: ListScope): string {
  switch (scope.kind) {
    case "view":
      return `view:${scope.view}`;
    case "feed":
      return `feed:${scope.feedId}`;
    case "account":
      return `account:${scope.accountId}`;
    case "folder":
      return `folder:${scope.folder}`;
    case "label":
      return `label:${scope.label}`;
  }
}

/** 记住显示方式用的键。内置视图在只看邮件或只看订阅时各记各的，选项卡因此也各记各的。 */
export function listKey(scope: ListScope, kind: ItemKind | null): string {
  const base = scopeKey(scope);
  return scope.kind === "view" && kind ? `${base}:${kind}` : base;
}

/** 当前列表的显示方式。 */
export function useListView(): ListView {
  return useUi((s) => s.listViews[listKey(s.scope, s.kindFilter)] ?? "list");
}

/** 当前列表的折叠方式。 */
export function useGroupMode(): GroupMode {
  return useUi((s) => s.groupModes[listKey(s.scope, s.kindFilter)] ?? "none");
}

/**
 * 搜索时的查询条件：在当前列表里搜，或者在全部内容（收件箱和归档）里搜。
 * 只看邮件、只看订阅的切换仍然有效。
 */
export function searchFilterOf(scope: ListScope, kind: ItemKind | null, query: string, all: boolean): ItemFilter {
  const base: ItemFilter = all ? { view: "all", ...(kind ? { kind } : {}) } : filterOf(scope, kind);
  return { ...base, query };
}

/** 把列表范围换算成查询条件。订阅源、文件夹和邮箱里显示的是未归档的条目。 */
export function filterOf(scope: ListScope, kind: ItemKind | null = null): ItemFilter {
  switch (scope.kind) {
    case "feed":
      return { view: "inbox", source_id: scope.feedId };
    case "account":
      return { view: "inbox", source_id: scope.accountId };
    case "folder":
      return { view: "inbox", folder: scope.folder };
    case "label":
      return kind ? { view: "inbox", label: scope.label, kind } : { view: "inbox", label: scope.label };
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
  if (a.kind === "label" && b.kind === "label") return a.label === b.label;
  return false;
}

/** 列表的样式设置，缺的字段补成默认值。 */
export function useListStyle(): ListStyle {
  const raw = useUi((s) => s.listStyle);
  return useMemo(() => normalizeListStyle(raw), [raw]);
}

/** 阅读区的排版设置，缺的字段补成默认值。 */
export function useReaderStyle(): ReaderStyle {
  const raw = useUi((s) => s.readerStyle);
  return useMemo(() => normalizeReaderStyle(raw), [raw]);
}
