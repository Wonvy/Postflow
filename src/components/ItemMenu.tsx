// 条目的右键菜单。列表里的行和阅读面板共用，操作和工具栏按钮、快捷键走同一套逻辑（lib/actions）。

import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Archive,
  ArchiveRestore,
  AtSign,
  Copy,
  ExternalLink,
  FilePen,
  Folder,
  FolderInput,
  Forward,
  Inbox,
  Link,
  Mail,
  MailOpen,
  Reply,
  ReplyAll,
  Star,
  Tag,
  Trash2,
} from "lucide-react";
import { useActions } from "../lib/actions";
import { api, type Item } from "../lib/api";
import { useT } from "../lib/i18n";
import { useCategories, useLabelColor } from "../lib/labels";
import { copyText, openExternal } from "../lib/platform";
import { keys } from "../lib/queries";
import { useUi } from "../lib/store";
import { SourceIcon } from "./SourceIcon";
import { MenuItem, MenuLabel, MenuSeparator, MenuSub } from "./ui";

/** 给一批条目设置分类，然后刷新列表、正文和侧栏的计数。 */
export function useSetLabel() {
  const qc = useQueryClient();
  return (ids: string[], label: string | null) =>
    void api.setLabel(ids, label).then(() => {
      void qc.invalidateQueries({ queryKey: ["items"] });
      void qc.invalidateQueries({ queryKey: ["item"] });
      void qc.invalidateQueries({ queryKey: keys.nav });
    });
}

function LabelSub({ items }: { items: Item[] }) {
  const t = useT();
  const categories = useCategories();
  const labelColor = useLabelColor();
  const setLabel = useSetLabel();
  if (!categories || categories.length === 0) return null;
  const ids = items.map((i) => i.id);
  const current = items.every((i) => i.label === items[0]?.label) ? items[0]?.label : undefined;
  return (
    <MenuSub icon={<Tag size={14} />} label={t("label.set")}>
      {categories.map((c) => (
        <MenuItem
          key={c.name}
          icon={<span aria-hidden className="size-2 rounded-full" style={{ background: labelColor(c.name) }} />}
          onSelect={() => setLabel(ids, c.name)}
        >
          <span className="flex-1">{c.name}</span>
          {current === c.name && <span className="ml-3 text-fg-3">✓</span>}
        </MenuItem>
      ))}
      <MenuSeparator />
      <MenuItem onSelect={() => setLabel(ids, null)}>{t("label.none")}</MenuItem>
    </MenuSub>
  );
}

/** 「移到文件夹」：同一个邮箱里的邮件才能一起移，草稿不移。 */
function MoveSub({ items }: { items: Item[] }) {
  const t = useT();
  const lead = items[0];
  if (!lead || items.some((i) => i.kind !== "mail" || i.draft || i.source_id !== lead.source_id)) return null;
  return (
    <MenuSub icon={<FolderInput size={14} />} label={t("move.to")}>
      <MoveTargets items={items} accountId={lead.source_id} />
    </MenuSub>
  );
}

/** 子菜单打开时才去列文件夹。 */
function MoveTargets({ items, accountId }: { items: Item[]; accountId: string }) {
  const t = useT();
  const { moveTo } = useActions();
  const scope = useUi((s) => s.scope);
  const current = scope.kind === "mailbox" && scope.accountId === accountId ? scope.mailbox : null;
  const folders = useQuery({
    queryKey: ["folders", accountId],
    queryFn: () => api.folders(accountId).then((r) => r.folders),
    staleTime: 5 * 60_000,
    retry: 1,
  });
  const inInbox = items.every((i) => !i.archived);
  return (
    <>
      {!inInbox && (
        <MenuItem icon={<Inbox size={14} />} onSelect={() => moveTo(items, "INBOX")}>
          {t("nav.inbox")}
        </MenuItem>
      )}
      {folders.isPending && <MenuLabel>{t("folders.loading")}</MenuLabel>}
      {folders.isError && <MenuLabel>{t("folders.failed")}</MenuLabel>}
      {folders.data?.length === 0 && <MenuLabel>{t("folders.none")}</MenuLabel>}
      {folders.data
        ?.filter((f) => f.selectable && f.name !== current)
        .map((f) => {
          const parts = f.delimiter ? f.path.split(f.delimiter).filter(Boolean) : [f.path];
          return (
            <MenuItem key={f.name} icon={<Folder size={14} />} onSelect={() => moveTo(items, f.name)}>
              <span className="max-w-[240px] truncate" style={{ paddingLeft: (parts.length - 1) * 12 }}>
                {parts[parts.length - 1] ?? f.path}
              </span>
            </MenuItem>
          );
        })}
    </>
  );
}

