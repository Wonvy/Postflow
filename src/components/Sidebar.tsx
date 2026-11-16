import { useQueryClient } from "@tanstack/react-query";
import {
  Archive,
  ChevronRight,
  CircleDot,
  Copy,
  Ellipsis,
  FolderPlus,
  Inbox,
  KeyRound,
  type LucideIcon,
  Mail,
  Pencil,
  Plus,
  RefreshCw,
  Rss,
  Settings,
  Star,
  Sun,
  Trash2,
  TriangleAlert,
} from "lucide-react";
import { type DragEvent, type FormEvent, type ReactNode, useEffect, useMemo, useState } from "react";
import { useActions } from "../lib/actions";
import { type Account, api, type Feed, type ItemPatch, type Nav, type View } from "../lib/api";
import { type DragPayload, type DropPosition, dropPosition, endDrag, isOurs, moveInList, startDrag, useDrag } from "../lib/dnd";
import { useFolderActions } from "../lib/folders";
import { hasProviderIcon, ProviderIcon, providerOf } from "./ProviderIcon";
import { UpdatePill } from "./UpdateDialog";
import { errorText, useT } from "../lib/i18n";
import { copyText, hasInsetTitlebar } from "../lib/platform";
import { keys, useNav, usePresets } from "../lib/queries";
import { type ListScope, sameScope, useUi } from "../lib/store";
import {
  Avatar,
  Button,
  cx,
  Favicon,
  Field,
  ICON,
  IconButton,
  Logo,
  Menu,
  MenuItem,
  MenuSeparator,
  Modal,
  Spinner,
  TextInput,
  Tip,
} from "./ui";

const VIEWS: { view: View; icon: LucideIcon; label: `nav.${View}` }[] = [
  { view: "inbox", icon: Inbox, label: "nav.inbox" },
  { view: "today", icon: Sun, label: "nav.today" },
  { view: "unread", icon: CircleDot, label: "nav.unread" },
  { view: "starred", icon: Star, label: "nav.starred" },
  { view: "archived", icon: Archive, label: "nav.archived" },
];

/** 拖放用到的事件，挂在一行上。 */
type DragHandlers = {
  draggable?: boolean;
  onDragStart?: (e: DragEvent) => void;
  onDragEnd?: () => void;
  onDragOver?: (e: DragEvent) => void;
  onDragLeave?: (e: DragEvent) => void;
  onDrop?: (e: DragEvent) => void;
};

function Row({
  active,
  onClick,
  indent = false,
  leading,
  label,
  trailing,
  drag,
  drop,
}: {
  active: boolean;
  onClick: () => void;
  indent?: boolean;
  leading: ReactNode;
  label: string;
  trailing?: ReactNode;
  drag?: DragHandlers;
  /** 拖到这一行上时放下的位置：上方、下方，或者放进去。 */
  drop?: DropPosition | null;
}) {
  return (
    <div
      role="button"
      tabIndex={0}
      aria-current={active || undefined}
      onClick={onClick}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onClick();
        }
      }}
      {...drag}
      className={cx(
        "group relative flex h-7 items-center gap-2 rounded-md pr-1.5 outline-offset-[-2px] transition-colors",
        indent ? "pl-[26px]" : "pl-2",
        drop === "inside"
          ? "bg-accent-soft text-fg ring-1 ring-inset ring-accent/60"
          : active
            ? "bg-active text-fg"
            : "text-fg-2 hover:bg-hover hover:text-fg",
      )}
    >
      {(drop === "before" || drop === "after") && (
        <span
          aria-hidden
          className={cx(
            "pointer-events-none absolute right-1 h-0.5 rounded-full bg-accent",
            indent ? "left-[26px]" : "left-2",
            drop === "before" ? "-top-px" : "-bottom-px",
          )}
        />
      )}
      <span className={cx("flex size-4 shrink-0 items-center justify-center", active ? "text-fg" : "text-fg-3")}>
        {leading}
      </span>
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {trailing}
    </div>
  );
}

function Count({ n }: { n: number }) {
  if (n <= 0) return null;
  return <span className="tabular shrink-0 px-1 text-[12px] text-fg-3">{n > 999 ? "999+" : n}</span>;
}

