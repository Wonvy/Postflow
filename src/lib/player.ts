// 播客播放器：整个界面共用一个 <audio>，换条目、换列表、切到邮件都接着放。
//
// - 每一期听到哪里记在本机（localStorage，最多记 300 期），下次从那里接着放。播放进度变得很快，不放进同步数据。
// - 倍速和「播完接着放下一期」记在本机。
// - 播完把这一期标为已读（由 PlayerBridge 做，它能拿到查询缓存），开着「接着放」时放队列里的下一期。
// - 系统的媒体控制（键盘上的播放键、锁屏、控制中心）通过 Media Session 接上。

import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { Item, Media } from "./api";

export { formatTime } from "./duration";

/** 一期节目：放的时候要用到的信息。 */
export interface Episode {
  id: string;
  title: string;
  source: string;
  icon: string | null;
  image: string | null;
  media: Media;
}

export function episodeOf(item: Item): Episode | null {
  if (!item.media || !item.media.mime.startsWith("audio/")) return null;
  return {
    id: item.id,
    title: item.title,
    source: item.source_title,
    icon: item.source_icon,
    image: item.image,
    media: item.media,
  };
}

export const RATES = [0.8, 1, 1.2, 1.5, 1.75, 2, 2.5, 3] as const;
export const BACK_SECONDS = 15;
export const FORWARD_SECONDS = 30;

interface PlayerState {
  current: Episode | null;
  /** 播完后接着放的，按列表顺序。 */
  queue: Episode[];
  playing: boolean;
  /** 正在缓冲。 */
  waiting: boolean;
  position: number;
  duration: number;
  error: string | null;
  rate: number;
  autoNext: boolean;
  /** 睡眠定时：到这个时间（毫秒）暂停；"end" 是这一期放完就停。 */
  sleep: number | "end" | null;
  setRate: (rate: number) => void;
  setAutoNext: (on: boolean) => void;
}

export const usePlayer = create<PlayerState>()(
  persist(
    (set) => ({
      current: null,
      queue: [],
      playing: false,
      waiting: false,
      position: 0,
      duration: 0,
      error: null,
      rate: 1,
      autoNext: false,
      sleep: null,
      setRate: (rate) => {
        set({ rate });
        if (audio) audio.playbackRate = rate;
      },
      setAutoNext: (autoNext) => set({ autoNext }),
    }),
    {
      name: "postflow.player",
      partialize: (s) => ({ rate: s.rate, autoNext: s.autoNext }),
    },
  ),
);

// ---------------------------------------------------------------- 听到哪里

const PROGRESS_KEY = "postflow.playback";
const PROGRESS_LIMIT = 300;
/** 离结尾不到这么多秒就算听完了，下次从头放。 */
const DONE_MARGIN = 20;

type Progress = Record<string, { t: number; at: number }>;

// 列表里每一行都要看听到哪里，读一次放在内存里
let progressCache: Progress | null = null;

function readProgress(): Progress {
  if (progressCache) return progressCache;
  try {
    progressCache = JSON.parse(localStorage.getItem(PROGRESS_KEY) ?? "{}") as Progress;
  } catch {
    progressCache = {};
  }
  return progressCache;
}

function writeProgress(progress: Progress) {
  const entries = Object.entries(progress).sort((a, b) => b[1].at - a[1].at).slice(0, PROGRESS_LIMIT);
  progressCache = Object.fromEntries(entries);
  try {
    localStorage.setItem(PROGRESS_KEY, JSON.stringify(progressCache));
  } catch {
    // 存不下就算了，下次从头放
  }
}

/** 这一期上次听到第几秒。没听过、听完了是 0。 */
export function savedPosition(id: string): number {
  return readProgress()[id]?.t ?? 0;
}

function savePosition(id: string, t: number, duration: number) {
  const progress = { ...readProgress() };
  if (t < 5 || (duration > 0 && duration - t < DONE_MARGIN)) delete progress[id];
  else progress[id] = { t: Math.floor(t), at: Date.now() };
  writeProgress(progress);
}

// ---------------------------------------------------------------- 播放

let audio: HTMLAudioElement | null = null;
/** 下载到本机的单集：条目 ID → 播放地址。见 episodes.ts。 */
let localFiles: Record<string, string> = {};

export function setLocalFiles(files: Record<string, string>) {
  localFiles = files;
}
let lastSaved = 0;
let onEnded: ((episode: Episode) => void) | null = null;

/** 播完一期时调用（标为已读、放下一期）。由 PlayerBridge 设置。 */
export function setEndedHandler(handler: ((episode: Episode) => void) | null) {
  onEnded = handler;
}

