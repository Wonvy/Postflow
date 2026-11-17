import * as Popover from "@radix-ui/react-popover";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  Archive,
  ArchiveRestore,
  Check,
  CheckCheck,
  ChevronDown,
  ChevronRight,
  Inbox,
  Languages,
  LayoutGrid,
  Layers,
  List,
  ListCollapse,
  type LucideIcon,
  Mail,
  MailOpen,
  Minus,
  Paperclip,
  Plus,
  RefreshCw,
  Search,
  Rows3,
  Rss,
  Sparkles,
  Star,
  X,
} from "lucide-react";
import {
  type CSSProperties,
  type DragEvent,
  type MouseEvent,
  type ReactNode,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useActions } from "../lib/actions";
import { requestAbstracts, useAbstracts } from "../lib/ai";
import { api } from "../lib/api";
import { formatSize } from "../lib/platform";
import { requestListTranslations, useListTranslations } from "../lib/listTranslate";
import { languageName, useModelDownloads, useTranslateEngine, useTranslateTarget } from "../lib/translate";
import type { Item, ItemKind } from "../lib/api";
import { endDrag, startDrag } from "../lib/dnd";
import type { Group, GroupMode, Row } from "../lib/grouping";
import { type MessageKey, type Translate, useLang, useT } from "../lib/i18n";
import { dateSection, sectionLabel } from "../lib/dateSections";
import { useListModel } from "../lib/listModel";
import {
  isDefaultListStyle,
  LIST_STYLE_DEFAULT,
  LIST_STYLE_RANGE,
  type ListMetrics,
  type ListStyle,
  listMetrics,
  THUMB_SIZE,
} from "../lib/listStyle";
import { isMac } from "../lib/platform";
import { useNav, useSettings } from "../lib/queries";
import {
  CARD_MAX,
  CARD_MIN,
  type Density,
  isMailOnlyView,
  type ListScope,
  type ListView,
  listKey,
  useGroupMode,
  useListStyle,
  useListView,
  useUi,
} from "../lib/store";
import { useLabelColor } from "../lib/labels";
import { shortTime } from "../lib/time";
import { ItemMenuItems } from "./ItemMenu";
import { SenderIcon, useMixedAccounts } from "./SourceIcon";
import {
  Button,
  ContextMenu,
  cx,
  ICON,
  IconButton,
  Menu,
  MenuItem,
  MenuSeparator,
  Modal,
  Segmented,
  Spinner,
  Switch,
  Tip,
} from "./ui";

/** 展开的组的标题行。 */
const HEADER_HEIGHT = 30;
/** 按日期分段时每段的标题行。 */
const SECTION_HEIGHT = 30;
/** 每个列表上次滚到的位置，只在这次打开应用期间记着。 */
const scrollPositions = new Map<string, number>();

/** 滚动到离末尾还剩这么多条时开始加载下一页。 */
const LOAD_AHEAD = 30;

/**
 * 虚拟列表里的一行：展开的组的标题、一条（或折叠的一组），或者卡片视图里的一排卡片。
 */
type Line =
  | { kind: "section"; key: string; label: string; count: number }
  | { kind: "header"; group: Group }
  | { kind: "row"; row: Row }
  | { kind: "cards"; rows: Row[] };

/** 一行代表的条目的时间：组用最新的那一条。 */
const rowTime = (row: Row) => (row.kind === "item" ? row.item : row.group.lead).published_at;

/**
 * 把列表的行排成虚拟列表的一行行。`section` 给出时按日期分段：每段前面一行标题。
 * 展开的组里的条目跟着组头走，不会被拆到别的段里。
 */
function linesOf(
  rows: Row[],
  view: ListView,
  columns: number,
  section?: (ms: number) => { key: string; label: string },
): Line[] {
  const lines: Line[] = [];
  let current: Row[] = [];
  let heading: Extract<Line, { kind: "section" }> | null = null;
  const flush = () => {
    if (current.length > 0) lines.push({ kind: "cards", rows: current });
    current = [];
  };
  for (const row of rows) {
    const child = row.kind === "item" && row.child;
    if (section && !child) {
      const next = section(rowTime(row));
      if (next.key !== heading?.key) {
        flush();
        heading = { kind: "section", ...next, count: 0 };
        lines.push(heading);
      }
      heading.count += 1;
    }
    if (view !== "cards") {
      lines.push(row.kind === "group" && row.expanded ? { kind: "header", group: row.group } : { kind: "row", row });
      continue;
    }
    if (row.kind === "group" && row.expanded) {
      flush();
      lines.push({ kind: "header", group: row.group });
      continue;
    }
    // 展开的组里的卡片自成几排，不和组外的混在一排
    const first = current[0];
    if (first && (first.kind === "item" && first.child) !== child) flush();
    current.push(row);
    if (current.length >= columns) flush();
  }
  flush();
  return lines;
}

/** 一行里能停留的位置。 */
function lineIds(line: Line): string[] {
  if (line.kind === "header" || line.kind === "section") return [];
  return (line.kind === "row" ? [line.row] : line.rows).map(rowId);
}

const VIEW_OPTIONS: { value: ListView; icon: LucideIcon; label: "view.list" | "view.summary" | "view.cards" }[] = [
  { value: "list", icon: List, label: "view.list" },
  { value: "summary", icon: Rows3, label: "view.summary" },
  { value: "cards", icon: LayoutGrid, label: "view.cards" },
];

/**
 * 工具栏里的显示方式和列表样式：列表、摘要、卡片，以及字号、间距、显示哪些部分。
 * 改动立即生效，所有列表共用一套样式。
 */
