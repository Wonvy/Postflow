// 播客和视频在界面上的样子：
// - 阅读区里带音频的条目顶上一条播放条（没放的时候是「播放」按钮和时长，放着的时候是完整的控制）；
//   带视频附件的直接显示视频。
// - 列表底下的迷你播放器：换条目、切到邮件都在，点标题回到那一期。
// - PlayerBridge：播完标为已读。
// 播放本身在 lib/player.ts。

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, CircleCheck, Download, Gauge, Moon, Pause, Play, RotateCcw, RotateCw, X } from "lucide-react";
import { createContext, type CSSProperties, type ReactNode, useContext, useEffect, useMemo, useState } from "react";
import { api, type Item } from "../lib/api";
import { formatLength } from "../lib/duration";
import { useDownloadActions, useDownloads } from "../lib/episodes";
import { errorText, useLang, useT } from "../lib/i18n";
import { useListModel } from "../lib/listModel";
import {
  BACK_SECONDS,
  type Episode,
  episodeOf,
  FORWARD_SECONDS,
  formatTime,
  play,
  RATES,
  savedPosition,
  seek,
  setEndedHandler,
  setSleep,
  skip,
  stop,
  toggle,
  usePlayer,
} from "../lib/player";
import { keys } from "../lib/queries";
import { activeIndex, parseChapters, parseTranscript } from "../lib/transcript";
import { useUi } from "../lib/store";
import { toast } from "./Toast";
import { cx, Favicon, IconButton, Menu, MenuItem, MenuSeparator, Spinner } from "./ui";

/** 当前列表里能放的几期，按列表顺序：播完一期接着放下一期时用。 */
function useQueue(): Episode[] {
  const { items } = useListModel();
  return useMemo(() => items.map(episodeOf).filter((e): e is Episode => e !== null), [items]);
}

/** 进度条：可以点、可以拖。 */
function Scrubber({ position, duration, className }: { position: number; duration: number; className?: string }) {
  const t = useT();
  const max = duration > 0 ? duration : Math.max(position, 1);
  const percent = Math.min(100, (position / max) * 100);
  return (
    <input
      type="range"
      min={0}
      max={max}
      step={1}
      value={Math.min(position, max)}
      aria-label={t("player.position")}
      aria-valuetext={`${formatTime(position)} / ${formatTime(duration)}`}
      disabled={duration <= 0}
      onChange={(e) => seek(Number(e.target.value))}
      className={cx("player-range w-full", className)}
      style={{ "--played": `${percent}%` } as CSSProperties}
    />
  );
}

function RateMenu({ compact = false }: { compact?: boolean }) {
  const t = useT();
  const rate = usePlayer((s) => s.rate);
  const setRate = usePlayer((s) => s.setRate);
  const autoNext = usePlayer((s) => s.autoNext);
  const setAutoNext = usePlayer((s) => s.setAutoNext);
  const sleep = usePlayer((s) => s.sleep);
  const lang = useLang();
  const mark = (on: boolean) => (on ? <Check size={14} /> : <span className="inline-block size-[14px]" />);
  return (
    <Menu
      align="end"
      trigger={
        <button
          type="button"
          aria-label={t("player.rate")}
          className={cx(
            "inline-flex h-7 shrink-0 items-center justify-center gap-1 rounded-md px-1.5 text-[12px] font-medium tabular-nums text-fg-2 transition-colors hover:bg-hover hover:text-fg touch:h-10",
            compact && "px-1",
          )}
        >
          {sleep !== null ? (
            <Moon size={13} strokeWidth={1.75} className="text-accent" />
          ) : (
            !compact && <Gauge size={14} strokeWidth={1.75} />
          )}
          {rate}×
        </button>
      }
    >
      {RATES.map((r) => (
        <MenuItem key={r} icon={mark(r === rate)} onSelect={() => setRate(r)}>
          <span className="tabular-nums">{r}×</span>
        </MenuItem>
      ))}
      <MenuSeparator />
      <MenuItem icon={mark(autoNext)} onSelect={() => setAutoNext(!autoNext)}>
        {t("player.autoNext")}
      </MenuItem>
      <MenuSeparator />
      {/* 睡眠定时：到时间暂停，或者这一期放完就停 */}
      <MenuItem icon={mark(sleep === null)} onSelect={() => setSleep(null)}>
        {t("player.sleepOff")}
      </MenuItem>
      {[15, 30, 60].map((m) => (
        <MenuItem key={m} icon={mark(false)} onSelect={() => setSleep(m)}>
          {t("player.sleepIn", { time: formatLength(m * 60, lang === "zh" ? "zh" : "en") })}
        </MenuItem>
      ))}
      <MenuItem icon={mark(sleep === "end")} onSelect={() => setSleep("end")}>
        {t("player.sleepEnd")}
      </MenuItem>
      {typeof sleep === "number" && (
        <p className="px-2 pb-1 pt-0.5 text-[11px] text-fg-3">
          {t("player.sleepAt", { time: new Date(sleep).toLocaleTimeString(lang === "zh" ? "zh-CN" : "en", { hour: "2-digit", minute: "2-digit" }) })}
        </p>
      )}
    </Menu>
  );
}

