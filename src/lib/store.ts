import { useMemo } from "react";
import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { ItemFilter, ItemKind, View } from "./api";
import type { ComposeMode } from "./compose";
import type { GroupMode } from "./grouping";
import { LIST_STYLE_DEFAULT, type ListStyle, normalizeListStyle } from "./listStyle";
import { normalizeReaderStyle, READER_DEFAULT, type ReaderStyle } from "./readerStyle";
import { normalizeLayout, type ToolbarId, type ToolbarLayout } from "./toolbars";
import { startOfToday } from "./time";
import { devicePreset, type WebDevice } from "./webDevice";

/** 列表当前显示的范围：一个内置视图、一个订阅源，或一个文件夹。 */
export type ListScope =
  | { kind: "view"; view: View }
  | { kind: "feed"; feedId: string }
  | { kind: "account"; accountId: string }
  /** 邮箱里的一个普通文件夹。`label` 是侧栏里显示的名字。 */
  | { kind: "mailbox"; accountId: string; mailbox: string; label: string }
  | { kind: "folder"; folder: string }
  | { kind: "label"; label: string };

export type ThemePref = "system" | "dark" | "light";
/** 列表的显示方式：一行一条、带摘要和缩略图、卡片。 */
export type ListView = "list" | "summary" | "cards";

/** 侧栏布局：邮箱和订阅上下排列（可折叠），或者分成选项卡。 */
export type SidebarSection = "mail" | "feeds" | "labels";

export const CARD_MIN = 160;
export const CARD_MAX = 480;
export type Density = "compact" | "comfortable";
/** 界面语言：跟随系统、内置的中英文，或者导入的语言（`pack:代码`，见 langPacks.ts）。 */
export type LangPref = "system" | "zh" | "en" | `pack:${string}`;

