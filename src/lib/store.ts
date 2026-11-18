import { useMemo } from "react";
import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { ContentKind, ItemFilter, ItemKind, View } from "./api";
import type { ComposeMode } from "./compose";
import type { GroupMode } from "./grouping";
import { IS_TABLET } from "./device";
import { isDefaultListStyle, type ListStyle, listStyleDefault, normalizeListStyle } from "./listStyle";
import { isDefaultStyle, normalizeReaderStyle, type ReaderStyle, readerDefault } from "./readerStyle";
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
  | { kind: "label"; label: string }
  /** 订阅里的一个 Newsletter 发件人（放到订阅里看的订阅邮件）。 */
  | { kind: "newsletter"; address: string };

export type ThemePref = "system" | "dark" | "light";
/** 列表的显示方式：一行一条、带摘要和缩略图、卡片。 */
export type ListView = "list" | "summary" | "cards";

/** 侧栏里能折叠的几组：邮箱账户、订阅源、分类。 */
export type SidebarSection = "mail" | "feeds" | "labels" | "views-mail" | "views-feeds" | "newsletters";

/**
 * 邮件和订阅是两块独立的空间：侧栏顶部切换，各有自己的视图（收件箱、未读、星标……）、
 * 按钮、分类和右键菜单，列表只显示这一类条目。
 */
export type Space = "mail" | "feeds";

/** 空间里的条目是哪一类。 */
export function kindOfSpace(space: Space): ItemKind {
  return space === "mail" ? "mail" : "article";
}

/** 内置视图在侧栏和列表标题里的名字。订阅那边没有「收件箱」，叫「全部文章」。 */
export function viewLabelKey(view: View, space: Space): `nav.${View}` | "nav.allArticles" {
  return space === "feeds" && view === "inbox" ? "nav.allArticles" : `nav.${view}`;
}

/** 这个列表属于哪个空间；内置视图和分类两边都有，返回 null。 */
export function spaceOfScope(scope: ListScope): Space | null {
  switch (scope.kind) {
    case "account":
    case "mailbox":
      return "mail";
    case "feed":
    case "folder":
    case "newsletter":
      return "feeds";
    case "view":
      return isMailOnlyView(scope.view) ? "mail" : null;
    case "label":
      return null;
  }
}

export const CARD_MIN = 160;
export const CARD_MAX = 480;
export type Density = "compact" | "comfortable";
/** 界面语言：跟随系统、内置的中英文，或者导入的语言（`pack:代码`，见 langPacks.ts）。 */
export type LangPref = "system" | "zh" | "en" | `pack:${string}`;

