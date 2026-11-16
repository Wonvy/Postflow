import { useVirtualizer } from "@tanstack/react-virtual";
import {
  Archive,
  ArchiveRestore,
  Check,
  CheckCheck,
  ChevronDown,
  ChevronRight,
  Inbox,
  Layers,
  ListCollapse,
  type LucideIcon,
  Mail,
  MailOpen,
  Paperclip,
  RefreshCw,
  Rss,
  Star,
  X,
} from "lucide-react";
import { type MouseEvent, useEffect, useRef } from "react";
import { useActions } from "../lib/actions";
import type { Item, ItemKind } from "../lib/api";
import type { Group, GroupMode, Row } from "../lib/grouping";
import { type Translate, useLang, useT } from "../lib/i18n";
import { useListModel } from "../lib/listModel";
import { useNav } from "../lib/queries";
import { type ListScope, useUi } from "../lib/store";
import { shortTime } from "../lib/time";
import { Avatar, Button, cx, Favicon, ICON, IconButton, Menu, MenuItem, Spinner, Tip } from "./ui";

const ROW_HEIGHT = { compact: 34, comfortable: 58 } as const;
/** 滚动到离末尾还剩这么多行时开始加载下一页。 */
const LOAD_AHEAD = 30;

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

  const scroller = useRef<HTMLDivElement>(null);
  const anchor = useRef<string | null>(null);
  const rowHeight = ROW_HEIGHT[density];
  const virtualizer = useVirtualizer({
    count: listRows.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => rowHeight,
    overscan: 12,
  });

  useEffect(() => virtualizer.measure(), [rowHeight, virtualizer]);

  // 换了列表就回到顶部
  useEffect(() => {
    scroller.current?.scrollTo({ top: 0 });
    anchor.current = null;
  }, [scope]);

  // 用键盘移动时让打开的那一行保持在视野里
  useEffect(() => {
    if (!openId) return;
    const index = listRows.findIndex((r) => (r.kind === "item" || !r.expanded) && rowId(r) === openId);
    if (index >= 0) virtualizer.scrollToIndex(index, { align: "auto" });
    // 只在打开的条目变化时滚动，列表内容更新时不动
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openId]);

  const virtualRows = virtualizer.getVirtualItems();
  const lastVisible = virtualRows[virtualRows.length - 1]?.index ?? 0;
  useEffect(() => {
    if (hasNextPage && !isFetchingNextPage && lastVisible >= listRows.length - LOAD_AHEAD) {
      void fetchNextPage();
    }
  }, [fetchNextPage, hasNextPage, isFetchingNextPage, listRows.length, lastVisible]);

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
        {scope.kind === "view" && <KindFilter />}
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
              const row = listRows[virtual.index];
              if (!row) return null;
              const position = { height: virtual.size - 1, transform: `translateY(${virtual.start}px)` };

              // 展开的组：一行标题，点一下收起
              if (row.kind === "group" && row.expanded) {
                return (
                  <div
                    key={`h:${row.group.key}`}
                    role="button"
                    tabIndex={-1}
                    onClick={() => setGroupExpanded(row.group.key, false)}
                    className="absolute inset-x-0 top-0 flex items-center gap-2 rounded-md px-2.5 text-[12px] text-fg-3 hover:bg-hover hover:text-fg-2"
                    style={position}
                  >
                    <ChevronDown size={14} strokeWidth={2} className="shrink-0" />
                    <span className="min-w-0 flex-1 truncate font-medium">
                      {groupMode === "thread" ? row.group.lead.title || t("reader.untitled") : row.group.people[0]}
                    </span>
                    <span className="tabular shrink-0">{t("group.count", { n: row.group.items.length })}</span>
                  </div>
                );
              }

              const id = rowId(row);
              const item = row.kind === "item" ? row.item : row.group.lead;
              const group = row.kind === "group" ? row.group : undefined;
              const members = group ? group.items.map((i) => i.id) : [item.id];
              const opened = id === openId;
              const selected = members.every((m) => selection.includes(m));
              const child = row.kind === "item" && row.child;
              const props: RowProps = {
                item,
                group,
                groupMode,
                time: shortTime(item.published_at, lang),
                untitled: t("reader.untitled"),
                onExpand: group ? () => setGroupExpanded(group.key, true) : undefined,
                expandLabel: t("group.expand"),
              };
              return (
                <div
                  key={group ? `g:${group.key}` : item.id}
                  role="option"
                  aria-selected={opened || selected}
                  onClick={(e) => onRowClick(e, id)}
                  className={cx(
                    "absolute inset-x-0 top-0 flex rounded-md px-2.5 transition-colors",
                    density === "compact" ? "items-center gap-2.5" : "flex-col justify-center gap-1",
                    child && "ml-5",
                    selected ? "bg-accent-soft" : opened ? "bg-active" : "hover:bg-hover",
                    selected && opened && "ring-1 ring-inset ring-accent/50",
                  )}
                  style={position}
                >
                  {density === "compact" ? <CompactRow {...props} /> : <ComfortableRow {...props} />}
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