function FeedRow({
  feed,
  indent,
  active,
  drag,
  drop,
}: {
  feed: Feed;
  indent: boolean;
  active: boolean;
  drag: DragHandlers;
  drop: DropPosition | null;
}) {
  const t = useT();
  const qc = useQueryClient();
  const setScope = useUi((s) => s.setScope);
  const setDialog = useUi((s) => s.setDialog);

  const unsubscribe = async () => {
    await api.removeFeed(feed.id);
    if (active) setScope({ kind: "view", view: "inbox" });
    void qc.invalidateQueries({ queryKey: keys.nav });
    void qc.invalidateQueries({ queryKey: ["items"] });
  };

  return (
    <Row
      active={active}
      indent={indent}
      onClick={() => setScope({ kind: "feed", feedId: feed.id })}
      drag={drag}
      drop={drop}
      leading={<Favicon src={feed.icon_url} />}
      label={feed.title || feed.url}
      trailing={
        <>
          {feed.error ? (
            <Tip label={`${t("edit.lastError")}: ${feed.error}`} side="right">
              <span className="flex size-5 shrink-0 items-center justify-center text-warning group-hover:hidden">
                <TriangleAlert size={13} strokeWidth={2} />
              </span>
            </Tip>
          ) : (
            <span className="group-hover:hidden group-has-[[data-state=open]]:hidden">
              <Count n={feed.unread} />
            </span>
          )}
          <Menu
            align="start"
            trigger={
              <button
                type="button"
                aria-label={t("action.more")}
                onClick={(e) => e.stopPropagation()}
                onKeyDown={(e) => e.stopPropagation()}
                className="hidden size-5 shrink-0 items-center justify-center rounded text-fg-3 hover:bg-active hover:text-fg group-hover:flex data-[state=open]:flex"
              >
                <Ellipsis size={14} strokeWidth={2} />
              </button>
            }
          >
            <MenuItem icon={<Pencil size={14} />} onSelect={() => setDialog({ type: "edit-feed", feedId: feed.id })}>
              {t("action.edit")}
            </MenuItem>
            <MenuItem icon={<RefreshCw size={14} />} onSelect={() => void api.refresh(feed.id)}>
              {t("action.refresh")}
            </MenuItem>
            <MenuItem icon={<Copy size={14} />} onSelect={() => void copyText(feed.url)}>
              {t("action.copyUrl")}
            </MenuItem>
            <MenuSeparator />
            <MenuItem danger icon={<Trash2 size={14} />} onSelect={() => void unsubscribe()}>
              {t("action.unsubscribe")}
            </MenuItem>
          </Menu>
        </>
      }
    />
  );
}

function AccountRow({
  account,
  active,
  drag,
  drop,
}: {
  account: Account;
  active: boolean;
  drag: DragHandlers;
  drop: DropPosition | null;
}) {
  const t = useT();
  const setScope = useUi((s) => s.setScope);
  const setDialog = useUi((s) => s.setDialog);
  const presets = usePresets();
  const label = account.name || account.email;
  const needsPassword = account.error_code === "mail_no_password";
  const provider = providerOf(account, presets.data);

  return (
    <Row
      active={active}
      onClick={() => setScope({ kind: "account", accountId: account.id })}
      drag={drag}
      drop={drop}
      leading={
        hasProviderIcon(provider) ? (
          <ProviderIcon id={provider} size={16} />
        ) : (
          <Avatar name={label} seed={account.email} />
        )
      }
      label={label}
      trailing={
        <>
          {account.error_code ? (
            <Tip
              label={needsPassword ? t("account.needsPassword") : errorText(t, { code: account.error_code })}
              side="right"
            >
              <button
                type="button"
                aria-label={t("action.accountSettings")}
                onClick={(e) => {
                  e.stopPropagation();
                  setDialog({ type: "edit-account", accountId: account.id });
                }}
                className="flex size-5 shrink-0 items-center justify-center text-warning group-hover:hidden"
              >
                {needsPassword ? <KeyRound size={13} strokeWidth={2} /> : <TriangleAlert size={13} strokeWidth={2} />}
              </button>
            </Tip>
          ) : (
            <span className="group-hover:hidden group-has-[[data-state=open]]:hidden">
              <Count n={account.unread} />
            </span>
          )}
          <Menu
            align="start"
            trigger={
              <button
                type="button"
                aria-label={t("action.more")}
                onClick={(e) => e.stopPropagation()}
                onKeyDown={(e) => e.stopPropagation()}
                className="hidden size-5 shrink-0 items-center justify-center rounded text-fg-3 hover:bg-active hover:text-fg group-hover:flex data-[state=open]:flex"
              >
                <Ellipsis size={14} strokeWidth={2} />
              </button>
            }
          >
            <MenuItem
              icon={<Pencil size={14} />}
              onSelect={() => setDialog({ type: "edit-account", accountId: account.id })}
            >
              {t("action.accountSettings")}
            </MenuItem>
            <MenuItem icon={<RefreshCw size={14} />} onSelect={() => void api.syncAccount(account.id)}>
              {t("action.syncNow")}
            </MenuItem>
            <MenuItem icon={<Copy size={14} />} onSelect={() => void copyText(account.email)}>
              {account.email}
            </MenuItem>
            <MenuSeparator />
            <MenuItem
              danger
              icon={<Trash2 size={14} />}
              onSelect={() => setDialog({ type: "remove-account", accountId: account.id })}
            >
              {t("action.removeAccount")}
            </MenuItem>
          </Menu>
        </>
      }
    />
  );
}

