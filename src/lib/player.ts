// 播客播放器：整个界面共用一个 <audio>，换条目、换列表、切到邮件都接着放。
//
// - 每一期听到哪里记在本机（localStorage，最多记 300 期），下次从那里接着放。播放进度变得很快，不放进同步数据。
// - 倍速、「播完接着放下一期」和「从旧到新」记在本机。
// - 播完把这一期标为已读（由 PlayerBridge 做，它能拿到查询缓存），开着「接着放」时放队列里的下一期。
// - 系统的媒体控制（键盘上的播放键、锁屏、控制中心）通过 Media Session 接上。

import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { Item, Media } from "./api";
import { nextEntry } from "./playerQueue";

export { formatTime } from "./duration";

/** 一期节目：放的时候要用到的信息。 */
export interface Episode {
  id: string;
  title: string;
  source: string;
  icon: string | null;
  image: string | null;
  media: Media;
  /** 发布时间和放之前是不是已经听完（已读）：「从旧到新」挑下一期用。 */
  published: number;
  read: boolean;
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
    published: item.published_at,
    read: item.read,
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
  /** 接着放时从旧到新、跳过听完的（追一个节目用）；关着时放列表里的下一条。 */
  catchUp: boolean;
  /** 睡眠定时：到这个时间（毫秒）暂停；"end" 是这一期放完就停。 */
  sleep: number | "end" | null;
  setRate: (rate: number) => void;
  setAutoNext: (on: boolean) => void;
  setCatchUp: (on: boolean) => void;
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
      catchUp: false,
      sleep: null,
      setRate: (rate) => {
        set({ rate });
        if (audio) audio.playbackRate = rate;
      },
      setAutoNext: (autoNext) => set({ autoNext }),
      setCatchUp: (catchUp) => set({ catchUp }),
    }),
    {
      name: "postflow.player",
      partialize: (s) => ({ rate: s.rate, autoNext: s.autoNext, catchUp: s.catchUp }),
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
/** 这次打开以后放完的几期（队列里的「已读」是开始放时的样子，后来放完的记在这里）。 */
const played = new Set<string>();
/** 下载到本机的单集：条目 ID → 播放地址。见 episodes.ts。 */
let localFiles: Record<string, string> = {};

export function setLocalFiles(files: Record<string, string>) {
  localFiles = files;
}

/** 现在放的是哪个地址。 */
let playingSrc: string | null = null;

/** 现在放到第几秒，比状态里的 `position`（一秒变一次）细；没在放是 null。字幕用它对时间。 */
export function playbackTime(): number | null {
  return audio && usePlayer.getState().current ? audio.currentTime : null;
}

/**
 * 正在放的这一期刚下载到了本机：换成放本机文件，接着刚才的位置。
 * 本机转写的字幕是照着下载的那份文件转的。有的节目每次请求插的广告不一样，在线听到的和下载的
 * 那份时间对不上，换过来字幕才跟得准。
 */
export function switchToLocal(id: string, url: string): void {
  const el = audio;
  const s = usePlayer.getState();
  if (!el || s.current?.id !== id || playingSrc === url || brokenLocal.has(id)) return;
  clearTimeout(retryTimer);
  retries = 0;
  reload(el, url, el.currentTime, !el.paused);
}
let lastSaved = 0;

// ---------------------------------------------------------------- 出错时自己接上

/** 本机文件放不了的几期：这次打开应用期间改放网上的地址，不再换回本机文件。 */
const brokenLocal = new Set<string>();
/** 网上的地址连续出错后，隔多久重新接上（毫秒）。用完了才告诉用户放不了。 */
const RETRY_DELAYS = [1000, 3000, 8000];
let retries = 0;
let retryTimer: ReturnType<typeof setTimeout> | undefined;
/** 这个地址有没有放出过声音：一次都没放出来的「格式不支持」多半是地址本身坏了，不用重试。 */
let srcStarted = false;

/** 换到 `src` 并回到第 `at` 秒，原来在放就接着放。 */
function reload(el: HTMLAudioElement, src: string, at: number, resume: boolean) {
  playingSrc = src;
  srcStarted = false;
  el.src = src;
  el.playbackRate = usePlayer.getState().rate;
  if (at > 0) {
    const seekOnce = () => {
      el.currentTime = at;
      el.removeEventListener("loadedmetadata", seekOnce);
    };
    el.addEventListener("loadedmetadata", seekOnce);
  }
  if (resume) void el.play().catch(() => undefined);
}

/**
 * 播放出错时先自己接上，接不上才报错：
 * - 放的是下载到本机的文件：改放网上的地址，接着刚才的位置（本机文件坏了、桌面应用读不了它都会这样）。
 * - 放的是网上的地址：网络断了一下、服务器掐了长连接，隔一会儿从刚才的位置重新请求，最多三次。
 */
function recover(el: HTMLAudioElement) {
  const s = usePlayer.getState();
  const episode = s.current;
  if (!episode) return;
  const err = el.error;
  console.warn("postflow: playback error", err?.code, err?.message, playingSrc);
  const at = el.currentTime || s.position;
  const resume = s.playing || s.waiting;
  const remote = episode.media.url;
  clearTimeout(retryTimer);
  if (playingSrc && playingSrc !== remote) {
    brokenLocal.add(episode.id);
    usePlayer.setState({ waiting: resume });
    reload(el, remote, at, resume);
    return;
  }
  const hopeless = err?.code === MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED && !srcStarted;
  if (hopeless || retries >= RETRY_DELAYS.length) {
    usePlayer.setState({ playing: false, waiting: false, error: "load" });
    return;
  }
  const delay = RETRY_DELAYS[retries++];
  usePlayer.setState({ waiting: resume });
  retryTimer = setTimeout(() => {
    // 这期间换了一期或关掉了播放器，就不接了
    if (usePlayer.getState().current?.id !== episode.id || playingSrc !== remote) return;
    reload(el, remote, at, resume);
  }, delay);
}

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
  el.addEventListener("playing", () => {
    srcStarted = true;
    retries = 0;
    usePlayer.setState({ waiting: false, error: null });
  });
  el.addEventListener("canplay", () => usePlayer.setState({ waiting: false }));
  el.addEventListener("error", () => recover(el));
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
    played.add(finished.id);
    const next = s.autoNext ? nextEntry(s.queue, finished.id, s.catchUp, played) : undefined;
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
  // 下载过的放本机文件，不用联网（这次放出过错的除外）
  clearTimeout(retryTimer);
  retries = 0;
  const local = brokenLocal.has(episode.id) ? undefined : localFiles[episode.id];
  const start = at ?? savedPosition(episode.id);
  lastSaved = start;
  reload(el, local ?? episode.media.url, start, false);
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
  clearTimeout(retryTimer);
  retries = 0;
  if (audio) {
    if (s.current) savePosition(s.current.id, audio.currentTime, s.duration);
    audio.pause();
    audio.removeAttribute("src");
    audio.load();
    playingSrc = null;
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

/** 系统的播放键交还给播客（朗读用过以后）。 */
export function restoreMediaSession() {
  if (!("mediaSession" in navigator)) return;
  setupMediaSession();
  const current = usePlayer.getState().current;
  if (current) updateMediaMetadata(current);
  else navigator.mediaSession.metadata = null;
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
