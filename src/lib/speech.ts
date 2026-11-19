// 朗读和听新闻：用本机声音（核心那台电脑上的 Kokoro 模型合成，见 voices.ts），或者系统自带的语音（speechSynthesis）。
// 每篇念什么（全文、订阅源的摘要、AI 总结、只念标题）、念完接着念不念下一篇、念完标不标已读，在设置里选（见 voices.ts）。
//
// 一篇的文字切成一句一句排队念（见 speechText.ts），可以暂停、上一句、下一句、调语速。换着看别的文章也接着念，
// 列表底下有一条小控制条（SpeechBar）。念完一篇，按开始念时那个列表的顺序接着念下一篇：开着的正是刚念完的那篇，
// 就跟着打开下一篇。下一篇的文字（全文、AI 总结）在念这一篇的时候就去准备。
//
// AI 总结要花钱：只给正在念的和下一篇要，写过的直接用（和 AI 面板里的总结是同一份），出错了这次就不再要，改念摘要。
//
// 本机声音一句一句向核心要：这一句念好就放，同时要后面两句，所以句子之间不用等。念不出来（模型被删了、
// 加载失败）就改用系统语音接着念。没有系统语音（Linux 的系统网页视图多半没有）也没装本机声音时，不显示朗读按钮。

import { useEffect } from "react";
import { create } from "zustand";
import { toast } from "../components/Toast";
import { useAiStreams } from "./ai";
import { api, type Item } from "./api";
import { activePack, errorText, type MessageKey, resolveLang, translate } from "./i18n";
import { firstSentences, heading, type ListenContent, nextInQueue, spokenSummary } from "./listenText";
import { pause as pausePodcast, restoreMediaSession, usePlayer } from "./player";
import { useSetItems } from "./queries";
import { chunks, guessLang } from "./speechText";
import { useUi } from "./store";
import { isSilence, knownVoiceOverview, localVoiceFor, unlockAudio, useVoices, voiceOverview, wavUrl } from "./voices";

export const SPEECH_RATES = [0.8, 1, 1.2, 1.5, 1.8, 2] as const;

const LANG_TAGS: Record<ReturnType<typeof guessLang>, string> = {
  zh: "zh-CN",
  ja: "ja-JP",
  ko: "ko-KR",
  en: "en-US",
};

/** 有没有系统语音。 */
export function speechSupported(): boolean {
  return typeof window !== "undefined" && "speechSynthesis" in window && typeof SpeechSynthesisUtterance !== "undefined";
}

/** 朗读按钮显不显示：有系统语音，或者核心那边装了本机声音。 */
export function useReadAloudAvailable(): boolean {
  const installed = useVoices((s) => s.installed);
  return speechSupported() || installed;
}

/** 要念的一篇。 */
export interface ListenItem {
  id: string;
  title: string;
  /** 订阅源的名字或者发件人。 */
  source: string;
  /** 订阅源自带的摘要（纯文字）。 */
  summary: string;
  /** 开始念时已经读过了。 */
  read: boolean;
}

export function listenItemOf(item: Item): ListenItem {
  return { id: item.id, title: item.title, source: item.source_title, summary: item.summary, read: item.read };
}

/** 列表里能念的：播客和视频本身就是声音，跳过。 */
export function listenQueueOf(items: readonly Item[]): ListenItem[] {
  return items.filter((i) => !i.media && !i.draft).map(listenItemOf);
}

interface Speech {
  /** 正在念的是哪一篇。 */
  itemId: string | null;
  title: string;
  source: string;
  parts: string[];
  index: number;
  playing: boolean;
  /** 在等本机声音念好这一句。 */
  waiting: boolean;
  /** 在准备这一篇的文字（取全文、等 AI 总结）。 */
  preparing: boolean;
  lang: string;
  rate: number;
  /** 用本机声音念时是哪个模型、哪种声音；null 是用系统语音。 */
  local: { model: string; voice: string } | null;
  /** 念完接着念的列表：开始念时那个列表的样子。 */
  queue: ListenItem[];
  /** 定时停止：到这个时间（毫秒）停；"item" 是念完这一篇停。 */
  sleep: number | "item" | null;
  /** 这次让 AI 新写了几篇总结（用写过的不算）。 */
  aiCount: number;
  /**
   * 开始念一篇。`shown` 是阅读区里显示着的文字（开着翻译就是译文），念全文时用它；
   * `queue` 是念完接着念的列表；`announce` 时先报来源。
   */
  start: (item: ListenItem, shown: string | null, queue?: ListenItem[], announce?: boolean) => void;
  toggle: () => void;
  jump: (delta: number) => void;
  /** 跳到列表里的下一篇。 */
  next: () => void;
  /** 按现在的「念什么」把这一篇重新念一遍（在菜单里换了念法时）。 */
  restart: () => void;
  stop: () => void;
  setRate: (rate: number) => void;
  /** 几分钟后停，"item" 念完这一篇停，null 不定时。 */
  setSleep: (sleep: number | "item" | null) => void;
}