function PlayButton({ episode, queue, size = "md" }: { episode: Episode; queue: Episode[]; size?: "md" | "lg" }) {
  const t = useT();
  const current = usePlayer((s) => s.current?.id === episode.id);
  const playing = usePlayer((s) => s.playing && s.current?.id === episode.id);
  const waiting = usePlayer((s) => s.waiting && s.current?.id === episode.id);
  const big = size === "lg";
  return (
    <button
      type="button"
      aria-label={playing ? t("player.pause") : t("player.play")}
      onClick={() => (current ? toggle() : play(episode, queue))}
      className={cx(
        "inline-flex shrink-0 items-center justify-center rounded-full bg-accent text-white transition-transform hover:brightness-110 active:scale-95",
        big ? "size-11 touch:size-12" : "size-8 touch:size-10",
      )}
    >
      {waiting && playing ? (
        <Spinner size={big ? 16 : 13} />
      ) : playing ? (
        <Pause size={big ? 18 : 14} fill="currentColor" strokeWidth={0} />
      ) : (
        <Play size={big ? 18 : 14} fill="currentColor" strokeWidth={0} className="translate-x-px" />
      )}
    </button>
  );
}

/** 列表把自己能放的几期传给每一行，从列表里点播放也能「播完接着放下一期」。 */
const QueueContext = createContext<Episode[]>([]);

export function EpisodeQueue({ items, children }: { items: readonly Item[]; children: ReactNode }) {
  const queue = useMemo(() => items.map(episodeOf).filter((e): e is Episode => e !== null), [items]);
  return <QueueContext.Provider value={queue}>{children}</QueueContext.Provider>;
}

/**
 * 列表里播客单集右边的小按钮：播放 / 暂停，旁边是时长；听过一半的是还剩多久，正在放的是剩下的时间。
 * 点它不会打开这一条。
 */
export function EpisodeChip({ item, size }: { item: Item; size: number }) {
  const t = useT();
  const lang = useLang();
  const queue = useContext(QueueContext);
  const episode = useMemo(() => episodeOf(item), [item]);
  const current = usePlayer((s) => s.current?.id === item.id);
  const playing = usePlayer((s) => s.playing && s.current?.id === item.id);
  const left = usePlayer((s) =>
    s.current?.id === item.id && s.duration > 0 ? Math.max(0, s.duration - s.position) : null,
  );
  if (!episode) return null;
  const total = episode.media.duration ?? 0;
  const saved = savedPosition(item.id);
  const zh = lang === "zh" ? "zh" : "en";
  const label =
    left !== null
      ? `-${formatTime(left)}`
      : saved > 0 && total > saved
        ? t("player.left", { time: formatLength(total - saved, zh) })
        : total > 0
          ? formatLength(total, zh)
          : "";
  return (
    <button
      type="button"
      aria-label={playing ? t("player.pause") : t("player.play")}
      onClick={(e) => {
        e.stopPropagation();
        if (current) toggle();
        else play(episode, queue);
      }}
      onPointerDown={(e) => e.stopPropagation()}
      className={cx(
        "tabular inline-flex h-[18px] shrink-0 items-center gap-1 rounded-full border pl-1 pr-1.5 leading-none transition-colors touch:h-7 touch:pl-1.5 touch:pr-2",
        current ? "border-accent text-accent" : "border-line-strong text-fg-2 hover:border-accent hover:text-accent",
      )}
      style={{ fontSize: Math.min(size, 12) }}
    >
      {playing ? (
        <Pause size={10} fill="currentColor" strokeWidth={0} />
      ) : (
        <Play size={10} fill="currentColor" strokeWidth={0} />
      )}
      {label}
    </button>
  );
}

