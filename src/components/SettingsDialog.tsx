import { useQueryClient } from "@tanstack/react-query";
import { type ChangeEvent, useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { errorText, useLang, useT } from "../lib/i18n";
import { exportOpml, formatSize } from "../lib/platform";
import { isTauri } from "../lib/transport";
import { checkForUpdate, useUpdater } from "../lib/updater";
import { keys, useDevices, useSettings, useStatus } from "../lib/queries";
import { type Density, type LangPref, type SidebarLayout, type ThemePref, useUi } from "../lib/store";
import { languageName, useModelDownloads, useTranslateEngine, useTranslateLanguages } from "../lib/translate";
import { INTERVALS, intervalLabel } from "./FeedDialogs";
import { AiSettings, useAiProviders } from "./AiSettings";
import { Section, SettingRow } from "./SettingsParts";
import { Button, Modal, Segmented, Select, TextInput } from "./ui";

export function SettingsDialog() {
  const t = useT();
  const qc = useQueryClient();
  const open = useUi((s) => s.dialog?.type === "settings");
  const setDialog = useUi((s) => s.setDialog);
  const theme = useUi((s) => s.theme);
  const setTheme = useUi((s) => s.setTheme);
  const density = useUi((s) => s.density);
  const setDensity = useUi((s) => s.setDensity);
  const lang = useUi((s) => s.lang);
  const setLang = useUi((s) => s.setLang);
  const sidebarLayout = useUi((s) => s.sidebarLayout);
  const setSidebarLayout = useUi((s) => s.setSidebarLayout);

  const status = useStatus();
  const settings = useSettings();
  const devices = useDevices();
  const fileInput = useRef<HTMLInputElement>(null);
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

  return (
    <Modal open={open} onClose={() => setDialog(null)} title={t("settings.title")} width={540}>
      <Section title={t("settings.appearance")}>
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
        <SettingRow label={t("settings.sidebar")}>
          <Segmented<SidebarLayout>
            value={sidebarLayout}
            onChange={setSidebarLayout}
            options={[
              { value: "groups", label: t("settings.sidebarGroups") },
              { value: "tabs", label: t("settings.sidebarTabs") },
            ]}
          />
        </SettingRow>
        <SettingRow label={t("settings.language")}>
          <Segmented<LangPref>
            value={lang}
            onChange={setLang}
            options={[
              { value: "system", label: t("settings.langSystem") },
              { value: "zh", label: "中文" },
              { value: "en", label: "English" },
            ]}
          />
        </SettingRow>
      </Section>

      <Section title={t("settings.translation")}>
        <TranslationSettings open={open} />
      </Section>

      <Section title={t("settings.ai")}>
        <AiSettings open={open} />
      </Section>

      <Section title={t("settings.feeds")}>
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
      </Section>

      <Section title={t("settings.data")}>
        <SettingRow label={t("settings.dataDir")}>
          <span className="max-w-[300px] select-text truncate text-[12px] text-fg-2" title={status.data?.data_dir ?? ""}>
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
            <span className="max-w-[300px] truncate text-[12px] text-fg-2">
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
      </Section>

      <Section title={t("settings.about")}>
        <SettingRow label={t("settings.version")}>
          <span className="tabular text-[12px] text-fg-2">{status.data?.version}</span>
          {isTauri && <UpdateCheck />}
        </SettingRow>
      </Section>

      {notice && (
        <p
          role="status"
          className={
            "sticky bottom-0 border-t border-line bg-surface px-4 py-2.5 text-[12px] " +
            (notice.kind === "error" ? "text-danger" : "text-fg-2")
          }
        >
          {notice.text}
        </p>
      )}
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