function savedRate(): number {
  try {
    const r = Number(localStorage.getItem("postflow.speechRate"));
    return SPEECH_RATES.includes(r as (typeof SPEECH_RATES)[number]) ? r : 1;
  } catch {
    return 1;
  }
}

function t(key: MessageKey, vars?: Record<string, string | number>): string {
  const pref = useUi.getState().lang;
  return translate(resolveLang(pref), key, vars, activePack(pref));
}

/** 系统语音里这种语言用哪种声音：设置里选过的，否则本机的优先（不用联网），名字里带 Premium / Enhanced 的更自然。 */
export function systemVoiceFor(lang: string): SpeechSynthesisVoice | undefined {
  const all = window.speechSynthesis.getVoices();
  const chosen = useVoices.getState().system[lang.slice(0, 2)];
  const picked = chosen ? all.find((v) => v.voiceURI === chosen) : undefined;
  if (picked) return picked;
  const voices = all.filter((v) => v.lang.replace("_", "-").startsWith(lang.slice(0, 2)));
  const exact = voices.filter((v) => v.lang.replace("_", "-") === lang);
  const pool = exact.length > 0 ? exact : voices;
  return (
    pool.find((v) => /premium|enhanced|natural/i.test(v.name) && v.localService) ??
    pool.find((v) => v.localService) ??
    pool[0]
  );
}

/** 每次开始念一句（或者换一篇）就加一。等着的回调发现数字变了（换了句子、暂停、停止），就不再往下走。 */
let generation = 0;
/** 正在念的这一篇（重新念、跳到下一篇用）。 */
let current: ListenItem | null = null;

function speakFrom(index: number) {
  if (useSpeech.getState().local) void localFrom(index);
  else systemFrom(index);
}

// ---------------------------------------------------------------- 一篇的文字

/** 准备好的（或者正在准备的）文字：`条目|念法` → 文字。只留正在念的和下一篇。 */
const prepared = new Map<string, Promise<string>>();
/** 这次朗读里 AI 总结出过错（没设置 AI、余额不足……）：后面的都不再要，改念摘要。值是错误码。 */
let aiOff: string | null = null;

/** HTML 正文变成要念的文字：段落之间换行，代码、脚本、图片不念。 */
function htmlToText(html: string): string {
  const doc = new DOMParser().parseFromString(html, "text/html");
  doc.querySelectorAll("script, style, noscript, template, pre, svg, iframe, img, video, audio").forEach((e) => e.remove());
  doc.querySelectorAll("br").forEach((e) => e.replaceWith("\n"));
  doc
    .querySelectorAll("p, div, li, h1, h2, h3, h4, h5, h6, blockquote, tr, section, article, figcaption, dt, dd")
    .forEach((e) => e.append("\n"));
  return (doc.body?.textContent ?? "").replace(/[ \t]+/g, " ").replace(/\n\s*\n+/g, "\n").trim();
}

async function fullText(item: ListenItem): Promise<string> {
  const d = await api.getItem(item.id);
  return htmlToText(d.full_text || d.content || d.mail?.body || "");
}

/** 等一个 AI 请求写完。 */
function waitAi(requestId: string): Promise<string> {
  return new Promise((resolve, reject) => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const check = (byId: ReturnType<typeof useAiStreams.getState>["byId"]) => {
      const s = byId[requestId];
      if (!s?.done) return false;
      unsubscribe();
      clearTimeout(timer);
      useAiStreams.getState().forget(requestId);
      if (s.error) reject({ code: s.error, message: s.message ?? "" });
      else resolve(s.text);
      return true;
    };
    const unsubscribe = useAiStreams.subscribe((s) => void check(s.byId));
    timer = setTimeout(() => {
      unsubscribe();
      reject({ code: "ai_timeout", message: "" });
    }, 120_000);
    check(useAiStreams.getState().byId);
  });
}

