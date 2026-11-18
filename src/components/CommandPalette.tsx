// ⌘K 命令面板：一个框里搜邮箱、订阅源、文件夹、分类，也能直接执行命令（写信、刷新、切换空间、打开设置、
// 处理打开的这一条）。按 ⌘K / Ctrl+K 打开，侧栏底部也有按钮。排序在 lib/commands.ts。

import * as Dialog from "@radix-ui/react-dialog";
import { useQueryClient } from "@tanstack/react-query";
import { Search } from "lucide-react";
import { type KeyboardEvent, useEffect, useMemo, useRef, useState } from "react";
import { useActions } from "../lib/actions";
import { api } from "../lib/api";
import { type Command, rankCommands } from "../lib/commands";
import { type SnoozePreset, snoozePresets } from "../lib/snooze";
import { useFolders } from "../lib/folders";
import { useT } from "../lib/i18n";
import { useCategories } from "../lib/labels";
import { keys, useNav } from "../lib/queries";
import { type Space, useUi, viewLabelKey } from "../lib/store";
import { SETTINGS_PAGES } from "./SettingsDialog";
import { cx, Kbd } from "./ui";

const SPACE_VIEWS = ["inbox", "today", "unread", "starred", "snoozed", "archived"] as const;
const SNOOZE_LABEL: Record<SnoozePreset, "snooze.later" | "snooze.tonight" | "snooze.tomorrow" | "snooze.weekend" | "snooze.nextWeek"> = {
  later: "snooze.later",
  tonight: "snooze.tonight",
  tomorrow: "snooze.tomorrow",
  weekend: "snooze.weekend",
  nextWeek: "snooze.nextWeek",
};
const MAIL_FOLDERS = ["drafts", "sent", "junk", "trash"] as const;