function ViewControl() {
  const t = useT();
  const scope = useUi((s) => s.scope);
  const view = useListView();
  const setView = useUi((s) => s.setListView);
  const cardSize = useUi((s) => s.cardSize);
  const setCardSize = useUi((s) => s.setCardSize);
  const density = useUi((s) => s.density);
  const setDensity = useUi((s) => s.setDensity);
  const style = useListStyle();
  const set = useUi((s) => s.setListStyle);
  const Icon = VIEW_OPTIONS.find((o) => o.value === view)?.icon ?? List;
  // 单行的列表只有标题，没有摘要可调
  const oneLine = view === "list" && density === "compact";

  const step = (key: keyof typeof LIST_STYLE_RANGE, label: MessageKey, by = 1) => (
    <StyleLine label={t(label)}>
      <Stepper
        value={style[key]}
        min={LIST_STYLE_RANGE[key][0]}
        max={LIST_STYLE_RANGE[key][1]}
        onChange={(value) => set({ [key]: value })}
        by={by}
      />
    </StyleLine>
  );
  const toggle = (key: "showSummary" | "showIcon" | "showImage" | "dateSections", label: MessageKey) => (
    <StyleLine label={t(label)}>
      <Switch label={t(label)} checked={style[key]} onChange={(on) => set({ [key]: on })} />
    </StyleLine>
  );

  return (
    <Popover.Root>
      <Tip label={t("listStyle.title")}>
        <Popover.Trigger asChild>
          <button
            type="button"
            aria-label={t("listStyle.title")}
            className="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-fg-2 transition-colors hover:bg-hover hover:text-fg data-[state=open]:bg-hover data-[state=open]:text-fg"
          >
            <Icon {...ICON} />
          </button>
        </Popover.Trigger>
      </Tip>
      <Popover.Portal>
        <Popover.Content
          align="end"
          sideOffset={4}
          collisionPadding={8}
          aria-label={t("listStyle.title")}
          className="anim-pop z-50 flex max-h-[var(--radix-popover-content-available-height)] w-[296px] flex-col gap-2.5 overflow-y-auto rounded-lg bg-surface p-3 shadow-pop outline-none"
        >
          <StyleLine label={t("listStyle.view")}>
            <Segmented<ListView>
              value={view}
              onChange={(v) => setView(scope, v)}
              options={VIEW_OPTIONS.map(({ value, label }) => ({ value, label: t(label) }))}
            />
          </StyleLine>
          {view === "list" && (
            <StyleLine label={t("listStyle.lines")}>
              <Segmented<Density>
                value={density}
                onChange={setDensity}
                options={[
                  { value: "compact", label: t("listStyle.oneLine") },
                  { value: "comfortable", label: t("listStyle.twoLines") },
                ]}
              />
            </StyleLine>
          )}
          {view === "cards" && (
            <div>
              <StyleLine label={t("view.cardSize")}>
                <Stepper value={cardSize} min={CARD_MIN} max={CARD_MAX} by={40} onChange={setCardSize} />
              </StyleLine>
              <div className="mt-1 text-right text-[11px] text-fg-3">
                {t("view.cardSizeHint", { key: isMac ? "⌘" : "Ctrl" })}
              </div>
            </div>
          )}
          <div className="my-0.5 h-px bg-line" />
          {step("titleSize", "listStyle.titleSize")}
          {!oneLine && step("summarySize", "listStyle.summarySize")}
          {step("gap", "listStyle.gap")}
          {step("padding", "listStyle.padding")}
          {view === "summary" && style.showSummary && step("summaryLines", "listStyle.summaryLines")}
          <div className="my-0.5 h-px bg-line" />
          {toggle("dateSections", "listStyle.dateSections")}
          {!oneLine && toggle("showSummary", "listStyle.showSummary")}
          {toggle("showIcon", "listStyle.showIcon")}
          {view !== "list" && toggle("showImage", "listStyle.showImage")}
          <div className="flex justify-end border-t border-line pt-2">
            <button
              type="button"
              disabled={isDefaultListStyle(style)}
              onClick={() => set(LIST_STYLE_DEFAULT)}
              className="shrink-0 rounded px-1.5 py-0.5 text-[12px] text-accent hover:bg-hover disabled:pointer-events-none disabled:text-fg-4"
            >
              {t("listStyle.reset")}
            </button>
          </div>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

function StyleLine({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex min-h-7 items-center justify-between gap-3">
      <span className="text-[13px] text-fg-2">{label}</span>
      {children}
    </div>
  );
}

/** 减号、数字、加号。 */
function Stepper({
  value,
  min,
  max,
  by = 1,
  onChange,
}: {
  value: number;
  min: number;
  max: number;
  by?: number;
  onChange: (value: number) => void;
}) {
  const t = useT();
  const button =
    "inline-flex size-6 items-center justify-center rounded-md border border-line-strong text-fg-2 transition-colors hover:bg-hover hover:text-fg disabled:pointer-events-none disabled:opacity-40";
  return (
    <div className="flex items-center gap-1">
      <button
        type="button"
        aria-label={t("listStyle.less")}
        disabled={value <= min}
        onClick={() => onChange(Math.max(min, value - by))}
        className={button}
      >
        <Minus size={13} strokeWidth={2} />
      </button>
      <output aria-live="polite" className="tabular w-8 text-center text-[13px] text-fg">
        {Math.round(value)}
      </output>
      <button
        type="button"
        aria-label={t("listStyle.more")}
        disabled={value >= max}
        onClick={() => onChange(Math.min(max, value + by))}
        className={button}
      >
        <Plus size={13} strokeWidth={2} />
      </button>
    </div>
  );
}

function scopeTitle(scope: ListScope, t: Translate, sourceTitle: (id: string) => string | undefined) {
  switch (scope.kind) {
    case "view":
      return t(`nav.${scope.view}`);
    case "folder":
      return scope.folder;
    case "feed":
      return sourceTitle(scope.feedId) ?? "";
    case "account":
      return sourceTitle(scope.accountId) ?? "";
    case "label":
      return scope.label;
  }
}

const KIND_OPTIONS: { value: ItemKind | null; icon: LucideIcon; label: "filter.all" | "filter.mail" | "filter.article" }[] = [
  { value: null, icon: Layers, label: "filter.all" },
  { value: "mail", icon: Mail, label: "filter.mail" },
  { value: "article", icon: Rss, label: "filter.article" },
];

/** 内置视图里切换「全部 / 只看邮件 / 只看订阅」。 */
function KindFilter() {
  const t = useT();
  const kind = useUi((s) => s.kindFilter);
  const setKind = useUi((s) => s.setKindFilter);
  return (
    <div role="radiogroup" className="mr-1 inline-flex rounded-md border border-line p-px">
      {KIND_OPTIONS.map(({ value, icon: Icon, label }) => (
        <Tip key={label} label={t(label)}>
          <button
            type="button"
            role="radio"
            aria-checked={kind === value}
            aria-label={t(label)}
            onClick={() => setKind(value)}
            className={cx(
              "inline-flex size-6 items-center justify-center rounded-[5px] transition-colors",
              kind === value ? "bg-active text-fg" : "text-fg-3 hover:text-fg-2",
            )}
          >
            <Icon size={14} strokeWidth={1.75} />
          </button>
        </Tip>
      ))}
    </div>
  );
}

const GROUP_OPTIONS: { value: GroupMode; label: "group.none" | "group.thread" | "group.sender" }[] = [
  { value: "none", label: "group.none" },
  { value: "thread", label: "group.thread" },
  { value: "sender", label: "group.sender" },
];

/** 工具栏里的「折叠相同邮件」：不折叠、按会话、按发件人。 */
function GroupControl() {
  const t = useT();
  const mode = useGroupMode();
  const setMode = useUi((s) => s.setGroupMode);
  return (
    <Menu
      align="end"
      trigger={
        <button
          type="button"
          aria-label={t("group.label")}
          title={t("group.label")}
          className={cx(
            "inline-flex size-7 shrink-0 items-center justify-center rounded-md transition-colors hover:bg-hover hover:text-fg data-[state=open]:bg-hover",
            mode === "none" ? "text-fg-2" : "bg-accent-soft text-accent",
          )}
        >
          <ListCollapse {...ICON} />
        </button>
      }
    >
      <div className="px-2 pb-1 pt-1.5 text-[12px] font-medium text-fg-3">{t("group.label")}</div>
      {GROUP_OPTIONS.map(({ value, label }) => (
        <MenuItem key={value} icon={mode === value ? <Check size={14} /> : null} onSelect={() => setMode(value)}>
          {t(label)}
        </MenuItem>
      ))}
    </Menu>
  );
}

/** 一行代表的位置 ID：单条是它自己，组是组里最新的那一条。 */
function rowId(row: Row): string {
  return row.kind === "item" ? row.item.id : row.group.lead.id;
}

export function ItemList({ width }: { width: number }) {
  const t = useT();
  const lang = useLang();
  const scope = useUi((s) => s.scope);
  const openId = useUi((s) => s.openId);
  const selection = useUi((s) => s.selection);
  const density = useUi((s) => s.density);
  const refreshing = useUi((s) => s.refreshing);
  const setDialog = useUi((s) => s.setDialog);
  const nav = useNav();
  const setGroupExpanded = useUi((s) => s.setGroupExpanded);
  const { confirmMarkAllRead, refresh, membersOf, compose } = useActions();
  const { query, items, rows: listRows, stops, groupMode, search } = useListModel();
  const searchOpen = useUi((s) => s.searchOpen);
  const searchAll = useUi((s) => s.searchAll);
  const openSearch = useUi((s) => s.openSearch);
  const closeSearch = useUi((s) => s.closeSearch);
  const setSearchAll = useUi((s) => s.setSearchAll);
  const terms = useMemo(() => searchTerms(search), [search]);
  const { isLoading, hasNextPage, isFetchingNextPage, fetchNextPage } = query;
  const view = useListView();
  const cardSize = useUi((s) => s.cardSize);
  const layout = useUi((s) => s.sidebarLayout);
  const kindFilter = useUi((s) => s.kindFilter);
  const here = listKey(scope, kindFilter);
  const settings = useSettings();
  const showAbstracts = settings.data?.["ai.list_abstracts"] === true && view !== "list";
  const abstracts = useAbstracts((s) => s.byKey);
  const listTranslate = useUi((s) => s.listTranslate);
  const toggleListTranslate = useUi((s) => s.toggleListTranslate);
  const translateTo = useTranslateTarget();
  const engine = useTranslateEngine();
  const translations = useListTranslations((s) => s.byKey);
  const style = useListStyle();
  const metrics = useMemo(() => listMetrics(style), [style]);
  const mixedAccounts = useMixedAccounts() && scope.kind !== "account";

  const scroller = useRef<HTMLDivElement>(null);
  const anchor = useRef<string | null>(null);

  // 卡片按列表的实际宽度排列
  const [inner, setInner] = useState(width);
  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const update = () => setInner(el.clientWidth - 12);
    update();
    const observer = new ResizeObserver(update);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  const cardGap = metrics.cardGap;
  const columns = Math.max(1, Math.floor((inner + cardGap) / (cardSize + cardGap)));
  const cardWidth = Math.floor((inner - cardGap * (columns - 1)) / columns);
  const imageHeight = style.showImage ? Math.round(cardWidth * 0.56) : 0;
  const cardHeight = imageHeight + metrics.cardText;
  // 搜索结果总用两行，第二行放正文里命中的那一段
  const rows: Density = search ? "comfortable" : density;
  const rowHeight = view === "summary" ? metrics.summaryRow : rows === "compact" ? metrics.compact : metrics.comfortable;

  // 按日期分段：分段的界线每分钟重算一次，过了午夜「今天」的就挪到「昨天」
  const [minute, setMinute] = useState(() => Math.floor(Date.now() / 60_000));
  useEffect(() => {
    if (!style.dateSections) return;
    const timer = setInterval(() => setMinute(Math.floor(Date.now() / 60_000)), 60_000);
    return () => clearInterval(timer);
  }, [style.dateSections]);
  const sectionOf = useMemo(() => {
    if (!style.dateSections) return undefined;
    const now = minute * 60_000;
    return (ms: number) => {
      const section = dateSection(ms, now);
      return { key: section.key, label: sectionLabel(section, lang, now) };
    };
  }, [style.dateSections, minute, lang]);
  const lines = useMemo(() => linesOf(listRows, view, columns, sectionOf), [listRows, view, columns, sectionOf]);
  const sizeOf = (line: Line | undefined) =>
    !line
      ? 0
      : line.kind === "section"
        ? SECTION_HEIGHT
        : line.kind === "header"
        ? HEADER_HEIGHT
        : line.kind === "cards"
          ? cardHeight + cardGap
          : rowHeight + metrics.gap;
  const virtualizer = useVirtualizer({
    count: lines.length,
    getScrollElement: () => scroller.current,
    estimateSize: (i) => sizeOf(lines[i]),
    overscan: view === "cards" ? 3 : 12,
  });

  useEffect(() => virtualizer.measure(), [rowHeight, metrics.gap, cardGap, view, cardHeight, columns, virtualizer]);

  // 按住 Ctrl（Mac 上是 ⌘，或者触控板双指捏合）滚动滚轮调整卡片大小
  const viewRef = useRef(view);
  viewRef.current = view;
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (!(e.ctrlKey || e.metaKey) || viewRef.current !== "cards") return;
      e.preventDefault();
      const delta = Math.max(-60, Math.min(60, e.deltaY));
      const { cardSize: size, setCardSize } = useUi.getState();
      setCardSize(size * Math.exp(-delta * 0.004));
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, []);

  // 卡片换了列数时，让原来最上面的那张还在最上面
  const topId = useRef<string | null>(null);
  const shownColumns = useRef(columns);
  useLayoutEffect(() => {
    if (shownColumns.current === columns) return;
    shownColumns.current = columns;
    const index = lines.findIndex((l) => topId.current !== null && lineIds(l).includes(topId.current));
    if (index >= 0) virtualizer.scrollToIndex(index, { align: "start" });
  }, [columns, lines, virtualizer]);

  // 换了列表时回到它上次滚到的位置（第一次打开是顶部）
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    el.scrollTo({ top: scrollPositions.get(here) ?? 0 });
    anchor.current = null;
    const save = () => scrollPositions.set(here, el.scrollTop);
    el.addEventListener("scroll", save, { passive: true });
    return () => el.removeEventListener("scroll", save);
  }, [here]);

  // 用键盘移动时让打开的那一行保持在视野里
  useEffect(() => {
    if (!openId) return;
    const index = lines.findIndex((l) => lineIds(l).includes(openId));
    if (index >= 0) virtualizer.scrollToIndex(index, { align: "auto" });
    // 只在打开的条目变化时滚动，列表内容更新时不动
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openId]);

  const virtualRows = virtualizer.getVirtualItems();
  const offset = virtualizer.scrollOffset ?? 0;
  const firstVisible = virtualRows.find((v) => v.end > offset);
  useEffect(() => {
    const line = firstVisible ? lines[firstVisible.index] : undefined;
    if (line && shownColumns.current === columns) topId.current = lineIds(line)[0] ?? topId.current;
  });
  const lastVisible = virtualRows[virtualRows.length - 1]?.index ?? 0;
  const ahead = view === "cards" ? Math.ceil(LOAD_AHEAD / columns) : LOAD_AHEAD;
  useEffect(() => {
    if (hasNextPage && !isFetchingNextPage && lastVisible >= lines.length - ahead) {
      void fetchNextPage();
    }
  }, [ahead, fetchNextPage, hasNextPage, isFetchingNextPage, lines.length, lastVisible]);

  // 打开了「列表里显示 AI 摘要」：停止滚动一会儿以后，为看得到的条目要摘要
  const visibleIds = virtualRows
    .flatMap((v) => {
      const line = lines[v.index];
      if (!line || line.kind === "header" || line.kind === "section") return [];
      return (line.kind === "row" ? [line.row] : line.rows).map((r) => (r.kind === "item" ? r.item.id : r.group.lead.id));
    })
    .join(",");
  useEffect(() => {
    if (!showAbstracts || !visibleIds) return;
    const timer = setTimeout(() => requestAbstracts(visibleIds.split(","), lang), 400);
    return () => clearTimeout(timer);
  }, [lang, showAbstracts, visibleIds]);

  // 翻译模式：看得到的条目的标题和摘要
  useEffect(() => {
    if (!listTranslate || !visibleIds) return;
    const timer = setTimeout(() => requestListTranslations(visibleIds.split(","), translateTo, engine), 300);
    return () => clearTimeout(timer);
  }, [engine, listTranslate, translateTo, visibleIds]);

  // 拖到侧栏的视图上：勾选着的就拖勾选的那些，否则拖这一行代表的条目
  const onDragStart = (e: DragEvent, id: string, label: string) => {
    const ui = useUi.getState();
    const members = membersOf(id);
    const ids = members.every((m) => ui.selection.includes(m)) ? ui.selection : members;
    startDrag(e, { kind: "items", ids }, label);
  };

  /** 双击草稿：接着写。 */
  const onRowDoubleClick = (id: string) => {
    const item = items.find((i) => i.id === id);
    if (item?.draft) compose("draft", item);
  };

  const onRowClick = (e: MouseEvent, id: string) => {
    const ui = useUi.getState();
    const members = membersOf(id);
    if (e.metaKey || e.ctrlKey) {
      // 按住 ⌘ 或 Ctrl 逐个勾选。第一次勾选时把正在读的那一行也带上，符合直觉。
      const base =
        ui.selection.length > 0 ? ui.selection : ui.openId && ui.openId !== id ? membersOf(ui.openId) : [];
      const all = members.every((m) => base.includes(m));
      ui.setSelection(all ? base.filter((m) => !members.includes(m)) : [...new Set([...base, ...members])]);
      anchor.current = id;
      return;
    }
    if (e.shiftKey) {
      // 按住 Shift 勾选一段
      const from = stops.findIndex((s) => s.id === (anchor.current ?? ui.openId));
      const to = stops.findIndex((s) => s.id === id);
      if (from >= 0 && to >= 0) {
        const [start, end] = from < to ? [from, to] : [to, from];
        ui.setSelection(stops.slice(start, end + 1).flatMap((s) => s.members));
        return;
      }
    }
    anchor.current = id;
    ui.setSelection([]);
    ui.open(id);
  };

  const feeds = nav.data?.feeds;
  const accounts = nav.data?.accounts;
  const title = scopeTitle(
    scope,
    t,
    (id) =>
      feeds?.find((f) => f.id === id)?.title ??
      accounts?.map((a) => (a.id === id ? a.name || a.email : undefined)).find(Boolean),
  );
  const hasUnread = items.some((i) => !i.read);

  return (
    <section className="relative flex h-full shrink-0 flex-col border-l border-line bg-bg" style={{ width }}>
      <header data-tauri-drag-region className="flex h-11 shrink-0 items-center gap-0.5 border-b border-line pl-4 pr-2">
        <h1 data-tauri-drag-region className="min-w-0 flex-1 truncate text-[13px] font-semibold">
          {title}
        </h1>
        {scope.kind === "view" && layout !== "tabs" && <KindFilter />}
        <IconButton
          label={t("search.label")}
          shortcut="/"
          active={searchOpen}
          onClick={() => (searchOpen ? closeSearch() : openSearch())}
        >
          <Search {...ICON} />
        </IconButton>
        <IconButton label={t("list.translate")} active={listTranslate} onClick={toggleListTranslate}>
          <Languages {...ICON} />
        </IconButton>
        <ViewControl />
        <GroupControl />
        <IconButton label={t("action.markAllRead")} shortcut="⇧A" disabled={!hasUnread} onClick={confirmMarkAllRead}>
          <CheckCheck {...ICON} />
        </IconButton>
        <IconButton label={t("action.refresh")} onClick={refresh}>
          <RefreshCw {...ICON} className={cx(refreshing > 0 && "animate-spin")} />
        </IconButton>
      </header>

      {searchOpen && <SearchBar scopeName={title} fetching={query.isFetching} firstId={stops[0]?.id} />}
      {listTranslate && engine === "offline" && <ModelPrompt />}

      <div ref={scroller} role="listbox" aria-label={title} aria-multiselectable className="min-h-0 flex-1 overflow-y-auto">
        {isLoading ? (
          <div className="flex h-full items-center justify-center text-fg-3">
            <Spinner size={16} />
          </div>
        ) : items.length === 0 ? (
          <EmptyList
            search={search ? { query: search, all: searchAll, onAll: () => setSearchAll(true) } : undefined}
            noSources={feeds !== undefined && feeds.length === 0 && accounts !== undefined && accounts.length === 0}
            caughtUp={scope.kind === "view" && (scope.view === "inbox" || scope.view === "unread")}
            folderHint={
              scope.kind !== "view" || !isMailOnlyView(scope.view)
                ? null
                : scope.view === "drafts"
                  ? "list.draftsHint"
                  : "list.folderHint"
            }
            onAddAccount={() => setDialog({ type: "add-account" })}
            onAddFeed={() => setDialog({ type: "add-feed" })}
          />
        ) : (
          <div className="relative mx-1.5 my-1.5" style={{ height: virtualizer.getTotalSize() }}>
            {virtualRows.map((virtual) => {
              const line = lines[virtual.index];
              if (!line) return null;
              const position = { transform: `translateY(${virtual.start}px)` };

              // 日期分段的标题
              if (line.kind === "section") {
                return (
                  <div
                    // 列表不按时间排时（比如星标），同一段可能出现不止一次
                    key={`s:${line.key}:${virtual.index}`}
                    role="presentation"
                    className="absolute inset-x-0 top-0 flex items-end gap-2 pb-1.5 text-[12px] font-semibold text-fg-2"
                    style={{ ...position, height: SECTION_HEIGHT, paddingLeft: metrics.padX, paddingRight: metrics.padX }}
                  >
                    <span className="min-w-0 truncate">{line.label}</span>
                    <span className="tabular font-normal text-fg-4">{line.count}</span>
                    <span aria-hidden className="mb-[5px] h-px flex-1 bg-line" />
                  </div>
                );
              }

              // 展开的组：一行标题，点一下收起
              if (line.kind === "header") {
                const group = line.group;
                return (
                  <div
                    key={`h:${group.key}`}
                    role="button"
                    tabIndex={-1}
                    onClick={() => setGroupExpanded(group.key, false)}
                    className="absolute inset-x-0 top-0 flex items-center gap-2 rounded-md px-2.5 text-[12px] text-fg-3 hover:bg-hover hover:text-fg-2"
                    style={{ ...position, height: HEADER_HEIGHT - 1 }}
                  >
                    <ChevronDown size={14} strokeWidth={2} className="shrink-0" />
                    <span className="min-w-0 flex-1 truncate font-medium">
                      {groupMode === "thread" ? group.lead.title || t("reader.untitled") : group.people[0]}
                    </span>
                    <span className="tabular shrink-0">{t("group.count", { n: group.items.length })}</span>
                  </div>
                );
              }

              const describe = (row: Row) => {
                const id = rowId(row);
                const original = row.kind === "item" ? row.item : row.group.lead;
                // 翻译模式下显示译文，原文放在悬停提示里
                const translated = listTranslate ? translations[`${translateTo}:${original.id}`] : undefined;
                const shown = translated
                  ? { ...original, title: translated.title || original.title, summary: translated.summary || original.summary }
                  : original;
                // 搜索时摘要换成正文里命中的那一段
                const item = search && shown.snippet ? { ...shown, summary: shown.snippet } : shown;
                const group = row.kind === "group" ? row.group : undefined;
                const members = group ? group.items.map((i) => i.id) : [item.id];
                const props: RowProps = {
                  item,
                  group,
                  groupMode,
                  time: shortTime(item.published_at, lang),
                  untitled: t("reader.untitled"),
                  onExpand: group ? () => setGroupExpanded(group.key, true) : undefined,
                  expandLabel: t("group.expand"),
                  abstract: showAbstracts && !search ? abstracts[`${lang}:${item.id}`] : undefined,
                  look: { style, m: metrics, badge: mixedAccounts, terms },
                };
                return {
                  id,
                  tooltip: translated ? original.title : undefined,
                  key: group ? `g:${group.key}` : item.id,
                  props,
                  opened: id === openId,
                  selected: members.every((m) => selection.includes(m)),
                  child: row.kind === "item" && row.child,
                  drag: {
                    draggable: true,
                    onDragStart: (e: DragEvent) => onDragStart(e, id, item.title),
                    onDragEnd: endDrag,
                  },
                };
              };

              if (line.kind === "cards") {
                return (
                  <div
                    key={`c:${lineIds(line)[0]}`}
                    className="absolute inset-x-0 top-0 grid"
                    style={{
                      ...position,
                      height: cardHeight,
                      gridTemplateColumns: `repeat(${columns}, ${cardWidth}px)`,
                      columnGap: cardGap,
                    }}
                  >
                    {line.rows.map((row) => {
                      const d = describe(row);
                      return (
                        <ContextMenu key={d.key} bare items={<RowMenu id={d.id} />}>
                          <div
                            role="option"
                            aria-selected={d.opened || d.selected}
                            onClick={(e) => onRowClick(e, d.id)}
                            onDoubleClick={() => onRowDoubleClick(d.id)}
                            title={d.tooltip}
                            {...d.drag}
                            className={cx(
                              "overflow-hidden rounded-lg border transition-colors",
                              d.selected
                                ? "border-accent/60 bg-accent-soft"
                                : d.opened
                                  ? "border-accent/60 bg-active"
                                  : "border-line bg-surface hover:border-line-strong hover:bg-hover",
                              "data-[state=open]:border-accent/60",
                            )}
                            style={{ height: cardHeight }}
                          >
                            <Card {...d.props} imageHeight={imageHeight} />
                          </div>
                        </ContextMenu>
                      );
                    })}
                  </div>
                );
              }

              const d = describe(line.row);
              return (
                <ContextMenu key={d.key} bare items={<RowMenu id={d.id} />}>
                  <div
                    role="option"
                    aria-selected={d.opened || d.selected}
                    onClick={(e) => onRowClick(e, d.id)}
                    onDoubleClick={() => onRowDoubleClick(d.id)}
                    title={d.tooltip}
                    {...d.drag}
                    className={cx(
                      "absolute inset-x-0 top-0 flex rounded-md transition-colors",
                      view === "summary"
                        ? "items-center gap-3"
                        : rows === "compact"
                          ? "items-center gap-2.5"
                          : "flex-col justify-center gap-1",
                      d.child && "ml-5",
                      d.selected ? "bg-accent-soft" : d.opened ? "bg-active" : "hover:bg-hover",
                      d.selected && d.opened && "ring-1 ring-inset ring-accent/50",
                      "data-[state=open]:ring-1 data-[state=open]:ring-inset data-[state=open]:ring-accent/60",
                    )}
                    style={{ ...position, height: rowHeight, paddingLeft: metrics.padX, paddingRight: metrics.padX }}
                  >
                    {view === "summary" ? (
                      <SummaryRow {...d.props} />
                    ) : rows === "compact" ? (
                      <CompactRow {...d.props} />
                    ) : (
                      <ComfortableRow {...d.props} />
                    )}
                  </div>
                </ContextMenu>
              );
            })}
          </div>
        )}
        {isFetchingNextPage && (
          <div className="flex h-9 items-center justify-center text-fg-3">
            <Spinner />
          </div>
        )}
        {/* 给底部的操作条留出位置，最后几行不被它挡住 */}
        {selection.length > 0 && <div className="h-14" />}
      </div>
      {selection.length > 0 && <SelectionBar count={selection.length} />}
    </section>
  );
}

