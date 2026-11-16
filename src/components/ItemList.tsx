import { useVirtualizer } from "@tanstack/react-virtual";
import {
  Archive,
  ArchiveRestore,
  Check,
  CheckCheck,
  ChevronDown,
  ChevronRight,
  Inbox,
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
  Rows3,
  Rss,
  Star,
  X,
} from "lucide-react";
import { type CSSProperties, type DragEvent, type MouseEvent, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useActions } from "../lib/actions";
import type { Item, ItemKind } from "../lib/api";
import { endDrag, startDrag } from "../lib/dnd";
import type { Group, GroupMode, Row } from "../lib/grouping";
import { type Translate, useLang, useT } from "../lib/i18n";
import { useListModel } from "../lib/listModel";
import { isMac } from "../lib/platform";
import { useNav } from "../lib/queries";
import { CARD_MAX, CARD_MIN, type ListScope, type ListView, listKey, useListView, useUi } from "../lib/store";
import { shortTime } from "../lib/time";
import { Avatar, Button, cx, Favicon, ICON, IconButton, Menu, MenuItem, Spinner, Tip } from "./ui";

const ROW_HEIGHT = { compact: 34, comfortable: 58 } as const;
/** 摘要视图一行的高度：标题、两行摘要、来源。 */
const SUMMARY_HEIGHT = 98;
/** 展开的组的标题行。 */
const HEADER_HEIGHT = 30;
/** 卡片之间的间距，以及卡片下半部分文字区的高度。 */
const CARD_GAP = 10;
const CARD_TEXT = 128;
/** 每个列表上次滚到的位置，只在这次打开应用期间记着。 */
const scrollPositions = new Map<string, number>();

/** 滚动到离末尾还剩这么多条时开始加载下一页。 */
const LOAD_AHEAD = 30;

/**
 * 虚拟列表里的一行：展开的组的标题、一条（或折叠的一组），或者卡片视图里的一排卡片。
 */
type Line =
  | { kind: "header"; group: Group }
  | { kind: "row"; row: Row }
  | { kind: "cards"; rows: Row[] };

