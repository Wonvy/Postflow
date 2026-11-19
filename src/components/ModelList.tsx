// 语音模型的列表：识别（设置 → 字幕）和朗读（设置 → 朗读）共用。每个模型一行：介绍、大小、许可，
// 以及下载（带进度）、从文件夹导入、选用、删除。

import { Check, Download } from "lucide-react";
import { useState } from "react";
import type { SpeechModel } from "../lib/api";
import { errorText, useLang, useT } from "../lib/i18n";
import { formatSize, openExternal, pickDirectory } from "../lib/platform";
import { languageName } from "../lib/translate";
import { isTauri } from "../lib/transport";
import { Button, Spinner, TextInput } from "./ui";

export interface ModelDownloadState {
  received: number;
  total: number;
  state: "downloading" | "installed" | "failed";
  error?: string;
}

export function ModelList({
  models,
  downloads,
  busy,
  selected,
  recommend,
  onSelect,
  onDownload,
  onRemove,
  onImport,
  errorOf = (err) => err,
}: {
  models: SpeechModel[];
  downloads: Record<string, ModelDownloadState>;
  /** 正在做的操作（`remove:ID`、`select:ID`、`import:ID`），对应的按钮转圈。 */
  busy: string | null;
  /** 正在用的模型（只有能「选用」的列表才有）。 */
  selected?: string | null;
  /** 没装的、推荐给界面语言的模型标上「推荐」。 */
  recommend: boolean;
  onSelect?: (m: SpeechModel) => void;
  onDownload: (m: SpeechModel) => void;
  onRemove: (m: SpeechModel) => void;
  /** 从文件夹导入，成功了要返回 true（收起输入框）。 */
  onImport: (m: SpeechModel, path: string) => Promise<boolean>;
  /** 把核心的错误换成这类模型的说法。 */
  errorOf?: (err: unknown) => unknown;
}) {
  const t = useT();
  const lang = useLang();
  const [importing, setImporting] = useState<string | null>(null);
  const [folder, setFolder] = useState("");

  const importFrom = async (m: SpeechModel, path: string) => {
    if (!path.trim()) return;
    if (await onImport(m, path.trim())) {
      setImporting(null);
      setFolder("");
    }
  };

  const languages = (m: SpeechModel) =>
    m.languages.includes("multi")
      ? t("subtitles.manyLanguages")
      : m.languages
          .slice(0, 6)
          .map((code) => languageName(code, lang))
          .join(lang === "zh" ? "、" : ", ") + (m.languages.length > 6 ? t("subtitles.andMore", { n: m.languages.length - 6 }) : "");

  return (
    <ul className="flex flex-col gap-1.5">
      {models.map((m) => {
        const d = downloads[m.id];
        const downloading = !m.installed && (d?.state === "downloading" || m.received !== null);
        const received = d?.state === "downloading" ? d.received : (m.received ?? 0);
        const percent = m.size > 0 ? Math.min(100, Math.floor((received / m.size) * 100)) : 0;
        const inUse = !!onSelect && selected === m.id;
        const recommended = recommend && !m.installed && m.recommended.includes(lang);
        return (
          <li key={m.id} className="rounded-md border border-line px-3 py-2">
            <div className="flex items-start gap-3">
              <div className="min-w-0 flex-1">
                <p className="font-medium text-fg">
                  {m.name}
                  <span className="ml-1.5 text-[12px] font-normal text-fg-3">{formatSize(m.size)}</span>
                  {recommended && (
                    <span className="ml-1.5 rounded bg-accent-soft px-1.5 py-px text-[11px] font-normal text-accent">
                      {t("subtitles.recommended")}
                    </span>
                  )}
                  {inUse && (
                    <span className="ml-1.5 inline-flex items-center gap-0.5 text-[11px] font-normal text-accent">
                      <Check size={12} />
                      {t("subtitles.inUse")}
                    </span>
                  )}
                </p>
                <p className="mt-0.5 text-[12px] leading-relaxed text-fg-3">{m.desc[lang] ?? m.desc.en ?? ""}</p>
                <p className="mt-0.5 text-[11px] leading-relaxed text-fg-3">
                  {languages(m)}
                  {m.license && (
                    <>
                      {" · "}
                      <button
                        type="button"
                        className="underline decoration-dotted underline-offset-2 hover:text-accent"
                        onClick={() => void openExternal(m.license_url || m.homepage)}
                      >
                        {m.license}
                      </button>
                    </>
                  )}
                  {m.retired && ` · ${t("subtitles.retired")}`}
                </p>
              </div>
              <div className="flex shrink-0 items-center gap-1.5 pt-0.5">
                {!m.supported ? (
                  <span className="text-[12px] text-fg-3">{t("subtitles.needsUpdate")}</span>
                ) : m.installed ? (
                  <>
                    {onSelect && !inUse && (
                      <Button className="h-6 px-2 text-[12px]" busy={busy === `select:${m.id}`} onClick={() => onSelect(m)}>
                        {t("subtitles.use")}
                      </Button>
                    )}
                    <Button
                      variant="ghost"
                      className="h-6 px-2 text-[12px]"
                      busy={busy === `remove:${m.id}`}
                      onClick={() => onRemove(m)}
                    >
                      {t("action.remove")}
                    </Button>
                  </>
                ) : downloading ? (
                  <span className="tabular inline-flex items-center gap-1.5 text-[12px] text-fg-3" aria-live="polite">
                    <Spinner size={12} />
                    {percent}%
                  </span>
                ) : (
                  <>
                    <Button
                      variant={recommended ? "primary" : "secondary"}
                      className="h-6 px-2 text-[12px]"
                      onClick={() => onDownload(m)}
                    >
                      <Download size={12} />
                      {d?.state === "failed" ? t("action.retry") : t("action.download")}
                    </Button>
                    <Button
                      variant="ghost"
                      className="h-6 px-2 text-[12px]"
                      busy={busy === `import:${m.id}`}
                      onClick={async () => {
                        // 桌面应用里直接选文件夹；网页里文件夹在核心那台电脑上，只能填路径
                        const picked = isTauri ? await pickDirectory() : null;
                        if (picked) void importFrom(m, picked);
                        else if (!isTauri) setImporting(importing === m.id ? null : m.id);
                      }}
                    >
                      {t("subtitles.import")}
                    </Button>
                  </>
                )}
              </div>
            </div>
            {d?.state === "failed" && !m.installed && (
              <p className="mt-1.5 text-[12px] text-danger">
                {errorText(t, errorOf({ code: d.error ?? "speech_download_failed" }))} {t("subtitles.importTip")}
              </p>
            )}
            {importing === m.id && !m.installed && (
              <form
                className="mt-2 flex items-center gap-2"
                onSubmit={(e) => {
                  e.preventDefault();
                  void importFrom(m, folder);
                }}
              >
                <TextInput
                  autoFocus
                  className="h-7 flex-1"
                  placeholder={t("subtitles.importPath")}
                  value={folder}
                  onChange={(e) => setFolder(e.target.value)}
                />
                <Button type="submit" className="h-7" busy={busy === `import:${m.id}`} disabled={!folder.trim()}>
                  {t("subtitles.import")}
                </Button>
              </form>
            )}
          </li>
        );
      })}
    </ul>
  );
}