/**
 * 列表里一行的右键菜单。点在勾选着的行上作用于所有勾选的，否则只作用于这一行（折叠的组是整组）。
 * 只有菜单打开时才挂载，平时不占资源。
 */
function RowMenu({ id }: { id: string }) {
  const t = useT();
  const { membersOf, itemsOf, toggleSelected } = useActions();
  const [targets] = useState(() => {
    const { selection } = useUi.getState();
    const members = membersOf(id);
    const inSelection = selection.length > 0 && members.every((m) => selection.includes(m));
    return { ids: inSelection ? selection : members, inSelection };
  });
  const items = itemsOf(targets.ids);
  const open = useUi((s) => s.openId === id);
  return (
    <>
      {!open && !targets.inSelection && (
        <>
          <MenuItem icon={<MailOpen size={14} />} onSelect={() => useUi.getState().open(id)}>
            {t("menu.open")}
          </MenuItem>
          <MenuSeparator />
        </>
      )}
      <ItemMenuItems items={items} />
      <MenuSeparator />
      <MenuItem icon={targets.inSelection ? <X size={14} /> : <Check size={14} />} onSelect={() => toggleSelected(id)}>
        {targets.inSelection ? t("menu.deselect") : t("menu.select")}
      </MenuItem>
    </>
  );
}

