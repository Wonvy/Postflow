import { useQueryClient } from "@tanstack/react-query";
import { TriangleAlert } from "lucide-react";
import { type FormEvent, useState } from "react";
import { api, type Status } from "../lib/api";
import { errorText, useT } from "../lib/i18n";
import { pickDirectory } from "../lib/platform";
import { keys } from "../lib/queries";
import { isTauri } from "../lib/transport";
import { Button, Field, Logo, TextInput } from "./ui";

/** 首次启动：选数据目录，给这台设备起个名字。 */
export function Onboarding({ status }: { status: Status }) {
  const t = useT();
  const qc = useQueryClient();
  const [dataDir, setDataDir] = useState(status.suggested_data_dir);
  const [deviceName, setDeviceName] = useState(status.suggested_device_name);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const choose = async () => {
    const picked = await pickDirectory(dataDir || undefined);
    if (picked) setDataDir(picked);
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!dataDir.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const next = await api.setup(dataDir.trim(), deviceName.trim());
      qc.setQueryData(keys.status, next);
    } catch (err) {
      setError(errorText(t, err));
      setBusy(false);
    }
  };

  return (
    <div data-tauri-drag-region className="flex h-full items-center justify-center bg-sidebar p-6">
      <form
        onSubmit={(e) => void submit(e)}
        className="anim-pop flex w-full max-w-[440px] flex-col gap-5 rounded-xl bg-surface p-7 shadow-pop"
      >
        <div className="flex flex-col gap-3">
          <Logo size={36} />
          <div>
            <h1 className="text-[18px] font-semibold tracking-tight">{t("onboarding.title")}</h1>
            <p className="mt-1 text-fg-3">{t("onboarding.subtitle")}</p>
          </div>
        </div>

        {status.error && !error && (
          <p className="flex items-start gap-2 rounded-md border border-line bg-hover px-3 py-2 text-[12px] text-fg-2">
            <TriangleAlert size={14} strokeWidth={2} className="mt-px shrink-0 text-warning" />
            <span className="select-text">
              {t("onboarding.previousError")}: {status.error}
            </span>
          </p>
        )}

        <Field label={t("onboarding.dataDir")} hint={t("onboarding.dataDirHint")}>
          <div className="flex gap-2">
            <TextInput
              value={dataDir}
              onChange={(e) => setDataDir(e.target.value)}
              disabled={busy}
              aria-label={t("onboarding.dataDir")}
            />
            {isTauri && (
              <Button onClick={() => void choose()} disabled={busy} className="h-8">
                {t("action.choose")}
              </Button>
            )}
          </div>
        </Field>

        <Field label={t("onboarding.deviceName")} hint={t("onboarding.deviceNameHint")}>
          <TextInput value={deviceName} onChange={(e) => setDeviceName(e.target.value)} disabled={busy} />
        </Field>

        {error && (
          <p role="alert" className="flex items-center gap-2 text-[13px] text-danger">
            <TriangleAlert size={14} strokeWidth={2} className="shrink-0" />
            {error}
          </p>
        )}

        <Button variant="primary" type="submit" busy={busy} disabled={!dataDir.trim()} className="h-8">
          {t("onboarding.start")}
        </Button>
      </form>
    </div>
  );
}