/** 设置面板的各页。 */
export type SettingsPage =
  | "appearance"
  | "feeds"
  | "translation"
  | "ai"
  | "categories"
  | "rules"
  | "plugins"
  | "data"
  | "storage"
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
  /** 自定义工具栏，打开时停在 `toolbar` 那一套。 */
  | { type: "toolbars"; toolbar?: ToolbarId }
  /** `scope` 省略时是当前打开的列表；侧栏右键菜单会指定别的列表。 */
  | { type: "mark-all-read"; scope?: ListScope }
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
  /** 侧栏里展开了文件夹的邮箱。 */
  expandedAccounts: string[];
  /** 写信默认带格式。 */
  composeRich: boolean;
  setComposeRich: (on: boolean) => void;
  /** 翻译成哪种语言，模型目录里的语言代码。为空时跟界面语言一致。 */
  translateTo: string | null;
  /** 每个列表各自的显示方式，键见 `listKey`。没选过的是列表。改一个列表不影响别的。 */
  listViews: Record<string, ListView>;
  /** 每个列表各自的折叠方式，键同上。没选过的不折叠。 */
  groupModes: Record<string, GroupMode>;
  /** 卡片的最小宽度。按住 Ctrl 滚动滚轮调整。 */
  cardSize: number;
  collapsedSections: SidebarSection[];
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
  /** 发件人头像用发件人所在网站的图标，没有时显示首字母。 */
  senderLogos: boolean;
  /** 列表和阅读区的工具栏布局（按钮放顶部、状态栏还是隐藏，以及顺序）。读的时候用 `useToolbarLayout`。 */
  toolbars: Partial<Record<ToolbarId, ToolbarLayout>>;
  /** 中间列表是滚动还是一页一页翻。 */
  listPaging: "scroll" | "pages";
  /** 阅读区最大化：藏起侧栏和列表，阅读区占满整个窗口（看视频、长文章）。不保存。 */
  readerMaximized: boolean;
  /** 在这台设备上启用的插件 ID。插件只有手动启用才会加载。 */
  enabledPlugins: string[];
  /** 新手引导开着时停在第几步，关着是 null。别的对话框打开时引导先藏起来，关掉后接着显示。不保存。 */
  welcomeStep: number | null;
  /** 这台设备上看完或跳过了新手引导，以后不再自动弹出（关于页里可以再打开）。 */
  welcomeDone: boolean;
  /** 变一下就重新扫描插件目录、重新加载插件。不保存。 */
  pluginReload: number;
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
  toggleAccountFolders: (accountId: string) => void;
  setTranslateTo: (code: string | null) => void;
  setListView: (scope: ListScope, view: ListView) => void;
  setCardSize: (size: number) => void;
  toggleSection: (section: SidebarSection) => void;
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
  setSenderLogos: (on: boolean) => void;
  setToolbar: (id: ToolbarId, layout: ToolbarLayout | null) => void;
  setListPaging: (mode: "scroll" | "pages") => void;
  setReaderMaximized: (on: boolean) => void;
  setPluginEnabled: (id: string, on: boolean) => void;
  /** 打开新手引导，或者翻到第 `step` 步。 */
  setWelcomeStep: (step: number) => void;
  /** 看完或跳过新手引导。 */
  finishWelcome: () => void;
  reloadPlugins: () => void;
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
      expandedAccounts: [],
      composeRich: true,
      setComposeRich: (composeRich) => set({ composeRich }),
      translateTo: null,
      listViews: {},
      groupModes: {},
      cardSize: 260,
      collapsedSections: [],
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
      senderLogos: true,
      toolbars: {},
      listPaging: "scroll",
      readerMaximized: false,
      enabledPlugins: [],
      pluginReload: 0,
      welcomeStep: null,
      welcomeDone: false,
      setWelcomeStep: (welcomeStep) => set({ welcomeStep }),
      finishWelcome: () => set({ welcomeStep: null, welcomeDone: true }),

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
      toggleAccountFolders: (accountId) =>
        set((s) => ({
          expandedAccounts: s.expandedAccounts.includes(accountId)
            ? s.expandedAccounts.filter((a) => a !== accountId)
            : [...s.expandedAccounts, accountId],
        })),
      toggleFolder: (folder) =>
        set((s) => ({
          collapsedFolders: s.collapsedFolders.includes(folder)
            ? s.collapsedFolders.filter((f) => f !== folder)
            : [...s.collapsedFolders, folder],
        })),
      setTranslateTo: (translateTo) => set({ translateTo }),
      setListView: (scope, view) =>
        set((s) => ({ listViews: { ...s.listViews, [listKey(scope, s.kindFilter)]: view } })),
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
      setSenderLogos: (senderLogos) => set({ senderLogos }),
      setToolbar: (id, layout) =>
        set((s) => {
          const toolbars = { ...s.toolbars };
          if (layout) toolbars[id] = layout;
          else delete toolbars[id];
          return { toolbars };
        }),
      setListPaging: (listPaging) => set({ listPaging }),
      setReaderMaximized: (readerMaximized) => set({ readerMaximized }),
      setPluginEnabled: (id, on) =>
        set((s) => ({
          enabledPlugins: on ? [...new Set([...s.enabledPlugins, id])] : s.enabledPlugins.filter((p) => p !== id),
        })),
      reloadPlugins: () => set((s) => ({ pluginReload: s.pluginReload + 1 })),
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
      setCardSize: (size) => set({ cardSize: clamp(size, CARD_MIN, CARD_MAX) }),
      requestTranslate: () => set((s) => ({ translateRequest: s.translateRequest + 1 })),
      openSearch: () => set((s) => ({ searchOpen: true, searchFocus: s.searchFocus + 1, mainView: "list" })),
      closeSearch: () => set({ searchOpen: false, searchQuery: "", selection: [] }),
      setSearchQuery: (searchQuery) => set({ searchQuery, selection: [], expandedGroups: [] }),
      setSearchAll: (searchAll) => set({ searchAll, selection: [] }),
    }),
    {
      name: "postflow.ui",
      // 以前的「选项卡」侧栏模式已经去掉。用过它的人，选项卡留下的「只看邮件 / 只看订阅」筛选也一起清掉，
      // 不然列表会莫名只显示一类
      merge: (persisted, current) => {
        const saved = (persisted ?? {}) as Partial<UiState> & { sidebarLayout?: string; tab?: string; tabScopes?: unknown };
        const { sidebarLayout, tab: _tab, tabScopes: _tabScopes, ...rest } = saved;
        return { ...current, ...rest, ...(sidebarLayout === "tabs" ? { kindFilter: null } : {}) };
      },
      partialize: (s) => ({
        theme: s.theme,
        density: s.density,
        lang: s.lang,
        sidebarWidth: s.sidebarWidth,
        listWidth: s.listWidth,
        collapsedFolders: s.collapsedFolders,
        expandedAccounts: s.expandedAccounts,
        composeRich: s.composeRich,
        kindFilter: s.kindFilter,
        translateTo: s.translateTo,
        listViews: s.listViews,
        groupModes: s.groupModes,
        cardSize: s.cardSize,
        collapsedSections: s.collapsedSections,
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
        senderLogos: s.senderLogos,
        toolbars: s.toolbars,
        listPaging: s.listPaging,
        enabledPlugins: s.enabledPlugins,
        welcomeDone: s.welcomeDone,
      }),
    },
  ),
);

/** 列表范围的标识，用来分别记住每个列表的显示方式。 */
/** 这些视图里的条目都不在收件箱，取消归档（移回收件箱）会让它们离开列表。 */
export function isArchiveLike(view: View | "all" | undefined): boolean {
  return view === "archived" || view === "junk" || view === "trash" || view === "folder";
}

/** 只有邮件才有的视图：草稿、已发送、垃圾邮件和已删除。 */
export function isMailOnlyView(view: View): boolean {
  return view === "junk" || view === "trash" || view === "drafts" || view === "sent";
}

export function scopeKey(scope: ListScope): string {
  switch (scope.kind) {
    case "view":
      return `view:${scope.view}`;
    case "feed":
      return `feed:${scope.feedId}`;
    case "account":
      return `account:${scope.accountId}`;
    case "mailbox":
      return `mailbox:${scope.accountId}:${scope.mailbox}`;
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
    case "mailbox":
      return { view: "folder", source_id: scope.accountId, mailbox: scope.mailbox };
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
  if (a.kind === "mailbox" && b.kind === "mailbox") return a.accountId === b.accountId && a.mailbox === b.mailbox;
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

/** 一套工具栏的布局，补齐旧版本缺的按钮。`extra` 是插件加的按钮，没排过的接在最后。 */
export function useToolbarLayout(id: ToolbarId, extra: readonly string[] = NO_EXTRA): ToolbarLayout {
  const raw = useUi((s) => s.toolbars[id]);
  const key = extra.join("|");
  // biome-ignore lint/correctness/useExhaustiveDependencies: key 已经代表 extra
  return useMemo(() => normalizeLayout(id, raw, extra), [id, raw, key]);
}
const NO_EXTRA: readonly string[] = [];