/** AI 总结：写过的直接用（不花钱），没写过的让 AI 写。 */
async function aiSummary(item: ListenItem): Promise<string> {
  if (aiOff) throw { code: aiOff };
  const lang = resolveLang(useUi.getState().lang);
  const r = await api.aiRun(item.id, "summary", lang, false);
  if (r.cached) return spokenSummary(r.cached.content);
  if (!r.request_id) throw { code: "unknown" };
  useSpeech.setState((s) => ({ aiCount: s.aiCount + 1 }));
  return spokenSummary(await waitAi(r.request_id));
}

/** 订阅源的摘要；没有的话，全文的开头几句。 */
async function summaryOf(item: ListenItem): Promise<string> {
  return item.summary.trim() || firstSentences(await fullText(item).catch(() => ""));
}

function codeOf(err: unknown): string {
  return err && typeof err === "object" && "code" in err ? String((err as { code: unknown }).code) : "unknown";
}

async function bodyOf(item: ListenItem, content: ListenContent): Promise<string> {
  switch (content) {
    case "title":
      return "";
    case "summary":
      return summaryOf(item);
    case "ai":
      try {
        return await aiSummary(item);
      } catch (err) {
        const code = codeOf(err);
        // 邮件按设置不能发给云端 AI：只是这一封念摘要。其余的错误（没设置、余额、网络）这次都不再要
        if (code !== "ai_mail_blocked" && !aiOff) {
          aiOff = code;
          toast(t("listen.aiFailed", { error: errorText(t, err) }));
        }
        return summaryOf(item);
      }
    default:
      return fullText(item);
  }
}

/** 这一篇按这种念法的文字：准备过的直接用。 */
function textOf(item: ListenItem, content: ListenContent): Promise<string> {
  const key = `${item.id}|${content}`;
  let p = prepared.get(key);
  if (!p) {
    p = bodyOf(item, content);
    p.catch(() => prepared.delete(key));
    prepared.set(key, p);
  }
  return p;
}

/** 念这一篇的时候准备下一篇，只准备一篇（AI 总结要花钱）。 */
function prepareNext(item: ListenItem) {
  const { autoNext, content } = useVoices.getState();
  const next = autoNext ? nextInQueue(useSpeech.getState().queue, item.id) : undefined;
  const keep = new Set([`${item.id}|${content}`, next ? `${next.id}|${content}` : ""]);
  for (const key of prepared.keys()) if (!keep.has(key)) prepared.delete(key);
  if (next) void textOf(next, content).catch(() => undefined);
}

/** 文字能马上拿到的念法（不用取全文、不用等 AI）：在点击里直接开始念，Safari 的系统语音要这样。 */
function textNow(item: ListenItem, content: ListenContent, shown: string | null): string | null {
  if (content === "full" && shown !== null) return shown;
  if (content === "title") return "";
  if (content === "summary" && item.summary.trim()) return item.summary.trim();
  return null;
}

/** 开始念一篇。 */
function begin(item: ListenItem, shown: string | null, announce: boolean) {
  generation++;
  const mine = generation;
  if (speechSupported()) window.speechSynthesis.cancel();
  stopAudio();
  forgetClips();
  current = item;
  useSpeech.setState({
    itemId: item.id,
    title: item.title,
    source: item.source,
    parts: [],
    index: 0,
    playing: true,
    waiting: true,
    preparing: true,
  });
  claimMediaSession();
  const content = useVoices.getState().content;
  const go = (body: string) => {
    if (mine !== generation) return;
    const parts = chunks(`${heading(item.title, item.source, announce)}。\n${body}`);
    const lang = guessLang(body.trim() || item.title);
    const ready = (local: Speech["local"]) => {
      if (mine !== generation) return;
      if (!local && !speechSupported()) {
        useSpeech.setState({ playing: false, waiting: false, preparing: false });
        return;
      }
      useSpeech.setState({ parts, lang: LANG_TAGS[lang], local, preparing: false });
      speakFrom(0);
      prepareNext(item);
    };
    // 用什么念最好马上定下来（还在点击里时系统语音才念得出来）
    const known = knownVoiceOverview();
    if (known) ready(localVoiceFor(known, lang));
    else
      voiceOverview()
        .then((o) => localVoiceFor(o, lang))
        .catch(() => null)
        .then(ready);
  };
  const now = textNow(item, content, shown);
  if (now !== null) go(now);
  else
    textOf(item, content)
      .catch(() => item.summary)
      .then(go);
}

