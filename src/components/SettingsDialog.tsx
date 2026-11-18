import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Activity,
  Database,
  HardDrive,
  Workflow,
  Puzzle,
  Info,
  Languages,
  type LucideIcon,
  Palette,
  Rss,
  Sparkles,
  Tags,
} from "lucide-react";
import { type ChangeEvent, type KeyboardEvent, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
// 更新说明随应用打包，关于页里不联网也能看到这个版本改了什么
import CHANGELOG from "../../CHANGELOG.md?raw";
import { api } from "../lib/api";
import { compareVersions, parseChangelog } from "../lib/changelog";
import { errorText, type MessageKey, useLang, useT } from "../lib/i18n";
import { copyText, exportOpml, formatSize, revealFolder } from "../lib/platform";
import { isTauri } from "../lib/transport";
import { checkForUpdate, useUpdater } from "../lib/updater";
import { keys, useDevices, useSettings, useStatus } from "../lib/queries";
import { type Density, type SettingsPage, type ThemePref, useUi } from "../lib/store";
import { languageName, useModelDownloads, useTranslateEngine, useTranslateLanguages } from "../lib/translate";
import { INTERVALS, intervalLabel } from "./FeedDialogs";
import { AiSettings, CategorySettings, useAiProviders } from "./AiSettings";
import { SettingRow } from "./SettingsParts";
import { NotesSection } from "./UpdateDialog";
import { Button, cx, Modal, Segmented, Select, Switch, TextInput } from "./ui";
import { StorageSettings } from "./StorageSettings";
import { LanguageSettings } from "./LanguageSettings";
import { RulesSettings } from "./RulesSettings";
import { PluginsSettings } from "./PluginsSettings";
import { RsshubServer } from "./RsshubServer";

/** 设置的各页，按组排：常用的、AI、数据和应用本身。 */
const PAGES: { page: SettingsPage; icon: LucideIcon; label: MessageKey; hint?: MessageKey; group: number }[] = [
  { page: "appearance", icon: Palette, label: "settings.appearance", group: 0 },
  { page: "feeds", icon: Rss, label: "settings.feeds", group: 0 },
  { page: "translation", icon: Languages, label: "settings.translation", group: 0 },
  { page: "ai", icon: Sparkles, label: "settings.ai", group: 1 },
  { page: "categories", icon: Tags, label: "ai.categories", hint: "ai.categoriesHint", group: 1 },
  { page: "rules", icon: Workflow, label: "rules.title", group: 1 },
  { page: "plugins", icon: Puzzle, label: "plugins.title", group: 1 },
  { page: "data", icon: Database, label: "settings.data", group: 2 },
  { page: "storage", icon: HardDrive, label: "storage.title", group: 2 },
  { page: "diagnostics", icon: Activity, label: "settings.diagnostics", group: 2 },
  { page: "about", icon: Info, label: "settings.about", group: 2 },
];

export function SettingsDialog() {
  const t = useT();
  const qc = useQueryClient();
  const dialog = useUi((s) => s.dialog);
  const open = dialog?.type === "settings";
  const asked = dialog?.type === "settings" ? dialog.page : undefined;
  const setDialog = useUi((s) => s.setDialog);
  const theme = useUi((s) => s.theme);
  const setTheme = useUi((s) => s.setTheme);
  const density = useUi((s) => s.density);
  const setDensity = useUi((s) => s.setDensity);
  const notifyMail = useUi((s) => s.notifyMail);
  const setNotifyMail = useUi((s) => s.setNotifyMail);
  const badgeUnread = useUi((s) => s.badgeUnread);
  const senderLogos = useUi((s) => s.senderLogos);
  const setSenderLogos = useUi((s) => s.setSenderLogos);
  const setBadgeUnread = useUi((s) => s.setBadgeUnread);
  const showFeeds = useUi((s) => s.showFeeds);
  const setShowFeeds = useUi((s) => s.setShowFeeds);

  const status = useStatus();
  const settings = useSettings();
  const devices = useDevices();
  const fileInput = useRef<HTMLInputElement>(null);
  // 没指定哪一页时停在上次看的那一页
  const [page, setPage] = useState<SettingsPage>("appearance");
  const [notice, setNotice] = useState<{ kind: "ok" | "error"; text: string } | null>(null);
  const [busy, setBusy] = useState<"import" | "export" | "rebuild" | null>(null);
  const [deviceName, setDeviceName] = useState("");

  useEffect(() => {
    if (open) {
      setNotice(null);
      setDeviceName(status.data?.device_name ?? "");
    }
    // 只在打开时同步一次，之后以用户的输入为准
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  useEffect(() => {
    if (open && asked) setPage(asked);
  }, [open, asked]);

  const go = (next: SettingsPage) => {
    setPage(next);
    setNotice(null);
  };

  // 上下（窄屏上是左右）方向键在各页之间切换
  const onNavKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const step =
      e.key === "ArrowDown" || e.key === "ArrowRight" ? 1 : e.key === "ArrowUp" || e.key === "ArrowLeft" ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    const at = PAGES.findIndex((p) => p.page === page);
    const next = PAGES[(at + step + PAGES.length) % PAGES.length]?.page;
    if (!next) return;
    go(next);
    document.getElementById(`settings-tab-${next}`)?.focus();
  };

  const interval =
    typeof settings.data?.refresh_interval_min === "number" ? settings.data.refresh_interval_min : 30;

  const run = async (kind: "import" | "export" | "rebuild", task: () => Promise<string | null>) => {
    setBusy(kind);
    setNotice(null);
    try {
      const text = await task();
      if (text) setNotice({ kind: "ok", text });
    } catch (err) {
      setNotice({ kind: "error", text: errorText(t, err) });
    } finally {
      setBusy(null);
    }
  };

  const onImportFile = (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = "";
    if (!file) return;
    void run("import", async () => {
      const result = await api.importOpml(await file.text());
      void qc.invalidateQueries({ queryKey: keys.nav });
      return t("settings.imported", result);
    });
  };

  const saveDeviceName = () => {
    const name = deviceName.trim();
    if (!name || name === status.data?.device_name) return;
    void api.renameDevice(name).then(() => {
      void qc.invalidateQueries({ queryKey: keys.status });
      void qc.invalidateQueries({ queryKey: keys.devices });
    });
  };

  const body: Record<SettingsPage, () => ReactNode> = {
    appearance: () => (
      <>
        <SettingRow label={t("settings.theme")}>
          <Segmented<ThemePref>
            value={theme}
            onChange={setTheme}
            options={[
              { value: "system", label: t("settings.themeSystem") },
              { value: "dark", label: t("settings.themeDark") },
              { value: "light", label: t("settings.themeLight") },
            ]}
          />
        </SettingRow>
        <SettingRow label={t("settings.density")}>
          <Segmented<Density>
            value={density}
            onChange={setDensity}
            options={[
              { value: "compact", label: t("settings.compact") },
              { value: "comfortable", label: t("settings.comfortable") },
            ]}
          />
        </SettingRow>
        <SettingRow label={t("settings.showFeeds")} hint={t("settings.showFeedsHint")}>
          <Switch label={t("settings.showFeeds")} checked={showFeeds} onChange={setShowFeeds} />
        </SettingRow>
        <SettingRow label={t("settings.senderLogos")} hint={t("settings.senderLogosHint")}>
          <Switch label={t("settings.senderLogos")} checked={senderLogos} onChange={setSenderLogos} />
        </SettingRow>
        <SettingRow label={t("settings.notifyMail")} hint={t("settings.notifyMailHint")}>
          <Switch label={t("settings.notifyMail")} checked={notifyMail} onChange={setNotifyMail} />
        </SettingRow>
        <SettingRow label={t("settings.badge")} hint={t("settings.badgeHint")}>
          <Switch label={t("settings.badge")} checked={badgeUnread} onChange={setBadgeUnread} />
        </SettingRow>
        <LanguageSettings />
      </>
    ),
    feeds: () => (
      <>
        <SettingRow label={t("settings.refreshInterval")}>
          <Select
            className="w-32"
            value={interval}
            onChange={(e) => {
              const value = Number(e.target.value);
              qc.setQueryData(keys.settings, { ...settings.data, refresh_interval_min: value });
              void api.setSetting("refresh_interval_min", value);
            }}
          >
            {INTERVALS.map((m) => (
              <option key={m} value={m}>
                {intervalLabel(t, m)}
              </option>
            ))}
          </Select>
        </SettingRow>
        <SettingRow label={t("settings.opml")} hint={t("settings.opmlHint")}>
          <input
            ref={fileInput}
            type="file"
            accept=".opml,.xml,text/xml,application/xml,text/x-opml"
            className="hidden"
            onChange={onImportFile}
          />
          <Button busy={busy === "import"} onClick={() => fileInput.current?.click()}>
            {t("settings.import")}
          </Button>
          <Button
            busy={busy === "export"}
            onClick={() =>
              void run("export", async () => {
                const n = await exportOpml();
                return n === null ? null : t("settings.exported", { n });
              })
            }
          >
            {t("settings.export")}
          </Button>
        </SettingRow>
        <div className="flex flex-col gap-2">
          <div>
            <div className="text-fg">{t("settings.rsshub")}</div>
            <div className="mt-0.5 text-[12px] leading-relaxed text-fg-3">{t("settings.rsshubHint")}</div>
          </div>
          <RsshubServer />
        </div>
      </>
    ),
    translation: () => <TranslationSettings open={open} />,
    ai: () => <AiSettings open={open} />,
    categories: () => <CategorySettings />,
    rules: () => <RulesSettings />,
    plugins: () => <PluginsSettings open={open && page === "plugins"} />,
    data: () => (
      <>
        <SettingRow label={t("settings.dataDir")}>
          <span className="max-w-[150px] select-text truncate text-[12px] text-fg-2 sm:max-w-[300px]" title={status.data?.data_dir ?? ""}>
            {status.data?.data_dir}
          </span>
        </SettingRow>
        <SettingRow label={t("settings.thisDevice")}>
          <TextInput
            className="w-48"
            value={deviceName}
            onChange={(e) => setDeviceName(e.target.value)}
            onBlur={saveDeviceName}
            onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          />
        </SettingRow>
        {devices.data && devices.data.length > 1 && (
          <SettingRow label={t("settings.devices")}>
            <span className="max-w-[150px] truncate text-[12px] text-fg-2 sm:max-w-[300px]">
              {devices.data.map((d) => d.name || d.id).join("、")}
            </span>
          </SettingRow>
        )}
        <SettingRow label={t("settings.rebuild")} hint={t("settings.rebuildHint")}>
          <Button
            busy={busy === "rebuild"}
            onClick={() =>
              void run("rebuild", async () => {
                const stats = await api.rebuildIndex();
                void qc.invalidateQueries();
                return t("settings.rebuilt", { n: stats.articles });
              })
            }
          >
            {t("settings.rebuild")}
          </Button>
        </SettingRow>
      </>
    ),
    storage: () => <StorageSettings open={open && page === "storage"} />,
    diagnostics: () => <Diagnostics open={open} />,
    about: () => (
      <>
        <SettingRow label={t("settings.version")}>
          <span className="tabular text-[12px] text-fg-2">{status.data?.version}</span>
          {isTauri && <UpdateCheck />}
        </SettingRow>
        <SettingRow label={t("welcome.reopen")} hint={t("welcome.reopenHint")}>
          <Button
            onClick={() => {
              setDialog(null);
              useUi.getState().setWelcomeStep(0);
            }}
          >
            {t("welcome.reopenButton")}
          </Button>
        </SettingRow>
        <CurrentNotes version={status.data?.version ?? null} />
      </>
    ),
  };

  const current = PAGES.find((p) => p.page === page);

  return (
    <Modal open={open} onClose={() => setDialog(null)} title={t("settings.title")} width={760}>
      <div className="flex h-[min(600px,calc(76vh-44px))] flex-col sm:flex-row">
        <div
          role="tablist"
          aria-label={t("settings.title")}
          aria-orientation="vertical"
          onKeyDown={onNavKey}
          className="flex shrink-0 gap-0.5 overflow-x-auto border-b border-line p-2 sm:w-[184px] sm:flex-col sm:overflow-x-visible sm:overflow-y-auto sm:rounded-bl-xl sm:border-b-0 sm:border-r sm:bg-sidebar"
        >
          {PAGES.map((p, i) => {
            const selected = p.page === page;
            return (
              <button
                key={p.page}
                type="button"
                role="tab"
                id={`settings-tab-${p.page}`}
                aria-selected={selected}
                aria-controls="settings-panel"
                tabIndex={selected ? 0 : -1}
                onClick={() => go(p.page)}
                className={cx(
                  "flex h-7 shrink-0 items-center gap-2 rounded-md px-2 text-left text-[13px] outline-offset-[-2px] transition-colors",
                  i > 0 && PAGES[i - 1]?.group !== p.group && "sm:mt-3",
                  selected ? "bg-active text-fg" : "text-fg-2 hover:bg-hover hover:text-fg",
                )}
              >
                <p.icon size={15} strokeWidth={1.75} className={cx("shrink-0", selected ? "text-fg" : "text-fg-3")} />
                <span className="truncate">{t(p.label)}</span>
              </button>
            );
          })}
        </div>
        <div
          role="tabpanel"
          id="settings-panel"
          aria-labelledby={`settings-tab-${page}`}
          className="flex min-h-0 min-w-0 flex-1 flex-col"
        >
          <div className="min-h-0 flex-1 overflow-y-auto px-5 pb-6 pt-4">
            {current && <h2 className="text-[15px] font-semibold text-fg">{t(current.label)}</h2>}
            {current?.hint && <p className="mt-1 text-[12px] leading-relaxed text-fg-3">{t(current.hint)}</p>}
            <div className="mt-4 flex flex-col gap-4">{body[page]()}</div>
          </div>
          {notice && (
            <p
              role="status"
              className={cx(
                "shrink-0 border-t border-line px-5 py-2.5 text-[12px]",
                notice.kind === "error" ? "text-danger" : "text-fg-2",
              )}
            >
              {notice.text}
            </p>
          )}
        </div>
      </div>
    </Modal>
  );
}

function TranslationSettings({ open }: { open: boolean }) {
  const t = useT();
  const lang = useLang();
  const qc = useQueryClient();
  const translateTo = useUi((s) => s.translateTo);
  const setTranslateTo = useUi((s) => s.setTranslateTo);
  const languages = useTranslateLanguages(open);
  const downloads = useModelDownloads((s) => s.byPair);
  const [removing, setRemoving] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const name = (code: string) => languageName(code, lang);
  const targets = [...new Set([...(languages.data?.targets ?? []), ...(translateTo ? [translateTo] : [])])].sort(
    (a, b) => name(a).localeCompare(name(b), lang === "zh" ? "zh-CN" : "en"),
  );
  const installed = languages.data?.installed ?? [];
  const active = Object.entries(downloads).filter(
    ([pair, d]) => d.state === "downloading" && !installed.some((m) => m.pair === pair),
  );

  const remove = async (pair: string) => {
    setRemoving(pair);
    setError(null);
    try {
      await api.removeModel(pair);
      await qc.invalidateQueries({ queryKey: ["translate"] });
    } catch (err) {
      setError(errorText(t, err));
    } finally {
      setRemoving(null);
    }
  };

  const engine = useTranslateEngine();
  const ai = useAiProviders(open);
  const hasAi = (ai.data?.providers.length ?? 0) > 0;

  return (
    <>
      <SettingRow label={t("settings.translateEngine")} hint={t("settings.translateEngineHint")}>
        <Select
          className="w-44"
          value={engine}
          onChange={(e) => {
            const value = e.target.value;
            qc.setQueryData(keys.settings, (old: Record<string, unknown> | undefined) => ({
              ...old,
              "ai.translate_engine": value,
            }));
            void api.setSetting("ai.translate_engine", value);
          }}
        >
          <option value="offline">{t("settings.engineOffline")}</option>
          <option value="ai" disabled={!hasAi}>
            {hasAi ? t("settings.engineAi") : t("settings.engineAiMissing")}
          </option>
        </Select>
      </SettingRow>
      <SettingRow label={t("settings.translateTo")} hint={t("settings.translateToHint")}>
        <Select className="w-44" value={translateTo ?? ""} onChange={(e) => setTranslateTo(e.target.value || null)}>
          <option value="">{t("settings.translateFollow")}</option>
          {targets.map((code) => (
            <option key={code} value={code}>
              {name(code)}
            </option>
          ))}
        </Select>
      </SettingRow>
      <div>
        <div className="text-fg">{t("settings.models")}</div>
        <div className="mt-0.5 text-[12px] leading-relaxed text-fg-3">{t("settings.modelsHint")}</div>
        {languages.data && !languages.data.catalog && (
          <div className="mt-1 text-[12px] text-fg-3">{t("settings.modelsOffline")}</div>
        )}
        <ul className="mt-2 flex flex-col">
          {installed.length === 0 && active.length === 0 && (
            <li className="text-[12px] text-fg-3">{t("settings.noModels")}</li>
          )}
          {installed.map((m) => (
            <li key={m.pair} className="flex h-8 items-center gap-3">
              <span className="min-w-0 flex-1 truncate text-fg-2">
                {name(m.from)} → {name(m.to)}
              </span>
              <span className="tabular text-[12px] text-fg-3">{formatSize(m.size)}</span>
              <Button
                variant="ghost"
                className="h-6 px-2 text-[12px]"
                busy={removing === m.pair}
                onClick={() => void remove(m.pair)}
              >
                {t("action.remove")}
              </Button>
            </li>
          ))}
          {active.map(([pair, d]) => {
            const [from = "", to = ""] = pair.split("-");
            const percent = d.total > 0 ? Math.floor((d.received / d.total) * 100) : 0;
            return (
              <li key={pair} className="flex h-8 items-center gap-3">
                <span className="min-w-0 flex-1 truncate text-fg-2">
                  {name(from)} → {name(to)}
                </span>
                <span className="tabular text-[12px] text-fg-3">{t("translate.downloading")} {percent}%</span>
              </li>
            );
          })}
        </ul>
        {error && <div className="mt-1 text-[12px] text-danger">{error}</div>}
      </div>
    </>
  );
}

/** 日志：查看、复制、打开所在目录，以及详细日志开关。 */
function Diagnostics({ open }: { open: boolean }) {
  const t = useT();
  const logs = useQuery({ queryKey: ["logs"], queryFn: () => api.logs(800), enabled: open });
  const [viewing, setViewing] = useState(false);
  const qc = useQueryClient();
  const data = logs.data;

  if (data && !data.available) {
    return <p className="text-[12px] text-fg-3">{t("settings.logsUnavailable")}</p>;
  }
  return (
    <>
      <SettingRow label={t("settings.logs")} hint={t("settings.logsHint")}>
        <Button
          onClick={() => {
            void qc.invalidateQueries({ queryKey: ["logs"] });
            setViewing(true);
          }}
        >
          {t("settings.viewLogs")}
        </Button>
        {isTauri && data?.dir && (
          <Button onClick={() => void revealFolder(data.dir ?? "")}>{t("settings.openLogFolder")}</Button>
        )}
      </SettingRow>
      <SettingRow label={t("settings.verboseLogs")} hint={t("settings.verboseLogsHint")}>
        <Switch
          label={t("settings.verboseLogs")}
          checked={data?.verbose ?? false}
          onChange={(v) => void api.setLogging(v).then(() => qc.invalidateQueries({ queryKey: ["logs"] }))}
        />
      </SettingRow>
      <LogViewer open={viewing} onClose={() => setViewing(false)} />
    </>
  );
}

function LogViewer({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useT();
  const logs = useQuery({ queryKey: ["logs"], queryFn: () => api.logs(800), enabled: open });
  const qc = useQueryClient();
  const [copied, setCopied] = useState(false);
  const pre = useRef<HTMLPreElement>(null);
  const text = logs.data?.text ?? "";

  // 打开时、刷新后滚到最新的一行
  useEffect(() => {
    if (open && pre.current) pre.current.scrollTop = pre.current.scrollHeight;
  }, [open, text]);

  return (
    <Modal open={open} onClose={onClose} title={t("settings.logs")} width={760}>
      <div className="flex flex-col gap-3 p-4">
        <p className="text-[12px] leading-relaxed text-fg-3">{t("settings.logsPrivacy")}</p>
        <pre
          ref={pre}
          className="h-[52vh] select-text overflow-auto whitespace-pre-wrap break-all rounded-md border border-line bg-hover p-3 font-mono text-[11px] leading-[1.55] text-fg-2"
        >
          {logs.isLoading ? "…" : text || t("settings.logsEmpty")}
        </pre>
        <div className="flex items-center justify-between gap-2">
          <span className="min-w-0 truncate text-[12px] text-fg-3" title={logs.data?.dir ?? ""}>
            {logs.data?.dir}
          </span>
          <div className="flex shrink-0 gap-2">
            <Button busy={logs.isFetching} onClick={() => void qc.invalidateQueries({ queryKey: ["logs"] })}>
              {t("settings.refreshLogs")}
            </Button>
            <Button
              variant="primary"
              disabled={!text}
              onClick={() =>
                void copyText(text).then(() => {
                  setCopied(true);
                  setTimeout(() => setCopied(false), 1500);
                })
              }
            >
              {copied ? t("settings.copied") : t("settings.copyLogs")}
            </Button>
          </div>
        </div>
      </div>
    </Modal>
  );
}

/** 关于页里：这个版本更新了什么。说明随应用一起打包，不用联网。 */
function CurrentNotes({ version }: { version: string | null }) {
  const t = useT();
  const entry = useMemo(
    () => (version ? parseChangelog(CHANGELOG).find((e) => compareVersions(e.version, version) === 0) : undefined),
    [version],
  );
  if (!entry) return null;
  return (
    <div className="mt-2 border-t border-line pt-4">
      <p className="mb-2.5 text-[12px] font-medium text-fg-3">{t("settings.whatsNew")}</p>
      <NotesSection entry={entry} />
    </div>
  );
}

function UpdateCheck() {
  const t = useT();
  const phase = useUpdater((s) => s.phase);
  const version = useUpdater((s) => s.version);
  const setDialogOpen = useUpdater((s) => s.setDialogOpen);

  if (phase === "available" && version) {
    return (
      <Button variant="primary" onClick={() => setDialogOpen(true)}>
        {t("settings.updateAvailable", { version })}
      </Button>
    );
  }
  const note =
    phase === "latest" ? t("settings.upToDate") : phase === "error" ? t("settings.checkFailed") : null;
  return (
    <>
      {note && <span className="text-[12px] text-fg-3">{note}</span>}
      <Button busy={phase === "checking"} onClick={() => void checkForUpdate(true)}>
        {t("settings.checkUpdates")}
      </Button>
    </>
  );
}