function SectionHeader({
  label,
  addLabel,
  onAdd,
  drag,
  highlight = false,
  extra,
}: {
  label: string;
  addLabel: string;
  onAdd: () => void;
  drag?: DragHandlers;
  highlight?: boolean;
  extra?: ReactNode;
}) {
  return (
    <div
      {...drag}
      className={cx(
        "group/section mb-1 mt-5 flex h-6 items-center justify-between rounded-md pl-2 pr-0.5 transition-colors",
        highlight && "bg-accent-soft ring-1 ring-inset ring-accent/60",
      )}
    >
      <span className="text-[12px] font-medium text-fg-3">{label}</span>
      <span className="flex-1" />
      {extra}
      <Tip label={addLabel} side="right">
        <button
          type="button"
          aria-label={addLabel}
          onClick={onAdd}
          className="flex size-5 items-center justify-center rounded text-fg-3 opacity-0 hover:bg-active hover:text-fg focus-visible:opacity-100 group-hover/section:opacity-100"
        >
          <Plus size={14} strokeWidth={2} />
        </button>
      </Tip>
    </div>
  );
}

export function Sidebar({ width }: { width: number }) {
  const t = useT();
  const nav = useNav();
  const scope = useUi((s) => s.scope);
  const setScope = useUi((s) => s.setScope);
  const setDialog = useUi((s) => s.setDialog);
  const refreshing = useUi((s) => s.refreshing);
  const collapsed = useUi((s) => s.collapsedFolders);
  const toggleFolder = useUi((s) => s.toggleFolder);
  const qc = useQueryClient();
  const { applyTo } = useActions();
  const folderActions = useFolderActions();

  const feeds = nav.data?.feeds;
  const accounts = nav.data?.accounts;
  const counts = nav.data?.counts;

  const { folders, loose } = useMemo(() => {
    const byFolder = new Map<string, Feed[]>(folderActions.folders.map((name) => [name, []]));
    const loose: Feed[] = [];
    for (const feed of feeds ?? []) {
      if (!feed.folder) {
        loose.push(feed);
        continue;
      }
      const list = byFolder.get(feed.folder);
      if (list) list.push(feed);
      else byFolder.set(feed.folder, [feed]);
    }
    return { folders: [...byFolder.entries()], loose };
  }, [feeds, folderActions.folders]);

  // ---- 拖放 ----
  const [drop, setDrop] = useState<{ key: string; position: DropPosition } | null>(null);
  const dragging = useDrag((s) => s.payload);
  useEffect(() => {
    if (!dragging) setDrop(null);
  }, [dragging]);
  const dropAt = (key: string) => (drop?.key === key ? drop.position : null);

  /** 一个放置目标：`decide` 判断这次拖的东西能不能放、放在哪；`act` 真正执行。 */
  const target = (
    key: string,
    decide: (payload: DragPayload, e: DragEvent) => DropPosition | null,
    act: (payload: DragPayload, position: DropPosition) => void,
  ): DragHandlers => ({
    onDragOver: (e) => {
      const payload = useDrag.getState().payload;
      if (!payload || !isOurs(e)) return;
      const position = decide(payload, e);
      if (!position) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = "move";
      setDrop((d) => (d?.key === key && d.position === position ? d : { key, position }));
    },
    onDragLeave: (e) => {
      if (e.currentTarget.contains(e.relatedTarget as Node | null)) return;
      setDrop((d) => (d?.key === key ? null : d));
    },
    onDrop: (e) => {
      const payload = useDrag.getState().payload;
      setDrop(null);
      if (!payload) return;
      const position = decide(payload, e);
      if (!position) return;
      e.preventDefault();
      act(payload, position);
      endDrag();
    },
  });

  const source = (payload: DragPayload, label: string): DragHandlers => ({
    draggable: true,
    onDragStart: (e) => startDrag(e, payload, label),
    onDragEnd: endDrag,
  });

  /** 把订阅源放到某个源的前后，或者放进文件夹的末尾。 */
  const moveFeed = (id: string, folder: string, anchor: string | null, position: DropPosition) => {
    const inside = folderActions.feedsIn(folder);
    const order =
      anchor && position !== "inside"
        ? moveInList(inside, id, anchor, position)
        : [...inside.filter((x) => x !== id), id];
    folderActions.place(folder, order);
  };

  const feedTarget = (feed: Feed) =>
    target(
      `feed:${feed.id}`,
      (p, e) => (p.kind === "feed" && p.id !== feed.id ? dropPosition(e) : null),
      (p, position) => p.kind === "feed" && moveFeed(p.id, feed.folder, feed.id, position),
    );

  const folderTarget = (name: string) =>
    target(
      `folder:${name}`,
      (p, e) =>
        p.kind === "feed"
          ? p.folder === name
            ? null
            : "inside"
          : p.kind === "folder" && p.name !== name
            ? dropPosition(e)
            : null,
      (p, position) => {
        if (p.kind === "feed") moveFeed(p.id, name, null, "inside");
        else if (p.kind === "folder" && position !== "inside") {
          folderActions.orderFolders(moveInList(folderActions.folders, p.name, name, position));
        }
      },
    );

  // 拖到「订阅」标题上：移出文件夹
  const feedsSectionTarget = target(
    "section:feeds",
    (p) => (p.kind === "feed" && p.folder !== "" ? "inside" : null),
    (p) => p.kind === "feed" && moveFeed(p.id, "", null, "inside"),
  );

  const accountTarget = (account: Account) =>
    target(
      `account:${account.id}`,
      (p, e) => (p.kind === "account" && p.id !== account.id ? dropPosition(e) : null),
      (p, position) => {
        if (p.kind !== "account" || position === "inside" || !accounts) return;
        const order = moveInList(
          accounts.map((a) => a.id),
          p.id,
          account.id,
          position,
        );
        qc.setQueryData<Nav>(keys.nav, (old) =>
          old
            ? { ...old, accounts: order.map((id) => old.accounts.find((a) => a.id === id)).filter((a): a is Account => !!a) }
            : old,
        );
        api.reorderAccounts(order).catch(() => void qc.invalidateQueries({ queryKey: keys.nav }));
      },
    );

  // 把列表里的条目拖到视图上：收件箱是移回收件箱，星标是加星标，已归档是归档，未读是标为未读
  const VIEW_DROP: Partial<Record<View, ItemPatch>> = {
    inbox: { archived: false },
    unread: { read: false },
    starred: { starred: true },
    archived: { archived: true },
  };
  const viewTarget = (view: View) => {
    const patch = VIEW_DROP[view];
    return patch
      ? target(
          `view:${view}`,
          (p) => (p.kind === "items" ? "inside" : null),
          (p) => p.kind === "items" && applyTo(p.ids, patch),
        )
      : undefined;
  };

  const isActive = (target: ListScope) => sameScope(scope, target);
  const countFor = (view: View) =>
    view === "inbox" ? counts?.unread : view === "today" ? counts?.today : undefined;

  return (
    <aside className="flex h-full shrink-0 flex-col bg-sidebar" style={{ width }}>
      <header
        data-tauri-drag-region
        className={cx("flex h-11 shrink-0 items-center gap-0.5 pr-2", hasInsetTitlebar ? "pl-[78px]" : "pl-3.5")}
      >
        {!hasInsetTitlebar && (
          <div className="pointer-events-none flex items-center gap-2">
            <Logo />
            <span className="text-[13px] font-semibold tracking-tight">Postflow</span>
          </div>
        )}
        <div className="flex-1" data-tauri-drag-region />
        <IconButton label={t("action.refreshAll")} shortcut="R" onClick={() => void api.refresh()}>
          <RefreshCw {...ICON} className={cx(refreshing > 0 && "animate-spin")} />
        </IconButton>
        <Menu
          align="end"
          trigger={
            <button
              type="button"
              aria-label={t("action.addMenu")}
              className="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-fg-2 transition-colors hover:bg-hover hover:text-fg data-[state=open]:bg-hover data-[state=open]:text-fg"
            >
              <Plus {...ICON} />
            </button>
          }
        >
          <MenuItem icon={<Mail size={14} />} onSelect={() => setDialog({ type: "add-account" })}>
            {t("action.addAccount")}
          </MenuItem>
          <MenuItem icon={<Rss size={14} />} onSelect={() => setDialog({ type: "add-feed" })}>
            {t("action.addFeed")}
          </MenuItem>
          <MenuItem icon={<FolderPlus size={14} />} onSelect={() => setDialog({ type: "folder" })}>
            {t("folder.new")}
          </MenuItem>
        </Menu>
      </header>

      <nav className="min-h-0 flex-1 overflow-y-auto px-2 pb-3">
        <div className="flex flex-col gap-px">
          {VIEWS.map(({ view, icon: Icon, label }) => (
            <Row
              key={view}
              active={isActive({ kind: "view", view })}
              onClick={() => setScope({ kind: "view", view })}
              drag={viewTarget(view)}
              drop={dropAt(`view:${view}`)}
              leading={<Icon {...ICON} />}
              label={t(label)}
              trailing={<Count n={countFor(view) ?? 0} />}
            />
          ))}
        </div>

        <SectionHeader
          label={t("nav.mail")}
          addLabel={t("action.addAccount")}
          onAdd={() => setDialog({ type: "add-account" })}
        />
        <div className="flex flex-col gap-px">
          {accounts?.map((account) => (
            <AccountRow
              key={account.id}
              account={account}
              active={isActive({ kind: "account", accountId: account.id })}
              drag={{ ...source({ kind: "account", id: account.id }, account.name || account.email), ...accountTarget(account) }}
              drop={dropAt(`account:${account.id}`)}
            />
          ))}
          {accounts && accounts.length === 0 && (
            <button
              type="button"
              onClick={() => setDialog({ type: "add-account" })}
              className="flex h-7 items-center gap-2 rounded-md pl-2 text-fg-3 hover:bg-hover hover:text-fg"
            >
              <Plus {...ICON} />
              {t("action.addAccount")}
            </button>
          )}
        </div>

        <SectionHeader
          label={t("nav.feeds")}
          addLabel={t("action.addFeed")}
          onAdd={() => setDialog({ type: "add-feed" })}
          drag={feedsSectionTarget}
          highlight={dropAt("section:feeds") !== null}
          extra={
            <Tip label={t("folder.new")} side="right">
              <button
                type="button"
                aria-label={t("folder.new")}
                onClick={() => setDialog({ type: "folder" })}
                className="flex size-5 items-center justify-center rounded text-fg-3 opacity-0 hover:bg-active hover:text-fg focus-visible:opacity-100 group-hover/section:opacity-100"
              >
                <FolderPlus size={14} strokeWidth={2} />
              </button>
            </Tip>
          }
        />

        <div className="flex flex-col gap-px">
          {folders.map(([folder, list]) => {
            const open = !collapsed.includes(folder);
            return (
              <div key={folder} className="flex flex-col gap-px">
                <Row
                  active={isActive({ kind: "folder", folder })}
                  onClick={() => setScope({ kind: "folder", folder })}
                  drag={{ ...source({ kind: "folder", name: folder }, folder), ...folderTarget(folder) }}
                  drop={dropAt(`folder:${folder}`)}
                  leading={
                    <button
                      type="button"
                      aria-expanded={open}
                      aria-label={folder}
                      onClick={(e) => {
                        e.stopPropagation();
                        toggleFolder(folder);
                      }}
                      className="flex size-5 items-center justify-center rounded text-fg-3 hover:bg-active hover:text-fg"
                    >
                      <ChevronRight
                        size={14}
                        strokeWidth={2}
                        className={cx("transition-transform", open && "rotate-90")}
                      />
                    </button>
                  }
                  label={folder}
                  trailing={<FolderTrailing folder={folder} unread={list.reduce((sum, f) => sum + f.unread, 0)} />}
                />
                {open &&
                  list.map((feed) => (
                    <FeedRow
                      key={feed.id}
                      feed={feed}
                      indent
                      active={isActive({ kind: "feed", feedId: feed.id })}
                      drag={{ ...source({ kind: "feed", id: feed.id, folder: feed.folder }, feed.title), ...feedTarget(feed) }}
                      drop={dropAt(`feed:${feed.id}`)}
                    />
                  ))}
              </div>
            );
          })}
          {loose.map((feed) => (
            <FeedRow
              key={feed.id}
              feed={feed}
              indent={false}
              active={isActive({ kind: "feed", feedId: feed.id })}
              drag={{ ...source({ kind: "feed", id: feed.id, folder: "" }, feed.title), ...feedTarget(feed) }}
              drop={dropAt(`feed:${feed.id}`)}
            />
          ))}
          {feeds && feeds.length === 0 && (
            <button
              type="button"
              onClick={() => setDialog({ type: "add-feed" })}
              className="flex h-7 items-center gap-2 rounded-md pl-2 text-fg-3 hover:bg-hover hover:text-fg"
            >
              <Plus {...ICON} />
              {t("action.addFeed")}
            </button>
          )}
        </div>
      </nav>

      <footer className="flex h-10 shrink-0 items-center gap-1 border-t border-line px-2">
        <IconButton label={t("action.settings")} tipSide="top" onClick={() => setDialog({ type: "settings" })}>
          <Settings {...ICON} />
        </IconButton>
        {refreshing > 0 && (
          <span className="flex min-w-0 items-center gap-1.5 text-[12px] text-fg-3">
            <Spinner size={12} />
            <span className="truncate">{t("status.refreshing", { n: refreshing })}</span>
          </span>
        )}
        <UpdatePill />
      </footer>
      <FolderDialog />
    </aside>
  );
}

