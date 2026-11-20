// 朗读用什么声音：本机声音（Kokoro 模型，核心那台电脑上合成）还是系统语音，每种语言选哪种声音；
// 以及听新闻的选项：每篇念什么、念完接着念下一篇、念完标为已读。
//
// 选择记在这个浏览器（桌面应用、手机上的网页各记各的）：手机上可以继续用手机自己的系统语音，
// 也可以选本机声音——那样是电脑念好、手机放。模型装在核心那边，列表从核心取。

import { create } from "zustand";
import { persist } from "zustand/middleware";
import { api, type SpeechOverview, type VoiceSample } from "./api";
import type { ListenContent } from "./listenText";
import type { CoreEvent } from "./transport";
import { pickModel, pickSample, pickVoice, type VoiceLang } from "./voiceNames";

type VoiceModelEvent = Extract<CoreEvent, { type: "voice_model" }>;

export interface ModelDownload {
  received: number;
  total: number;
  state: VoiceModelEvent["state"];
  error?: string;
}

interface VoicePrefs {
  /** "auto"：装了本机声音就用，没装用系统语音；"system"：总用系统语音；其余是模型 ID。 */
  engine: string;
  setEngine: (engine: string) => void;
  /** 每个模型每种语言选的声音，键是 `模型 ID:语言`。 */
  chosen: Record<string, string>;
  choose: (model: string, lang: VoiceLang, voice: string) => void;
  /** 系统语音每种语言选的声音（voiceURI）；没选的自动挑。 */
  system: Record<string, string>;
  chooseSystem: (lang: string, uri: string | null) => void;
  /** 核心那边装了本机声音模型（决定没有系统语音的地方显不显示朗读按钮）。 */
  installed: boolean;
  /** 模型的下载进度。 */
  downloads: Record<string, ModelDownload>;
  /** 每篇念什么：全文、订阅源的摘要、AI 总结，或者只念标题。 */
  content: ListenContent;
  setContent: (content: ListenContent) => void;
  /** 念完一篇接着念列表里的下一篇。 */
  autoNext: boolean;
  setAutoNext: (on: boolean) => void;
  /** 念完一篇就标为已读。 */
  markRead: boolean;
  setMarkRead: (on: boolean) => void;
  /** 列表底下的控制条上显示正在念的这一句（字幕）。 */
  captions: boolean;
  setCaptions: (on: boolean) => void;
}

export const useVoices = create<VoicePrefs>()(
  persist(
    (set) => ({
      engine: "auto",
      setEngine: (engine) => set({ engine }),
      chosen: {},
      choose: (model, lang, voice) => set((s) => ({ chosen: { ...s.chosen, [`${model}:${lang}`]: voice } })),
      system: {},
      chooseSystem: (lang, uri) =>
        set((s) => {
          const system = { ...s.system };
          if (uri) system[lang] = uri;
          else delete system[lang];
          return { system };
        }),
      installed: false,
      downloads: {},
      content: "full",
      setContent: (content) => set({ content }),
      autoNext: true,
      setAutoNext: (autoNext) => set({ autoNext }),
      markRead: true,
      setMarkRead: (markRead) => set({ markRead }),
      captions: true,
      setCaptions: (captions) => set({ captions }),
    }),
    {
      name: "postflow.voices",
      partialize: (s) => ({
        engine: s.engine,
        chosen: s.chosen,
        system: s.system,
        installed: s.installed,
        content: s.content,
        autoNext: s.autoNext,
        markRead: s.markRead,
        captions: s.captions,
      }),
    },
  ),
);

/** 核心那边的朗读模型，加上 ZipVoice 用的声音样本。 */
export type VoiceState = SpeechOverview & { samples: VoiceSample[] };

let overview: Promise<VoiceState> | null = null;
/** 上次取到的，不用等。 */
let known: VoiceState | null = null;

/** 能用的声音样本（填了录音里说的话的）。 */
export function readySamples(samples: readonly VoiceSample[]): string[] {
  return samples.filter((s) => s.text.trim()).map((s) => s.id);
}

