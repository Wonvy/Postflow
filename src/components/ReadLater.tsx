// 稍后读：把任意网页存下来，和订阅的文章一样读。
// - 列表顶上的输入框：贴网址回车存下；旁边「导入」读 Pocket、Instapaper 的导出文件或浏览器书签。
// - 在别处（不在输入框里）粘贴一个网址，直接存进稍后读。
// - 文章、邮件里右键链接 →「存到稍后读」（见 Reader.tsx）。
// - 浏览器里点「存到 Postflow」书签按钮：桌面应用收到 postflow://save?url=…（见 src-tauri/src/deep_link.rs），
//   网页版打开 /?save=… 。
// 存的时候核心把网页的标题、封面和正文一起抓下来存好，见 crates/core/src/later.rs。

import { useQueryClient } from "@tanstack/react-query";
import { Bookmark, BookmarkPlus, Copy, Import } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { errorText, useT } from "../lib/i18n";
import { copyText, isMac } from "../lib/platform";
import { isTauri } from "../lib/transport";
import { keys } from "../lib/queries";
import { toast } from "./Toast";
import { Button, cx, Spinner } from "./ui";

/** 一段文字是不是只有一个网址。 */
export function singleUrl(text: string): string | null {
  const s = text.trim();
  if (!/^https?:\/\/\S+$/i.test(s)) return null;
  try {
    const url = new URL(s);
    return url.hostname.includes(".") || url.hostname === "localhost" ? url.href : null;
  } catch {
    return null;
  }
}

/** 存一个网页，存完提示一下、刷新列表和计数。 */
export function useSaveLater() {
  const t = useT();
  const qc = useQueryClient();
  return async (url: string): Promise<boolean> => {
    try {
      const saved = await api.laterSave(url);
      void qc.invalidateQueries({ queryKey: ["items"] });
      void qc.invalidateQueries({ queryKey: keys.nav });
      toast(
        saved.warning
          ? t("later.savedNoText", { title: saved.title })
          : t(saved.existed ? "later.existed" : "later.saved", { title: saved.title }),
      );
      return true;
    } catch (err) {
      toast(t("later.saveFailed", { error: errorText(t, err) }));
      return false;
    }
  };
}

/** 在输入框外面粘贴一个网址：存进稍后读。 */
export function usePasteToSave() {
  const save = useSaveLater();
  const saveRef = useRef(save);
  saveRef.current = save;
  useEffect(() => {
    const onPaste = (e: ClipboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target?.closest("input, textarea, select, [contenteditable=''], [contenteditable='true']")) return;
      const url = singleUrl(e.clipboardData?.getData("text/plain") ?? "");
      if (!url) return;
      e.preventDefault();
      void saveRef.current(url);
    };
    document.addEventListener("paste", onPaste);
    return () => document.removeEventListener("paste", onPaste);
  }, []);
}

/** 桌面应用的书签按钮：把当前网页交给 Postflow。 */
export const DESKTOP_BOOKMARKLET =
  "javascript:location.href='postflow://save?url='+encodeURIComponent(location.href)";

/** 网页版的书签按钮：在新标签页里打开 Postflow，带着要存的地址。 */
export function webBookmarklet(origin: string): string {
  return `javascript:window.open('${origin}/?save='+encodeURIComponent(location.href),'_blank')`;
}

/**
 * 从外面来的「存到 Postflow」：桌面应用收 postflow:// 链接转来的事件（应用被链接拉起来时，先取走
 * 早到的那几个）；网页版看地址栏里的 ?save=，存完把它从地址里去掉。
 */
export function useExternalSaves() {
  const save = useSaveLater();
  const saveRef = useRef(save);
  saveRef.current = save;
  useEffect(() => {
    if (!isTauri) {
      const params = new URLSearchParams(window.location.search);
      const target = singleUrl(params.get("save") ?? "");
      if (target) {
        params.delete("save");
        const rest = params.toString();
        window.history.replaceState(null, "", `${window.location.pathname}${rest ? `?${rest}` : ""}`);
        void saveRef.current(target);
      }
      return;
    }
    let stop: (() => void) | undefined;
    let alive = true;
    void (async () => {
      const [{ listen }, { invoke }] = await Promise.all([import("@tauri-apps/api/event"), import("@tauri-apps/api/core")]);
      const off = await listen<string>("postflow://save-later", ({ payload }) => {
        // 事件到了就不用再等「取走」：清掉那份，免得存两次
        void invoke<string[]>("pending_saves").catch(() => []);
        const target = singleUrl(payload);
        if (target) void saveRef.current(target);
      });
      if (!alive) {
        off();
        return;
      }
      stop = off;
      const early = await invoke<string[]>("pending_saves").catch(() => [] as string[]);
      for (const url of early) {
        const target = singleUrl(url);
        if (target) void saveRef.current(target);
      }
    })();
    return () => {
      alive = false;
      stop?.();
    };
  }, []);
}