/**
 * 全部标为已读之前确认一下，免得手滑把一大片还没看的标掉。
 * 放在外壳里一直挂着：在订阅统计页按 Shift+A 也有对话框，不会留下一个看不见的对话框挡住快捷键。
 */
export function MarkAllReadDialog() {
  const t = useT();
  const dialog = useUi((s) => s.dialog);
  const setDialog = useUi((s) => s.setDialog);
  const scope = useUi((s) => s.scope);
  const nav = useNav();
  const title = scopeTitle(
    scope,
    t,
    (id) =>
      nav.data?.feeds.find((f) => f.id === id)?.title ??
      nav.data?.accounts.map((a) => (a.id === id ? a.name || a.email : undefined)).find(Boolean),
  );
  const { items, query } = useListModel();
  const { markAllRead } = useActions();
  const open = dialog?.type === "mark-all-read";
  const unread = items.filter((i) => !i.read).length;
  // 列表还有没加载的部分时说不准有多少条，不写数字
  const counted = !query.hasNextPage;
  const close = () => setDialog(null);
  return (
    <Modal open={open} onClose={close} title={t("markAll.title")} width={400}>
      <div className="flex flex-col gap-4 p-4">
        <div>
          <p className="leading-relaxed text-fg-2">
            {counted ? t("markAll.bodyCount", { name: title, n: unread }) : t("markAll.body", { name: title })}
          </p>
          {!counted && <p className="mt-1 text-[12px] leading-relaxed text-fg-3">{t("markAll.hint")}</p>}
        </div>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={close}>
            {t("action.cancel")}
          </Button>
          <Button
            variant="primary"
            data-autofocus=""
            onClick={() => {
              markAllRead();
              close();
            }}
          >
            <CheckCheck size={14} />
            {t("markAll.confirm")}
          </Button>
        </div>
      </div>
    </Modal>
  );
}

