import { useQueryClient } from "@tanstack/react-query";
import { TriangleAlert } from "lucide-react";
import { siYoutube } from "simple-icons";
import { type FormEvent, useEffect, useRef, useState } from "react";
import { api, type Candidate, type Feed, type FeedPreview, type Preview } from "../lib/api";
import { errorText, type Translate, useLang, useT } from "../lib/i18n";
import { useFolders } from "../lib/folders";
import { openExternal } from "../lib/platform";
import { keys, useNav, useSettings } from "../lib/queries";
import { useUi } from "../lib/store";
import { type Lang, shortTime } from "../lib/time";
import { FolderPicker } from "./FolderPicker";
import { Button, cx, Favicon, Field, Modal, Select, Spinner, TextInput } from "./ui";

/** 可选的刷新间隔，单位分钟。 */
export const INTERVALS = [15, 30, 60, 120, 360, 720, 1440];

export function intervalLabel(t: Translate, minutes: number): string {
  return minutes < 60
    ? t("interval.minutes", { n: minutes })
    : t("interval.hours", { n: minutes / 60 });
}

/** 抓取订阅源出错时核心给的原话，比如「server returned HTTP 404」。 */
function fetchDetail(err: unknown): string | null {
  if (!err || typeof err !== "object" || !("code" in err) || !("message" in err)) return null;
  const code = String(err.code);
  const message = String(err.message ?? "").trim();
  return ["http_status", "network", "not_a_feed", "too_large", "invalid_url"].includes(code) && message ? message : null;
}

/** 「3 天前」这样的相对时间，一周以前的写日期。 */
function ago(t: Translate, ms: number, lang: Lang, now = Date.now()): string {
  const diff = Math.max(0, now - ms);
  if (diff < 3_600_000) return t("preview.justNow");
  if (diff < 86_400_000) return t("preview.hoursAgo", { n: Math.floor(diff / 3_600_000) });
  if (diff < 7 * 86_400_000) return t("preview.daysAgo", { n: Math.floor(diff / 86_400_000) });
  return shortTime(ms, lang, now);
}

/** 按最早、最新一篇的时间和篇数估算更新频率。 */
function frequency(t: Translate, preview: FeedPreview): string | null {
  const { entry_count: count, newest_ms: newest, oldest_ms: oldest } = preview;
  if (count < 2 || newest === null || oldest === null || newest <= oldest) return null;
  const perDay = (count - 1) / ((newest - oldest) / 86_400_000);
  const round = (n: number) => (n >= 10 ? Math.round(n) : Math.round(n * 10) / 10);
  if (perDay >= 1) return t("preview.perDay", { n: round(perDay) });
  if (perDay * 7 >= 1) return t("preview.perWeek", { n: round(perDay * 7) });
  return t("preview.perMonth", { n: round(perDay * 30) });
}

function hostOf(url: string | null): string {
  if (!url) return "";
  try {
    return new URL(url).host.replace(/^www\./, "");
  } catch {
    return url;
  }
}

/** 看起来像一个网址：带点、没有空格。 */
const looksLikeUrl = (text: string) => /^\S+\.\S+$/.test(text.trim());