/** 设置面板的各页。 */
export type SettingsPage =
  | "general"
  | "appearance"
  | "feeds"
  | "translation"
  | "ai"
  | "categories"
  | "rules"
  | "plugins"
  | "data"
  | "web"
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
  /** 全部快捷键一览（按 ?）。 */
  | { type: "shortcuts" }
  /** ⌘K 命令面板。 */
  | { type: "commands" }
  /** 定时发送里等着发的邮件。 */
  | { type: "outbox" }
  /** 自己挑稍后提醒的时间。 */
  | { type: "snooze"; ids: string[] }
  /** 查看一封邮件或一篇文章的源代码。 */
  | { type: "source"; itemId: string; title: string }
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
  /** 其中正在同步的邮箱数。订阅那边转圈看 `refreshing - refreshingMail`。 */
  refreshingMail: number;
  /** 当前在哪个空间：邮件还是订阅。 */
  space: Space;
  /** 手机布局里侧栏（从左边滑出）开着。不保存。 */
  drawerOpen: boolean;
  setDrawerOpen: (open: boolean) => void;
  /** 每个空间最后打开的列表，切回去时回到那里。不保存。 */
  spaceScopes: Partial<Record<Space, ListScope>>;
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
  /** 主题色：预设的 ID（见 accent.ts），或者自己挑的 #rrggbb。只存在这台设备上。 */
  accentColor: string;
  setAccentColor: (color: string) => void;
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
  /**
   * 列表的显示方式和折叠方式：默认所有列表共用一套（`sharedView`、`sharedGroup`），改哪个列表都一样。
   * 锁定的列表（`lockedLists`）或者打开了「每个列表记住自己的视图」（`rememberViews`）时，
   * 各列表各记各的（`listViews`、`groupModes`，键见 `listKey`），像文件管理器里的文件夹一样。
   */
  sharedView: ListView;
  sharedGroup: GroupMode;
  listViews: Record<string, ListView>;
  groupModes: Record<string, GroupMode>;
  lockedLists: string[];
  rememberViews: boolean;
  /** 下载离线听的播客单集最多留几期（早下的先删）。只在这台设备上。 */
  episodesKeep: number;
  setEpisodesKeep: (n: number) => void;
  /** 订阅文章在列表里滚过去（整行移出顶部）就标为已读。 */
  markReadOnScroll: boolean;
  setMarkReadOnScroll: (on: boolean) => void;
  /** 鼠标停在侧栏、列表、阅读区中的一栏上时，另外两栏变淡。 */
  focusPane: boolean;
  setFocusPane: (on: boolean) => void;
  /** 侧栏里每一边不显示的内置视图（收件箱、今天、未读、星标、稍后提醒、归档）。右键视图那一组可以改。 */
  hiddenViews: Record<Space, View[]>;
  toggleViewShown: (space: Space, view: View) => void;
  /** 侧栏里每一边的视图排成一列（带名字），还是一行图标（悬停看名字）。右键视图切换。 */
  viewsStyle: Record<Space, "rows" | "icons">;
  toggleViewsStyle: (space: Space) => void;
  /** 订阅里只看哪一类源：全部、文章、视频、播客。 */
  feedKind: ContentKind | "all";
  setFeedKind: (kind: ContentKind | "all") => void;
  /** 阅读区放在列表右边（三栏），还是打开条目时弹出来盖在上面（两栏）。手机上一直是一次一栏。 */
  readerLayout: ReaderLayout;
  setReaderLayout: (layout: ReaderLayout) => void;
  /** 锁定或解锁当前列表的视图。锁定时把现在的样子记下来。 */
  toggleViewLock: (media?: MediaList) => void;
  setRememberViews: (on: boolean) => void;
  /** 卡片的最小宽度。按住 Ctrl 滚动滚轮调整。 */
  cardSize: number;
  collapsedSections: SidebarSection[];
  /** 侧栏显示「订阅」空间。关掉后只有邮件，没有切换条。 */
  showFeeds: boolean;
  /** 「添加订阅 → 发现」里的目录显示成图标网格还是列表。 */
  catalogView: "grid" | "list";
  setCatalogView: (view: "grid" | "list") => void;
  /** 桌面应用：关掉窗口时留在后台继续收信，从托盘图标打开。 */
  keepRunning: boolean;
  setKeepRunning: (on: boolean) => void;
  /** 点「发送」后等几秒才真的发，这段时间里可以撤销。0 是马上发。 */
  undoSendSeconds: number;
  setUndoSendSeconds: (n: number) => void;
  /** 推荐订阅源的目录每天在线检查一次更新。 */
  catalogOnline: boolean;
  setCatalogOnline: (on: boolean) => void;
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
  /** 每种设备自己拖出来的大小（竖着拿时的宽高），没有的用预设。 */
  webSizes: Partial<Record<WebDevice, { width: number; height: number }>>;
  setWebSize: (device: WebDevice, size: { width: number; height: number } | null) => void;
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
  /** 列表行悬停时显示摘要；主要用于单行列表。 */
  listHoverSummary: boolean;
  /** 阅读区最大化：藏起侧栏和列表，阅读区占满整个窗口（看视频、长文章）。不保存。 */
  readerMaximized: boolean;
  /** 在这台设备上启用的插件 ID。插件只有手动启用才会加载。 */
  enabledPlugins: string[];
  /** 新手引导开着时停在第几步，关着是 null。别的对话框打开时引导先藏起来，关掉后接着显示。不保存。 */
  welcomeStep: number | null;
  /** 这台设备上看完或跳过了新手引导，以后不再自动弹出（关于页里可以再打开）。 */
  welcomeDone: boolean;
  /** 这台设备已经放过示例插件（第一次启动时放一次，不启用）。之后删掉的不会再放回来。 */
  examplePluginsSeeded: boolean;
  setExamplePluginsSeeded: () => void;
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
  setRefreshing: (total: number, mail: number) => void;
  /** 切换空间，回到那边最后打开的列表。 */
  setSpace: (space: Space) => void;
  setShowFeeds: (on: boolean) => void;
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
  /** `media`：这是视频或播客的列表（见 useListMedia），它们各有一套共用的视图。 */
  setListView: (scope: ListScope, view: ListView, media?: MediaList) => void;
  /** 视频、播客列表共用的视图，和文章分开：视频默认卡片，播客默认列表。 */
  mediaViews: Partial<Record<"video" | "podcast", ListView>>;
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
  setListHoverSummary: (on: boolean) => void;
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