/** 阅读区里的附件：音频是播放条，视频直接放。 */
export function MediaAttachment({ item }: { item: Item }) {
  const media = item.media;
  if (!media) return null;
  if (media.mime.startsWith("video/")) {
    return (
      // biome-ignore lint/a11y/useMediaCaption: 订阅源里的视频没有字幕文件
      <video
        controls
        preload="metadata"
        src={media.url}
        poster={item.image ?? undefined}
        className="mb-8 aspect-video w-full rounded-lg bg-black"
      />
    );
  }
  return <EpisodeBar item={item} />;
}

function EpisodeBar({ item }: { item: Item }) {
  const t = useT();
  const lang = useLang();
  const queue = useQueue();
  const episode = useMemo(() => episodeOf(item), [item]);
  const current = usePlayer((s) => s.current?.id === item.id);
  const position = usePlayer((s) => (s.current?.id === item.id ? s.position : 0));
  const duration = usePlayer((s) => (s.current?.id === item.id ? s.duration : 0));
  const error = usePlayer((s) => (s.current?.id === item.id ? s.error : null));
  if (!episode) return null;
  const total = current && duration > 0 ? duration : (episode.media.duration ?? 0);
  const resume = current ? 0 : savedPosition(item.id);
  return (
    <div className="mb-8 rounded-xl border border-line bg-surface px-4 py-3">
      <div className="flex items-center gap-3">
        <PlayButton episode={episode} queue={queue} size="lg" />
        <div className="min-w-0 flex-1">
          {current ? (
            <>
              <Scrubber position={position} duration={total} />
              <div className="mt-1 flex justify-between text-[11px] tabular-nums text-fg-3">
                <span>{formatTime(position)}</span>
                <span>{total > 0 ? `-${formatTime(total - position)}` : ""}</span>
              </div>
            </>
          ) : (
            <div className="text-[13px]">
              <div className="font-medium text-fg">{resume > 0 ? t("player.resume", { time: formatTime(resume) }) : t("player.playEpisode")}</div>
              {total > 0 && (
                <div className="mt-0.5 text-[12px] text-fg-3">{formatLength(total, lang === "zh" ? "zh" : "en")}</div>
              )}
            </div>
          )}
        </div>
        <DownloadButton id={item.id} />
        {current && (
          <div className="flex shrink-0 items-center gap-0.5">
            <IconButton label={t("player.back", { n: BACK_SECONDS })} onClick={() => skip(-BACK_SECONDS)}>
              <RotateCcw size={16} strokeWidth={1.75} />
            </IconButton>
            <IconButton label={t("player.forward", { n: FORWARD_SECONDS })} onClick={() => skip(FORWARD_SECONDS)}>
              <RotateCw size={16} strokeWidth={1.75} />
            </IconButton>
            <RateMenu />
          </div>
        )}
      </div>
      {error && (
        <p className="mt-2 text-[12px] text-danger">
          {t("player.error")}{" "}
          <a href={episode.media.url} target="_blank" rel="noreferrer" className="underline">
            {t("player.openFile")}
          </a>
        </p>
      )}
      <EpisodeExtras episode={episode} queue={queue} />
    </div>
  );
}

