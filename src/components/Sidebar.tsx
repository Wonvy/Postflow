import { useQueryClient } from "@tanstack/react-query";
import {
  Archive,
  ChevronRight,
  CircleDot,
  Copy,
  Ellipsis,
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
import { type ReactNode, useMemo } from "react";
import { type Account, api, type Feed, type View } from "../lib/api";
import { hasProviderIcon, ProviderIcon, providerOf } from "./ProviderIcon";
import { UpdatePill } from "./UpdateDialog";
import { errorText, useT } from "../lib/i18n";
import { copyText, hasInsetTitlebar } from "../lib/platform";
import { keys, useNav, usePresets } from "../lib/queries";
import { type ListScope, sameScope, useUi } from "../lib/store";
import {
  Avatar,
  cx,
  Favicon,
  ICON,
  IconButton,
  Logo,
  Menu,
  MenuItem,
  MenuSeparator,
  Spinner,
  Tip,
} from "./ui";

const VIEWS: { view: View; icon: LucideIcon; label: `nav.${View}` }[] = [
  { view: "inbox", icon: Inbox, label: "nav.inbox" },
  { view: "today", icon: Sun, label: "nav.today" },
  { view: "unread", icon: CircleDot, label: "nav.unread" },
  { view: "starred", icon: Star, label: "nav.starred" },
  { view: "archived", icon: Archive, label: "nav.archived" },
];

function Row({
  active,
  onClick,
  indent = false,
  leading,
  label,
  trailing,
}: {
  active: boolean;
  onClick: () => void;
  indent?: boolean;
  leading: ReactNode;
  label: string;
  trailing?: ReactNode;
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
      className={cx(
        "group flex h-7 items-center gap-2 rounded-md pr-1.5 outline-offset-[-2px] transition-colors",
        indent ? "pl-[26px]" : "pl-2",
        active ? "bg-active text-fg" : "text-fg-2 hover:bg-hover hover:text-fg",
      )}
    >
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

function FeedRow({ feed, indent, active }: { feed: Feed; indent: boolean; active: boolean }) {
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

function AccountRow({ account, active }: { account: Account; active: boolean }) {
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

function SectionHeader({ label, addLabel, onAdd }: { label: string; addLabel: string; onAdd: () => void }) {
  return (
    <div className="group/section mb-1 mt-5 flex h-6 items-center justify-between pl-2 pr-0.5">
      <span className="text-[12px] font-medium text-fg-3">{label}</span>
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

  const feeds = nav.data?.feeds;
  const accounts = nav.data?.accounts;
  const counts = nav.data?.counts;

  const { folders, loose } = useMemo(() => {
    const byFolder = new Map<string, Feed[]>();
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
  }, [feeds]);

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
        </Menu>
      </header>

      <nav className="min-h-0 flex-1 overflow-y-auto px-2 pb-3">
        <div className="flex flex-col gap-px">
          {VIEWS.map(({ view, icon: Icon, label }) => (
            <Row
              key={view}
              active={isActive({ kind: "view", view })}
              onClick={() => setScope({ kind: "view", view })}
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
        />

        <div className="flex flex-col gap-px">
          {folders.map(([folder, list]) => {
            const open = !collapsed.includes(folder);
            return (
              <div key={folder} className="flex flex-col gap-px">
                <Row
                  active={isActive({ kind: "folder", folder })}
                  onClick={() => setScope({ kind: "folder", folder })}
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
                  trailing={<Count n={list.reduce((sum, f) => sum + f.unread, 0)} />}
                />
                {open &&
                  list.map((feed) => (
                    <FeedRow
                      key={feed.id}
                      feed={feed}
                      indent
                      active={isActive({ kind: "feed", feedId: feed.id })}
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
    </aside>
  );
}