/** 一篇念完了：标为已读，接着念下一篇（或者停下）。 */
function itemDone() {
  const s = useSpeech.getState();
  const done = current;
  if (!done) return;
  const prefs = useVoices.getState();
  if (prefs.markRead && !done.read) markReadHandler?.(done.id);
  if (s.sleep === "item") {
    useSpeech.getState().stop();
    return;
  }
  const next = prefs.autoNext ? nextInQueue(s.queue, done.id) : undefined;
  if (!next) {
    if (prefs.autoNext && s.queue.length > 1 && s.queue.some((e) => e.id === done.id)) toast(t("listen.finished"));
    useSpeech.getState().stop();
    return;
  }
  moveTo(next, done.id);
}

/** 换到下一篇；阅读区开着的正是上一篇的话，跟着打开它。 */
function moveTo(next: ListenItem, from: string) {
  const ui = useUi.getState();
  if (ui.openId === from) ui.open(next.id);
  begin(next, null, true);
}

// ---------------------------------------------------------------- 系统语音

/** 念第 `index` 段，念完自动接下一段。 */
function systemFrom(index: number) {
  const s = useSpeech.getState();
  generation++;
  const mine = generation;
  stopAudio();
  if (!speechSupported()) {
    useSpeech.setState({ playing: false, waiting: false });
    return;
  }
  const synth = window.speechSynthesis;
  synth.cancel();
  const text = s.parts[index];
  if (text === undefined) {
    itemDone();
    return;
  }
  const u = new SpeechSynthesisUtterance(text);
  u.lang = s.lang;
  u.rate = s.rate;
  const voice = systemVoiceFor(s.lang);
  if (voice) u.voice = voice;
  u.onend = () => {
    if (mine !== generation) return;
    systemFrom(index + 1);
  };
  u.onerror = (e) => {
    if (mine !== generation || e.error === "interrupted" || e.error === "canceled") return;
    useSpeech.setState({ playing: false });
  };
  useSpeech.setState({ index, playing: true, waiting: false });
  synth.speak(u);
}

// ---------------------------------------------------------------- 本机声音

let audio: HTMLAudioElement | null = null;
/** 念好了（或者正在念）的句子：第几句 → 放的地址（没什么可念的句子是 null）。 */
let clips = new Map<number, Promise<string | null>>();
/** 这批句子是用什么念的：换了文章、声音或者语速就作废。 */
let clipKey = "";
/** 播放器里放着的是第几句：暂停以后接着放用。 */
let audioIndex = -1;

function player(): HTMLAudioElement {
  if (!audio) {
    audio = new Audio();
    audio.preload = "auto";
  }
  return audio;
}

function stopAudio() {
  if (!audio) return;
  audio.onended = null;
  if (!isSilence(audio)) audio.pause();
  audioIndex = -1;
}

function revoke(p: Promise<string | null>) {
  p.then((url) => url && URL.revokeObjectURL(url)).catch(() => undefined);
}

function forgetClips() {
  for (const p of clips.values()) revoke(p);
  clips = new Map();
  clipKey = "";
}

/** 第 `index` 句的声音：念过的直接给，没念过的向核心要。 */
function clip(index: number): Promise<string | null> | undefined {
  const s = useSpeech.getState();
  const text = s.parts[index];
  if (text === undefined || !s.local) return undefined;
  const key = `${s.itemId}|${s.local.model}|${s.local.voice}|${s.rate}`;
  if (key !== clipKey) {
    forgetClips();
    clipKey = key;
  }
  let p = clips.get(index);
  if (!p) {
    const batch = clips;
    p = api.voicesSpeak(s.local.model, s.local.voice, text, s.rate).then((r) => {
      const url = r.audio ? wavUrl(r.audio) : null;
      // 念好的时候这批已经作废了（换了声音、停了）：地址不用留
      if (url && clips !== batch) URL.revokeObjectURL(url);
      return clips === batch ? url : null;
    });
    // 预先要的那几句出错不用管：轮到它的时候再报
    p.catch(() => undefined);
    clips.set(index, p);
  }
  return p;
}

