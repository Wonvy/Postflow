import { ArrowUpCircle, TriangleAlert } from "lucide-react";
import { useT } from "../lib/i18n";
import { openExternal, formatSize } from "../lib/platform";
import { useStatus } from "../lib/queries";
import { installUpdate, RELEASES_URL, useUpdater } from "../lib/updater";
import { Button, cx, Modal, Tip } from "./ui";

/** 侧边栏底部的「有新版本」提示，点开是更新对话框。 */
export function UpdatePill() {
  const t = useT();
  const phase = useUpdater((s) => s.phase);
  const version = useUpdater((s) => s.version);
  const setDialogOpen = useUpdater((s) => s.setDialogOpen);
  const busy = phase === "downloading" || phase === "installing";
  if (!version || !(phase === "available" || busy || (phase === "error" && version))) return null;

  return (
    <Tip label={t("update.available", { version })} side="top">
      <button
        type="button"
        onClick={() => setDialogOpen(true)}
        className={cx(
          "ml-auto flex h-6 items-center gap-1.5 rounded-full bg-accent-soft px-2.5 text-[12px] font-medium text-accent transition-colors hover:bg-accent hover:text-white",
        )}
      >
        <ArrowUpCircle size={13} strokeWidth={2} />
        {busy ? t("update.updating") : t("update.pill")}
      </button>
    </Tip>
  );
}

export function UpdateDialog() {
  const t = useT();
  const status = useStatus();
  const { dialogOpen, setDialogOpen, phase, version, downloaded, total, error, supported } = useUpdater();
  const busy = phase === "downloading" || phase === "installing";
  const percent = total ? Math.min(100, Math.round((downloaded / total) * 100)) : null;
  const releaseUrl = version ? `${RELEASES_URL}/tag/v${version}` : RELEASES_URL;

  return (
    <Modal
      open={dialogOpen && version !== null}
      onClose={() => !busy && setDialogOpen(false)}
      title={t("update.title")}
      width={420}
    >
      <div className="flex flex-col gap-4 p-4">
        <div>
          <p className="text-[15px] font-semibold">{t("update.available", { version: version ?? "" })}</p>
          <p className="mt-1 text-[12px] text-fg-3">
            {t("update.current", { version: status.data?.version ?? "" })} ·{" "}
            <button
              type="button"
              onClick={() => void openExternal(releaseUrl)}
              className="text-accent underline-offset-2 hover:underline"
            >
              {t("update.notes")}
            </button>
          </p>
        </div>

        {!supported && <p className="text-[13px] leading-relaxed text-fg-2">{t("update.manual")}</p>}

        {busy && (
          <div>
            <div className="h-1.5 overflow-hidden rounded-full bg-active">
              <div
                className={cx("h-full rounded-full bg-accent transition-[width]", percent === null && "animate-pulse")}
                style={{ width: `${percent ?? 100}%` }}
              />
            </div>
            <p className="tabular mt-2 text-[12px] text-fg-3">
              {phase === "installing"
                ? t("update.installing")
                : total
                  ? t("update.progress", { done: formatSize(downloaded), total: formatSize(total) })
                  : t("update.downloading")}
            </p>
          </div>
        )}

        {phase === "error" && error && (
          <p role="alert" className="flex items-start gap-2 text-[13px] leading-relaxed text-danger">
            <TriangleAlert size={14} strokeWidth={2} className="mt-0.5 shrink-0" />
            <span className="min-w-0 break-words">{t("update.failed", { error })}</span>
          </p>
        )}

        <div className="flex justify-end gap-2">
          <Button variant="ghost" disabled={busy} onClick={() => setDialogOpen(false)}>
            {t("update.later")}
          </Button>
          {supported ? (
            <Button variant="primary" busy={busy} onClick={() => void installUpdate()}>
              {phase === "error" ? t("action.retry") : t("update.install")}
            </Button>
          ) : (
            <Button variant="primary" onClick={() => void openExternal(releaseUrl)}>
              {t("update.download")}
            </Button>
          )}
        </div>
      </div>
    </Modal>
  );
}
