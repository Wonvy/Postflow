// 附件总览：所有邮件里的附件排成一张表，按类型筛、按文件名或发件人搜，点一下保存或回到那封邮件。
//
// 附件是核心收录邮件时从原文里取出来的（见 crates/store/src/search.rs 的 mail_attachments）。
// 超过 2 MB 的大邮件同步时只取信头，附件要打开邮件才知道，单独列在最下面。

import { keepPreviousData, useQuery } from "@tanstack/react-query";
import {
  Download,
  File,
  FileArchive,
  FileImage,
  FileSpreadsheet,
  FileText,
  FileType,
  type LucideIcon,
  Paperclip,
  Presentation,
  Search,
  X,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { type AttachmentRow, api } from "../lib/api";
import { errorText, type MessageKey, useLang, useT } from "../lib/i18n";
import { formatSize, saveAttachment } from "../lib/platform";
import { useUi } from "../lib/store";
import { shortTime } from "../lib/time";
import { SenderAvatar } from "./SourceIcon";
import { toast } from "./Toast";
import { cx, ICON, IconButton, Spinner } from "./ui";
import { useWindowControlsGap } from "./WindowControls";

export type FileKind = "image" | "pdf" | "doc" | "sheet" | "slides" | "archive" | "other";

const KINDS: { kind: FileKind | "all"; label: MessageKey }[] = [
  { kind: "all", label: "files.all" },
  { kind: "image", label: "files.image" },
  { kind: "pdf", label: "files.pdf" },
  { kind: "doc", label: "files.doc" },
  { kind: "sheet", label: "files.sheet" },
  { kind: "slides", label: "files.slides" },
  { kind: "archive", label: "files.archive" },
  { kind: "other", label: "files.other" },
];

const ICONS: Record<FileKind, LucideIcon> = {
  image: FileImage,
  pdf: FileType,
  doc: FileText,
  sheet: FileSpreadsheet,
  slides: Presentation,
  archive: FileArchive,
  other: File,
};

/** 按 MIME 类型和扩展名把附件分类。 */
export function fileKind(name: string, mime: string): FileKind {
  const ext = name.toLowerCase().split(".").pop() ?? "";
  const m = mime.toLowerCase();
  if (m.startsWith("image/") || ["png", "jpg", "jpeg", "gif", "webp", "heic", "svg", "bmp", "tiff"].includes(ext)) return "image";
  if (m === "application/pdf" || ext === "pdf") return "pdf";
  if (["xls", "xlsx", "csv", "numbers", "ods", "tsv"].includes(ext) || m.includes("spreadsheet") || m.includes("excel")) return "sheet";
  if (["ppt", "pptx", "key", "odp"].includes(ext) || m.includes("presentation") || m.includes("powerpoint")) return "slides";
  if (["doc", "docx", "pages", "odt", "rtf", "txt", "md"].includes(ext) || m.includes("wordprocessing") || m.includes("msword") || m.startsWith("text/")) return "doc";
  if (["zip", "rar", "7z", "gz", "tgz", "tar", "bz2", "xz"].includes(ext) || m.includes("zip") || m.includes("compressed")) return "archive";
  return "other";
}

export function AttachmentsPage() {
  const t = useT();
  const lang = useLang();
  const gap = useWindowControlsGap(true);
  const setMainView = useUi((s) => s.setMainView);
  const [text, setText] = useState("");
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState<FileKind | "all">("all");
  const [saving, setSaving] = useState<string | null>(null);

  useEffect(() => {
    const timer = setTimeout(() => setQuery(text.trim()), 250);
    return () => clearTimeout(timer);
  }, [text]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !document.querySelector("[role=dialog]")) setMainView("list");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setMainView]);

  const list = useQuery({
    queryKey: ["attachments", query],
    queryFn: () => api.mailAttachments(query),
    placeholderData: keepPreviousData,
  });
  const files = list.data?.files;
  const counts = useMemo(() => {
    const c: Partial<Record<FileKind | "all", number>> = { all: files?.length ?? 0 };
    for (const f of files ?? []) {
      const k = fileKind(f.name, f.mime);
      c[k] = (c[k] ?? 0) + 1;
    }
    return c;
  }, [files]);
  const shown = useMemo(
    () => (files ?? []).filter((f) => kind === "all" || fileKind(f.name, f.mime) === kind),
    [files, kind],
  );

  const openMail = (itemId: string, accountId: string) => {
    const ui = useUi.getState();
    ui.setMainView("list");
    ui.setScope({ kind: "account", accountId });
    ui.open(itemId);
  };

  const save = async (f: AttachmentRow) => {
    const key = `${f.item_id}:${f.index}`;
    setSaving(key);
    try {
      if (await saveAttachment(f.item_id, f.index, f.name)) toast(t("mail.saved", { name: f.name }));
    } catch (err) {
      toast(errorText(t, err));
    } finally {
      setSaving(null);
    }
  };

  const zh = lang === "zh" ? "zh" : "en";
  const pending = list.data?.not_downloaded ?? [];

  return (
    <section className="flex min-w-0 flex-1 flex-col bg-bg">
      <header
        data-tauri-drag-region
        className="flex h-11 shrink-0 items-center gap-2 border-b border-line pl-4 pr-2"
        style={gap ? { paddingRight: gap + 8 } : undefined}
      >
        <Paperclip size={15} strokeWidth={1.75} className="text-fg-3" />
        <h1 data-tauri-drag-region className="min-w-0 flex-1 truncate text-[13px] font-semibold">
          {t("files.title")}
        </h1>
        <IconButton label={t("action.close")} onClick={() => setMainView("list")}>
          <X {...ICON} />
        </IconButton>
      </header>

      <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-line px-4 py-2">
        <div className="flex h-7 min-w-[200px] flex-1 items-center gap-2 rounded-md border border-line bg-surface px-2 focus-within:border-accent touch:h-10">
          <Search size={13} strokeWidth={1.75} className="shrink-0 text-fg-3" />
          <input
            type="search"
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={t("files.search")}
            aria-label={t("files.search")}
            spellCheck={false}
            className="min-w-0 flex-1 bg-transparent text-[13px] text-fg outline-none placeholder:text-fg-3"
          />
          {list.isFetching && <Spinner size={12} />}
        </div>
        <div role="radiogroup" aria-label={t("files.kind")} className="flex flex-wrap gap-1">
          {KINDS.filter((k) => k.kind === "all" || (counts[k.kind] ?? 0) > 0 || kind === k.kind).map((k) => (
            <button
              key={k.kind}
              type="button"
              role="radio"
              aria-checked={kind === k.kind}
              onClick={() => setKind(k.kind)}
              className={cx(
                "h-6 rounded-full border px-2 text-[12px] transition-colors touch:h-9 touch:px-3",
                kind === k.kind ? "border-accent bg-accent-soft text-accent" : "border-line text-fg-2 hover:text-fg",
              )}
            >
              {t(k.label)}
              <span className="ml-1 tabular-nums text-fg-3">{counts[k.kind] ?? 0}</span>
            </button>
          ))}
        </div>
      </div>

      <div className={cx("min-h-0 flex-1 overflow-y-auto", list.isPlaceholderData && "opacity-60")}>
        {!list.data ? (
          <div className="flex h-full items-center justify-center">
            <Spinner />
          </div>
        ) : shown.length === 0 && pending.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-3 px-8 text-center">
            <Paperclip size={28} strokeWidth={1.25} className="text-fg-4" />
            <p className="max-w-[320px] text-[13px] text-fg-3">{t(query ? "files.noMatch" : "files.empty")}</p>
          </div>
        ) : (
          <div className="mx-auto max-w-[1080px] px-2 pb-10 pt-1">
            <ul aria-label={t("files.title")}>
              {shown.map((f) => {
                const Icon = ICONS[fileKind(f.name, f.mime)];
                const key = `${f.item_id}:${f.index}`;
                const sender = f.from_name || f.from_address || "";
                return (
                  <li key={key} className="group flex items-center gap-3 rounded-md px-2 py-1.5 hover:bg-hover touch:py-2.5">
                    <Icon size={18} strokeWidth={1.5} className="shrink-0 text-fg-3" />
                    <button
                      type="button"
                      onClick={() => openMail(f.item_id, f.account_id)}
                      title={t("files.openMail")}
                      className="min-w-0 flex-1 text-left"
                    >
                      <span className="block truncate text-[13px] text-fg">{f.name}</span>
                      <span className="flex min-w-0 items-center gap-1.5 text-[12px] text-fg-3">
                        <SenderAvatar name={sender} address={f.from_address} size={12} />
                        <span className="truncate">
                          {sender} · {f.title}
                        </span>
                      </span>
                    </button>
                    <span className="hidden w-16 shrink-0 text-right text-[12px] tabular-nums text-fg-3 sm:block">
                      {formatSize(f.size)}
                    </span>
                    <span className="w-24 shrink-0 text-right text-[12px] tabular-nums text-fg-3">
                      {shortTime(f.published_at, zh)}
                    </span>
                    <IconButton label={t("files.save", { name: f.name })} onClick={() => void save(f)}>
                      {saving === key ? <Spinner size={13} /> : <Download size={15} strokeWidth={1.75} />}
                    </IconButton>
                  </li>
                );
              })}
            </ul>
            {pending.length > 0 && kind === "all" && (
              <div className="mt-4 border-t border-line pt-3">
                <p className="px-2 pb-1 text-[12px] text-fg-3">{t("files.notDownloaded", { n: pending.length })}</p>
                <ul>
                  {pending.map((m) => (
                    <li key={m.id}>
                      <button
                        type="button"
                        onClick={() => openMail(m.id, m.source_id)}
                        className="flex w-full items-center gap-3 rounded-md px-2 py-1.5 text-left hover:bg-hover"
                      >
                        <Paperclip size={16} strokeWidth={1.5} className="shrink-0 text-fg-4" />
                        <span className="min-w-0 flex-1 truncate text-[13px] text-fg-2">
                          {m.author || m.sender_address} · {m.title}
                        </span>
                        <span className="w-24 shrink-0 text-right text-[12px] tabular-nums text-fg-3">
                          {shortTime(m.published_at, zh)}
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </div>
        )}
      </div>
    </section>
  );
}