/** 前面隔得远的句子用不上了，放掉；留着上一句，点「上一句」时不用重念。 */
function trimClips(index: number) {
  for (const [i, p] of clips) {
    if (i < index - 1 || i > index + 3) {
      revoke(p);
      clips.delete(i);
    }
  }
}

async function localFrom(index: number) {
  const s = useSpeech.getState();
  generation++;
  const mine = generation;
  if (speechSupported()) window.speechSynthesis.cancel();
  stopAudio();
  if (s.parts[index] === undefined) {
    itemDone();
    return;
  }
  useSpeech.setState({ index, playing: true, waiting: true });
  trimClips(index);
  let url: string | null;
  try {
    url = (await clip(index)) ?? null;
  } catch (err) {
    if (mine !== generation) return;
    fallBack(index, err);
    return;
  }
  if (mine !== generation) return;
  // 这一句好了再要后面两句：同时要的话，核心可能先念后面的，这一句反而要等
  void clip(index + 1)
    ?.then(() => clip(index + 2))
    .catch(() => undefined);
  if (!url) {
    void localFrom(index + 1);
    return;
  }
  const el = player();
  el.src = url;
  audioIndex = index;
  el.onended = () => {
    if (mine === generation) void localFrom(index + 1);
  };
  try {
    await el.play();
  } catch {
    if (mine === generation) useSpeech.setState({ playing: false, waiting: false });
    return;
  }
  if (mine === generation) useSpeech.setState({ waiting: false });
}

/** 本机声音念不出来：有系统语音就换成它接着念，没有就停下。都告诉用户为什么。 */
function fallBack(index: number, err: unknown) {
  const reason = errorText(t, voiceError(err));
  if (speechSupported()) {
    toast(t("voices.fellBack", { error: reason }));
    useSpeech.setState({ local: null });
    systemFrom(index);
  } else {
    toast(reason);
    useSpeech.setState({ playing: false, waiting: false });
  }
}

/** 核心报的是识别模型的错误码（两边共用一套下载代码）：换成朗读模型的说法。 */
export function voiceError(err: unknown): unknown {
  if (err && typeof err === "object" && "code" in err) {
    const code = String((err as { code: unknown }).code);
    if (code.startsWith("speech_") && code !== "speech_import_missing") {
      return { code: code.replace(/^speech_/, "voice_") };
    }
  }
  return err;
}

// ---------------------------------------------------------------- 系统的媒体控制（耳机、键盘上的播放键）

function claimMediaSession() {
  if (!("mediaSession" in navigator)) return;
  const session = navigator.mediaSession;
  const s = useSpeech.getState();
  try {
    session.metadata = typeof MediaMetadata === "undefined" ? null : new MediaMetadata({ title: s.title, artist: s.source });
  } catch {
    // 有的网页视图没有 MediaMetadata
  }
  const handlers: [MediaSessionAction, MediaSessionActionHandler | null][] = [
    ["play", () => !useSpeech.getState().playing && useSpeech.getState().toggle()],
    ["pause", () => useSpeech.getState().playing && useSpeech.getState().toggle()],
    ["nexttrack", () => useSpeech.getState().next()],
    ["previoustrack", () => useSpeech.getState().jump(-1)],
    ["stop", () => useSpeech.getState().stop()],
    ["seekbackward", null],
    ["seekforward", null],
    ["seekto", null],
  ];
  for (const [action, handler] of handlers) {
    try {
      session.setActionHandler(action, handler);
    } catch {
      // 有的浏览器不支持某些动作
    }
  }
}

// ---------------------------------------------------------------- 标为已读

/** 标为已读要走界面的查询缓存（列表马上变灰），由界面里一直在的组件交进来，见 useSpeechBridge。 */
let markReadHandler: ((id: string) => void) | null = null;

/** 让念完的文章能标为已读。放在一直显示着的组件里（列表底下的控制条）。 */
export function useSpeechBridge() {
  const { mutate } = useSetItems();
  useEffect(() => {
    markReadHandler = (id) => mutate({ ids: [id], patch: { read: true } });
    return () => {
      markReadHandler = null;
    };
  }, [mutate]);
}

// ---------------------------------------------------------------- 状态

let sleepTimer: ReturnType<typeof setTimeout> | null = null;