function linesOf(rows: Row[], view: ListView, columns: number): Line[] {
  if (view !== "cards") {
    return rows.map((row) =>
      row.kind === "group" && row.expanded ? { kind: "header", group: row.group } : { kind: "row", row },
    );
  }
  const lines: Line[] = [];
  let current: Row[] = [];
  const flush = () => {
    if (current.length > 0) lines.push({ kind: "cards", rows: current });
    current = [];
  };
  for (const row of rows) {
    if (row.kind === "group" && row.expanded) {
      flush();
      lines.push({ kind: "header", group: row.group });
      continue;
    }
    // 展开的组里的卡片自成几排，不和组外的混在一排
    const child = row.kind === "item" && row.child;
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
  if (line.kind === "header") return [];
  return (line.kind === "row" ? [line.row] : line.rows).map(rowId);
}

const VIEW_OPTIONS: { value: ListView; icon: LucideIcon; label: "view.list" | "view.summary" | "view.cards" }[] = [
  { value: "list", icon: List, label: "view.list" },
  { value: "summary", icon: Rows3, label: "view.summary" },
  { value: "cards", icon: LayoutGrid, label: "view.cards" },
];

/** 工具栏里切换显示方式：列表、摘要、卡片。卡片视图下还能调大小。 */
function ViewControl() {
  const t = useT();
  const scope = useUi((s) => s.scope);
  const view = useListView();
  const setView = useUi((s) => s.setListView);
  const cardSize = useUi((s) => s.cardSize);
  const setCardSize = useUi((s) => s.setCardSize);
  const Icon = VIEW_OPTIONS.find((o) => o.value === view)?.icon ?? List;
  return (
    <Menu
      align="end"
      trigger={
        <button
          type="button"
          aria-label={t("view.label")}
          title={t("view.label")}
          className="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-fg-2 transition-colors hover:bg-hover hover:text-fg data-[state=open]:bg-hover"
        >
          <Icon {...ICON} />
        </button>
      }
    >
      <div className="px-2 pb-1 pt-1.5 text-[12px] font-medium text-fg-3">{t("view.label")}</div>
      {VIEW_OPTIONS.map(({ value, icon: OptionIcon, label }) => (
        <MenuItem key={value} icon={<OptionIcon size={14} />} onSelect={() => setView(scope, value)}>
          <span className="flex-1">{t(label)}</span>
          {view === value && <Check size={14} className="ml-3 text-fg-2" />}
        </MenuItem>
      ))}
      {view === "cards" && (
        <div className="mt-1 border-t border-line px-2 pb-1.5 pt-2">
          <div className="flex items-center gap-1">
            <span className="flex-1 text-[12px] text-fg-2">{t("view.cardSize")}</span>
            <button
              type="button"
              aria-label={t("view.smaller")}
              disabled={cardSize <= CARD_MIN}
              onClick={() => setCardSize(cardSize - 40)}
              className="flex size-6 items-center justify-center rounded text-fg-2 hover:bg-hover hover:text-fg disabled:opacity-40"
            >
              <Minus size={13} />
            </button>
            <button
              type="button"
              aria-label={t("view.larger")}
              disabled={cardSize >= CARD_MAX}
              onClick={() => setCardSize(cardSize + 40)}
              className="flex size-6 items-center justify-center rounded text-fg-2 hover:bg-hover hover:text-fg disabled:opacity-40"
            >
              <Plus size={13} />
            </button>
          </div>
          <div className="mt-1 text-[11px] text-fg-3">{t("view.cardSizeHint", { key: isMac ? "⌘" : "Ctrl" })}</div>
        </div>
      )}
    </Menu>
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
  const mode = useUi((s) => s.groupMode);
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
  const { markAllRead, refresh, membersOf } = useActions();
  const { query, items, rows: listRows, stops, groupMode } = useListModel();
  const { isLoading, hasNextPage, isFetchingNextPage, fetchNextPage } = query;
  const view = useListView();
  const cardSize = useUi((s) => s.cardSize);
  const layout = useUi((s) => s.sidebarLayout);
  const kindFilter = useUi((s) => s.kindFilter);
  const here = listKey(scope, kindFilter);

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
  const columns = Math.max(1, Math.floor((inner + CARD_GAP) / (cardSize + CARD_GAP)));
  const cardWidth = Math.floor((inner - CARD_GAP * (columns - 1)) / columns);
  const imageHeight = Math.round(cardWidth * 0.56);
  const cardHeight = imageHeight + CARD_TEXT;

  const lines = useMemo(() => linesOf(listRows, view, columns), [listRows, view, columns]);
  const sizeOf = (line: Line | undefined) =>
    !line
      ? 0
      : line.kind === "header"
        ? HEADER_HEIGHT
        : line.kind === "cards"
          ? cardHeight + CARD_GAP
          : view === "summary"
            ? SUMMARY_HEIGHT
            : ROW_HEIGHT[density];
  const virtualizer = useVirtualizer({
    count: lines.length,
    getScrollElement: () => scroller.current,
    estimateSize: (i) => sizeOf(lines[i]),
    overscan: view === "cards" ? 3 : 12,
  });

  useEffect(() => virtualizer.measure(), [density, view, cardHeight, columns, virtualizer]);

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

  // 拖到侧栏的视图上：勾选着的就拖勾选的那些，否则拖这一行代表的条目
  const onDragStart = (e: DragEvent, id: string, label: string) => {
    const ui = useUi.getState();
    const members = membersOf(id);
    const ids = members.every((m) => ui.selection.includes(m)) ? ui.selection : members;
    startDrag(e, { kind: "items", ids }, label);
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
        <ViewControl />
        <GroupControl />
        <IconButton label={t("action.markAllRead")} shortcut="⇧A" disabled={!hasUnread} onClick={markAllRead}>
          <CheckCheck {...ICON} />
        </IconButton>
        <IconButton label={t("action.refresh")} onClick={refresh}>
          <RefreshCw {...ICON} className={cx(refreshing > 0 && "animate-spin")} />
        </IconButton>
      </header>

      <div ref={scroller} role="listbox" aria-label={title} aria-multiselectable className="min-h-0 flex-1 overflow-y-auto">
        {isLoading ? (
          <div className="flex h-full items-center justify-center text-fg-3">
            <Spinner size={16} />
          </div>
        ) : items.length === 0 ? (
          <EmptyList
            noSources={feeds !== undefined && feeds.length === 0 && accounts !== undefined && accounts.length === 0}
            caughtUp={scope.kind === "view" && (scope.view === "inbox" || scope.view === "unread")}
            onAddAccount={() => setDialog({ type: "add-account" })}
            onAddFeed={() => setDialog({ type: "add-feed" })}
          />
        ) : (
          <div className="relative mx-1.5 my-1.5" style={{ height: virtualizer.getTotalSize() }}>
            {virtualRows.map((virtual) => {
              const line = lines[virtual.index];
              if (!line) return null;
              const position = { height: virtual.size - 1, transform: `translateY(${virtual.start}px)` };

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
                    style={position}
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
                const item = row.kind === "item" ? row.item : row.group.lead;
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
                };
                return {
                  id,
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
                      columnGap: CARD_GAP,
                    }}
                  >
                    {line.rows.map((row) => {
                      const d = describe(row);
                      return (
                        <div
                          key={d.key}
                          role="option"
                          aria-selected={d.opened || d.selected}
                          onClick={(e) => onRowClick(e, d.id)}
                          {...d.drag}
                          className={cx(
                            "overflow-hidden rounded-lg border transition-colors",
                            d.selected
                              ? "border-accent/60 bg-accent-soft"
                              : d.opened
                                ? "border-accent/60 bg-active"
                                : "border-line bg-surface hover:border-line-strong hover:bg-hover",
                          )}
                          style={{ height: cardHeight }}
                        >
                          <Card {...d.props} imageHeight={imageHeight} />
                        </div>
                      );
                    })}
                  </div>
                );
              }

              const d = describe(line.row);
              return (
                <div
                  key={d.key}
                  role="option"
                  aria-selected={d.opened || d.selected}
                  onClick={(e) => onRowClick(e, d.id)}
                  {...d.drag}
                  className={cx(
                    "absolute inset-x-0 top-0 flex rounded-md px-2.5 transition-colors",
                    view === "summary"
                      ? "items-center gap-3"
                      : density === "compact"
                        ? "items-center gap-2.5"
                        : "flex-col justify-center gap-1",
                    d.child && "ml-5",
                    d.selected ? "bg-accent-soft" : d.opened ? "bg-active" : "hover:bg-hover",
                    d.selected && d.opened && "ring-1 ring-inset ring-accent/50",
                  )}
                  style={position}
                >
                  {view === "summary" ? (
                    <SummaryRow {...d.props} />
                  ) : density === "compact" ? (
                    <CompactRow {...d.props} />
                  ) : (
                    <ComfortableRow {...d.props} />
                  )}
                </div>
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
}

function Sender({ item, size = 16 }: { item: Item; size?: number }) {
  if (item.kind === "mail") {
    return <Avatar name={item.author || item.sender_address || "?"} seed={item.sender_address} size={size} />;
  }
  return <Favicon src={item.source_icon} size={size} />;
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

function Trailing({ item, group, time, onExpand, expandLabel }: RowProps) {
  const attachments = group ? group.hasAttachments : item.has_attachments;
  const starred = group ? group.starred : item.starred;
  return (
    <>
      {group && onExpand && <CountPill count={group.items.length} onExpand={onExpand} label={expandLabel} />}
      {attachments && <Paperclip size={12} className="shrink-0 text-fg-3" />}
      {starred && <Star size={12} className="shrink-0 fill-warning text-warning" />}
      <span className="tabular shrink-0 text-[12px] text-fg-3">{time}</span>
    </>
  );
}

/** 邮件这一行左边显示谁：单封是发件人，一个会话是参与的人。 */
function senderLabel(item: Item, group: Group | undefined, groupMode: GroupMode): string {
  if (group && groupMode === "thread") return group.people.join(", ");
  return item.author || item.sender_address || "";
}

function CompactRow(props: RowProps) {
  const { item, group, groupMode, untitled } = props;
  const unread = group ? group.unread > 0 : !item.read;
  const unreadText = unread ? "font-medium text-fg" : "text-fg-3";
  return (
    <>
      <UnreadDot unread={unread} />
      <Sender item={item} />
      {item.kind === "mail" ? (
        <>
          <span className={cx("w-[110px] shrink-0 truncate", unreadText)}>{senderLabel(item, group, groupMode)}</span>
          <span className={cx("min-w-0 flex-1 truncate", unread ? "text-fg" : "text-fg-3")}>{item.title || untitled}</span>
        </>
      ) : (
        <span className={cx("min-w-0 flex-1 truncate", unreadText)}>{item.title || untitled}</span>
      )}
      <Trailing {...props} />
    </>
  );
}

function ComfortableRow(props: RowProps) {
  const { item, group, groupMode, untitled } = props;
  const unread = group ? group.unread > 0 : !item.read;
  const source = item.kind === "mail" ? senderLabel(item, group, groupMode) : item.source_title;
  return (
    <>
      <div className="flex items-center gap-2.5">
        <UnreadDot unread={unread} />
        <span className={cx("min-w-0 flex-1 truncate", unread ? "font-medium text-fg" : "text-fg-3")}>
          {item.title || untitled}
        </span>
        <Trailing {...props} />
      </div>
      <div className="flex items-center gap-2 pl-4 text-[12px] text-fg-3">
        <Sender item={item} size={14} />
        <span className="min-w-0 truncate">
          <span className="text-fg-2">{source}</span>
          {item.summary && <span> · {item.summary}</span>}
        </span>
      </div>
    </>
  );
}

/** 来源：文章是订阅源的图标和名字，邮件是发件人。 */
function SourceLine(props: RowProps & { size?: number }) {
  const { item, group, groupMode, size = 14 } = props;
  const source = item.kind === "mail" ? senderLabel(item, group, groupMode) : item.source_title;
  return (
    <div className="flex min-w-0 items-center gap-1.5 text-[12px] text-fg-3">
      <Sender item={item} size={size} />
      <span className="min-w-0 flex-1 truncate">{source}</span>
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

/** 摘要视图：标题、两行摘要、来源，有图时右边一张缩略图。 */
function SummaryRow(props: RowProps) {
  const { item, group, untitled } = props;
  const unread = group ? group.unread > 0 : !item.read;
  return (
    <>
      <UnreadDot unread={unread} />
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <span className={cx("truncate text-[14px]", unread ? "font-semibold text-fg" : "text-fg-2")}>
          {item.title || untitled}
        </span>
        <p className="line-clamp-2 text-[12px] leading-[17px] text-fg-3">{item.summary}</p>
        <SourceLine {...props} />
      </div>
      {item.image && (
        <Picture src={item.image} className="size-[72px] shrink-0 rounded-md bg-active object-cover" />
      )}
    </>
  );
}

/** 卡片视图里的一张。没有图的把位置留给摘要。 */
function Card(props: RowProps & { imageHeight: number }) {
  const { item, group, untitled, imageHeight } = props;
  const unread = group ? group.unread > 0 : !item.read;
  const [broken, setBroken] = useState(false);
  useEffect(() => setBroken(false), [item.image]);
  const picture = item.image && !broken;
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
      <div className="flex min-h-0 flex-1 flex-col gap-1.5 px-3 pb-2.5 pt-2.5">
        <div className="flex h-5 shrink-0 items-center gap-1.5">
          <UnreadDot unread={unread} />
          <div className="min-w-0 flex-1">
            <SourceLine {...props} />
          </div>
        </div>
        <span
          className={cx(
            "shrink-0 text-[14px] leading-[19px]",
            picture ? "line-clamp-2" : "line-clamp-3",
            unread ? "font-semibold text-fg" : "text-fg-2",
          )}
        >
          {item.title || untitled}
        </span>
        <p
          className={cx(
            "min-h-0 overflow-hidden text-[12px] leading-[17px] text-fg-3",
            picture ? "line-clamp-2" : "flex-1 [mask-image:linear-gradient(to_bottom,black_70%,transparent)]",
          )}
        >
          {item.summary}
        </p>
      </div>
    </div>
  );
}

function EmptyList({
  noSources,
  caughtUp,
  onAddAccount,
  onAddFeed,
}: {
  noSources: boolean;
  caughtUp: boolean;
  onAddAccount: () => void;
  onAddFeed: () => void;
}) {
  const t = useT();
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
        <p className="text-fg-3">{caughtUp ? t("list.caughtUp") : t("list.empty")}</p>
      )}
    </div>
  );
}