/** 搜索框里的词：按空格拆开、去掉引号，长的排前面，高亮时先匹配长的。 */
function searchTerms(query: string): string[] {
  const words = query
    .split(/\s+/)
    .map((w) => w.replace(/^"+|"+$/g, ""))
    .filter(Boolean);
  return [...new Set(words)].sort((a, b) => b.length - a.length);
}

const escapeRegExp = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** 把文字里的搜索词标出来。 */
function Hl({ text, terms }: { text: string; terms: string[] }) {
  if (terms.length === 0 || !text) return <>{text}</>;
  const parts = text.split(new RegExp(`(${terms.map(escapeRegExp).join("|")})`, "gi"));
  return (
    <>
      {parts.map((part, i) =>
        i % 2 === 1 ? (
          <mark key={i} className="rounded-[2px] bg-warning/30 text-inherit">
            {part}
          </mark>
        ) : (
          part
        ),
      )}
    </>
  );
}

/** 单行列表里，搜索命中在正文时，在标题后面跟上那一段。 */
function SearchSnippet({ item, look }: { item: Item; look: Look }) {
  if (look.terms.length === 0 || !item.snippet) return null;
  return (
    <span className="font-normal text-fg-3" style={{ fontSize: look.m.summary.size }}>
      {" · "}
      <Hl text={item.snippet} terms={look.terms} />
    </span>
  );
}