/**
 * 默认不显示的视图。订阅那边：什么都没选就是「全部文章」，再点一下选中的那一行也回到它，所以不单列；
 * 未读、稍后提醒、归档对订阅用处不大，需要时右键视图那一组打开。邮件那边全部显示。
 */
export const DEFAULT_HIDDEN_VIEWS: Record<Space, View[]> = {
  mail: [],
  feeds: ["inbox", "unread", "snoozed", "archived"],
};

const NO_HIDDEN: View[] = [];

/**
 * 文章能不能归档、稍后提醒：订阅那边显示着「已归档」「稍后提醒」视图时才能，不然归档、提醒了的文章没处找。
 * 邮件总能。已经归档、提醒着的文章照样能撤销。
 */
export function articleActions(hidden: readonly View[]): { archive: boolean; snooze: boolean } {
  return { archive: !hidden.includes("archived"), snooze: !hidden.includes("snoozed") };
}

export function useArticleActions(): { archive: boolean; snooze: boolean } {
  const hidden = useUi((s) => s.hiddenViews.feeds ?? NO_HIDDEN);
  return useMemo(() => articleActions(hidden), [hidden]);
}

/** 这些条目能不能归档（有邮件、或者文章能归档、或者都已经归档了要撤销）。 */
export function canArchive(items: readonly { kind: string; archived: boolean }[], allowed: { archive: boolean }): boolean {
  return allowed.archive || items.some((i) => i.kind === "mail") || (items.length > 0 && items.every((i) => i.archived));
}

/** 这些条目能不能稍后提醒。 */
export function canSnooze(items: readonly { kind: string }[], allowed: { snooze: boolean }): boolean {
  return allowed.snooze || items.some((i) => i.kind === "mail");
}