/** 下载这一期离线听：没下的是下载按钮，下着的是进度，下好的点一下删掉。 */
function DownloadButton({ id }: { id: string }) {
  const t = useT();
  const { downloaded, downloading } = useDownloads();
  const { download, remove } = useDownloadActions();
  const done = downloaded.find((d) => d.id === id);
  const progress = downloading[id];
  if (done) {
    return (
      <IconButton label={t("player.downloaded", { size: formatSize(done.size) })} active onClick={() => void remove(id)}>
        <CircleCheck size={16} strokeWidth={1.75} />
      </IconButton>
    );
  }
  if (progress && !progress.error) {
    const percent = progress.total ? Math.round((progress.received / progress.total) * 100) : null;
    return (
      <span className="tabular inline-flex h-7 shrink-0 items-center gap-1.5 px-1.5 text-[12px] text-fg-3" aria-live="polite">
        <Spinner size={12} />
        {percent !== null ? `${percent}%` : formatSize(progress.received)}
      </span>
    );
  }
  return (
    <IconButton
      label={progress?.error ? t("player.downloadFailed", { error: progress.error }) : t("player.download")}
      onClick={() => void download(id).catch((err) => toast(errorText(t, err)))}
    >
      <Download size={16} strokeWidth={1.75} className={cx(progress?.error && "text-danger")} />
    </IconButton>
  );
}