/** 核心那边的朗读模型和声音样本。取一次留着，模型、样本变了时重新取。 */
export function voiceOverview(): Promise<VoiceState> {
  if (!overview) {
    const pending = Promise.all([
      api.voicesOverview(),
      api.voicesSamples().catch(() => ({ samples: [] as VoiceSample[] })),
    ]).then(([o, { samples }]) => {
      const state: VoiceState = { ...o, samples };
      known = state;
      const ready = readySamples(samples).length > 0;
      useVoices.setState({
        installed: o.models.some((m) => m.installed && m.supported && (m.kind !== "zipvoice" || ready)),
      });
      return state;
    });
    pending.catch(() => {
      // 下次再试
      if (overview === pending) overview = null;
    });
    overview = pending;
  }
  return overview;
}

/** 已经取到的朗读模型，还没取到时是 null。点「朗读」时要马上决定用什么念（见 unlockAudio）。 */
export function knownVoiceOverview(): VoiceState | null {
  return known;
}

/** 模型装上或删掉了：下次重新取。 */
export function forgetVoiceOverview(): void {
  overview = null;
  void voiceOverview().catch(() => undefined);
}

/** 核心的事件：朗读模型的下载进度。 */
export function onVoiceModelEvent(event: VoiceModelEvent): void {
  useVoices.setState((s) => ({
    downloads: {
      ...s.downloads,
      [event.id]: { received: event.received, total: event.total, state: event.state, error: event.error },
    },
  }));
  if (event.state === "installed") forgetVoiceOverview();
}

/** 用本机的哪个模型、哪种声音念这种语言；用系统语音时是 null。 */
export function localVoiceFor(o: VoiceState, lang: string): { model: string; voice: string } | null {
  const { engine, chosen } = useVoices.getState();
  const samples = readySamples(o.samples);
  const id = pickModel(o.models, lang, engine, samples);
  const model = id ? o.models.find((m) => m.id === id) : undefined;
  if (!model || (lang !== "zh" && lang !== "en")) return null;
  const want = chosen[`${model.id}:${lang}`];
  const voice =
    model.kind === "zipvoice" ? pickSample(samples, want) : pickVoice(model.speakers ?? [], model.options, lang, want);
  return voice ? { model: model.id, voice } : null;
}

/** 试听用的句子，按声音的语言。 */
export const PREVIEW_TEXT: Record<VoiceLang, string> = {
  zh: "你好，今天的新闻由我来为你朗读。",
  en: "Hello, I'll read today's news to you.",
};

let silence: string | null = null;

/**
 * 在点击的那一刻让这个播放器先放一下（一小段静音）。Safari 和桌面应用的网页视图只允许在用户点击时开始放声音，
 * 而本机声音要等核心念好才有：先这样「解锁」，之后换成真正的声音、一句接一句放都不用再点。
 */
export function unlockAudio(el: HTMLAudioElement): void {
  if (el.dataset.unlocked) return;
  el.dataset.unlocked = "1";
  if (!silence) {
    // 0.05 秒的 16 位单声道静音
    const samples = 1200;
    const bytes = new Uint8Array(44 + samples * 2);
    const view = new DataView(bytes.buffer);
    const text = (at: number, s: string) => [...s].forEach((c, i) => view.setUint8(at + i, c.charCodeAt(0)));
    text(0, "RIFF");
    view.setUint32(4, 36 + samples * 2, true);
    text(8, "WAVEfmt ");
    view.setUint32(16, 16, true);
    view.setUint16(20, 1, true);
    view.setUint16(22, 1, true);
    view.setUint32(24, 24000, true);
    view.setUint32(28, 48000, true);
    view.setUint16(32, 2, true);
    view.setUint16(34, 16, true);
    text(36, "data");
    view.setUint32(40, samples * 2, true);
    silence = URL.createObjectURL(new Blob([bytes], { type: "audio/wav" }));
  }
  const url = silence;
  el.src = url;
  el.play()
    .then(() => {
      // 真正的声音可能已经换上了，那就别停
      if (el.src === url) el.pause();
    })
    .catch((err: unknown) => {
      // 没解锁成功（不在点击里）：下次点击再试。换上真正的声音打断了它（AbortError）不算
      if (err instanceof DOMException && err.name === "NotAllowedError") delete el.dataset.unlocked;
    });
}

/** 播放器里放的是解锁用的静音：不用停它，停了反而可能让 Safari 当作没解锁。 */
export function isSilence(el: HTMLAudioElement): boolean {
  return silence !== null && el.src === silence;
}

/** Base64 的 WAV 变成能放的地址。用完要 URL.revokeObjectURL。 */
export function wavUrl(base64: string): string {
  const bin = atob(base64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return URL.createObjectURL(new Blob([bytes], { type: "audio/wav" }));
}
