// 设置里的「字幕」：开关，以及语音识别模型的下载、选用、删除。
//
// 模型不随安装包带，在这里按需下载到本机（核心的 speech/ 目录），可以装好几个、随时换。
// 目录来自仓库里的 public/catalog/speech.json，核心每天联网看一次有没有新的。

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, Download, FolderOpen } from "lucide-react";
import { useState } from "react";
import { api, type SpeechModel } from "../lib/api";
import { errorText, useLang, useT } from "../lib/i18n";
import { formatSize, openExternal, pickDirectory, revealFolder } from "../lib/platform";
import { usePlayer } from "../lib/player";
import { startLocal, useSubtitles } from "../lib/subtitles";
import { languageName } from "../lib/translate";
import { isTauri } from "../lib/transport";
import { SettingRow } from "./SettingsParts";
import { toast } from "./Toast";
import { Button, Spinner, Switch, TextInput } from "./ui";

export function SubtitlesSettings({ open }: { open: boolean }) {
  const t = useT();
  const lang = useLang();
  const qc = useQueryClient();
  const on = useSubtitles((s) => s.on);
  const setOn = useSubtitles((s) => s.setOn);
  const downloads = useSubtitles((s) => s.downloads);
  const overview = useQuery({
    queryKey: ["speech"],
    queryFn: api.speechOverview,
    enabled: open,
    staleTime: 0,
  });
  const [busy, setBusy] = useState<string | null>(null);
  const [importing, setImporting] = useState<string | null>(null);
  const [folder, setFolder] = useState("");

  const run = async (key: string, action: () => Promise<unknown>) => {
    setBusy(key);
    try {
      await action();
      await qc.invalidateQueries({ queryKey: ["speech"] });
    } catch (err) {
      toast(errorText(t, err));
    } finally {
      setBusy(null);
    }
  };

  const download = (m: SpeechModel) => {
    useSubtitles.setState((s) => ({
      downloads: { ...s.downloads, [m.id]: { received: 0, total: m.size, state: "downloading" } },
    }));
    api.speechDownload(m.id).catch((err) => {
      useSubtitles.setState((s) => ({
        downloads: { ...s.downloads, [m.id]: { received: 0, total: m.size, state: "failed" } },
      }));
      toast(errorText(t, err));
    });
  };

  const importFrom = async (m: SpeechModel, path: string) => {
    if (!path.trim()) return;
    await run(`import:${m.id}`, async () => {
      await api.speechImport(m.id, path.trim());
      setImporting(null);
      setFolder("");
      // 正等着模型的字幕接着开始
      const current = usePlayer.getState().current;
      if (useSubtitles.getState().needsModel && current) void startLocal(current.id);
    });
  };

  const languages = (m: SpeechModel) =>
    m.languages.includes("multi")
      ? t("subtitles.manyLanguages")
      : m.languages
          .slice(0, 6)
          .map((code) => languageName(code, lang))
          .join(lang === "zh" ? "、" : ", ") + (m.languages.length > 6 ? t("subtitles.andMore", { n: m.languages.length - 6 }) : "");

  const models = overview.data?.models ?? [];
  const installed = models.filter((m) => m.installed);

  return (
    <div className="flex flex-col gap-4">
      <SettingRow label={t("subtitles.show")} hint={t("subtitles.showHint")}>
        <Switch label={t("subtitles.show")} checked={on} onChange={setOn} />
      </SettingRow>

      <div>
        <div className="text-fg">{t("subtitles.models")}</div>
        <div className="mt-0.5 text-[12px] leading-relaxed text-fg-3">{t("subtitles.modelsHint")}</div>
      </div>

      {overview.isPending && (
        <div className="flex justify-center py-6 text-fg-3">
          <Spinner />
        </div>
      )}
      {overview.isError && <p className="text-[12px] text-danger">{errorText(t, overview.error)}</p>}

      <ul className="flex flex-col gap-1.5">
        {models.map((m) => {
          const d = downloads[m.id];
          const downloading = !m.installed && (d?.state === "downloading" || m.received !== null);
          const received = d?.state === "downloading" ? d.received : (m.received ?? 0);
          const percent = m.size > 0 ? Math.min(100, Math.floor((received / m.size) * 100)) : 0;
          const selected = overview.data?.selected === m.id;
          const recommended = !m.installed && installed.length === 0 && m.recommended.includes(lang);
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
                    {selected && (
                      <span className="ml-1.5 inline-flex items-center gap-0.5 text-[11px] font-normal text-accent">
                        <Check size={12} />
                        {t("subtitles.inUse")}
                      </span>
                    )}
                  </p>
                  <p className="mt-0.5 text-[12px] leading-relaxed text-fg-3">
                    {m.desc[lang] ?? m.desc.en ?? ""}
                  </p>
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
                      {!selected && (
                        <Button
                          className="h-6 px-2 text-[12px]"
                          busy={busy === `select:${m.id}`}
                          onClick={() => void run(`select:${m.id}`, () => api.speechSelect(m.id))}
                        >
                          {t("subtitles.use")}
                        </Button>
                      )}
                      <Button
                        variant="ghost"
                        className="h-6 px-2 text-[12px]"
                        busy={busy === `remove:${m.id}`}
                        onClick={() => void run(`remove:${m.id}`, () => api.speechRemove(m.id))}
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
                        onClick={() => download(m)}
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
                  {errorText(t, { code: d.error ?? "speech_download_failed" })} {t("subtitles.importTip")}
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

      <SettingRow label={t("subtitles.saved")} hint={t("subtitles.savedHint")}>
        <Button
          busy={busy === "clear"}
          onClick={() =>
            void run("clear", async () => {
              await api.subtitlesClear();
              await qc.invalidateQueries({ queryKey: ["subtitles"] });
              toast(t("subtitles.cleared"));
            })
          }
        >
          {t("subtitles.clear")}
        </Button>
      </SettingRow>

      <div className="flex flex-wrap items-center gap-2">
        {isTauri && overview.data && (
          <Button onClick={() => void revealFolder(overview.data.dir)}>
            <FolderOpen size={14} />
            {t("subtitles.openFolder")}
          </Button>
        )}
      </div>
      {overview.data && (
        <p className="select-text truncate text-[11px] text-fg-3" title={overview.data.dir}>
          {t("subtitles.folder", { dir: overview.data.dir })}
        </p>
      )}
      <p className="text-[11px] leading-relaxed text-fg-3">{t("subtitles.privacy")}</p>
    </div>
  );
}