/** 拖到浏览器书签栏的「存到 Postflow」按钮，和复制它的代码。 */
function BookmarkletPanel() {
  const t = useT();
  const code = isTauri ? DESKTOP_BOOKMARKLET : webBookmarklet(window.location.origin);
  const link = useRef<HTMLAnchorElement>(null);
  // React 不让在 href 里写 javascript: 地址，直接设到元素上
  useEffect(() => {
    link.current?.setAttribute("href", code);
  }, [code]);
  return (
    <div className="flex shrink-0 flex-col gap-2 border-b border-line bg-hover px-3 py-2.5 text-[12px] leading-relaxed text-fg-2">
      <p>{t(isTauri ? "later.bookmarkletHint" : "later.bookmarkletHintWeb")}</p>
      <div className="flex flex-wrap items-center gap-2">
        <a
          ref={link}
          draggable
          onClick={(e) => e.preventDefault()}
          className="inline-flex h-7 cursor-grab items-center gap-1.5 rounded-md border border-line-strong bg-surface px-2.5 font-medium text-fg"
        >
          <Bookmark size={13} />
          {t("later.bookmarkletName")}
        </a>
        <button
          type="button"
          onClick={() => void copyText(code).then(() => toast(t("later.bookmarkletCopied")))}
          className="inline-flex h-7 items-center gap-1 rounded-md px-2 text-fg-3 hover:bg-surface hover:text-fg"
        >
          <Copy size={13} />
          {t("later.bookmarkletCopy")}
        </button>
      </div>
    </div>
  );
}

/** 列表顶上：贴网址存下、导入。 */
export function LaterBar() {
  const t = useT();
  const qc = useQueryClient();
  const save = useSaveLater();
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState<"save" | "import" | null>(null);
  const [helpOpen, setHelpOpen] = useState(false);
  const file = useRef<HTMLInputElement>(null);
  const url = singleUrl(value) ?? (value.trim() && !/\s/.test(value.trim()) ? value.trim() : null);

  const submit = async () => {
    if (!url || busy) return;
    setBusy("save");
    if (await save(url)) setValue("");
    setBusy(null);
  };

  const importFile = async (f: File) => {
    setBusy("import");
    try {
      const { found, added } = await api.laterImport(await f.text());
      void qc.invalidateQueries({ queryKey: ["items"] });
      void qc.invalidateQueries({ queryKey: keys.nav });
      toast(t("later.imported", { added, found }));
    } catch (err) {
      toast(t("later.importFailed", { error: errorText(t, err) }));
    } finally {
      setBusy(null);
    }
  };

  return (
    <>
    <form
      className="flex shrink-0 items-center gap-1.5 border-b border-line px-3 py-2"
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <input
        type="url"
        inputMode="url"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        placeholder={t("later.placeholder")}
        aria-label={t("later.placeholder")}
        spellCheck={false}
        className="h-7 min-w-0 flex-1 rounded-md border border-line bg-surface px-2 text-[13px] text-fg outline-none placeholder:text-fg-3 focus:border-accent touch:h-10"
      />
      <Button type="submit" variant="primary" disabled={!url} busy={busy === "save"} className="h-7 touch:h-10">
        {t("later.save")}
      </Button>
      <button
        type="button"
        title={t("later.importHint")}
        aria-label={t("later.importHint")}
        disabled={busy !== null}
        onClick={() => file.current?.click()}
        className={cx(
          "inline-flex h-7 shrink-0 items-center gap-1 rounded-md px-1.5 text-[12px] text-fg-2 transition-colors hover:bg-hover hover:text-fg touch:h-10",
          busy !== null && "opacity-50",
        )}
      >
        {busy === "import" ? <Spinner size={13} /> : <Import size={14} strokeWidth={1.75} />}
        {t("later.import")}
      </button>
      <input
        ref={file}
        type="file"
        accept=".html,.htm,.csv,.txt,text/html,text/csv,text/plain"
        className="hidden"
        onChange={(e) => {
          const f = e.target.files?.[0];
          e.target.value = "";
          if (f) void importFile(f);
        }}
      />
      <button
        type="button"
        title={t("later.bookmarklet")}
        aria-label={t("later.bookmarklet")}
        aria-expanded={helpOpen}
        onClick={() => setHelpOpen((o) => !o)}
        className={cx(
          "inline-flex size-7 shrink-0 items-center justify-center rounded-md transition-colors hover:bg-hover hover:text-fg touch:size-10",
          helpOpen ? "bg-hover text-fg" : "text-fg-3",
        )}
      >
        <BookmarkPlus size={14} strokeWidth={1.75} />
      </button>
    </form>
    {helpOpen && <BookmarkletPanel />}
    </>
  );
}

/** 稍后读里还什么都没有。 */
export function LaterEmpty() {
  const t = useT();
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 px-8 text-center">
      <Bookmark size={28} strokeWidth={1.25} className="text-fg-4" />
      <div>
        <p className="font-medium text-fg-2">{t("later.emptyTitle")}</p>
        <p className="mt-1 max-w-[300px] text-[12px] leading-relaxed text-fg-3">
          {t("later.emptyHint", { paste: isMac ? "⌘V" : "Ctrl+V" })}
        </p>
      </div>
    </div>
  );
}