/** 阅读区的位置：列表右边一栏，或者弹出来。 */
export type ReaderLayout = "pane" | "overlay";

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
      refreshingMail: 0,
      space: "mail",
      spaceScopes: {},
      drawerOpen: false,
      setDrawerOpen: (drawerOpen) => set({ drawerOpen }),
      expandedGroups: [],
      translateRequest: 0,
      searchOpen: false,
      searchQuery: "",
      searchAll: false,
      searchFocus: 0,

      theme: "system",
      accentColor: "postflow",
      density: "compact",
      lang: "system",
      sidebarWidth: 244,
      listWidth: 380,
      collapsedFolders: [],
      expandedAccounts: [],
      composeRich: true,
      setComposeRich: (composeRich) => set({ composeRich }),
      translateTo: null,
      sharedView: "list",
      mediaViews: {},
      sharedGroup: "none",
      listViews: {},
      groupModes: {},
      lockedLists: [],
      rememberViews: false,
      focusPane: true,
      markReadOnScroll: false,
      episodesKeep: 10,
      readerLayout: "pane",
      feedKind: "all",
      hiddenViews: DEFAULT_HIDDEN_VIEWS,
      viewsStyle: { mail: "rows", feeds: "icons" },
      toggleViewLock: (media) =>
        set((s) => {
          const key = listKey(s.scope, s.space);
          if (s.lockedLists.includes(key)) {
            // 解锁：回到共用的那一套，自己的记录不要了
            const { [key]: _v, ...listViews } = s.listViews;
            const { [key]: _g, ...groupModes } = s.groupModes;
            return { lockedLists: s.lockedLists.filter((k) => k !== key), listViews, groupModes };
          }
          return {
            lockedLists: [...s.lockedLists, key],
            listViews: { ...s.listViews, [key]: s.listViews[key] ?? sharedViewOf(s, media) },
            groupModes: { ...s.groupModes, [key]: s.groupModes[key] ?? s.sharedGroup },
          };
        }),
      setRememberViews: (rememberViews) => set({ rememberViews }),
      setFocusPane: (focusPane) => set({ focusPane }),
      setMarkReadOnScroll: (markReadOnScroll) => set({ markReadOnScroll }),
      setEpisodesKeep: (episodesKeep) => set({ episodesKeep }),
      setReaderLayout: (readerLayout) => set({ readerLayout }),
      toggleViewsStyle: (space) =>
        set((s) => ({
          viewsStyle: { ...s.viewsStyle, [space]: (s.viewsStyle?.[space] ?? (space === "mail" ? "rows" : "icons")) === "rows" ? "icons" : "rows" },
        })),
      toggleViewShown: (space, view) =>
        set((s) => {
          const hidden = s.hiddenViews[space] ?? [];
          return {
            hiddenViews: {
              ...s.hiddenViews,
              [space]: hidden.includes(view) ? hidden.filter((v) => v !== view) : [...hidden, view],
            },
          };
        }),
      // 换一类时，停在某个订阅源或文件夹上的回到「全部文章」，免得列表和侧栏对不上
      setFeedKind: (feedKind) =>
        set((s) => ({
          feedKind,
          selection: [],
          expandedGroups: [],
          ...(s.space === "feeds" && (s.scope.kind === "feed" || s.scope.kind === "folder" || s.scope.kind === "newsletter")
            ? { scope: { kind: "view", view: "inbox" } as ListScope, openId: null }
            : {}),
        })),
      cardSize: 260,
      collapsedSections: [],
      showFeeds: true,
      catalogView: "grid",
      setCatalogView: (catalogView) => set({ catalogView }),
      catalogOnline: true,
      undoSendSeconds: 5,
      keepRunning: true,
      setKeepRunning: (keepRunning) => set({ keepRunning }),
      setUndoSendSeconds: (undoSendSeconds) => set({ undoSendSeconds }),
      setCatalogOnline: (catalogOnline) => set({ catalogOnline }),
      aiPanel: false,
      aiPanelFor: null,
      mainView: "list",
      listTranslate: false,
      readerTranslate: false,
      readerFullText: false,
      readerStyle: readerDefault(IS_TABLET),
      listStyle: listStyleDefault(IS_TABLET),
      readerWeb: false,
      webDevice: "desktop",
      webLandscape: false,
      webSizes: {},
      setWebSize: (device, size) =>
        set((s) => {
          const webSizes = { ...s.webSizes };
          if (size) webSizes[device] = size;
          else delete webSizes[device];
          return { webSizes };
        }),
      recentFolders: [],
      notifyMail: true,
      badgeUnread: true,
      senderLogos: true,
      toolbars: {},
      listPaging: "scroll",
      listHoverSummary: true,
      readerMaximized: false,
      enabledPlugins: [],
      pluginReload: 0,
      welcomeStep: null,
      welcomeDone: false,
      examplePluginsSeeded: false,
      setExamplePluginsSeeded: () => set({ examplePluginsSeeded: true }),
      setWelcomeStep: (welcomeStep) => set({ welcomeStep }),
      finishWelcome: () => set({ welcomeStep: null, welcomeDone: true }),

      // 换列表时收起搜索。打开的是另一边的订阅源或邮箱时，空间跟着换过去
      setScope: (scope) =>
        set((s) => ({
          scope,
          space: spaceOfScope(scope) ?? s.space,
          ...(spaceOfScope(scope) === "feeds" ? { showFeeds: true } : {}),
          mainView: "list",
          // 手机上在侧栏里选了一个列表，侧栏就收起来
          drawerOpen: false,
          openId: null,
          selection: [],
          expandedGroups: [],
          searchOpen: false,
          searchQuery: "",
        })),
      setMainView: (mainView) => set({ mainView, drawerOpen: false }),
      open: (id) => set({ openId: id }),
      setSelection: (ids) => set({ selection: ids }),
      setDialog: (dialog) => set({ dialog }),
      setRefreshing: (refreshing, refreshingMail) => set({ refreshing, refreshingMail }),
      setSpace: (space) =>
        set((s) => {
          if (space === s.space) return {};
          return {
            space,
            spaceScopes: { ...s.spaceScopes, [s.space]: s.scope },
            scope: s.spaceScopes[space] ?? { kind: "view", view: "inbox" },
            openId: null,
            selection: [],
            expandedGroups: [],
            mainView: "list",
            searchOpen: false,
            searchQuery: "",
          };
        }),
      setShowFeeds: (showFeeds) =>
        set((s) =>
          !showFeeds && s.space === "feeds"
            ? {
                showFeeds,
                space: "mail",
                spaceScopes: { ...s.spaceScopes, feeds: s.scope },
                scope: s.spaceScopes.mail ?? { kind: "view", view: "inbox" },
                openId: null,
                selection: [],
                expandedGroups: [],
                mainView: "list",
                searchOpen: false,
                searchQuery: "",
              }
            : { showFeeds },
        ),
      setGroupMode: (mode) =>
        set((s) => {
          const key = listKey(s.scope, s.space);
          return {
            ...(ownsView(s, key) ? { groupModes: { ...s.groupModes, [key]: mode } } : { sharedGroup: mode }),
            selection: [],
            expandedGroups: [],
          };
        }),
      setGroupExpanded: (key, expanded) =>
        set((s) => ({
          expandedGroups: expanded
            ? [...new Set([...s.expandedGroups, key])]
            : s.expandedGroups.filter((k) => k !== key),
        })),
      setTheme: (theme) => set({ theme }),
      setAccentColor: (accentColor) => set({ accentColor }),
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
      setListView: (scope, view, media) =>
        set((s) => {
          const key = listKey(scope, s.space);
          if (ownsView(s, key)) return { listViews: { ...s.listViews, [key]: view } };
          return media ? { mediaViews: { ...s.mediaViews, [media]: view } } : { sharedView: view };
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
      setSenderLogos: (senderLogos) => set({ senderLogos }),
      setToolbar: (id, layout) =>
        set((s) => {
          const toolbars = { ...s.toolbars };
          if (layout) toolbars[id] = layout;
          else delete toolbars[id];
          return { toolbars };
        }),
      setListPaging: (listPaging) => set({ listPaging }),
      setListHoverSummary: (listHoverSummary) => set({ listHoverSummary }),
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
      // 旧版本的「只看邮件 / 只看订阅」筛选和「邮箱 / 订阅」选项卡换成了空间：选项卡停在订阅的，升级后停在订阅空间
      merge: (persisted, current) => {
        const saved = (persisted ?? {}) as Partial<UiState> & {
          sidebarLayout?: unknown;
          tab?: unknown;
          tabScopes?: unknown;
          kindFilter?: unknown;
          sidebarTabs?: unknown;
          sidebarTab?: unknown;
        };
        const {
          sidebarLayout: _layout,
          tab: _tab,
          tabScopes: _tabScopes,
          kindFilter: _kind,
          sidebarTabs,
          sidebarTab,
          ...rest
        } = saved;
        const merged: UiState = { ...current, ...rest };
        if (saved.space !== "mail" && saved.space !== "feeds") {
          merged.space = sidebarTabs === true && sidebarTab === "feeds" ? "feeds" : "mail";
        }
        if (!merged.showFeeds) merged.space = "mail";
        // 平板上存的还是旧的通用默认（从没调过）时，换成平板的默认字号
        if (IS_TABLET) {
          if (isDefaultStyle(normalizeReaderStyle(merged.readerStyle))) {
            merged.readerStyle = readerDefault(true);
          }
          if (isDefaultListStyle(normalizeListStyle(merged.listStyle))) merged.listStyle = listStyleDefault(true);
        }
        merged.listViews = splitViewKeys(merged.listViews);
        merged.groupModes = splitViewKeys(merged.groupModes);
        // 以前每个列表都各记各的。升级后改成默认共用；用户单独设过的列表锁住，看到的和以前一样
        if (!Array.isArray(saved.lockedLists)) {
          merged.lockedLists = [...new Set([...Object.keys(merged.listViews), ...Object.keys(merged.groupModes)])];
        }
        return merged;
      },
      partialize: (s) => ({
        theme: s.theme,
        accentColor: s.accentColor,
        density: s.density,
        lang: s.lang,
        sidebarWidth: s.sidebarWidth,
        listWidth: s.listWidth,
        collapsedFolders: s.collapsedFolders,
        expandedAccounts: s.expandedAccounts,
        composeRich: s.composeRich,
        space: s.space,
        translateTo: s.translateTo,
        sharedView: s.sharedView,
        mediaViews: s.mediaViews,
        sharedGroup: s.sharedGroup,
        listViews: s.listViews,
        groupModes: s.groupModes,
        lockedLists: s.lockedLists,
        rememberViews: s.rememberViews,
        focusPane: s.focusPane,
        markReadOnScroll: s.markReadOnScroll,
        episodesKeep: s.episodesKeep,
        readerLayout: s.readerLayout,
        feedKind: s.feedKind,
        hiddenViews: s.hiddenViews,
        viewsStyle: s.viewsStyle,
        cardSize: s.cardSize,
        collapsedSections: s.collapsedSections,
        showFeeds: s.showFeeds,
        catalogView: s.catalogView,
        catalogOnline: s.catalogOnline,
        undoSendSeconds: s.undoSendSeconds,
        keepRunning: s.keepRunning,
        aiPanel: s.aiPanel,
        listTranslate: s.listTranslate,
        readerTranslate: s.readerTranslate,
        readerFullText: s.readerFullText,
        readerStyle: s.readerStyle,
        listStyle: s.listStyle,
        readerWeb: s.readerWeb,
        webDevice: s.webDevice,
        webLandscape: s.webLandscape,
        webSizes: s.webSizes,
        recentFolders: s.recentFolders,
        searchAll: s.searchAll,
        notifyMail: s.notifyMail,
        badgeUnread: s.badgeUnread,
        senderLogos: s.senderLogos,
        toolbars: s.toolbars,
        listPaging: s.listPaging,
        listHoverSummary: s.listHoverSummary,
        enabledPlugins: s.enabledPlugins,
        welcomeDone: s.welcomeDone,
        examplePluginsSeeded: s.examplePluginsSeeded,
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

/** 内置视图和分类两个空间都有，按当前空间只看那一类；邮件专属的视图本来就只有邮件。 */
function splitsBySpace(scope: ListScope): boolean {
  return scope.kind === "label" || (scope.kind === "view" && !isMailOnlyView(scope.view));
}

/** 旧版本按列表记的显示方式里，不分邮件和订阅的内置视图拆成两边各一份。 */
function splitViewKeys<T>(record: Record<string, T> | undefined): Record<string, T> {
  const out: Record<string, T> = { ...(record ?? {}) };
  for (const [key, value] of Object.entries(record ?? {})) {
    const m = /^view:(inbox|today|unread|starred|archived)$/.exec(key);
    if (!m) continue;
    delete out[key];
    out[`${key}:mail`] ??= value;
    out[`${key}:article`] ??= value;
  }
  return out;
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
    case "newsletter":
      return `newsletter:${scope.address}`;
  }
}

/** 记住显示方式用的键。内置视图和分类在两个空间里各记各的。 */
export function listKey(scope: ListScope, space: Space): string {
  const base = scopeKey(scope);
  return splitsBySpace(scope) ? `${base}:${kindOfSpace(space)}` : base;
}

/** 这个列表用自己的视图：锁定了，或者打开了「每个列表记住自己的视图」。 */
function ownsView(s: Pick<UiState, "rememberViews" | "lockedLists">, key: string): boolean {
  return s.rememberViews || s.lockedLists.includes(key);
}

/** 当前列表锁定了自己的视图。 */
export function useViewLocked(): boolean {
  return useUi((s) => s.lockedLists.includes(listKey(s.scope, s.space)));
}

/** 当前列表的显示方式。 */
/** 视频或播客的列表；文章、邮件是 undefined。 */
export type MediaList = "video" | "podcast" | undefined;

/** 视频默认卡片（看得到缩略图），播客默认列表（时长一目了然）。 */
const MEDIA_VIEW_DEFAULT: Record<"video" | "podcast", ListView> = { video: "cards", podcast: "list" };

function sharedViewOf(s: Pick<UiState, "sharedView" | "mediaViews">, media: MediaList): ListView {
  return media ? (s.mediaViews[media] ?? MEDIA_VIEW_DEFAULT[media]) : s.sharedView;
}

/** 当前列表的视图。`media` 见 useListMedia。 */
export function useListView(media?: MediaList): ListView {
  return useUi((s) => {
    const key = listKey(s.scope, s.space);
    return (ownsView(s, key) ? s.listViews[key] : undefined) ?? sharedViewOf(s, media);
  });
}

/** 当前列表的折叠方式。 */
export function useGroupMode(): GroupMode {
  return useUi((s) => {
    const key = listKey(s.scope, s.space);
    return (ownsView(s, key) ? s.groupModes[key] : undefined) ?? s.sharedGroup;
  });
}

/** 搜索时的查询条件：在当前列表里搜，或者在当前空间的全部内容（收件箱和归档）里搜。 */
export function searchFilterOf(
  scope: ListScope,
  space: Space,
  query: string,
  all: boolean,
  feedKind: ContentKind | "all" = "all",
): ItemFilter {
  const base: ItemFilter = all
    ? withFeedKind({ view: "all", kind: kindOfSpace(space) }, space, feedKind)
    : filterOf(scope, space, feedKind);
  return { ...base, query };
}

/** 订阅那边选了某一类源时，只看这一类。 */
function withFeedKind(filter: ItemFilter, space: Space, feedKind: ContentKind | "all"): ItemFilter {
  return space === "feeds" && feedKind !== "all" ? { ...filter, feed_kind: feedKind } : filter;
}

/** 把列表范围换算成查询条件。订阅源、文件夹和邮箱里显示的是未归档的条目。 */
export function filterOf(scope: ListScope, space: Space, feedKind: ContentKind | "all" = "all"): ItemFilter {
  const kind = kindOfSpace(space);
  switch (scope.kind) {
    case "feed":
      return { view: "inbox", source_id: scope.feedId };
    case "account":
      // 放到订阅里看的订阅邮件不在邮箱里显示
      return { view: "inbox", source_id: scope.accountId, kind: "mail" };
    case "newsletter":
      return { view: "inbox", sender: scope.address };
    case "mailbox":
      return { view: "folder", source_id: scope.accountId, mailbox: scope.mailbox };
    case "folder":
      return withFeedKind({ view: "inbox", folder: scope.folder }, space, feedKind);
    case "label":
      return withFeedKind({ view: "inbox", label: scope.label, kind }, space, feedKind);
    case "view": {
      const filter: ItemFilter =
        scope.view === "today" ? { view: "today", since: startOfToday() } : { view: scope.view };
      if (isMailOnlyView(scope.view)) return filter;
      filter.kind = kind;
      return withFeedKind(filter, space, feedKind);
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
  if (a.kind === "newsletter" && b.kind === "newsletter") return a.address === b.address;
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