/**
 * 对 `items` 的操作：已读、星标、归档、分类，有链接的还能打开和复制，最后是前往所在的订阅源或邮箱。
 * 多于一条时（勾选了几条，或者是折叠的一组）作用到每一条，链接之类只看第一条。
 */
export function ItemMenuItems({ items }: { items: Item[] }) {
  const t = useT();
  const { toggle, goToSource, compose, deleteDrafts } = useActions();
  const lead = items[0];
  if (!lead) return null;
  const all = (flag: "read" | "starred" | "archived") => items.every((i) => i[flag]);
  // 一组里各条来源可能不同，只有都来自同一处时才给「前往」
  const oneSource = items.every((i) => i.source_id === lead.source_id);

  return (
    <>
      {items.length > 1 && <MenuLabel>{t("menu.items", { n: items.length })}</MenuLabel>}
      {items.every((i) => i.draft) && (
        <>
          {items.length === 1 && (
            <MenuItem icon={<FilePen size={14} />} onSelect={() => compose("draft", lead)}>
              {t("compose.editDraft")}
            </MenuItem>
          )}
          <MenuItem icon={<Trash2 size={14} />} onSelect={() => deleteDrafts(items)}>
            {t("compose.deleteDraft")}
          </MenuItem>
          <MenuSeparator />
        </>
      )}
      {items.length === 1 && lead.kind === "mail" && !lead.draft && (
        <>
          <MenuItem icon={<Reply size={14} />} onSelect={() => compose("reply", lead)}>
            {t("compose.reply")}
          </MenuItem>
          <MenuItem icon={<ReplyAll size={14} />} onSelect={() => compose("reply-all", lead)}>
            {t("compose.replyAll")}
          </MenuItem>
          <MenuItem icon={<Forward size={14} />} onSelect={() => compose("forward", lead)}>
            {t("compose.forward")}
          </MenuItem>
          <MenuSeparator />
        </>
      )}
      <MenuItem icon={all("read") ? <Mail size={14} /> : <MailOpen size={14} />} onSelect={() => toggle("read", items)}>
        {all("read") ? t("action.markUnread") : t("action.markRead")}
      </MenuItem>
      <MenuItem
        icon={<Star size={14} className={all("starred") ? "fill-warning text-warning" : undefined} />}
        onSelect={() => toggle("starred", items)}
      >
        {all("starred") ? t("action.unstar") : t("action.star")}
      </MenuItem>
      {!lead.draft && !lead.sent && (
        <MenuItem
          icon={all("archived") ? <ArchiveRestore size={14} /> : <Archive size={14} />}
          onSelect={() => toggle("archived", items)}
        >
          {all("archived") ? t("action.unarchive") : t("action.archive")}
        </MenuItem>
      )}
      <LabelSub items={items} />
      <MoveSub items={items} />
      <MenuSeparator />
      {lead.url && (
        <>
          <MenuItem icon={<ExternalLink size={14} />} onSelect={() => void openExternal(lead.url ?? "")}>
            {t("action.openOriginal")}
          </MenuItem>
          <MenuItem icon={<Link size={14} />} onSelect={() => void copyText(lead.url ?? "")}>
            {t("menu.copyItemLink")}
          </MenuItem>
        </>
      )}
      <MenuItem icon={<Copy size={14} />} onSelect={() => void copyText(lead.title)}>
        {t("menu.copyTitle")}
      </MenuItem>
      {lead.kind === "mail" && lead.sender_address && (
        <MenuItem icon={<AtSign size={14} />} onSelect={() => void copyText(lead.sender_address ?? "")}>
          {t("menu.copySender")}
        </MenuItem>
      )}
      {oneSource && lead.source_title && (
        <>
          <MenuSeparator />
          <MenuItem icon={<SourceIcon item={lead} size={14} />} onSelect={() => goToSource(lead)}>
            <span className="max-w-[240px] truncate">{t("menu.goToSource", { name: lead.source_title })}</span>
          </MenuItem>
        </>
      )}
    </>
  );
}