function formatSize(bytes: number): string {
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
  if (bytes >= 1024 * 1024) return `${Math.round(bytes / 1024 / 1024)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

/** 章节和文字稿：节目提供了才有。点一条就从那里放。 */
function EpisodeExtras({ episode, queue }: { episode: Episode; queue: Episode[] }) {
  const t = useT();
  const [panel, setPanel] = useState<"chapters" | "transcript" | null>(null);
  const { chapters: chaptersUrl, transcript } = episode.media;
  const chapters = useQuery({
    queryKey: ["media-text", chaptersUrl],
    queryFn: () => api.mediaText(chaptersUrl ?? "").then((r) => parseChapters(r.text)),
    enabled: !!chaptersUrl,
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  });
  const cues = useQuery({
    queryKey: ["media-text", transcript?.url],
    queryFn: () => api.mediaText(transcript?.url ?? "").then((r) => parseTranscript(r.text, transcript?.mime ?? "")),
    enabled: !!transcript && panel === "transcript",
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  });
  const position = usePlayer((s) => (s.current?.id === episode.id ? s.position : -1));
  const hasChapters = (chapters.data?.length ?? 0) > 0;
  if (!hasChapters && !transcript) return null;
  const jump = (at: number) => play(episode, queue, at);
  const tab = (key: "chapters" | "transcript", label: string) => (
    <button
      type="button"
      aria-pressed={panel === key}
      onClick={() => setPanel(panel === key ? null : key)}
      className={cx(
        "h-6 rounded-md px-2 text-[12px] transition-colors touch:h-9",
        panel === key ? "bg-active text-fg" : "text-fg-3 hover:bg-hover hover:text-fg-2",
      )}
    >
      {label}
    </button>
  );
  const current = (list: readonly { start: number | null }[]) => (position >= 0 ? activeIndex(list, position) : -1);
  return (
    <div className="mt-2 border-t border-line pt-2">
      <div className="flex gap-1">
        {hasChapters && tab("chapters", t("player.chapters", { n: chapters.data?.length ?? 0 }))}
        {transcript && tab("transcript", t("player.transcript"))}
      </div>
      {panel === "chapters" && hasChapters && (
        <ol className="mt-1 max-h-72 overflow-y-auto">
          {chapters.data?.map((c, i) => (
            <li key={`${c.start}:${c.title}`}>
              <button
                type="button"
                onClick={() => jump(c.start)}
                className={cx(
                  "flex w-full items-baseline gap-3 rounded-md px-2 py-1 text-left text-[13px] hover:bg-hover touch:py-2",
                  i === current(chapters.data ?? []) ? "text-accent" : "text-fg-2",
                )}
              >
                <span className="tabular w-14 shrink-0 text-[12px] text-fg-3">{formatTime(c.start)}</span>
                <span className="min-w-0 flex-1">{c.title}</span>
              </button>
            </li>
          ))}
        </ol>
      )}
      {panel === "transcript" && (
        <div className="mt-1 max-h-96 overflow-y-auto pr-1">
          {cues.isFetching && !cues.data ? (
            <div className="flex justify-center py-4 text-fg-3">
              <Spinner size={14} />
            </div>
          ) : cues.error || (cues.data && cues.data.length === 0) ? (
            <p className="px-2 py-2 text-[12px] text-fg-3">{t("player.transcriptFailed")}</p>
          ) : (
            cues.data?.map((c, i) => (
              <p
                key={`${i}:${c.start}`}
                className={cx(
                  "select-text rounded-md px-2 py-1 text-[13px] leading-relaxed",
                  i === current(cues.data ?? []) ? "bg-accent-soft text-fg" : "text-fg-2",
                )}
              >
                {c.start !== null && (
                  <button
                    type="button"
                    onClick={() => jump(c.start ?? 0)}
                    className="tabular mr-2 text-[11px] text-fg-3 hover:text-accent"
                  >
                    {formatTime(c.start)}
                  </button>
                )}
                {c.speaker && <span className="mr-1 font-medium text-fg">{c.speaker}:</span>}
                {c.text}
              </p>
            ))
          )}
        </div>
      )}
    </div>
  );
}

/** 列表底下的迷你播放器。 */
export function MiniPlayer() {
  const t = useT();
  const current = usePlayer((s) => s.current);
  const position = usePlayer((s) => s.position);
  const duration = usePlayer((s) => s.duration);
  const playing = usePlayer((s) => s.playing);
  const waiting = usePlayer((s) => s.waiting);
  const open = useUi((s) => s.open);
  if (!current) return null;
  const total = duration > 0 ? duration : (current.media.duration ?? 0);
  return (
    <div className="shrink-0 border-t border-line bg-surface">
      <Scrubber position={position} duration={total} className="player-range-flat block" />
      <div className="flex items-center gap-1.5 py-1.5 pl-2.5 pr-1.5 touch:py-2">
        <button
          type="button"
          onClick={() => open(current.id)}
          title={current.title}
          className="flex min-w-0 flex-1 items-center gap-2 rounded-md text-left"
        >
          <Favicon src={current.icon} size={18} />
          <span className="min-w-0 flex-1">
            <span className="block truncate text-[12px] font-medium text-fg touch:text-[14px]">{current.title}</span>
            <span className="block truncate text-[11px] tabular-nums text-fg-3 touch:text-[12px]">
              {formatTime(position)}
              {total > 0 && ` / ${formatTime(total)}`} · {current.source}
            </span>
          </span>
        </button>
        <IconButton label={t("player.back", { n: BACK_SECONDS })} onClick={() => skip(-BACK_SECONDS)}>
          <RotateCcw size={15} strokeWidth={1.75} />
        </IconButton>
        <IconButton label={playing ? t("player.pause") : t("player.play")} onClick={toggle}>
          {waiting && playing ? (
            <Spinner size={13} />
          ) : playing ? (
            <Pause size={16} fill="currentColor" strokeWidth={0} />
          ) : (
            <Play size={16} fill="currentColor" strokeWidth={0} />
          )}
        </IconButton>
        <IconButton label={t("player.forward", { n: FORWARD_SECONDS })} onClick={() => skip(FORWARD_SECONDS)}>
          <RotateCw size={15} strokeWidth={1.75} />
        </IconButton>
        <RateMenu compact />
        <IconButton label={t("player.close")} onClick={stop}>
          <X size={15} strokeWidth={1.75} />
        </IconButton>
      </div>
    </div>
  );
}

/** 播完一期：标为已读，列表和计数跟着更新。 */
export function PlayerBridge() {
  const qc = useQueryClient();
  useEffect(() => {
    setEndedHandler((episode) => {
      void api.setItems([episode.id], { read: true }).then(() => {
        void qc.invalidateQueries({ queryKey: ["items"] });
        void qc.invalidateQueries({ queryKey: keys.nav });
      });
    });
    return () => setEndedHandler(null);
  }, [qc]);
  return null;
}
