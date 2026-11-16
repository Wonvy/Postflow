import { useQueryClient } from "@tanstack/react-query";
import { TriangleAlert } from "lucide-react";
import { type FormEvent, useEffect, useId, useState } from "react";
import { api, type Candidate, type Feed } from "../lib/api";
import { errorText, type Translate, useT } from "../lib/i18n";
import { keys, useNav, useSettings } from "../lib/queries";
import { useUi } from "../lib/store";
import { Button, Field, Modal, Select, Spinner, TextInput } from "./ui";

/** 可选的刷新间隔，单位分钟。 */
export const INTERVALS = [15, 30, 60, 120, 360, 720, 1440];

export function intervalLabel(t: Translate, minutes: number): string {
  return minutes < 60
    ? t("interval.minutes", { n: minutes })
    : t("interval.hours", { n: minutes / 60 });
}

function FolderInput({
  value,
  onChange,
  folders,
  placeholder,
}: {
  value: string;
  onChange: (value: string) => void;
  folders: string[];
  placeholder: string;
}) {
  const listId = useId();
  return (
    <>
      <TextInput
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        list={listId}
      />
      <datalist id={listId}>
        {folders.map((f) => (
          <option key={f} value={f} />
        ))}
      </datalist>
    </>
  );
}

function useFolders(): string[] {
  const nav = useNav();
  return [...new Set((nav.data?.feeds ?? []).map((f) => f.folder).filter(Boolean))].sort();
}

export function AddFeedDialog() {
  const t = useT();
  const qc = useQueryClient();
  const open = useUi((s) => s.dialog?.type === "add-feed");
  const setDialog = useUi((s) => s.setDialog);
  const setScope = useUi((s) => s.setScope);
  const folders = useFolders();

  const [url, setUrl] = useState("");
  const [folder, setFolder] = useState("");
  const [phase, setPhase] = useState<"idle" | "searching" | "adding">("idle");
  const [candidates, setCandidates] = useState<Candidate[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setUrl("");
      setFolder("");
      setPhase("idle");
      setCandidates([]);
      setError(null);
    }
  }, [open]);

  const add = async (feedUrl: string) => {
    setPhase("adding");
    setError(null);
    try {
      const feed = await api.addFeed(feedUrl, folder.trim());
      void qc.invalidateQueries({ queryKey: keys.nav });
      void qc.invalidateQueries({ queryKey: ["items"] });
      setScope({ kind: "feed", feedId: feed.id });
      setDialog(null);
    } catch (err) {
      setError(errorText(t, err));
      setPhase("idle");
    }
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!url.trim() || phase !== "idle") return;
    setPhase("searching");
    setError(null);
    setCandidates([]);
    try {
      const found = await api.discover(url.trim());
      const only = found.length === 1 ? found[0] : undefined;
      if (only) {
        await add(only.url);
      } else {
        setCandidates(found);
        setPhase("idle");
      }
    } catch (err) {
      setError(errorText(t, err));
      setPhase("idle");
    }
  };

  const busy = phase !== "idle";

  return (
    <Modal open={open} onClose={() => setDialog(null)} title={t("add.title")}>
      <form onSubmit={(e) => void submit(e)} className="flex flex-col gap-4 p-4">
        <Field label={t("add.url")}>
          <TextInput
            autoFocus
            value={url}
            onChange={(e) => {
              setUrl(e.target.value);
              setCandidates([]);
            }}
            placeholder={t("add.urlPlaceholder")}
            inputMode="url"
            disabled={busy}
          />
        </Field>
        <Field label={t("add.folder")}>
          <FolderInput value={folder} onChange={setFolder} folders={folders} placeholder={t("add.folderPlaceholder")} />
        </Field>

        {candidates.length > 1 && (
          <div>
            <p className="mb-1.5 text-[12px] font-medium text-fg-2">{t("add.choose")}</p>
            <ul className="flex flex-col gap-px rounded-md border border-line p-1">
              {candidates.map((c) => (
                <li key={c.url}>
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => void add(c.url)}
                    className="flex w-full flex-col rounded px-2 py-1.5 text-left hover:bg-hover disabled:opacity-50"
                  >
                    <span className="truncate text-fg">{c.title || c.url}</span>
                    <span className="truncate text-[12px] text-fg-3">{c.url}</span>
                  </button>
                </li>
              ))}
            </ul>
          </div>
        )}

        {error && (
          <p role="alert" className="flex items-center gap-2 text-[13px] text-danger">
            <TriangleAlert size={14} strokeWidth={2} className="shrink-0" />
            {error}
          </p>
        )}

        <div className="flex items-center justify-end gap-2">
          {busy && (
            <span className="mr-auto flex items-center gap-2 text-[12px] text-fg-3">
              <Spinner size={12} />
              {phase === "searching" ? t("add.searching") : t("add.adding")}
            </span>
          )}
          <Button variant="ghost" onClick={() => setDialog(null)}>
            {t("action.cancel")}
          </Button>
          <Button variant="primary" type="submit" disabled={!url.trim() || busy}>
            {t("action.add")}
          </Button>
        </div>
      </form>
    </Modal>
  );
}

