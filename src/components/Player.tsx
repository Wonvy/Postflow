// 播客和视频在界面上的样子：
// - 阅读区里带音频的条目顶上一条播放条（没放的时候是「播放」按钮和时长，放着的时候是完整的控制）；
//   带视频附件的直接显示视频。
// - 列表底下的迷你播放器：换条目、切到邮件都在，点标题回到那一期。
// - PlayerBridge：播完标为已读。
// - 字幕：开着的时候播放条和迷你播放器下面多一行，声音放到哪儿显示哪一句（lib/subtitles.ts）；
//   样子见 SubtitleStyles.tsx，译文来自 lib/subtitleTranslate.ts。
// 播放本身在 lib/player.ts。

import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Captions,
  Check,
  ChevronDown,
  CircleCheck,
  Download,
  Gauge,
  Moon,
  Pause,
  Play,
  RotateCcw,
  RotateCw,
  Settings2,
  X,
} from "lucide-react";
import {
  createContext,
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { api, type Item, type Voice } from "../lib/api";
import { formatLength } from "../lib/duration";
import { useDownloadActions, useDownloads } from "../lib/episodes";
import { errorText, type Translate, useLang, useT } from "../lib/i18n";
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
import { formatSize as formatBytes } from "../lib/platform";
import type { Lang } from "../lib/time";
import { coveredPercent, type Cue, cueAt, isCovered, speakerOrder } from "../lib/subtitleCues";
import { redoLocal, startLocal, SUBTITLE_STYLES, useSubtitles } from "../lib/subtitles";
import { type SubtitleTranslation, translationOf, useTranslationFor } from "../lib/subtitleTranslate";
import { activeIndex, parseChapters, parseTranscript } from "../lib/transcript";
import { languageName, useTranslateTarget } from "../lib/translate";
import { useUi } from "../lib/store";
import { clockRate, SpeakerMark, SubtitleView, usePlaybackClock } from "./SubtitleStyles";
import { toast } from "./Toast";
import { ContextMenu, cx, Favicon, IconButton, Menu, MenuItem, MenuLabel, MenuSeparator, MenuSub, Spinner } from "./ui";

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
  const catchUp = usePlayer((s) => s.catchUp);
  const setCatchUp = usePlayer((s) => s.setCatchUp);
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
      {autoNext && (
        <MenuItem icon={mark(catchUp)} onSelect={() => setCatchUp(!catchUp)}>
          <span className="flex flex-col">
            <span>{t("player.catchUp")}</span>
            <span className="text-[11px] text-fg-3">{t("player.catchUpHint")}</span>
          </span>
        </MenuItem>
      )}
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

/**
 * 卡片视图里播客单集的大播放按钮：有封面时压在封面左下角，没有时放在卡片里。旁边写着时长
 * （听过一半的是还剩多久，正在放的是剩下的时间）。点它不会打开这一条。
 */
export function EpisodeCardButton({ item, overlay }: { item: Item; overlay: boolean }) {
  const t = useT();
  const lang = useLang();
  const queue = useContext(QueueContext);
  const episode = useMemo(() => episodeOf(item), [item]);
  const current = usePlayer((s) => s.current?.id === item.id);
  const playing = usePlayer((s) => s.playing && s.current?.id === item.id);
  const waiting = usePlayer((s) => s.waiting && s.current?.id === item.id);
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
    <span
      className={cx(
        "inline-flex items-center gap-2",
        overlay && "absolute bottom-2 left-2 rounded-full bg-black/55 py-1 pl-1 pr-2.5 text-white backdrop-blur-sm",
      )}
    >
      <button
        type="button"
        aria-label={playing ? t("player.pause") : t("player.play")}
        onClick={(e) => {
          e.stopPropagation();
          if (current) toggle();
          else play(episode, queue);
        }}
        onPointerDown={(e) => e.stopPropagation()}
        className="inline-flex size-9 shrink-0 items-center justify-center rounded-full bg-accent text-white shadow-sm transition-transform hover:brightness-110 active:scale-95 touch:size-11"
      >
        {waiting && playing ? (
          <Spinner size={14} />
        ) : playing ? (
          <Pause size={15} fill="currentColor" strokeWidth={0} />
        ) : (
          <Play size={15} fill="currentColor" strokeWidth={0} className="translate-x-px" />
        )}
      </button>
      {label && (
        <span className={cx("tabular text-[12px]", overlay ? "text-white" : current ? "text-accent" : "text-fg-3")}>
          {label}
        </span>
      )}
    </span>
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
      {/* 窄的时候（三栏布局里的阅读区）按钮换到第二行，不把进度条挤没了 */}
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <PlayButton episode={episode} queue={queue} size="lg" />
        <div className="min-w-[150px] flex-1">
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
        <div className="ml-auto flex shrink-0 items-center gap-0.5">
          <DownloadButton id={item.id} />
          {current && (
            <>
              <IconButton label={t("player.back", { n: BACK_SECONDS })} onClick={() => skip(-BACK_SECONDS)}>
                <RotateCcw size={16} strokeWidth={1.75} />
              </IconButton>
              <IconButton label={t("player.forward", { n: FORWARD_SECONDS })} onClick={() => skip(FORWARD_SECONDS)}>
                <RotateCw size={16} strokeWidth={1.75} />
              </IconButton>
              <SubtitleToggle />
              <SubtitleMenu />
              <RateMenu />
            </>
          )}
        </div>
      </div>
      {current && <SubtitleLine id={item.id} />}
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

/** 字幕开关。开着的时候放哪一期就准备哪一期的字幕。 */
function SubtitleToggle({ size = 16 }: { size?: number }) {
  const t = useT();
  const on = useSubtitles((s) => s.on);
  const setOn = useSubtitles((s) => s.setOn);
  return (
    <IconButton label={on ? t("subtitles.turnOff") : t("subtitles.turnOn")} active={on} aria-pressed={on} onClick={() => setOn(!on)}>
      <Captions size={size} strokeWidth={1.75} />
    </IconButton>
  );
}

/** 字幕的选项：样子、自动翻译、区分说话人。开着字幕时在字幕按钮旁边。 */
function SubtitleMenu() {
  const t = useT();
  const on = useSubtitles((s) => s.on);
  const style = useSubtitles((s) => s.style);
  const setStyle = useSubtitles((s) => s.setStyle);
  const translate = useSubtitles((s) => s.translate);
  const setTranslate = useSubtitles((s) => s.setTranslate);
  const speakers = useSubtitles((s) => s.speakers);
  const setSpeakers = useSubtitles((s) => s.setSpeakers);
  const setDialog = useUi((s) => s.setDialog);
  if (!on) return null;
  const mark = (yes: boolean) => (yes ? <Check size={14} /> : <span className="inline-block size-[14px]" />);
  return (
    <Menu
      align="end"
      trigger={
        <button
          type="button"
          aria-label={t("subtitles.options")}
          title={t("subtitles.options")}
          className="-ml-1 inline-flex h-7 w-4 shrink-0 items-center justify-center rounded-md text-fg-3 transition-colors hover:bg-hover hover:text-fg touch:h-10 touch:w-6"
        >
          <ChevronDown size={12} strokeWidth={2} />
        </button>
      }
    >
      <MenuSub
        label={
          <span className="flex items-center justify-between gap-3">
            {t("subtitles.style")}
            <span className="text-fg-3">{t(`subtitles.style.${style}`)}</span>
          </span>
        }
      >
        {SUBTITLE_STYLES.map((s) => {
          const pick = () => {
            setStyle(s);
            // 双语要有译文
            if (s === "bilingual") setTranslate(true);
          };
          return (
            <MenuItem key={s} icon={mark(s === style)} onSelect={pick}>
              {t(`subtitles.style.${s}`)}
            </MenuItem>
          );
        })}
      </MenuSub>
      <MenuSeparator />
      <MenuItem icon={mark(translate)} onSelect={() => setTranslate(!translate)}>
        {t("subtitles.translate")}
      </MenuItem>
      <MenuItem icon={mark(speakers)} onSelect={() => setSpeakers(!speakers)}>
        {t("subtitles.speakers")}
      </MenuItem>
      <MenuSeparator />
      <MenuItem icon={<Settings2 size={14} />} onSelect={() => setDialog({ type: "settings", page: "subtitles" })}>
        {t("subtitles.settings")}
      </MenuItem>
    </Menu>
  );
}

/** 译文那里的说明：缺翻译模型时问要不要下载，出错时可以再试，正在翻的时候说一声。没什么要说的是 null。 */
function translationNote(t: Translate, lang: Lang, value: SubtitleTranslation, waiting: boolean): ReactNode {
  const link = "underline decoration-dotted underline-offset-2 hover:text-accent";
  switch (value.status) {
    case "need": {
      const { plan, download } = value;
      const name = (code: string | null) => (code ? languageName(code, lang) : "");
      const size = formatBytes(plan.missing.reduce((n, m) => n + m.size, 0));
      return (
        <>
          {plan.missing.length > 1
            ? t("translate.needPivot", { from: name(plan.from), to: name(plan.to), size })
            : t("translate.need", { to: name(plan.to), size })}{" "}
          <button type="button" className={link} onClick={download}>
            {t("translate.download")}
          </button>
        </>
      );
    }
    case "downloading":
      return t("translate.downloading");
    case "error":
      return (
        <>
          <span className="text-danger">{errorText(t, value.error)}</span>{" "}
          <button type="button" className={link} onClick={value.retry}>
            {t("action.retry")}
          </button>
        </>
      );
    case "on":
      return waiting && value.working ? t("subtitles.translating") : null;
    default:
      return null;
  }
}

/** 字幕右键菜单里能选的翻译语言（再加上现在用的那种）。 */
const SUBTITLE_LANGUAGES = ["zh", "en", "ja", "ko", "fr", "de", "es", "ru", "pt", "it"];

/** 字幕上右键：看原文、原文和译文、只看译文，翻译成什么语言；字号、高度恢复默认。 */
function SubtitleContextItems() {
  const t = useT();
  const lang = useLang();
  const translate = useSubtitles((s) => s.translate);
  const setTranslate = useSubtitles((s) => s.setTranslate);
  const show = useSubtitles((s) => s.show);
  const setShow = useSubtitles((s) => s.setShow);
  const chosen = useSubtitles((s) => s.translateTo);
  const setTranslateTo = useSubtitles((s) => s.setTranslateTo);
  const scale = useSubtitles((s) => s.scale);
  const setScale = useSubtitles((s) => s.setScale);
  const height = useSubtitles((s) => s.height);
  const setHeight = useSubtitles((s) => s.setHeight);
  const general = useTranslateTarget();
  const target = chosen ?? general;
  const mark = (yes: boolean) => (yes ? <Check size={14} /> : <span className="inline-block size-[14px]" />);
  const codes = SUBTITLE_LANGUAGES.includes(target) ? SUBTITLE_LANGUAGES : [target, ...SUBTITLE_LANGUAGES];
  const pickLanguage = (code: string | null) => {
    setTranslateTo(code);
    setTranslate(true);
  };
  return (
    <>
      <MenuLabel>{t("subtitles.menuLanguage")}</MenuLabel>
      <MenuItem icon={mark(!translate)} onSelect={() => setTranslate(false)}>
        {t("subtitles.showOriginal")}
      </MenuItem>
      <MenuItem
        icon={mark(translate && show === "both")}
        onSelect={() => {
          setShow("both");
          setTranslate(true);
        }}
      >
        {t("subtitles.showBoth")}
      </MenuItem>
      <MenuItem
        icon={mark(translate && show === "translation")}
        onSelect={() => {
          setShow("translation");
          setTranslate(true);
        }}
      >
        {t("subtitles.showTranslation")}
      </MenuItem>
      <MenuSub
        label={
          <span className="flex items-center justify-between gap-3">
            {t("subtitles.translateInto")}
            <span className="text-fg-3">{languageName(target, lang)}</span>
          </span>
        }
      >
        <MenuItem icon={mark(chosen === null)} onSelect={() => pickLanguage(null)}>
          {t("subtitles.followSettings", { lang: languageName(general, lang) })}
        </MenuItem>
        <MenuSeparator />
        {codes.map((code) => (
          <MenuItem key={code} icon={mark(chosen === code)} onSelect={() => pickLanguage(code)}>
            {languageName(code, lang)}
          </MenuItem>
        ))}
      </MenuSub>
      {(scale !== 1 || height !== null) && <MenuSeparator />}
      {scale !== 1 && (
        <MenuItem icon={<span className="inline-block size-[14px]" />} onSelect={() => setScale(1)}>
          {t("subtitles.resetSize", { n: Math.round(scale * 100) })}
        </MenuItem>
      )}
      {height !== null && (
        <MenuItem icon={<span className="inline-block size-[14px]" />} onSelect={() => setHeight(null)}>
          {t("subtitles.resetHeight")}
        </MenuItem>
      )}
    </>
  );
}

/**
 * 字幕区域：鼠标放在上面按住 Ctrl（Mac 上 ⌘ 也行，触控板双指捏合也是）滚滚轮调字号；
 * 拖下沿调高度，双击下沿恢复；右键切换语言。迷你播放器里只能调字号、右键，不能拖高度。
 */
function SubtitleArea({ compact, children }: { compact: boolean; children: ReactNode }) {
  const t = useT();
  const scale = useSubtitles((s) => s.scale);
  const height = useSubtitles((s) => s.height);
  const box = useRef<HTMLDivElement>(null);
  // 要拦住网页视图自己的整页缩放，得用不是 passive 的监听
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      const { scale, setScale } = useSubtitles.getState();
      setScale(scale * Math.exp(-e.deltaY * 0.0015));
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, []);
  const startResize = (e: ReactPointerEvent<HTMLDivElement>) => {
    const el = box.current;
    if (!el || e.button !== 0) return;
    e.preventDefault();
    const handle = e.currentTarget;
    const startY = e.clientY;
    const startHeight = el.getBoundingClientRect().height;
    handle.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => useSubtitles.getState().setHeight(startHeight + ev.clientY - startY);
    const end = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", end);
      handle.removeEventListener("pointercancel", end);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", end);
    handle.addEventListener("pointercancel", end);
  };
  return (
    <ContextMenu bare items={<SubtitleContextItems />}>
      <div
        ref={box}
        className={cx("relative flex flex-col justify-center", !compact && "group/subs")}
        style={{ minHeight: compact ? undefined : (height ?? undefined) }}
      >
        <div style={{ zoom: scale }}>{children}</div>
        {!compact && (
          <div
            role="separator"
            aria-orientation="horizontal"
            aria-label={t("subtitles.resizeHint")}
            title={t("subtitles.resizeHint")}
            onPointerDown={startResize}
            onDoubleClick={() => useSubtitles.getState().setHeight(null)}
            className="absolute inset-x-0 -bottom-1.5 flex h-3 cursor-ns-resize touch-none items-center justify-center"
          >
            <span className="h-1 w-10 rounded-full bg-line-strong opacity-0 transition-opacity group-hover/subs:opacity-100" />
          </div>
        )}
      </div>
    </ContextMenu>
  );
}

/**
 * 这一句是谁说的。本机分出来的按 `order`（speakerOrder）重新编号，没编号的不标；节目文字稿里的用名字。
 */
function speakerOf(
  t: Translate,
  cue: Cue,
  order: Map<number, number> | null,
  names: readonly string[] | undefined,
  voices?: readonly Voice[],
): { index: number; name: string; voice: Voice } | null {
  if (cue.speaker === undefined) return null;
  const index = order ? order.get(cue.speaker) : cue.speaker;
  if (index === undefined) return null;
  return {
    index,
    name: names?.[cue.speaker] ?? t("subtitles.speakerN", { n: index + 1 }),
    voice: voices?.[cue.speaker] ?? null,
  };
}

/** 说话人圆章的提示：名字，猜得出时加上男声女声。 */
function speakerTitle(t: Translate, name: string, voice: Voice): string {
  if (voice === "male") return `${name} · ${t("subtitles.voiceMale")}`;
  if (voice === "female") return `${name} · ${t("subtitles.voiceFemale")}`;
  return name;
}

/**
 * 字幕行：声音放到哪儿显示哪一句，样子按设置里选的（SubtitleStyles.tsx）。还没准备好的时候说明在等什么
 * （下载这一期、加载模型、正在识别），没装识别模型就给一个去下载的入口。关着字幕、或者不是正在放的这一期时什么都不显示。
 */
function SubtitleLine({ id, compact = false }: { id: string; compact?: boolean }) {
  const t = useT();
  const lang = useLang();
  const on = useSubtitles((s) => s.on);
  const entry = useSubtitles((s) => (s.entry?.id === id ? s.entry : null));
  const needsModel = useSubtitles((s) => s.needsModel);
  const style = useSubtitles((s) => s.style);
  const showSpeakers = useSubtitles((s) => s.speakers);
  const show = useSubtitles((s) => s.show);
  const translation = useTranslationFor(id);
  const position = usePlayer((s) => (s.current?.id === id ? s.position : -1));
  const playing = usePlayer((s) => s.playing && s.current?.id === id);
  const setDialog = useUi((s) => s.setDialog);
  const cues = entry?.cues;
  // 状态里的播放位置一秒才变一次，字幕要更细：放着的时候按样子要的频率读确切的时间
  const time = usePlaybackClock(on && playing && (cues?.length ?? 0) > 0, position, clockRate(style));
  const order = useMemo(() => (entry?.source === "local" ? speakerOrder(cues ?? []) : null), [entry?.source, cues]);
  if (!on || position < 0) return null;

  const index = cues ? cueAt(cues, time) : -1;
  const cue = index >= 0 ? cues?.[index] : undefined;
  let status: ReactNode = null;
  if (needsModel) {
    status = (
      <button
        type="button"
        className="underline decoration-dotted underline-offset-2 hover:text-accent"
        onClick={() => setDialog({ type: "settings", page: "subtitles" })}
      >
        {t("subtitles.needsModel")}
      </button>
    );
  } else if (!entry) {
    status = t("subtitles.preparing");
  } else if (entry.state === "failed") {
    status = (
      <>
        <span className="text-danger">{errorText(t, { code: entry.error ?? "unknown" })}</span>{" "}
        <button type="button" className="underline decoration-dotted underline-offset-2 hover:text-accent" onClick={() => void startLocal(id)}>
          {t("action.retry")}
        </button>
      </>
    );
  } else if (entry.state === "downloading") {
    const percent = entry.total ? Math.floor((entry.received / entry.total) * 100) : null;
    status = percent === null ? t("subtitles.downloadingEpisode") : t("subtitles.downloadingEpisodeAt", { n: percent });
  } else if (entry.state === "loading") {
    status = t("subtitles.loadingModel");
  } else if (!cue && entry.source === "local" && entry.state === "running" && !isCovered(entry.covered, position)) {
    // 边下边转：还没下到这儿时说下到哪儿了，下到了就是在识别
    // 按时间粗算这个位置在文件里的哪儿（码率大致不变），下载还没到就是在等下载
    const needed = entry.duration ? Math.min(1, position / entry.duration + 0.01) : 1;
    const waiting = entry.total ? entry.received / entry.total < needed : false;
    const percent = entry.total ? Math.floor((entry.received / entry.total) * 100) : 0;
    status = waiting ? t("subtitles.downloadingEpisodeAt", { n: percent }) : t("subtitles.recognizing");
  }

  const speaker = showSpeakers && cue ? speakerOf(t, cue, order, entry?.speakerNames, entry?.voices) : null;
  const translated = cue ? translationOf(translation, cue) : undefined;
  // 只看译文：有译文的句子把原文换成译文（逐字效果按这一句的时长平均分）
  const onlyTranslation = show === "translation" && !!translated;
  const shownCues =
    onlyTranslation && cue && cues ? cues.map((c, i) => (i === index ? { ...c, text: translated, t: undefined } : c)) : cues;
  return (
    <SubtitleArea compact={compact}>
      <SubtitleView
        style={style}
        cues={shownCues ?? []}
        index={index}
        time={time}
        translation={onlyTranslation ? undefined : translated}
        speaker={speaker}
        compact={compact}
        placeholder={status}
        note={compact ? null : translationNote(t, lang, translation, !!cue && !translated)}
      />
    </SubtitleArea>
  );
}

/** 这一期在本机转写出来的字幕：正在放的用实时的，别的去核心问一次。 */
function useLocalTranscript(id: string) {
  const live = useSubtitles((s) => (s.entry?.id === id && s.entry.source === "local" ? s.entry : null));
  const saved = useQuery({
    queryKey: ["subtitles", id],
    queryFn: () => api.subtitlesGet(id),
    enabled: !live,
    staleTime: 30_000,
    retry: false,
  });
  return live ?? saved.data ?? null;
}

/** 章节和文字稿：节目提供了才有；没有文字稿的，本机转写过也算。点一条就从那里放。 */
function EpisodeExtras({ episode, queue }: { episode: Episode; queue: Episode[] }) {
  const t = useT();
  const qc = useQueryClient();
  const [panel, setPanel] = useState<"chapters" | "transcript" | null>(null);
  const { chapters: chaptersUrl, transcript } = episode.media;
  const local = useLocalTranscript(episode.id);
  const hasLocal = !transcript && (local?.cues.length ?? 0) > 0;
  const speech = useQuery({
    queryKey: ["speech"],
    queryFn: api.speechOverview,
    enabled: hasLocal && panel === "transcript",
    staleTime: 60_000,
  });
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
  const showSpeakers = useSubtitles((s) => s.speakers);
  const translation = useTranslationFor(episode.id);
  const hasChapters = (chapters.data?.length ?? 0) > 0;
  // 说话人：节目文字稿里写的名字按出现的先后编号，本机分出来的直接是编号；编号决定颜色
  const shown = useMemo(() => {
    type Line = { start: number | null; text: string; speaker?: string; index?: number; voice: Voice; first: boolean };
    let lines: Omit<Line, "first">[] | undefined;
    if (transcript) {
      const names: string[] = [];
      lines = cues.data?.map((c) => {
        let index: number | undefined;
        if (c.speaker) {
          index = names.indexOf(c.speaker);
          if (index < 0) index = names.push(c.speaker) - 1;
        }
        return { start: c.start, text: c.text, speaker: c.speaker, index, voice: null };
      });
    } else if (hasLocal && local) {
      const order = speakerOrder(local.cues);
      lines = local.cues.map((c) => {
        const who = showSpeakers ? speakerOf(t, c, order, undefined, local.voices) : null;
        return { start: c.start as number | null, text: c.text, speaker: who?.name, index: who?.index, voice: who?.voice ?? null };
      });
    }
    // 换了人的那一句写上名字，同一个人接着说的只放记号。中间夹着认不出是谁的句子不算换人
    let last: string | undefined;
    return lines?.map((l): Line => {
      const first = !!l.speaker && l.speaker !== last;
      if (l.speaker) last = l.speaker;
      return { ...l, first };
    });
  }, [transcript, cues.data, hasLocal, local, showSpeakers, t]);
  const anySpeaker = showSpeakers && (shown?.some((l) => l.speaker) ?? false);
  const activeCue = panel === "transcript" && shown && position >= 0 ? activeIndex(shown, position) : -1;
  const activeRef = useRef<HTMLParagraphElement>(null);
  // 正在放的那一句跟着滚进面板里（只在面板里滚，不带着整页动）
  useEffect(() => {
    if (activeCue < 0) return;
    const el = activeRef.current;
    const box = el?.parentElement;
    if (!el || !box) return;
    const top = el.offsetTop - box.offsetTop;
    if (top < box.scrollTop || top + el.offsetHeight > box.scrollTop + box.clientHeight) {
      box.scrollTo({ top: Math.max(0, top - box.clientHeight / 3), behavior: "smooth" });
    }
  }, [activeCue]);
  if (!hasChapters && !transcript && !hasLocal) return null;
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
        {(transcript || hasLocal) && tab("transcript", t("player.transcript"))}
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
          {transcript && cues.isFetching && !cues.data ? (
            <div className="flex justify-center py-4 text-fg-3">
              <Spinner size={14} />
            </div>
          ) : transcript && (cues.error || (cues.data && cues.data.length === 0)) ? (
            <p className="px-2 py-2 text-[12px] text-fg-3">{t("player.transcriptFailed")}</p>
          ) : (
            // 三列：时间、说话人记号（形状分男女、颜色分人）、文字。换人的那一句上面写名字
            shown?.map((c, i) => {
              const tr = c.start !== null ? translationOf(translation, { start: c.start, text: c.text }) : undefined;
              const color = c.index !== undefined ? { color: `var(--spk-${c.index % 8})` } : undefined;
              return (
                <p
                  key={`${i}:${c.start}`}
                  ref={i === activeCue ? activeRef : undefined}
                  className={cx(
                    "grid select-text gap-x-2.5 rounded-md px-2 py-1 text-[13px] leading-[22px]",
                    anySpeaker ? "grid-cols-[2.75rem_8px_minmax(0,1fr)]" : "grid-cols-[2.75rem_minmax(0,1fr)]",
                    i === activeCue ? "bg-accent-soft text-fg" : "text-fg-2",
                    c.first && i > 0 && "mt-2",
                  )}
                >
                  {anySpeaker && c.first && c.speaker && (
                    <>
                      <span />
                      <span />
                      <span className="text-[12px] font-medium leading-5" style={color}>
                        {c.speaker}
                      </span>
                    </>
                  )}
                  {c.start !== null ? (
                    <button
                      type="button"
                      onClick={() => jump(c.start ?? 0)}
                      className="tabular self-start text-right text-[11px] leading-[22px] text-fg-3 hover:text-accent"
                    >
                      {formatTime(c.start)}
                    </button>
                  ) : (
                    <span />
                  )}
                  {anySpeaker && (
                    <span className="flex h-[22px] items-center justify-center self-start">
                      {c.speaker && c.index !== undefined && (
                        <SpeakerMark voice={c.voice} index={c.index} size="8px" title={speakerTitle(t, c.speaker, c.voice)} />
                      )}
                    </span>
                  )}
                  <span className="min-w-0">
                    {c.speaker && !anySpeaker && (
                      <span className="mr-1.5 font-medium text-fg" style={showSpeakers ? color : undefined}>
                        {c.speaker}:
                      </span>
                    )}
                    {c.text}
                    {tr && <span className="block text-[12px] leading-5 text-fg-3">{tr}</span>}
                  </span>
                </p>
              );
            })
          )}
          {hasLocal && local && (
            <p className="sticky bottom-0 flex flex-wrap items-center gap-x-2 bg-surface px-2 pb-0.5 pt-2 text-[11px] text-fg-3">
              <span>
                {t("subtitles.madeBy", {
                  model: speech.data?.models.find((m) => m.id === local.model)?.name ?? local.model,
                })}
                {!local.complete &&
                  coveredPercent(local.covered, local.duration) !== null &&
                  ` · ${t("subtitles.progress", { n: coveredPercent(local.covered, local.duration) ?? 0 })}`}
              </span>
              <button
                type="button"
                className="underline decoration-dotted underline-offset-2 hover:text-accent"
                onClick={() =>
                  void redoLocal(episode.id)
                    .catch((err) => toast(errorText(t, err)))
                    .finally(() => void qc.invalidateQueries({ queryKey: ["subtitles", episode.id] }))
                }
              >
                {t("subtitles.redo")}
              </button>
            </p>
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
      <SubtitleLine id={current.id} compact />
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
        <SubtitleToggle size={15} />
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