/** 现在能用的全部命令，按显示顺序：打开的这一条、操作、前往、设置。 */
function useCommands(): Command[] {
  const t = useT();
  const qc = useQueryClient();
  const nav = useNav().data;
  const categories = useCategories() ?? [];
  const folders = useFolders();
  const ui = useUi();
  const actions = useActions();
  const showFeeds = ui.showFeeds;

  const go = t("commands.go");
  const act = t("commands.do");
  const item = t("commands.item");
  const settings = t("action.settings");
  const spaceName = (s: Space) => t(s === "mail" ? "space.mail" : "space.feeds");
  const inSpace = (s: Space, run: () => void) => () => {
    ui.setSpace(s);
    run();
  };

  const list: Command[] = [];

  // 打开的这一条
  const open = ui.openId ? actions.itemsOf([ui.openId])[0] : undefined;
  if (open) {
    const mail = open.kind === "mail";
    list.push(
      { id: "item:archive", group: item, label: open.archived ? t("action.unarchive") : t("action.archive"), shortcut: "E", run: () => actions.toggle("archived") },
      { id: "item:star", group: item, label: open.starred ? t("action.unstar") : t("action.star"), shortcut: "S", run: () => actions.toggle("starred") },
      { id: "item:read", group: item, label: open.read ? t("action.markUnread") : t("action.markRead"), shortcut: "U", run: () => actions.toggle("read") },
    );
    if (mail && !open.draft) {
      list.push(
        { id: "item:reply", group: item, label: t("compose.reply"), shortcut: "⌘R", run: () => actions.compose("reply") },
        { id: "item:replyAll", group: item, label: t("compose.replyAll"), run: () => actions.compose("reply-all") },
        { id: "item:forward", group: item, label: t("compose.forward"), run: () => actions.compose("forward") },
      );
    }
    if (open.url) list.push({ id: "item:original", group: item, label: t("action.openOriginal"), shortcut: "V", run: actions.openOriginal });
    if (!open.draft && !open.sent) {
      for (const p of snoozePresets(new Date())) {
        list.push({
          id: `item:snooze:${p.key}`,
          group: item,
          label: `${t("snooze.title")} · ${t(SNOOZE_LABEL[p.key])}`,
          keywords: "snooze remind later 稍后 提醒",
          run: () => actions.snooze(p.at.getTime(), [open]),
        });
      }
      if ((open.snoozed_until ?? 0) > Date.now()) {
        list.push({ id: "item:unsnooze", group: item, label: t("snooze.cancel"), run: () => actions.snooze(null, [open]) });
      }
    }
    list.push(
      { id: "item:translate", group: item, label: t("action.translate"), shortcut: "T", run: ui.requestTranslate },
      { id: "item:ai", group: item, label: t("action.ai"), shortcut: "A", run: ui.toggleAiPanel },
    );
    for (const c of categories) {
      list.push({
        id: `item:label:${c.name}`,
        group: item,
        label: t("commands.setLabel", { name: c.name }),
        keywords: "category label 分类",
        run: () =>
          void api.setLabel([open.id], c.name).then(() => {
            void qc.invalidateQueries({ queryKey: ["items"] });
            void qc.invalidateQueries({ queryKey: keys.nav });
          }),
      });
    }
  }

  // 操作
  list.push(
    { id: "compose", group: act, label: t("compose.new"), shortcut: "C", keywords: "compose write mail 写信", run: () => actions.compose("new") },
    { id: "refresh", group: act, label: t("action.refresh"), shortcut: "R", keywords: "refresh sync 刷新 收信", run: actions.refreshAll },
    { id: "search", group: act, label: t("search.label"), shortcut: "/", keywords: "search find 搜索", run: ui.openSearch },
    { id: "markAll", group: act, label: t("action.markAllRead"), shortcut: "⇧A", run: actions.confirmMarkAllRead },
    { id: "space:mail", group: act, label: t("commands.switchTo", { name: spaceName("mail") }), keywords: "mail", run: () => ui.setSpace("mail") },
  );
  if (showFeeds) {
    list.push({ id: "space:feeds", group: act, label: t("commands.switchTo", { name: spaceName("feeds") }), keywords: "feeds rss", run: () => ui.setSpace("feeds") });
  }
  list.push(
    { id: "addAccount", group: act, label: t("action.addAccount"), keywords: "mail account 邮箱", run: () => ui.setDialog({ type: "add-account" }) },
    { id: "addFeed", group: act, label: t("action.addFeed"), keywords: "rss feed subscribe 订阅", run: () => ui.setDialog({ type: "add-feed" }) },
    { id: "newFolder", group: act, label: t("folder.new"), keywords: "folder", run: () => ui.setDialog({ type: "folder" }) },
  );

  // 前往：两个空间的视图、邮箱、订阅源、文件夹、分类
  const spaces: Space[] = showFeeds ? ["mail", "feeds"] : ["mail"];
  for (const s of spaces) {
    for (const view of SPACE_VIEWS) {
      list.push({
        id: `go:${s}:${view}`,
        group: go,
        label: `${spaceName(s)} · ${t(viewLabelKey(view, s))}`,
        keywords: view,
        run: inSpace(s, () => ui.setScope({ kind: "view", view })),
      });
    }
  }
  for (const view of MAIL_FOLDERS) {
    list.push({ id: `go:${view}`, group: go, label: t(`nav.${view}`), keywords: view, run: () => ui.setScope({ kind: "view", view }) });
  }
  for (const a of nav?.accounts ?? []) {
    list.push({ id: `go:account:${a.id}`, group: go, label: a.name || a.email, keywords: a.email, run: () => ui.setScope({ kind: "account", accountId: a.id }) });
  }
  if (showFeeds) {
    for (const f of folders) {
      list.push({ id: `go:folder:${f}`, group: go, label: f, keywords: "folder 文件夹", run: () => ui.setScope({ kind: "folder", folder: f }) });
    }
    for (const f of nav?.feeds ?? []) {
      list.push({ id: `go:feed:${f.id}`, group: go, label: f.title || f.url, keywords: f.url, run: () => ui.setScope({ kind: "feed", feedId: f.id }) });
    }
    list.push({ id: "go:stats", group: go, label: t("stats.title"), keywords: "stats", run: () => { ui.setSpace("feeds"); ui.setMainView("stats"); } });
  }
  for (const c of categories) {
    list.push({ id: `go:label:${c.name}`, group: go, label: `${t("nav.labels")} · ${c.name}`, keywords: "category label", run: () => ui.setScope({ kind: "label", label: c.name }) });
  }

  // 设置的各页
  for (const p of SETTINGS_PAGES) {
    list.push({ id: `settings:${p.page}`, group: settings, label: `${settings} · ${t(p.label)}`, keywords: `settings ${p.page}`, run: () => ui.setDialog({ type: "settings", page: p.page }) });
  }
  return list;
}

