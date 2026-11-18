// 播客和视频在界面上的样子：
// - 阅读区里带音频的条目顶上一条播放条（没放的时候是「播放」按钮和时长，放着的时候是完整的控制）；
//   带视频附件的直接显示视频。
// - 列表底下的迷你播放器：换条目、切到邮件都在，点标题回到那一期。
// - PlayerBridge：播完标为已读。
// 播放本身在 lib/player.ts。

import { useQueryClient } from "@tanstack/react-query";
import { Check, Gauge, Pause, Play, RotateCcw, RotateCw, X } from "lucide-react";
import { type CSSProperties, useEffect, useMemo } from "react";
import { api, type Item } from "../lib/api";
import { formatLength } from "../lib/duration";
import { useLang, useT } from "../lib/i18n";
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
  skip,
  stop,
  toggle,
  usePlayer,
} from "../lib/player";
import { keys } from "../lib/queries";
import { useUi } from "../lib/store";
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
          {!compact && <Gauge size={14} strokeWidth={1.75} />}
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
