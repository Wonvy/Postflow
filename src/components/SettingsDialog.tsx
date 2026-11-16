import { useQueryClient } from "@tanstack/react-query";
import { type ChangeEvent, type ReactNode, useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { errorText, useT } from "../lib/i18n";
import { exportOpml } from "../lib/platform";
import { keys, useDevices, useSettings, useStatus } from "../lib/queries";
import { type Density, type LangPref, type ThemePref, useUi } from "../lib/store";
import { INTERVALS, intervalLabel } from "./FeedDialogs";
import { Button, Modal, Segmented, Select, TextInput } from "./ui";

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="border-b border-line px-4 py-4 last:border-b-0">
      <h2 className="mb-3 text-[12px] font-medium text-fg-3">{title}</h2>
      <div className="flex flex-col gap-3">{children}</div>
    </section>
  );
}

function SettingRow({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="min-w-0">
        <div className="text-fg">{label}</div>
        {hint && <div className="mt-0.5 text-[12px] leading-relaxed text-fg-3">{hint}</div>}
      </div>
      <div className="flex shrink-0 items-center gap-2">{children}</div>
    </div>
  );
}

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
