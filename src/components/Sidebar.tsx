import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Archive,
  ChartColumn,
  CheckCheck,
  ChevronRight,
  ChevronsDownUp,
  CircleDot,
  Copy,
  Ellipsis,
  FilePen,
  Folder,
  FolderPlus,
  Inbox,
  KeyRound,
  ListTree,
  type LucideIcon,
  Mail,
  PanelTop,
  Pencil,
  Plus,
  RefreshCw,
  Rss,
  Send,
  Settings,
  ShieldAlert,
  Tags,
  SquarePen,
  Star,
  Sun,
  Trash2,
  TriangleAlert,
} from "lucide-react";
import { type DragEvent, type FormEvent, type ReactNode, useEffect, useMemo, useState } from "react";
import { useActions } from "../lib/actions";
import { type Account, api, type Feed, type ItemPatch, type Nav, type View } from "../lib/api";
import { type DragPayload, type DropPosition, dropPosition, endDrag, isOurs, moveInList, startDrag, useDrag } from "../lib/dnd";
import { moveTop, type TopEntry } from "../lib/feedTree";
import { useFolderActions, useTopLevel } from "../lib/folders";
import { useCategories, useLabelColor } from "../lib/labels";
import { hasProviderIcon, ProviderIcon, providerOf } from "./ProviderIcon";
import { UpdateButton } from "./UpdateDialog";
import { errorText, useT } from "../lib/i18n";
import { copyText, hasInsetTitlebar } from "../lib/platform";
import { keys, useNav, usePresets } from "../lib/queries";
import { type ListScope, type SidebarSection, type SidebarTab, sameScope, useUi } from "../lib/store";
import {
  Avatar,
  Button,
  ContextMenu,
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

/** 邮箱里另外同步的几个文件夹，放在邮箱列表下面。 */
const MAIL_FOLDERS: { view: View; icon: LucideIcon; label: `nav.${View}` }[] = [
  { view: "drafts", icon: FilePen, label: "nav.drafts" },
  { view: "sent", icon: Send, label: "nav.sent" },
  { view: "junk", icon: ShieldAlert, label: "nav.junk" },
  { view: "trash", icon: Trash2, label: "nav.trash" },
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
  depth,
  leading,
  label,
  trailing,
  drag,
  drop,
  groupEnd = false,
}: {
  active: boolean;
  onClick: () => void;
  indent?: boolean;
  /** 邮箱里的文件夹按层级往里缩：第 1 层和订阅源一样缩一格。 */
  depth?: number;
  leading: ReactNode;
  label: string;
  trailing?: ReactNode;
  drag?: DragHandlers;
  /** 拖到这一行上时放下的位置：上方、下方，或者放进去。 */
  drop?: DropPosition | null;
  /** 这是展开的文件夹里最后一个源，东西正要排到这个文件夹后面：在下沿画一条和文件夹对齐的线。 */
  groupEnd?: boolean;
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
      style={depth ? { paddingLeft: 26 + (depth - 1) * 14 } : undefined}
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
      {groupEnd && (
        <span aria-hidden className="pointer-events-none absolute -bottom-px left-2 right-1 h-0.5 rounded-full bg-accent" />
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

/** 「全部标为已读」：先问一下，再把这个范围里的未读都标为已读。侧栏里每个能打开的列表的右键菜单里都有。 */
function MarkAllReadItem({ scope }: { scope: ListScope }) {
  const t = useT();
  const setDialog = useUi((s) => s.setDialog);
  return (
    <MenuItem icon={<CheckCheck size={14} />} onSelect={() => setDialog({ type: "mark-all-read", scope })}>
      {t("action.markAllRead")}
    </MenuItem>
  );
}

function FeedRow({
  feed,
  indent,
  active,
  drag,
  drop,
  groupEnd = false,
}: {
  feed: Feed;
  indent: boolean;
  active: boolean;
  drag: DragHandlers;
  drop: DropPosition | null;
  groupEnd?: boolean;
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

  const items = (
    <>
      <MarkAllReadItem scope={{ kind: "feed", feedId: feed.id }} />
      <MenuSeparator />
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
    </>
  );

  return (
    <ContextMenu items={items}>
      <Row
        active={active}
        indent={indent}
        onClick={() => setScope({ kind: "feed", feedId: feed.id })}
        drag={drag}
        drop={drop}
        groupEnd={groupEnd}
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
              {items}
            </Menu>
          </>
        }
      />
    </ContextMenu>
  );
}

function AccountRow({
  account,
  active,
  drag,
  drop,
  folderDrop,
}: {
  account: Account;
  active: boolean;
  drag: DragHandlers;
  drop: DropPosition | null;
  /** 把邮件拖到这个邮箱的某个文件夹上：移过去。 */
  folderDrop: (mailbox: string) => { drag: DragHandlers; drop: DropPosition | null };
}) {
  const t = useT();
  const setScope = useUi((s) => s.setScope);
  const setDialog = useUi((s) => s.setDialog);
  const expanded = useUi((s) => s.expandedAccounts.includes(account.id));
  const toggleFolders = useUi((s) => s.toggleAccountFolders);
  const presets = usePresets();
  const label = account.name || account.email;
  const needsPassword = account.error_code === "mail_no_password";
  const provider = providerOf(account, presets.data);

  const items = (
    <>
      <MarkAllReadItem scope={{ kind: "account", accountId: account.id }} />
      <MenuSeparator />
      <MenuItem icon={<Folder size={14} />} onSelect={() => toggleFolders(account.id)}>
        {expanded ? t("folders.hide") : t("folders.show")}
      </MenuItem>
      <MenuItem icon={<SquarePen size={14} />} onSelect={() => setDialog({ type: "compose", mode: "new", accountId: account.id })}>
        {t("compose.fromAccount")}
      </MenuItem>
      <MenuItem icon={<Pencil size={14} />} onSelect={() => setDialog({ type: "edit-account", accountId: account.id })}>
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
    </>
  );

  return (
    <>
      <ContextMenu items={items}>
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
              <button
                type="button"
                aria-label={expanded ? t("folders.hide") : t("folders.show")}
                aria-expanded={expanded}
                onClick={(e) => {
                  e.stopPropagation();
                  toggleFolders(account.id);
                }}
                onKeyDown={(e) => e.stopPropagation()}
                className={cx(
                  "size-5 shrink-0 items-center justify-center rounded text-fg-3 hover:bg-active hover:text-fg",
                  expanded ? "flex" : "hidden group-hover:flex",
                )}
              >
                <ChevronRight size={13} strokeWidth={2} className={cx("transition-transform", expanded && "rotate-90")} />
              </button>
              {account.error_code ? (
                <Tip
                  label={
                    needsPassword
                      ? // Outlook、Microsoft 365 不用密码，要用微软账户登录
                        presets.data?.find((p) => p.id === provider)?.password_kind === "oauth"
                        ? t("oauth.needsSignIn", { provider: t("oauth.microsoftName") })
                        : t("account.needsPassword")
                      : errorText(t, { code: account.error_code })
                  }
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
                {items}
              </Menu>
            </>
          }
        />
      </ContextMenu>
      {expanded && <AccountFolders account={account} folderDrop={folderDrop} />}
    </>
  );
}

/** 一个邮箱里的普通文件夹，按层级缩进。点开时马上同步那个文件夹；把邮件拖上来就移进去。 */
function AccountFolders({
  account,
  folderDrop,
}: {
  account: Account;
  folderDrop: (mailbox: string) => { drag: DragHandlers; drop: DropPosition | null };
}) {
  const t = useT();
  const scope = useUi((s) => s.scope);
  const setScope = useUi((s) => s.setScope);
  const folders = useQuery({
    queryKey: ["folders", account.id],
    queryFn: () => api.folders(account.id).then((r) => r.folders),
    staleTime: 5 * 60_000,
    retry: 1,
  });
  const note = (text: ReactNode) => (
    <div className="flex h-7 items-center gap-2 pl-[26px] pr-2 text-[12px] text-fg-3">{text}</div>
  );
  if (folders.isPending) return note(t("folders.loading"));
  if (folders.isError) {
    return note(
      <>
        {t("folders.failed")}
        <button type="button" onClick={() => void folders.refetch()} className="text-accent hover:underline">
          {t("folders.retry")}
        </button>
      </>,
    );
  }
  if (folders.data.length === 0) return note(t("folders.none"));
  return (
    <>
      {folders.data.map((f) => {
        const parts = f.delimiter ? f.path.split(f.delimiter).filter(Boolean) : [f.path];
        const label = parts[parts.length - 1] ?? f.path;
        const active = scope.kind === "mailbox" && scope.accountId === account.id && scope.mailbox === f.name;
        const dnd = f.selectable ? folderDrop(f.name) : null;
        const row = (
          <Row
            key={f.name}
            depth={Math.max(1, parts.length)}
            active={active}
            drag={dnd?.drag}
            drop={dnd?.drop}
            onClick={() => {
              if (!f.selectable) return;
              setScope({ kind: "mailbox", accountId: account.id, mailbox: f.name, label });
              void api.openFolder(account.id, f.name).catch(() => {});
            }}
            leading={<Folder {...ICON} />}
            label={label}
          />
        );
        if (!f.selectable) return row;
        return (
          <ContextMenu
            key={f.name}
            items={
              <>
                <MarkAllReadItem scope={{ kind: "mailbox", accountId: account.id, mailbox: f.name, label }} />
                <MenuItem
                  icon={<RefreshCw size={14} />}
                  onSelect={() => void api.openFolder(account.id, f.name).catch(() => {})}
                >
                  {t("action.syncNow")}
                </MenuItem>
                <MenuSeparator />
                <MenuItem icon={<Copy size={14} />} onSelect={() => void copyText(f.path)}>
                  {t("folders.copyName")}
                </MenuItem>
              </>
            }
          >
            {row}
          </ContextMenu>
        );
      })}
    </>
  );
}

function SectionHeader({
  label,
  addLabel,
  onAdd,
  drag,
  highlight = false,
  extra,
  collapsed,
  onToggle,
  unread = 0,
  menu,
}: {
  label: string;
  addLabel: string;
  onAdd: () => void;
  drag?: DragHandlers;
  highlight?: boolean;
  extra?: ReactNode;
  collapsed: boolean;
  onToggle: () => void;
  /** 收起时在标题右边显示这一组的未读数。 */
  unread?: number;
  /** 右键菜单里另外的条目，排在「展开/收起」前面。 */
  menu?: ReactNode;
}) {
  const t = useT();
  return (
    <ContextMenu
      bare
      items={
        <>
          {menu}
          {menu && <MenuSeparator />}
          <MenuItem icon={<ChevronsDownUp size={14} />} onSelect={onToggle}>
            {collapsed ? t("section.expand") : t("section.collapse")}
          </MenuItem>
        </>
      }
    >
    <div
      {...drag}
      className={cx(
        "group/section mb-1 mt-5 flex h-6 items-center justify-between rounded-md pl-1 pr-0.5 transition-colors",
        highlight && "bg-accent-soft ring-1 ring-inset ring-accent/60",
      )}
    >
      <button
        type="button"
        aria-expanded={!collapsed}
        onClick={onToggle}
        className="flex min-w-0 flex-1 items-center gap-1 rounded text-left text-[12px] font-medium text-fg-3 hover:text-fg-2"
      >
        <ChevronRight
          size={12}
          strokeWidth={2.25}
          className={cx("shrink-0 transition-transform", !collapsed && "rotate-90")}
        />
        <span className="truncate">{label}</span>
      </button>
      {collapsed && <Count n={unread} />}
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
    </ContextMenu>
  );
}

export function Sidebar({ width }: { width: number }) {
  const t = useT();
  const nav = useNav();
  const scope = useUi((s) => s.scope);
  const setScope = useUi((s) => s.setScope);
  const setDialog = useUi((s) => s.setDialog);
  const mainView = useUi((s) => s.mainView);
  const setMainView = useUi((s) => s.setMainView);
  const refreshing = useUi((s) => s.refreshing);
  const collapsed = useUi((s) => s.collapsedFolders);
  const toggleFolder = useUi((s) => s.toggleFolder);
  const layout = useUi((s) => s.sidebarLayout);
  const setLayout = useUi((s) => s.setSidebarLayout);
  const tab = useUi((s) => s.tab);
  const setTab = useUi((s) => s.setTab);
  const collapsedSections = useUi((s) => s.collapsedSections);
  const toggleSection = useUi((s) => s.toggleSection);
  const isCollapsed = (section: SidebarSection) => collapsedSections.includes(section);
  // 选项卡模式下只显示当前选项卡的那一组
  const shows = (section: SidebarSection) =>
    layout === "groups" || tab === "all" || (tab === "mail") === (section === "mail");
  const qc = useQueryClient();
  const { applyTo, compose, itemsOf, moveTo } = useActions();
  const hasAccounts = (nav.data?.accounts.length ?? 0) > 0;
  const folderActions = useFolderActions();

  const feeds = nav.data?.feeds;
  const accounts = nav.data?.accounts;
  const counts = nav.data?.counts;
  const categories = useCategories();
  const labelColor = useLabelColor();
  const labelCounts = nav.data?.labels ?? [];
  // 设置里的类别在前，按设置的顺序；之后改了名字、只在旧条目上还有的分类跟在后面
  const labels = [
    ...(categories ?? []).map((c) => c.name),
    ...labelCounts.map((l) => l.label).filter((l) => !(categories ?? []).some((c) => c.name === l)),
  ];

  // 顶层：文件夹和文件夹外的源交错排列
  const top = useTopLevel();
  const { byFolder, feedById } = useMemo(() => {
    const byFolder = new Map<string, Feed[]>();
    const feedById = new Map<string, Feed>();
    for (const feed of feeds ?? []) {
      feedById.set(feed.id, feed);
      if (!feed.folder) continue;
      const list = byFolder.get(feed.folder);
      if (list) list.push(feed);
      else byFolder.set(feed.folder, [feed]);
    }
    return { byFolder, feedById };
  }, [feeds]);

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

  /** 把订阅源放到文件夹里某个源的前后，或者放进文件夹的末尾。 */
  const moveFeed = (id: string, folder: string, anchor: string | null, position: DropPosition) => {
    const inside = folderActions.feedsIn(folder);
    const order =
      anchor && position !== "inside"
        ? moveInList(inside, id, anchor, position)
        : [...inside.filter((x) => x !== id), id];
    folderActions.place(folder, order);
  };

  /** 拖的东西在顶层的样子：文件夹，或者文件夹外的源。 */
  const entryOf = (p: DragPayload): TopEntry | null =>
    p.kind === "folder" ? { kind: "folder", name: p.name } : p.kind === "feed" ? { kind: "feed", id: p.id } : null;

  /** 放到顶层某一项的前后；`anchor` 为空时放到最前（before）或最后（after）。 */
  const placeTop = (p: DragPayload, anchor: TopEntry | null, position: "before" | "after") => {
    const moved = entryOf(p);
    if (moved) folderActions.arrangeTop(moveTop(folderActions.currentTop(), moved, anchor, position));
  };

  // 文件夹里的源：只接订阅源，在这个文件夹里排前后
  const feedTarget = (feed: Feed) =>
    target(
      `feed:${feed.id}`,
      (p, e) => (p.kind === "feed" && p.id !== feed.id ? dropPosition(e) : null),
      (p, position) => p.kind === "feed" && moveFeed(p.id, feed.folder, feed.id, position),
    );

  // 文件夹外的源：订阅源和文件夹都能排在它前后
  const looseTarget = (feed: Feed) =>
    target(
      `feed:${feed.id}`,
      (p, e) => ((p.kind === "feed" && p.id !== feed.id) || p.kind === "folder" ? dropPosition(e) : null),
      (p, position) => position !== "inside" && placeTop(p, { kind: "feed", id: feed.id }, position),
    );

  // 文件夹：订阅源拖到上三分之一排在文件夹前面，中间放进去，下三分之一排在文件夹后面（同级）。
  // 展开着的文件夹也一样：排在整组后面，提示线画在组里最后一个源下面。
  // 想放进文件夹排第一，拖到文件夹里第一个源的上沿。文件夹拖到文件夹上只排前后。
  const folderTarget = (name: string) =>
    target(
      `folder:${name}`,
      (p, e) => {
        if (p.kind === "feed") {
          const position = dropPosition(e, true);
          return position === "inside" && p.folder === name ? null : position;
        }
        return p.kind === "folder" && p.name !== name ? dropPosition(e) : null;
      },
      (p, position) => {
        if (position === "inside") {
          if (p.kind === "feed") moveFeed(p.id, name, null, "inside");
        } else {
          placeTop(p, { kind: "folder", name }, position);
        }
      },
    );

  // 拖到「订阅」标题上：排到最前面，从文件夹里拖出来的源就移出了文件夹
  const feedsSectionTarget = target(
    "section:feeds",
    (p) => (p.kind === "feed" || p.kind === "folder" ? "inside" : null),
    (p) => placeTop(p, null, "before"),
  );

  // 拖到订阅列表下面的空白处：排到最后
  const feedsEndTarget = target(
    "end:feeds",
    (p) => (p.kind === "feed" || p.kind === "folder" ? "before" : null),
    (p) => placeTop(p, null, "after"),
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

  // 把列表里的条目拖到分类上：手动定分类
  const labelTarget = (label: string) =>
    target(
      `label:${label}`,
      (p) => (p.kind === "items" ? "inside" : null),
      (p) => {
        if (p.kind !== "items") return;
        void api.setLabel(p.ids, label).then(() => {
          void qc.invalidateQueries({ queryKey: ["items"] });
          void qc.invalidateQueries({ queryKey: keys.nav });
        });
      },
    );

  const isActive = (target: ListScope) => sameScope(scope, target);
  // 选项卡模式下视图的数字也只算这一类
  const kind = layout === "tabs" ? tab : "all";
  const countFor = (view: View) => {
    if (!counts) return undefined;
    if (view === "inbox") return kind === "mail" ? counts.unread_mail : kind === "feeds" ? counts.unread_article : counts.unread;
    if (view === "today") return kind === "mail" ? counts.today_mail : kind === "feeds" ? counts.today_article : counts.today;
    return undefined;
  };
  const TABS: { value: SidebarTab; label: string; count: number }[] = [
    { value: "all", label: t("tab.all"), count: counts?.unread ?? 0 },
    { value: "mail", label: t("nav.mail"), count: counts?.unread_mail ?? 0 },
    { value: "feeds", label: t("nav.feeds"), count: counts?.unread_article ?? 0 },
  ];

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
        <UpdateButton />
        <IconButton
          label={layout === "tabs" ? t("layout.toGroups") : t("layout.toTabs")}
          onClick={() => setLayout(layout === "tabs" ? "groups" : "tabs")}
        >
          {layout === "tabs" ? <ListTree {...ICON} /> : <PanelTop {...ICON} />}
        </IconButton>
        {hasAccounts && (
          <IconButton label={t("compose.new")} shortcut="C" onClick={() => compose("new")}>
            <SquarePen {...ICON} />
          </IconButton>
        )}
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

      {layout === "tabs" && (
        <div role="tablist" aria-label={t("layout.tabs")} className="mx-2 mb-2 flex shrink-0 rounded-md bg-hover p-0.5">
          {TABS.map((item, i) => (
            <Tip key={item.value} label={item.label} shortcut={String(i + 1)}>
              <button
                type="button"
                role="tab"
                aria-selected={tab === item.value}
                onClick={() => setTab(item.value)}
                className={cx(
                  "flex h-6 min-w-0 flex-1 items-center justify-center gap-1 rounded-[5px] px-1.5 text-[12px] transition-colors",
                  tab === item.value ? "bg-surface font-medium text-fg shadow-sm" : "text-fg-3 hover:text-fg-2",
                )}
              >
                <span className="truncate">{item.label}</span>
                {item.count > 0 && <span className="tabular text-[11px] text-fg-3">{item.count > 999 ? "999+" : item.count}</span>}
              </button>
            </Tip>
          ))}
        </div>
      )}

      <nav className="min-h-0 flex-1 overflow-y-auto px-2 pb-3">
        <div className="flex flex-col gap-px">
          {VIEWS.map(({ view, icon: Icon, label }) => (
            <ContextMenu key={view} items={<MarkAllReadItem scope={{ kind: "view", view }} />}>
              <Row
                active={isActive({ kind: "view", view })}
                onClick={() => setScope({ kind: "view", view })}
                drag={viewTarget(view)}
                drop={dropAt(`view:${view}`)}
                leading={<Icon {...ICON} />}
                label={t(label)}
                trailing={<Count n={countFor(view) ?? 0} />}
              />
            </ContextMenu>
          ))}
        </div>

        {shows("mail") && (
          <>
            <SectionHeader
              label={t("nav.mail")}
              addLabel={t("action.addAccount")}
              onAdd={() => setDialog({ type: "add-account" })}
              collapsed={isCollapsed("mail")}
              onToggle={() => toggleSection("mail")}
              unread={counts?.unread_mail}
              menu={
                <>
                  <MenuItem icon={<Plus size={14} />} onSelect={() => setDialog({ type: "add-account" })}>
                    {t("action.addAccount")}
                  </MenuItem>
                  {hasAccounts && (
                    <>
                      <MenuItem icon={<SquarePen size={14} />} onSelect={() => compose("new")}>
                        {t("compose.new")}
                      </MenuItem>
                      <MenuItem icon={<RefreshCw size={14} />} onSelect={() => void api.syncAccount()}>
                        {t("action.syncAllMail")}
                      </MenuItem>
                    </>
                  )}
                </>
              }
            />
            {!isCollapsed("mail") && (
              <div className="flex flex-col gap-px">
                {accounts?.map((account) => (
                  <AccountRow
                    key={account.id}
                    account={account}
                    active={isActive({ kind: "account", accountId: account.id })}
                    drag={{ ...source({ kind: "account", id: account.id }, account.name || account.email), ...accountTarget(account) }}
                    drop={dropAt(`account:${account.id}`)}
                    folderDrop={(mailbox) => {
                      const key = `mailbox:${account.id}:${mailbox}`;
                      // 只收这个邮箱里的邮件，草稿不收
                      const fits = (ids: string[]) => {
                        const items = itemsOf(ids);
                        return (
                          items.length > 0 &&
                          items.every((i) => i.kind === "mail" && !i.draft && i.source_id === account.id)
                        );
                      };
                      return {
                        drag: target(
                          key,
                          (p) => (p.kind === "items" && fits(p.ids) ? "inside" : null),
                          (p) => p.kind === "items" && moveTo(itemsOf(p.ids), mailbox),
                        ),
                        drop: dropAt(key),
                      };
                    }}
                  />
                ))}
                {accounts &&
                  accounts.length > 0 &&
                  MAIL_FOLDERS.map(({ view, icon: Icon, label }) => (
                    <ContextMenu key={view} items={<MarkAllReadItem scope={{ kind: "view", view }} />}>
                      <Row
                        active={isActive({ kind: "view", view })}
                        onClick={() => setScope({ kind: "view", view })}
                        leading={<Icon {...ICON} />}
                        label={t(label)}
                      />
                    </ContextMenu>
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
            )}
          </>
        )}

        {labels.length > 0 && (
          <>
            <SectionHeader
              label={t("nav.labels")}
              addLabel={t("label.edit")}
              onAdd={() => setDialog({ type: "settings", page: "categories" })}
              collapsed={isCollapsed("labels")}
              onToggle={() => toggleSection("labels")}
              unread={labelCounts.reduce((n, l) => n + l.unread, 0)}
              menu={
                <MenuItem icon={<Tags size={14} />} onSelect={() => setDialog({ type: "settings", page: "categories" })}>
                  {t("label.edit")}
                </MenuItem>
              }
            />
            {!isCollapsed("labels") && (
              <div className="flex flex-col gap-px">
                {labels.map((label) => (
                  <ContextMenu
                    key={label}
                    items={
                      <>
                        <MarkAllReadItem scope={{ kind: "label", label }} />
                        <MenuSeparator />
                        <MenuItem
                          icon={<Tags size={14} />}
                          onSelect={() => setDialog({ type: "settings", page: "categories" })}
                        >
                          {t("label.edit")}
                        </MenuItem>
                      </>
                    }
                  >
                  <Row
                    active={isActive({ kind: "label", label })}
                    onClick={() => setScope({ kind: "label", label })}
                    drag={labelTarget(label)}
                    drop={dropAt(`label:${label}`)}
                    leading={<span aria-hidden className="size-2 rounded-full" style={{ background: labelColor(label) }} />}
                    label={label}
                    trailing={<Count n={labelCounts.find((l) => l.label === label)?.unread ?? 0} />}
                  />
                  </ContextMenu>
                ))}
              </div>
            )}
          </>
        )}

        {shows("feeds") && (
          <>
            <SectionHeader
              label={t("nav.feeds")}
              addLabel={t("action.addFeed")}
              onAdd={() => setDialog({ type: "add-feed" })}
              collapsed={isCollapsed("feeds")}
              onToggle={() => toggleSection("feeds")}
              unread={counts?.unread_article}
              menu={
                <>
                  <MenuItem icon={<Plus size={14} />} onSelect={() => setDialog({ type: "add-feed" })}>
                    {t("action.addFeed")}
                  </MenuItem>
                  <MenuItem icon={<FolderPlus size={14} />} onSelect={() => setDialog({ type: "folder" })}>
                    {t("folder.new")}
                  </MenuItem>
                  <MenuItem icon={<RefreshCw size={14} />} onSelect={() => void api.refresh()}>
                    {t("action.refreshAll")}
                  </MenuItem>
                  <MenuItem
                    icon={<ChartColumn size={14} />}
                    onSelect={() => setMainView(mainView === "stats" ? "list" : "stats")}
                  >
                    {t("stats.title")}
                  </MenuItem>
                </>
              }
              drag={feedsSectionTarget}
              highlight={dropAt("section:feeds") !== null}
              extra={
                <>
                <Tip label={t("stats.title")} side="right">
                  <button
                    type="button"
                    aria-label={t("stats.title")}
                    aria-pressed={mainView === "stats"}
                    onClick={() => setMainView(mainView === "stats" ? "list" : "stats")}
                    className={cx(
                      "flex size-5 items-center justify-center rounded hover:bg-active hover:text-fg focus-visible:opacity-100 group-hover/section:opacity-100",
                      mainView === "stats" ? "text-accent opacity-100" : "text-fg-3 opacity-0",
                    )}
                  >
                    <ChartColumn size={14} strokeWidth={2} />
                  </button>
                </Tip>
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
                </>
              }
            />

            {!isCollapsed("feeds") && (
              <div className="flex flex-col gap-px">
                {top.map((entry) => {
                  if (entry.kind === "feed") {
                    const feed = feedById.get(entry.id);
                    if (!feed) return null;
                    return (
                      <FeedRow
                        key={feed.id}
                        feed={feed}
                        indent={false}
                        active={isActive({ kind: "feed", feedId: feed.id })}
                        drag={{ ...source({ kind: "feed", id: feed.id, folder: "" }, feed.title), ...looseTarget(feed) }}
                        drop={dropAt(`feed:${feed.id}`)}
                      />
                    );
                  }
                  const folder = entry.name;
                  const list = byFolder.get(folder) ?? [];
                  const open = !collapsed.includes(folder);
                  const folderDrop = dropAt(`folder:${folder}`);
                  // 展开着的文件夹，排到它后面时提示线画在整组下面
                  const afterGroup = folderDrop === "after" && open && list.length > 0;
                  return (
                    <div key={`folder:${folder}`} className="flex flex-col gap-px">
                      <ContextMenu items={<FolderMenuItems folder={folder} />}>
                        <Row
                          active={isActive({ kind: "folder", folder })}
                          onClick={() => setScope({ kind: "folder", folder })}
                          drag={{ ...source({ kind: "folder", name: folder }, folder), ...folderTarget(folder) }}
                          drop={afterGroup ? null : folderDrop}
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
                      </ContextMenu>
                      {open &&
                        list.map((feed, i) => (
                          <FeedRow
                            key={feed.id}
                            feed={feed}
                            indent
                            active={isActive({ kind: "feed", feedId: feed.id })}
                            drag={{ ...source({ kind: "feed", id: feed.id, folder: feed.folder }, feed.title), ...feedTarget(feed) }}
                            drop={dropAt(`feed:${feed.id}`)}
                            groupEnd={afterGroup && i === list.length - 1}
                          />
                        ))}
                    </div>
                  );
                })}
                {(dragging?.kind === "feed" || dragging?.kind === "folder") && (
                  <div {...feedsEndTarget} className="relative h-7">
                    {dropAt("end:feeds") && (
                      <span aria-hidden className="pointer-events-none absolute left-2 right-1 top-0 h-0.5 rounded-full bg-accent" />
                    )}
                  </div>
                )}
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
            )}
          </>
        )}
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
      </footer>
      <FolderDialog />
    </aside>
  );
}

/** 文件夹的菜单项：改名、删除。悬停时的「更多」按钮和右键菜单共用。 */
function FolderMenuItems({ folder }: { folder: string }) {
  const t = useT();
  const setDialog = useUi((s) => s.setDialog);
  const { remove } = useFolderActions();
  return (
    <>
      <MarkAllReadItem scope={{ kind: "folder", folder }} />
      <MenuSeparator />
      <MenuItem icon={<Plus size={14} />} onSelect={() => setDialog({ type: "add-feed", folder })}>
        {t("folder.addFeed")}
      </MenuItem>
      <MenuSeparator />
      <MenuItem icon={<Pencil size={14} />} onSelect={() => setDialog({ type: "folder", folder })}>
        {t("folder.rename")}
      </MenuItem>
      <MenuSeparator />
      <MenuItem danger icon={<Trash2 size={14} />} onSelect={() => remove(folder)}>
        {t("folder.delete")}
      </MenuItem>
    </>
  );
}

/** 文件夹行右边：未读数，悬停时换成菜单（在这里添加订阅、改名、删除）。 */
function FolderTrailing({ folder, unread }: { folder: string; unread: number }) {
  const t = useT();
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
        <FolderMenuItems folder={folder} />
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