/**
 * 列表上方的搜索栏。打字停一下才去搜；回车或 ↓ 打开第一条结果，焦点交给列表，接着用 J / K。
 * Esc 先清空，再按一次收起。
 */
function SearchBar({ scopeName, fetching, firstId }: { scopeName: string; fetching: boolean; firstId?: string }) {
  const t = useT();
  const query = useUi((s) => s.searchQuery);
  const setQuery = useUi((s) => s.setSearchQuery);
  const all = useUi((s) => s.searchAll);
  const setAll = useUi((s) => s.setSearchAll);
  const close = useUi((s) => s.closeSearch);
  const focus = useUi((s) => s.searchFocus);
  const [text, setText] = useState(query);
  const input = useRef<HTMLInputElement>(null);

  // 按 / 或点放大镜时把焦点放进来，选中已有的词
  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, [focus]);

  useEffect(() => {
    if (text.trim() === query.trim()) return;
    const timer = setTimeout(() => setQuery(text), 250);
    return () => clearTimeout(timer);
  }, [text, query, setQuery]);

  return (
    <div className="flex h-10 shrink-0 items-center gap-2 border-b border-line pl-4 pr-2">
      <Search size={14} strokeWidth={1.75} className="shrink-0 text-fg-3" />
      <input
        ref={input}
        type="text"
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            if (text) {
              setText("");
              setQuery("");
            } else {
              close();
            }
          } else if (e.key === "Enter" || e.key === "ArrowDown") {
            e.preventDefault();
            setQuery(text);
            if (firstId) {
              useUi.getState().open(firstId);
              input.current?.blur();
            }
          }
        }}
        placeholder={all ? t("search.placeholderAll") : t("search.placeholderIn", { name: scopeName })}
        aria-label={t("search.label")}
        spellCheck={false}
        autoComplete="off"
        className="h-7 min-w-0 flex-1 bg-transparent text-[13px] text-fg outline-none placeholder:text-fg-3"
      />
      {fetching && text.trim() !== "" && <Spinner size={13} />}
      <Segmented<"here" | "all">
        value={all ? "all" : "here"}
        onChange={(v) => setAll(v === "all")}
        options={[
          { value: "here", label: t("search.here") },
          { value: "all", label: t("search.all") },
        ]}
      />
      <IconButton label={t("search.close")} shortcut="Esc" className="size-6" onClick={close}>
        <X size={14} strokeWidth={1.75} />
      </IconButton>
    </div>
  );
}

/** 翻译模式用离线模型、但还缺模型时，列表上方提示下载。 */
function ModelPrompt() {
  const t = useT();
  const lang = useLang();
  const missing = useListTranslations((s) => s.missing);
  const downloads = useModelDownloads((s) => s.byPair);
  if (missing.length === 0) return null;
  const states = missing.map((m) => downloads[m.pair]);
  const downloading = states.some((s) => s?.state === "downloading");
  const received = states.reduce((n, s) => n + (s?.received ?? 0), 0);
  const total = states.reduce((n, s) => n + (s?.total ?? 0), 0);
  const pairs = missing.map((m) => `${languageName(m.from, lang)} → ${languageName(m.to, lang)}`).join("、");
  const size = formatSize(missing.reduce((n, m) => n + m.size, 0));
  return (
    <div className="flex shrink-0 items-center gap-2 border-b border-line bg-hover px-4 py-2 text-[12px] text-fg-2">
      <Languages size={13} strokeWidth={1.75} className="shrink-0 text-fg-3" />
      <span className="min-w-0 flex-1">
        {downloading
          ? `${t("translate.downloading")} ${total > 0 ? Math.floor((received / total) * 100) : 0}%`
          : t("list.translateNeedsModel", { pairs, size })}
      </span>
      {!downloading && (
        <Button
          variant="primary"
          className="h-6 px-2 text-[12px]"
          onClick={() => {
            const { update } = useModelDownloads.getState();
            for (const m of missing) update(m.pair, { received: 0, total: 0, state: "downloading" });
            void api.downloadModels(missing.map((m) => m.pair));
          }}
        >
          {t("translate.download")}
        </Button>
      )}
    </div>
  );
}