/** 文件夹行右边：未读数，悬停时换成菜单（改名、删除）。 */
function FolderTrailing({ folder, unread }: { folder: string; unread: number }) {
  const t = useT();
  const setDialog = useUi((s) => s.setDialog);
  const { remove } = useFolderActions();
  return (
    <>
      <span className="group-hover:hidden group-has-[[data-state=open]]:hidden">
        <Count n={unread} />
      </span>
      <Menu
        align="start"
        trigger={
          <button
            type="button"
            aria-label={t("action.more")}
            onClick={(e) => e.stopPropagation()}
            onKeyDown={(e) => e.stopPropagation()}
            className="hidden size-5 shrink-0 items-center justify-center rounded text-fg-3 hover:bg-active hover:text-fg group-hover:flex data-[state=open]:flex"
          >
            <Ellipsis size={14} strokeWidth={2} />
          </button>
        }
      >
        <MenuItem icon={<Pencil size={14} />} onSelect={() => setDialog({ type: "folder", folder })}>
          {t("folder.rename")}
        </MenuItem>
        <MenuSeparator />
        <MenuItem danger icon={<Trash2 size={14} />} onSelect={() => remove(folder)}>
          {t("folder.delete")}
        </MenuItem>
      </Menu>
    </>
  );
}

/** 新建文件夹，或者给文件夹改名。 */
function FolderDialog() {
  const t = useT();
  const dialog = useUi((s) => s.dialog);
  const setDialog = useUi((s) => s.setDialog);
  const { create, rename } = useFolderActions();
  const open = dialog?.type === "folder";
  const current = dialog?.type === "folder" ? dialog.folder : undefined;
  const [name, setName] = useState("");

  useEffect(() => {
    if (open) setName(current ?? "");
  }, [open, current]);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    const clean = name.trim();
    if (!clean) return;
    if (current) rename(current, clean);
    else create(clean);
    setDialog(null);
  };

  return (
    <Modal open={open} onClose={() => setDialog(null)} title={current ? t("folder.rename") : t("folder.new")} width={380}>
      <form onSubmit={submit} className="flex flex-col gap-4 p-4">
        <Field label={t("folder.name")}>
          <TextInput autoFocus value={name} onChange={(e) => setName(e.target.value)} />
        </Field>
        {!current && <p className="text-[12px] leading-relaxed text-fg-3">{t("folder.hint")}</p>}
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={() => setDialog(null)}>
            {t("action.cancel")}
          </Button>
          <Button variant="primary" type="submit" disabled={!name.trim()}>
            {current ? t("action.save") : t("folder.create")}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