export function CommandPalette() {
  const t = useT();
  const dialog = useUi((s) => s.dialog);
  const setDialog = useUi((s) => s.setDialog);
  const open = dialog?.type === "commands";
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const listRef = useRef<HTMLUListElement>(null);
  const commands = useCommands();
  const shown = useMemo(() => rankCommands(commands, query).slice(0, 60), [commands, query]);

  useEffect(() => {
    if (open) {
      setQuery("");
      setActive(0);
    }
  }, [open]);
  useEffect(() => setActive(0), [query]);
  useEffect(() => {
    listRef.current?.querySelector(`[data-index="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }, [active]);

  const run = (command: Command | undefined) => {
    if (!command) return;
    setDialog(null);
    // 等面板关上、焦点回去以后再执行，免得命令打开的对话框被关面板的动作关掉
    setTimeout(command.run, 0);
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((i) => Math.min(shown.length - 1, i + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((i) => Math.max(0, i - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      run(shown[active]);
    }
  };

  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && setDialog(null)}>
      <Dialog.Portal>
        <Dialog.Overlay className="anim-fade fixed inset-0 z-40 bg-overlay" />
        <Dialog.Content
          aria-describedby={undefined}
          className="anim-pop fixed left-1/2 top-[14vh] z-40 flex max-h-[70vh] w-[560px] max-w-[calc(100vw-16px)] -translate-x-1/2 flex-col overflow-hidden rounded-xl bg-surface shadow-pop outline-none max-md:top-[max(8px,env(safe-area-inset-top))]"
        >
          <Dialog.Title className="sr-only">{t("commands.title")}</Dialog.Title>
          <div className="flex h-12 shrink-0 items-center gap-2.5 border-b border-line px-4">
            <Search size={16} strokeWidth={1.75} className="shrink-0 text-fg-3" />
            <input
              // biome-ignore lint/a11y/noAutofocus: 打开面板就能直接输入
              autoFocus
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={onKeyDown}
              placeholder={t("commands.placeholder")}
              aria-label={t("commands.placeholder")}
              role="combobox"
              aria-expanded
              aria-controls="command-list"
              aria-activedescendant={shown[active] ? `command-${active}` : undefined}
              autoComplete="off"
              spellCheck={false}
              className="min-w-0 flex-1 bg-transparent text-[14px] text-fg outline-none placeholder:text-fg-3"
            />
          </div>
          <ul ref={listRef} id="command-list" role="listbox" className="min-h-0 flex-1 overflow-y-auto p-1.5">
            {shown.length === 0 ? (
              <li className="px-3 py-6 text-center text-[13px] text-fg-3">{t("commands.none")}</li>
            ) : (
              shown.map((command, i) => (
                <li
                  key={command.id}
                  id={`command-${i}`}
                  data-index={i}
                  role="option"
                  aria-selected={i === active}
                  onMouseMove={() => i !== active && setActive(i)}
                  onClick={() => run(command)}
                  className={cx(
                    "flex h-8 cursor-default items-center gap-3 rounded-md px-2.5 text-[13px]",
                    i === active ? "bg-active text-fg" : "text-fg-2",
                  )}
                >
                  <span className="min-w-0 flex-1 truncate">{command.label}</span>
                  <span className="shrink-0 text-[11px] text-fg-3">{command.group}</span>
                  {command.shortcut && <Kbd>{command.shortcut}</Kbd>}
                </li>
              ))
            )}
          </ul>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