export const useSpeech = create<Speech>()((set, get) => ({
  itemId: null,
  title: "",
  source: "",
  parts: [],
  index: 0,
  playing: false,
  waiting: false,
  preparing: false,
  lang: "en-US",
  rate: savedRate(),
  local: null,
  queue: [],
  sleep: null,
  aiCount: 0,
  start: (item, shown, queue, announce = false) => {
    // 播客在放的话先停下，两个声音叠在一起听不清
    if (usePlayer.getState().playing) pausePodcast();
    // 还在点击里：先解锁播放器（Safari 和桌面应用的网页视图只允许在点击时开始放声音）
    unlockAudio(player());
    const fresh = get().itemId === null;
    if (fresh) {
      aiOff = null;
      prepared.clear();
    }
    set({ queue: queue ?? [], ...(fresh ? { aiCount: 0 } : {}) });
    begin(item, shown, announce);
  },
  toggle: () => {
    const s = get();
    if (!s.itemId) return;
    if (s.playing) {
      if (s.local && audio && audioIndex === s.index && !s.waiting) {
        // 本机声音能真的暂停，接着放时从停下的地方继续
        audio.pause();
      } else {
        generation++;
        stopAudio();
        if (speechSupported()) window.speechSynthesis.cancel();
      }
      set({ playing: false, waiting: false });
    } else if (s.local && audio && audioIndex === s.index && audio.src && !audio.ended) {
      unlockAudio(audio);
      set({ playing: true });
      if (usePlayer.getState().playing) pausePodcast();
      claimMediaSession();
      void audio.play().catch(() => set({ playing: false }));
    } else if (s.parts.length === 0 && current) {
      // 停在准备文字的时候：重新开始这一篇
      unlockAudio(player());
      if (usePlayer.getState().playing) pausePodcast();
      begin(current, null, false);
    } else {
      if (usePlayer.getState().playing) pausePodcast();
      unlockAudio(player());
      claimMediaSession();
      // 从这一段开头接着念（系统语音有的暂停后接不上，干脆重念这一段）
      speakFrom(s.index);
    }
  },
  jump: (delta) => {
    const s = get();
    if (!s.itemId || s.parts.length === 0) return;
    const next = Math.max(0, Math.min(s.parts.length - 1, s.index + delta));
    if (s.playing) speakFrom(next);
    else {
      generation++;
      stopAudio();
      set({ index: next });
    }
  },
  next: () => {
    const s = get();
    if (!current) return;
    const next = nextInQueue(s.queue, current.id);
    if (next) {
      unlockAudio(player());
      moveTo(next, current.id);
    } else get().jump(1);
  },
  restart: () => {
    if (!current || !get().itemId) return;
    unlockAudio(player());
    begin(current, null, false);
  },
  stop: () => {
    generation++;
    if (speechSupported()) window.speechSynthesis.cancel();
    stopAudio();
    forgetClips();
    prepared.clear();
    current = null;
    if (sleepTimer) clearTimeout(sleepTimer);
    sleepTimer = null;
    set({
      itemId: null,
      title: "",
      source: "",
      parts: [],
      index: 0,
      playing: false,
      waiting: false,
      preparing: false,
      local: null,
      queue: [],
      sleep: null,
    });
    // 系统的播放键还给播客
    restoreMediaSession();
  },
  setRate: (rate) => {
    try {
      localStorage.setItem("postflow.speechRate", String(rate));
    } catch {
      // 存不了就只管这一次
    }
    set({ rate });
    const s = get();
    // 本机声音按语速念好的，换了语速要重念（clip 发现语速变了会自己作废旧的）
    if (s.playing && s.parts.length > 0) speakFrom(s.index);
  },
  setSleep: (sleep) => {
    if (sleepTimer) clearTimeout(sleepTimer);
    sleepTimer = null;
    if (typeof sleep === "number") {
      const ms = sleep * 60_000;
      sleepTimer = setTimeout(() => get().stop(), ms);
      set({ sleep: Date.now() + ms });
    } else set({ sleep });
  },
}));

// 播客开始放：朗读停下来（暂停，可以接着念），系统的播放键归播客
usePlayer.subscribe((p, prev) => {
  if (p.playing && !prev.playing && useSpeech.getState().playing) {
    useSpeech.getState().toggle();
    restoreMediaSession();
  }
});