/** 勾选了条目时浮在列表底部的操作条。 */
function SelectionBar({ count }: { count: number }) {
  const t = useT();
  const { targets, toggle } = useActions();
  const setSelection = useUi((s) => s.setSelection);
  const list = targets();
  const all = (flag: "read" | "starred" | "archived") => list.length > 0 && list.every((i) => i[flag]);

  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-3 flex justify-center px-3">
      <div
        role="toolbar"
        className="anim-pop pointer-events-auto flex h-9 items-center gap-0.5 rounded-lg bg-surface pl-3 pr-1 shadow-pop"
      >
        <span className="tabular mr-2 whitespace-nowrap text-[12px] font-medium text-fg-2">
          {t("reader.selected", { n: count })}
        </span>
        <IconButton
          label={all("read") ? t("action.markUnread") : t("action.markRead")}
          shortcut="U"
          tipSide="top"
          onClick={() => toggle("read")}
        >
          {all("read") ? <Mail {...ICON} /> : <MailOpen {...ICON} />}
        </IconButton>
        <IconButton
          label={all("starred") ? t("action.unstar") : t("action.star")}
          shortcut="S"
          tipSide="top"
          onClick={() => toggle("starred")}
        >
          <Star {...ICON} className={cx(all("starred") && "fill-warning text-warning")} />
        </IconButton>
        <IconButton
          label={all("archived") ? t("action.unarchive") : t("action.archive")}
          shortcut="E"
          tipSide="top"
          onClick={() => toggle("archived")}
        >
          {all("archived") ? <ArchiveRestore {...ICON} /> : <Archive {...ICON} />}
        </IconButton>
        <span className="mx-1 h-4 w-px bg-line-strong" />
        <IconButton label={t("action.clearSelection")} shortcut="Esc" tipSide="top" onClick={() => setSelection([])}>
          <X {...ICON} />
        </IconButton>
      </div>
    </div>
  );
}

function UnreadDot({ unread }: { unread: boolean }) {
  return (
    <span
      aria-hidden
      className={cx("size-1.5 shrink-0 rounded-full", unread ? "bg-accent" : "bg-transparent")}
    />
  );
}

interface RowProps {
  item: Item;
  /** 折叠着的一组时有值，这一行代表整组。 */
  group?: Group;
  groupMode: GroupMode;
  time: string;
  untitled: string;
  onExpand?: () => void;
  expandLabel: string;
  /** AI 写的一句话摘要，有的话代替原文开头。 */
  abstract?: string;
  look: Look;
}

/** 列表样式算出来的尺寸，以及邮件要不要标出是哪个邮箱的。 */
interface Look {
  /** 搜索时要高亮的词。 */
  terms: string[];
  style: ListStyle;
  m: ListMetrics;
  badge: boolean;
}

function Sender({ item, look, size = 16 }: { item: Item; look: Look; size?: number }) {
  if (!look.style.showIcon) return null;
  return <SenderIcon item={item} size={size} badge={look.badge} />;
}

/** 折叠的组右侧的数字，点一下展开。 */
function CountPill({ count, onExpand, label }: { count: number; onExpand: () => void; label: string }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={(e) => {
        e.stopPropagation();
        onExpand();
      }}
      className="tabular flex h-[18px] shrink-0 items-center gap-0.5 rounded-full border border-line-strong pl-1.5 pr-1 text-[11px] font-medium leading-none text-fg-2 hover:border-accent hover:text-accent"
    >
      {count}
      <ChevronRight size={11} strokeWidth={2.25} />
    </button>
  );
}

/** 分类的小标签。 */
export function LabelChip({ label }: { label: string }) {
  const labelColor = useLabelColor();
  return (
    <span className="inline-flex max-w-[96px] shrink-0 items-center gap-1 rounded-full bg-hover px-1.5 py-px text-[11px] leading-[16px] text-fg-2">
      <span aria-hidden className="size-1.5 shrink-0 rounded-full" style={{ background: labelColor(label) }} />
      <span className="truncate">{label}</span>
    </span>
  );
}

function Trailing({ item, group, time, onExpand, expandLabel, look }: RowProps) {
  const attachments = group ? group.hasAttachments : item.has_attachments;
  const starred = group ? group.starred : item.starred;
  return (
    <>
      {item.label && <LabelChip label={item.label} />}
      {group && onExpand && <CountPill count={group.items.length} onExpand={onExpand} label={expandLabel} />}
      {attachments && <Paperclip size={12} className="shrink-0 text-fg-3" />}
      {starred && <Star size={12} className="shrink-0 fill-warning text-warning" />}
      <span className="tabular shrink-0 text-fg-3" style={{ fontSize: Math.min(look.m.summary.size, 13) }}>
        {time}
      </span>
    </>
  );
}

/** 邮件这一行左边显示谁：单封是发件人，一个会话是参与的人。 */
function senderLabel(item: Item, group: Group | undefined, groupMode: GroupMode): string {
  if (group && groupMode === "thread") return group.people.join(", ");
  return item.author || item.sender_address || "";
}

const text = (font: { size: number; line: number }): CSSProperties => ({ fontSize: font.size, lineHeight: `${font.line}px` });

function CompactRow(props: RowProps) {
  const { item, group, groupMode, untitled, look } = props;
  const unread = group ? group.unread > 0 : !item.read;
  const unreadText = unread ? "font-medium text-fg" : "text-fg-3";
  const font = text(look.m.title);
  return (
    <>
      <UnreadDot unread={unread} />
      <Sender item={item} look={look} />
      {item.kind === "mail" ? (
        <>
          <span className={cx("w-[110px] shrink-0 truncate", unreadText)} style={font}>
            <Hl text={senderLabel(item, group, groupMode)} terms={look.terms} />
          </span>
          <span className={cx("min-w-0 flex-1 truncate", unread ? "text-fg" : "text-fg-3")} style={font}>
            <Hl text={item.title || untitled} terms={look.terms} />
            <SearchSnippet item={item} look={look} />
          </span>
        </>
      ) : (
        <span className={cx("min-w-0 flex-1 truncate", unreadText)} style={font}>
          <Hl text={item.title || untitled} terms={look.terms} />
          <SearchSnippet item={item} look={look} />
        </span>
      )}
      <Trailing {...props} />
    </>
  );
}

function ComfortableRow(props: RowProps) {
  const { item, group, groupMode, untitled, look } = props;
  const unread = group ? group.unread > 0 : !item.read;
  const source = item.kind === "mail" ? senderLabel(item, group, groupMode) : item.source_title;
  return (
    <>
      <div className="flex items-center gap-2.5">
        <UnreadDot unread={unread} />
        <span
          className={cx("min-w-0 flex-1 truncate", unread ? "font-medium text-fg" : "text-fg-3")}
          style={text(look.m.title)}
        >
          <Hl text={item.title || untitled} terms={look.terms} />
        </span>
        <Trailing {...props} />
      </div>
      <div className="flex items-center gap-2 pl-4 text-fg-3" style={text(look.m.summary)}>
        <Sender item={item} look={look} size={14} />
        <span className="min-w-0 truncate">
          <span className="text-fg-2">
            <Hl text={source} terms={look.terms} />
          </span>
          {(look.style.showSummary || look.terms.length > 0) && item.summary && (
            <span>
              {" · "}
              <Hl text={item.summary} terms={look.terms} />
            </span>
          )}
        </span>
      </div>
    </>
  );
}

