import { ArrowUpCircle, ExternalLink, TriangleAlert } from "lucide-react";
import { useMemo } from "react";
import { type ChangelogEntry, entriesBetween, entryText, parseChangelog } from "../lib/changelog";
import { useLang, useT } from "../lib/i18n";
import { openExternal, formatSize } from "../lib/platform";
import { useStatus } from "../lib/queries";
import { installUpdate, RELEASES_URL, useUpdater } from "../lib/updater";
import { Markdown } from "./Markdown";
import { Button, cx, IconButton, Modal, Spinner } from "./ui";

/** 一个版本的更新说明：版本号、日期，下面是列表。 */
export function NotesSection({ entry }: { entry: ChangelogEntry }) {
  const lang = useLang();
  return (
    <section>
      <h3 className="mb-1.5 flex items-baseline gap-2 text-[13px] font-semibold text-fg">
        {entry.version}
        {entry.date && <span className="tabular text-[12px] font-normal text-fg-3">{entry.date}</span>}
      </h3>
      <Markdown text={entryText(entry, lang)} className="text-[13px] leading-relaxed text-fg-2" />
    </section>
  );
}

/**
 * 更新窗口里的更新说明：从当前版本之后到新版本的每一段，跳过了几个版本也都列出来。
 * 说明不是 CHANGELOG 的格式时原样显示；没有说明时提示去发布页看。
 */
function ReleaseNotes({ notes, current, next }: { notes: string | null; current: string | null; next: string }) {
  const t = useT();
  const entries = useMemo(() => parseChangelog(notes ?? ""), [notes]);
  const shown = entriesBetween(entries, current, next);
  if (shown.length === 0 && !notes) {
    return <p className="text-[12px] leading-relaxed text-fg-3">{t("update.noNotes")}</p>;
  }
  return (
    <div className="flex max-h-[min(320px,40vh)] flex-col gap-4 overflow-y-auto rounded-lg border border-line bg-hover/40 px-3.5 py-3">
      {shown.length > 0 ? (
        shown.map((entry) => <NotesSection key={entry.version} entry={entry} />)
      ) : (
        <Markdown text={notes ?? ""} className="text-[13px] leading-relaxed text-fg-2" />
      )}
    </div>
  );
}

/** 侧栏顶上的更新图标：有新版本时一直在，正在更新时转圈。点开是更新窗口。 */
export function UpdateButton() {
  const t = useT();
  const phase = useUpdater((s) => s.phase);
  const version = useUpdater((s) => s.version);
  const setDialogOpen = useUpdater((s) => s.setDialogOpen);
  const busy = phase === "downloading" || phase === "installing";
  if (!version || !(phase === "available" || busy || phase === "error")) return null;

  return (
    <IconButton
      label={busy ? t("update.updating") : t("update.available", { version })}
      onClick={() => setDialogOpen(true)}
      className="relative text-accent hover:text-accent"
    >
      {busy ? <Spinner size={14} /> : <ArrowUpCircle size={16} strokeWidth={2} />}
      {!busy && <span className="absolute right-1 top-1 size-1.5 rounded-full bg-accent ring-2 ring-sidebar" />}
    </IconButton>
  );
}

/**
 * 发现新版本时右下角弹出的提示。每个版本只弹一次：点「查看更新」或「稍后」后就不再弹，
 * 侧栏顶上的更新图标会一直留着。
 */
export function UpdateNotice() {
  const t = useT();
  const status = useStatus();
  const phase = useUpdater((s) => s.phase);
  const version = useUpdater((s) => s.version);
  const seen = useUpdater((s) => s.seen);
  const dialogOpen = useUpdater((s) => s.dialogOpen);
  const setDialogOpen = useUpdater((s) => s.setDialogOpen);
  const markSeen = useUpdater((s) => s.markSeen);
  if (phase !== "available" || !version || version === seen || dialogOpen) return null;

  return (
    <div
      role="status"
      className="anim-pop fixed bottom-5 right-5 z-40 flex w-[300px] gap-3 rounded-lg bg-surface p-3.5 shadow-pop"
    >
      <ArrowUpCircle size={18} strokeWidth={2} className="mt-px shrink-0 text-accent" />
      <div className="min-w-0 flex-1">
        <p className="text-[13px] font-medium text-fg">{t("update.available", { version })}</p>
        {status.data?.version && (
          <p className="mt-0.5 text-[12px] text-fg-3">{t("update.current", { version: status.data.version })}</p>
        )}
        <div className="mt-3 flex gap-2">
          <Button variant="primary" onClick={() => setDialogOpen(true)}>
            {t("update.view")}
          </Button>
          <Button variant="ghost" onClick={markSeen}>
            {t("update.later")}
          </Button>
        </div>
      </div>
    </div>
  );
}

export function UpdateDialog() {
  const t = useT();
  const status = useStatus();
  const { dialogOpen, setDialogOpen, phase, version, notes, downloaded, total, error, supported, needsPassword } =
    useUpdater();
  const busy = phase === "downloading" || phase === "installing";
  const percent = total ? Math.min(100, Math.round((downloaded / total) * 100)) : null;
  const releaseUrl = version ? `${RELEASES_URL}/tag/v${version}` : RELEASES_URL;

  return (
    <Modal
      open={dialogOpen && version !== null}
      onClose={() => !busy && setDialogOpen(false)}
      title={t("update.title")}
      width={500}
    >
      <div className="flex flex-col gap-4 p-4">
        <div>
          <p className="text-[15px] font-semibold">{t("update.available", { version: version ?? "" })}</p>
          <p className="mt-1 text-[12px] text-fg-3">
            {t("update.current", { version: status.data?.version ?? "" })} ·{" "}
            <button
              type="button"
              onClick={() => void openExternal(releaseUrl)}
              className="inline-flex items-center gap-1 text-accent underline-offset-2 hover:underline"
            >
              {t("update.releasePage")}
              <ExternalLink size={11} />
            </button>
          </p>
        </div>

        {version && <ReleaseNotes notes={notes} current={status.data?.version ?? null} next={version} />}

        {!supported && <p className="text-[13px] leading-relaxed text-fg-2">{t("update.manual")}</p>}
        {supported && needsPassword && phase !== "error" && (
          <p className="text-[13px] leading-relaxed text-fg-2">{t("update.password")}</p>
        )}

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
            <span className="min-w-0 break-words">
              {error === "elevation_declined" ? t("update.elevationDeclined") : t("update.failed", { error })}
            </span>
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
