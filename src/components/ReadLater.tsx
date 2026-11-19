// 稍后读：把任意网页存下来，和订阅的文章一样读。
// - 列表顶上的输入框：贴网址回车存下；旁边「导入」读 Pocket、Instapaper 的导出文件或浏览器书签。
// - 在别处（不在输入框里）粘贴一个网址，直接存进稍后读。
// - 文章、邮件里右键链接 →「存到稍后读」（见 Reader.tsx）。
// 存的时候核心把网页的标题、封面和正文一起抓下来存好，见 crates/core/src/later.rs。

import { useQueryClient } from "@tanstack/react-query";
import { Bookmark, Import } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { errorText, useT } from "../lib/i18n";
import { isMac } from "../lib/platform";
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

/** 列表顶上：贴网址存下、导入。 */
export function LaterBar() {
  const t = useT();
  const qc = useQueryClient();
  const save = useSaveLater();
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState<"save" | "import" | null>(null);
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
    </form>
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