/** 来源：文章是订阅源的图标和名字，邮件是发件人。 */
function SourceLine(props: RowProps & { size?: number }) {
  const { item, group, groupMode, look, size = 14 } = props;
  const source = item.kind === "mail" ? senderLabel(item, group, groupMode) : item.source_title;
  return (
    <div className="flex min-w-0 items-center gap-1.5 text-fg-3" style={text(look.m.summary)}>
      <Sender item={item} look={look} size={size} />
      <span className="min-w-0 flex-1 truncate">
        <Hl text={source} terms={look.terms} />
      </span>
      <Trailing {...props} />
    </div>
  );
}

/** 图片加载不出来就不显示，而不是留一个破图标。 */
function Picture({ src, className, style }: { src: string; className?: string; style?: CSSProperties }) {
  const [failed, setFailed] = useState(false);
  useEffect(() => setFailed(false), [src]);
  if (failed) return null;
  return (
    <img
      src={src}
      alt=""
      loading="lazy"
      decoding="async"
      referrerPolicy="no-referrer"
      draggable={false}
      onError={() => setFailed(true)}
      className={className}
      style={style}
    />
  );
}

/** 摘要文字：有 AI 摘要时显示它，前面带一个小标记。 */
function Excerpt({ item, abstract, terms = [] }: { item: Item; abstract?: string; terms?: string[] }) {
  if (!abstract) return <Hl text={item.summary} terms={terms} />;
  return (
    <>
      <Sparkles size={11} strokeWidth={2} className="mr-1 inline-block align-[-1px] text-accent" />
      {abstract}
    </>
  );
}

/** 截成 `lines` 行，多出来的用省略号。 */
const clampLines = (lines: number): CSSProperties => ({
  display: "-webkit-box",
  WebkitBoxOrient: "vertical",
  WebkitLineClamp: lines,
  overflow: "hidden",
});

/** 摘要视图：标题、几行摘要、来源，有图时右边一张缩略图。 */
function SummaryRow(props: RowProps) {
  const { item, group, untitled, look } = props;
  const unread = group ? group.unread > 0 : !item.read;
  return (
    <>
      <UnreadDot unread={unread} />
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <span className={cx("truncate", unread ? "font-semibold text-fg" : "text-fg-2")} style={text(look.m.bigTitle)}>
          <Hl text={item.title || untitled} terms={look.terms} />
        </span>
        {look.style.showSummary && (
          <p
            className="text-fg-3"
            style={{
              ...text(look.m.summary),
              ...clampLines(look.style.summaryLines),
              height: look.m.summary.line * look.style.summaryLines,
            }}
          >
            <Excerpt item={item} abstract={props.abstract} terms={look.terms} />
          </p>
        )}
        <SourceLine {...props} />
      </div>
      {look.style.showImage && item.image && (
        <Picture
          src={item.image}
          className="shrink-0 rounded-md bg-active object-cover"
          style={{ width: THUMB_SIZE, height: THUMB_SIZE }}
        />
      )}
    </>
  );
}

/** 卡片视图里的一张。没有图的把位置留给摘要。 */
function Card(props: RowProps & { imageHeight: number }) {
  const { item, group, untitled, imageHeight, look } = props;
  const unread = group ? group.unread > 0 : !item.read;
  const [broken, setBroken] = useState(false);
  useEffect(() => setBroken(false), [item.image]);
  const picture = look.style.showImage && item.image && !broken;
  const padY = look.m.padY + 2;
  const padX = look.m.padX + 2;
  return (
    <div className="flex h-full flex-col">
      {picture && (
        <img
          src={item.image ?? ""}
          alt=""
          loading="lazy"
          decoding="async"
          referrerPolicy="no-referrer"
          draggable={false}
          onError={() => setBroken(true)}
          className="w-full shrink-0 bg-active object-cover"
          style={{ height: imageHeight }}
        />
      )}
      <div
        className="flex min-h-0 flex-1 flex-col gap-1.5"
        style={{ padding: `${padY}px ${padX}px` }}
      >
        <div className="flex h-5 shrink-0 items-center gap-1.5">
          <UnreadDot unread={unread} />
          <div className="min-w-0 flex-1">
            <SourceLine {...props} />
          </div>
        </div>
        <span
          className={cx("shrink-0", unread ? "font-semibold text-fg" : "text-fg-2")}
          style={{ ...text(look.m.bigTitle), ...clampLines(picture || !look.style.showSummary ? 2 : 3) }}
        >
          <Hl text={item.title || untitled} terms={look.terms} />
        </span>
        {look.style.showSummary && (
          <p
            className={cx(
              "min-h-0 overflow-hidden text-fg-3",
              !picture && "flex-1 [mask-image:linear-gradient(to_bottom,black_70%,transparent)]",
            )}
            style={{ ...text(look.m.summary), ...(picture ? clampLines(2) : {}) }}
          >
            <Excerpt item={item} abstract={props.abstract} terms={look.terms} />
          </p>
        )}
      </div>
    </div>
  );
}

function EmptyList({
  search,
  noSources,
  caughtUp,
  folderHint,
  onAddAccount,
  onAddFeed,
}: {
  /** 搜索没有结果。 */
  search?: { query: string; all: boolean; onAll: () => void };
  noSources: boolean;
  caughtUp: boolean;
  /** 垃圾邮件和已删除：提醒服务器可能没让邮件应用看到这个文件夹。 */
  /** 邮箱文件夹空着时的说明。 */
  folderHint: "list.folderHint" | "list.draftsHint" | null;
  onAddAccount: () => void;
  onAddFeed: () => void;
}) {
  const t = useT();
  if (search) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 px-8 text-center">
        <Search size={26} strokeWidth={1.25} className="text-fg-4" />
        <div>
          <p className="text-fg-2">{t("search.empty", { query: search.query })}</p>
          <p className="mt-1 max-w-[260px] text-[12px] leading-relaxed text-fg-3">{t("search.emptyHint")}</p>
        </div>
        {!search.all && <Button onClick={search.onAll}>{t("search.tryAll")}</Button>}
      </div>
    );
  }
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 px-8 text-center">
      <Inbox size={28} strokeWidth={1.25} className="text-fg-4" />
      {noSources ? (
        <>
          <div>
            <p className="font-medium text-fg-2">{t("list.noSourcesTitle")}</p>
            <p className="mt-1 max-w-[260px] text-[12px] leading-relaxed text-fg-3">{t("list.noSourcesBody")}</p>
          </div>
          <div className="flex gap-2">
            <Button variant="primary" onClick={onAddAccount}>
              <Mail size={14} />
              {t("action.addAccount")}
            </Button>
            <Button onClick={onAddFeed}>
              <Rss size={14} />
              {t("action.addFeed")}
            </Button>
          </div>
        </>
      ) : (
        <div>
          <p className="text-fg-3">{caughtUp ? t("list.caughtUp") : t("list.empty")}</p>
          {folderHint && (
            <p className="mt-1 max-w-[280px] text-[12px] leading-relaxed text-fg-3">{t(folderHint)}</p>
          )}
        </div>
      )}
    </div>
  );
}