/** 贴进地址后显示的预览：图标、名字、介绍、更新频率和最近几篇。 */
function PreviewCard({
  preview,
  busy,
  onPick,
}: {
  preview: Preview;
  busy: boolean;
  onPick: (url: string) => void;
}) {
  const t = useT();
  const lang = useLang();
  const { feed, candidates } = preview;
  const facts = [
    hostOf(feed.site_url ?? feed.url),
    frequency(t, feed),
    feed.entry_count > 0 ? t("preview.count", { n: feed.entry_count }) : null,
    feed.newest_ms ? t("preview.latest", { time: ago(t, feed.newest_ms, lang) }) : null,
  ].filter(Boolean);
  return (
    <div className="anim-fade overflow-hidden rounded-lg border border-line">
      <div className="flex gap-3 p-3">
        <Favicon src={feed.icon_url} size={40} />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="truncate font-semibold text-fg">{feed.title}</span>
            {feed.kind === "youtube" && (
              <span className="inline-flex shrink-0 items-center gap-1 rounded-full bg-[#ff0033]/12 px-1.5 py-px text-[11px] font-medium text-[#e0002b]">
                <svg viewBox="0 0 24 24" width={11} height={11} aria-hidden>
                  <path d={siYoutube.path} fill="currentColor" />
                </svg>
                {t("preview.youtube")}
              </span>
            )}
          </div>
          <div className="mt-0.5 truncate text-[12px] text-fg-3">{facts.join(" · ")}</div>
          {feed.description && (
            <p className="mt-1.5 line-clamp-2 text-[12px] leading-[17px] text-fg-2">{feed.description}</p>
          )}
        </div>
      </div>

      {candidates.length > 1 && (
        <div className="border-t border-line px-3 py-2">
          <p className="mb-1 text-[12px] font-medium text-fg-3">{t("preview.candidates", { n: candidates.length })}</p>
          <div role="radiogroup" className="flex flex-col gap-px">
            {candidates.map((c) => (
              <button
                key={c.url}
                type="button"
                role="radio"
                aria-checked={c.url === feed.url}
                disabled={busy}
                onClick={() => c.url !== feed.url && onPick(c.url)}
                className={cx(
                  "flex items-center gap-2 rounded px-2 py-1 text-left disabled:opacity-60",
                  c.url === feed.url ? "bg-active" : "hover:bg-hover",
                )}
              >
                <span
                  aria-hidden
                  className={cx(
                    "size-3 shrink-0 rounded-full border",
                    c.url === feed.url ? "border-[4px] border-accent" : "border-line-strong",
                  )}
                />
                <span className="min-w-0 flex-1 truncate text-[13px] text-fg">{c.title || c.url}</span>
                <span className="max-w-[45%] shrink-0 truncate text-[11px] text-fg-3">{c.url}</span>
              </button>
            ))}
          </div>
        </div>
      )}

      <div className="border-t border-line bg-hover/40">
        <p className="px-3 pb-1 pt-2 text-[12px] font-medium text-fg-3">{t("preview.recent")}</p>
        {feed.entries.length === 0 ? (
          <p className="px-3 pb-3 text-[12px] text-fg-3">{t("preview.empty")}</p>
        ) : (
          <ul className="max-h-[208px] overflow-y-auto px-1.5 pb-1.5">
            {feed.entries.map((entry, i) => (
              <li key={`${entry.url ?? ""}:${i}`}>
                <button
                  type="button"
                  disabled={!entry.url}
                  onClick={() => entry.url && void openExternal(entry.url)}
                  className="flex w-full items-center gap-2.5 rounded-md px-1.5 py-1.5 text-left hover:bg-hover disabled:hover:bg-transparent"
                >
                  {entry.image && (
                    <img
                      src={entry.image}
                      alt=""
                      loading="lazy"
                      referrerPolicy="no-referrer"
                      className="h-9 w-14 shrink-0 rounded bg-active object-cover"
                      onError={(e) => {
                        e.currentTarget.style.display = "none";
                      }}
                    />
                  )}
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[13px] text-fg">{entry.title || t("reader.untitled")}</span>
                    {entry.summary && <span className="block truncate text-[12px] text-fg-3">{entry.summary}</span>}
                  </span>
                  {entry.published_ms && (
                    <span className="tabular shrink-0 self-start pt-px text-[11px] text-fg-3">
                      {shortTime(entry.published_ms, lang)}
                    </span>
                  )}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

export function AddFeedDialog() {
  const t = useT();
  const qc = useQueryClient();
  const dialog = useUi((s) => s.dialog);
  const open = dialog?.type === "add-feed";
  const startFolder = dialog?.type === "add-feed" ? (dialog.folder ?? "") : "";
  const setDialog = useUi((s) => s.setDialog);
  const setScope = useUi((s) => s.setScope);
  const rememberFolder = useUi((s) => s.rememberFolder);
  const folders = useFolders();

  const [url, setUrl] = useState("");
  const [folder, setFolder] = useState("");
  const [name, setName] = useState("");
  const [phase, setPhase] = useState<"idle" | "searching" | "adding">("idle");
  const [candidates, setCandidates] = useState<Candidate[]>([]);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [previewing, setPreviewing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** 只认最后一次预览的结果，前面没回来的作废。 */
  const request = useRef(0);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(() => {
    if (open) {
      setUrl("");
      setFolder(startFolder);
      setName("");
      setPhase("idle");
      setCandidates([]);
      setPreview(null);
      setPreviewing(false);
      setError(null);
      request.current += 1;
    }
    return () => clearTimeout(timer.current);
  }, [open, startFolder]);

  /** 读取预览。`keepCandidates` 时是在同一个网站的几个源之间换，候选列表保留。 */
  const load = async (target: string, keepCandidates = false) => {
    clearTimeout(timer.current);
    const id = ++request.current;
    setPreviewing(true);
    setError(null);
    try {
      const found = await api.preview(target.trim());
      if (id !== request.current) return;
      setPreview((old) => (keepCandidates && old ? { ...found, candidates: old.candidates } : found));
      setCandidates([]);
    } catch (err) {
      if (id !== request.current) return;
      setPreview(null);
      setError(errorText(t, err));
    } finally {
      if (id === request.current) setPreviewing(false);
    }
  };

  const onUrlChange = (value: string) => {
    setUrl(value);
    setCandidates([]);
    setError(null);
    clearTimeout(timer.current);
    request.current += 1;
    setPreviewing(false);
    if (preview && preview.feed.url !== value.trim()) setPreview(null);
    // 打字时停一会儿再读，免得每敲一个字都去请求
    if (looksLikeUrl(value)) timer.current = setTimeout(() => void load(value), 800);
  };

  const add = async (feedUrl: string, icon?: string | null) => {
    setPhase("adding");
    setError(null);
    try {
      const feed = await api.addFeed(feedUrl, folder.trim(), { title: name.trim() || undefined, icon_url: icon });
      rememberFolder(folder);
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
    // 已经预览过：就订阅预览的那个
    if (preview) return void add(preview.feed.url, preview.feed.icon_url);
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
    <Modal open={open} onClose={() => setDialog(null)} title={t("add.title")} width={520}>
      <form onSubmit={(e) => void submit(e)} className="flex flex-col gap-4 p-4">
        <Field label={t("add.url")}>
          <div className="relative">
            <TextInput
              autoFocus
              value={url}
              onChange={(e) => onUrlChange(e.target.value)}
              onPaste={(e) => {
                // 粘贴进来马上读，不等
                const text = e.clipboardData.getData("text").trim();
                if (!looksLikeUrl(text)) return;
                e.preventDefault();
                setUrl(text);
                setCandidates([]);
                void load(text);
              }}
              placeholder={t("add.urlPlaceholder")}
              inputMode="url"
              disabled={busy}
              className="pr-8"
            />
            {previewing && (
              <span className="absolute right-2.5 top-1/2 -translate-y-1/2 text-fg-3">
                <Spinner size={13} />
              </span>
            )}
          </div>
        </Field>

        {preview && (
          <PreviewCard preview={preview} busy={busy || previewing} onPick={(u) => void load(u, true)} />
        )}
        {previewing && !preview && (
          <div className="flex items-center gap-3 rounded-lg border border-line p-3 text-[12px] text-fg-3">
            <span className="size-10 shrink-0 animate-pulse rounded-[6px] bg-active" />
            <div className="flex flex-1 flex-col gap-2">
              <span className="h-3 w-2/5 animate-pulse rounded bg-active" />
              <span className="h-2.5 w-3/5 animate-pulse rounded bg-active" />
            </div>
            {t("preview.loading")}
          </div>
        )}

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

        <div className="grid grid-cols-2 gap-3">
          <Field label={t("add.name")}>
            <TextInput
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={preview?.feed.title || t("add.namePlaceholder")}
              disabled={busy}
            />
          </Field>
          <Field label={t("add.folder")}>
            <FolderPicker value={folder} onChange={setFolder} folders={folders} placeholder={t("add.folderPlaceholder")} />
          </Field>
        </div>

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
          <Button variant="primary" type="submit" disabled={!url.trim() || busy || previewing}>
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
  const rememberFolder = useUi((s) => s.rememberFolder);
  const [title, setTitle] = useState(feed.title);
  const [folder, setFolder] = useState(feed.folder);
  const [interval, setIntervalMin] = useState<number | null>(feed.interval_min);
  const [url, setUrl] = useState(feed.url);
  const urlChanged = url.trim() !== "" && url.trim() !== feed.url;
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<{ text: string; detail: string | null } | null>(null);

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
      if (folder.trim() !== feed.folder) {
        patch.folder = folder.trim();
        rememberFolder(folder);
      }
      if (interval !== feed.interval_min) patch.interval_min = interval;
      // 新地址由核心先抓一次，确认能用才换；订阅源本身不变，已读和以前的文章都在
      if (urlChanged) patch.url = url.trim();
      if (Object.keys(patch).length > 0) {
        await api.updateFeed(feed.id, patch);
        void qc.invalidateQueries({ queryKey: keys.nav });
        void qc.invalidateQueries({ queryKey: ["items"] });
      }
      onDone();
    } catch (err) {
      // 换地址失败时，服务器的原话（比如 HTTP 404）比笼统的说明更有用
      setError({ text: errorText(t, err), detail: fetchDetail(err) });
      setSaving(false);
    }
  };

  return (
    <form onSubmit={(e) => void submit(e)} className="flex flex-col gap-4 p-4">
      <Field label={t("edit.name")}>
        <TextInput autoFocus={!feed.error} value={title} onChange={(e) => setTitle(e.target.value)} />
      </Field>
      <Field label={t("edit.folder")}>
        <FolderPicker value={folder} onChange={setFolder} folders={folders} placeholder={t("add.folderPlaceholder")} />
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
      <Field label={t("edit.url")} hint={urlChanged ? t("edit.urlHint") : undefined}>
        <TextInput
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          inputMode="url"
          disabled={saving}
          autoFocus={Boolean(feed.error)}
        />
      </Field>
      {feed.error && !urlChanged && (
        <p className="-mt-2 flex items-start gap-2 rounded-md border border-line bg-hover px-3 py-2 text-[12px] text-fg-2">
          <TriangleAlert size={14} strokeWidth={2} className="mt-px shrink-0 text-warning" />
          <span className="select-text">
            {t("edit.lastError")}: {feed.error}
            <span className="mt-1 block text-fg-3">{t("edit.errorHint")}</span>
          </span>
        </p>
      )}
      {error && (
        <p role="alert" className="text-[13px] text-danger">
          {error.text}
          {error.detail && <span className="mt-0.5 block select-text text-[12px] text-fg-3">{error.detail}</span>}
        </p>
      )}
      <div className="flex justify-end gap-2">
        <Button variant="ghost" onClick={onDone}>
          {t("action.cancel")}
        </Button>
        <Button variant="primary" type="submit" busy={saving}>
          {saving && urlChanged ? t("edit.checking") : t("action.save")}
        </Button>
      </div>
    </form>
  );
}