export function EditFeedDialog() {
  const dialog = useUi((s) => s.dialog);
  const setDialog = useUi((s) => s.setDialog);
  const nav = useNav();
  const feedId = dialog?.type === "edit-feed" ? dialog.feedId : null;
  const feed = nav.data?.feeds.find((f) => f.id === feedId);
  const t = useT();

  return (
    <Modal open={feed !== undefined} onClose={() => setDialog(null)} title={t("edit.title")}>
      {/* 用 key 让每个订阅源都有一份全新的表单状态 */}
      {feed && <EditFeedForm key={feed.id} feed={feed} onDone={() => setDialog(null)} />}
    </Modal>
  );
}

function EditFeedForm({ feed, onDone }: { feed: Feed; onDone: () => void }) {
  const t = useT();
  const qc = useQueryClient();
  const folders = useFolders();
  const settings = useSettings();
  const [title, setTitle] = useState(feed.title);
  const [folder, setFolder] = useState(feed.folder);
  const [interval, setIntervalMin] = useState<number | null>(feed.interval_min);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const globalDefault =
    typeof settings.data?.refresh_interval_min === "number" ? settings.data.refresh_interval_min : 30;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setSaving(true);
    setError(null);
    try {
      // 只提交真正改过的字段。同步是按字段合并的，原样写回没改的字段
      // 会盖掉另一台设备刚对那个字段做的修改。
      const patch: Parameters<typeof api.updateFeed>[1] = {};
      if (title.trim() && title.trim() !== feed.title) patch.title = title.trim();
      if (folder.trim() !== feed.folder) patch.folder = folder.trim();
      if (interval !== feed.interval_min) patch.interval_min = interval;
      if (Object.keys(patch).length > 0) {
        await api.updateFeed(feed.id, patch);
        void qc.invalidateQueries({ queryKey: keys.nav });
        void qc.invalidateQueries({ queryKey: ["items"] });
      }
      onDone();
    } catch (err) {
      setError(errorText(t, err));
      setSaving(false);
    }
  };

  return (
    <form onSubmit={(e) => void submit(e)} className="flex flex-col gap-4 p-4">
      <Field label={t("edit.name")}>
        <TextInput autoFocus value={title} onChange={(e) => setTitle(e.target.value)} />
      </Field>
      <Field label={t("edit.folder")}>
        <FolderInput value={folder} onChange={setFolder} folders={folders} placeholder={t("add.folderPlaceholder")} />
      </Field>
      <Field label={t("edit.interval")}>
        <Select
          value={interval ?? ""}
          onChange={(e) => setIntervalMin(e.target.value === "" ? null : Number(e.target.value))}
        >
          <option value="">
            {t("edit.intervalDefault")} ({intervalLabel(t, globalDefault)})
          </option>
          {INTERVALS.map((m) => (
            <option key={m} value={m}>
              {intervalLabel(t, m)}
            </option>
          ))}
        </Select>
      </Field>
      <p className="select-text truncate text-[12px] text-fg-3" title={feed.url}>
        {feed.url}
      </p>
      {feed.error && (
        <p className="flex items-start gap-2 rounded-md border border-line bg-hover px-3 py-2 text-[12px] text-fg-2">
          <TriangleAlert size={14} strokeWidth={2} className="mt-px shrink-0 text-warning" />
          <span className="select-text">
            {t("edit.lastError")}: {feed.error}
          </span>
        </p>
      )}
      {error && (
        <p role="alert" className="text-[13px] text-danger">
          {error}
        </p>
      )}
      <div className="flex justify-end gap-2">
        <Button variant="ghost" onClick={onDone}>
          {t("action.cancel")}
        </Button>
        <Button variant="primary" type="submit" busy={saving}>
          {t("action.save")}
        </Button>
      </div>
    </form>
  );
}