function element(): HTMLAudioElement {
  if (audio) return audio;
  const el = new Audio();
  el.preload = "metadata";
  const sync = () => {
    const s = usePlayer.getState();
    const position = el.currentTime || 0;
    const duration = Number.isFinite(el.duration) ? el.duration : s.current?.media.duration ?? 0;
    // 进度条一秒动一次就够了
    if (Math.floor(position) !== Math.floor(s.position) || duration !== s.duration) {
      usePlayer.setState({ position, duration });
    }
    if (s.current && Math.abs(position - lastSaved) >= 5) {
      lastSaved = position;
      savePosition(s.current.id, position, duration);
    }
  };
  el.addEventListener("timeupdate", sync);
  el.addEventListener("durationchange", sync);
  el.addEventListener("play", () => usePlayer.setState({ playing: true, error: null }));
  el.addEventListener("pause", () => {
    usePlayer.setState({ playing: false });
    const s = usePlayer.getState();
    if (s.current) savePosition(s.current.id, el.currentTime, s.duration);
  });
  el.addEventListener("waiting", () => usePlayer.setState({ waiting: true }));
  el.addEventListener("playing", () => usePlayer.setState({ waiting: false }));
  el.addEventListener("canplay", () => usePlayer.setState({ waiting: false }));
  el.addEventListener("error", () => usePlayer.setState({ playing: false, waiting: false, error: "load" }));
  el.addEventListener("ended", () => {
    const s = usePlayer.getState();
    const finished = s.current;
    if (!finished) return;
    savePosition(finished.id, 0, 0);
    usePlayer.setState({ playing: false, position: 0 });
    onEnded?.(finished);
    // 睡眠定时选的是「这一期放完」：到这里就停，不接着放
    if (s.sleep === "end") {
      setSleep(null);
      return;
    }
    const next = s.autoNext ? s.queue[s.queue.findIndex((e) => e.id === finished.id) + 1] : undefined;
    if (next) play(next, s.queue);
  });
  audio = el;
  setupMediaSession();
  return el;
}

/** 放一期。`queue` 是它所在的列表里能放的几期，按列表顺序；`at` 是从第几秒开始（章节、文字稿里点的）。 */
export function play(episode: Episode, queue: Episode[] = [], at?: number) {
  const el = element();
  const s = usePlayer.getState();
  if (s.current?.id === episode.id) {
    if (at !== undefined) seek(at);
    void el.play().catch(() => undefined);
    return;
  }
  // 换之前把上一期听到的地方记下
  if (s.current) savePosition(s.current.id, el.currentTime, s.duration);
  usePlayer.setState({
    current: episode,
    queue: queue.length > 0 ? queue : s.queue.some((e) => e.id === episode.id) ? s.queue : [episode],
    position: at ?? savedPosition(episode.id),
    duration: episode.media.duration ?? 0,
    error: null,
    waiting: true,
  });
  // 下载过的放本机文件，不用联网
  el.src = localFiles[episode.id] ?? episode.media.url;
  el.playbackRate = s.rate;
  const start = at ?? savedPosition(episode.id);
  lastSaved = start;
  if (start > 0) {
    const seekOnce = () => {
      el.currentTime = start;
      el.removeEventListener("loadedmetadata", seekOnce);
    };
    el.addEventListener("loadedmetadata", seekOnce);
  }
  void el.play().catch(() => usePlayer.setState({ playing: false, waiting: false }));
  updateMediaMetadata(episode);
}

export function toggle() {
  const el = element();
  if (!usePlayer.getState().current) return;
  if (el.paused) void el.play().catch(() => undefined);
  else el.pause();
}

export function pause() {
  audio?.pause();
}

export function seek(seconds: number) {
  const el = element();
  const s = usePlayer.getState();
  const max = s.duration > 0 ? s.duration : Number.POSITIVE_INFINITY;
  el.currentTime = Math.max(0, Math.min(max, seconds));
  usePlayer.setState({ position: el.currentTime });
}

export function skip(delta: number) {
  seek((audio?.currentTime ?? 0) + delta);
}

/** 关掉播放器：停下来，记下听到哪里。 */
export function stop() {
  setSleep(null);
  const s = usePlayer.getState();
  if (audio) {
    if (s.current) savePosition(s.current.id, audio.currentTime, s.duration);
    audio.pause();
    audio.removeAttribute("src");
    audio.load();
  }
  usePlayer.setState({ current: null, playing: false, waiting: false, position: 0, duration: 0, error: null });
  if ("mediaSession" in navigator) navigator.mediaSession.metadata = null;
}

// ---------------------------------------------------------------- 睡眠定时

let sleepTimer: ReturnType<typeof setTimeout> | undefined;

/** 睡眠定时：几分钟后暂停，"end" 是这一期放完就停，null 取消。 */
export function setSleep(choice: number | "end" | null) {
  clearTimeout(sleepTimer);
  if (typeof choice === "number") {
    const at = Date.now() + choice * 60_000;
    usePlayer.setState({ sleep: at });
    sleepTimer = setTimeout(() => {
      pause();
      usePlayer.setState({ sleep: null });
    }, choice * 60_000);
  } else {
    usePlayer.setState({ sleep: choice });
  }
}

/** 正在放的那一期：有就播放 / 暂停，返回 true；没有在放的返回 false。 */
export function toggleIfLoaded(): boolean {
  if (!usePlayer.getState().current) return false;
  toggle();
  return true;
}

// ---------------------------------------------------------------- 系统媒体控制

function setupMediaSession() {
  if (!("mediaSession" in navigator)) return;
  const session = navigator.mediaSession;
  const handlers: [MediaSessionAction, MediaSessionActionHandler][] = [
    ["play", () => toggle()],
    ["pause", () => toggle()],
    ["seekbackward", () => skip(-BACK_SECONDS)],
    ["seekforward", () => skip(FORWARD_SECONDS)],
    ["seekto", (d) => d.seekTime !== undefined && seek(d.seekTime)],
    ["stop", () => stop()],
  ];
  for (const [action, handler] of handlers) {
    try {
      session.setActionHandler(action, handler);
    } catch {
      // 有的浏览器不支持某些动作
    }
  }
}

function updateMediaMetadata(episode: Episode) {
  if (!("mediaSession" in navigator) || typeof MediaMetadata === "undefined") return;
  const art = episode.image ?? episode.icon;
  navigator.mediaSession.metadata = new MediaMetadata({
    title: episode.title,
    artist: episode.source,
    artwork: art && /^(https?:|data:)/.test(art) ? [{ src: art }] : [],
  });
}
