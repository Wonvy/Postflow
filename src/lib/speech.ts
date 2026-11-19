// 朗读文章：用系统自带的语音（浏览器的 speechSynthesis），不用下载模型。
// 正文切成一句一句排队念（见 speechText.ts），可以暂停、上一句、下一句、调语速。
// 换一篇文章也接着念，列表底下有一条小控制条（SpeechBar）。Linux 上的系统网页视图多半没有语音合成，
// 这时不显示朗读按钮。

import { create } from "zustand";
import { chunks, guessLang } from "./speechText";

export const SPEECH_RATES = [0.8, 1, 1.2, 1.5, 1.8, 2] as const;

const LANG_TAGS: Record<ReturnType<typeof guessLang>, string> = {
  zh: "zh-CN",
  ja: "ja-JP",
  ko: "ko-KR",
  en: "en-US",
};

export function speechSupported(): boolean {
  return typeof window !== "undefined" && "speechSynthesis" in window && typeof SpeechSynthesisUtterance !== "undefined";
}

interface Speech {
  /** 正在念的是哪一篇。 */
  itemId: string | null;
  title: string;
  parts: string[];
  index: number;
  playing: boolean;
  lang: string;
  rate: number;
  start: (itemId: string, title: string, text: string) => void;
  toggle: () => void;
  jump: (delta: number) => void;
  stop: () => void;
  setRate: (rate: number) => void;
}

function savedRate(): number {
  try {
    const r = Number(localStorage.getItem("postflow.speechRate"));
    return SPEECH_RATES.includes(r as (typeof SPEECH_RATES)[number]) ? r : 1;
  } catch {
    return 1;
  }
}

/** 这种语言里最好的声音：本机的优先（不用联网），名字里带 Premium / Enhanced 的更自然。 */
function voiceFor(lang: string): SpeechSynthesisVoice | undefined {
  const voices = window.speechSynthesis.getVoices().filter((v) => v.lang.replace("_", "-").startsWith(lang.slice(0, 2)));
  const exact = voices.filter((v) => v.lang.replace("_", "-") === lang);
  const pool = exact.length > 0 ? exact : voices;
  return (
    pool.find((v) => /premium|enhanced|natural/i.test(v.name) && v.localService) ??
    pool.find((v) => v.localService) ??
    pool[0]
  );
}

/** 念第 `index` 段，念完自动接下一段。`generation` 防止取消掉的那一段念完时又往下走。 */
let generation = 0;

function speakFrom(index: number) {
  const s = useSpeech.getState();
  const synth = window.speechSynthesis;
  generation++;
  const mine = generation;
  synth.cancel();
  const text = s.parts[index];
  if (text === undefined) {
    useSpeech.setState({ playing: false, index: 0, itemId: null, parts: [], title: "" });
    return;
  }
  const u = new SpeechSynthesisUtterance(text);
  u.lang = s.lang;
  u.rate = s.rate;
  const voice = voiceFor(s.lang);
  if (voice) u.voice = voice;
  u.onend = () => {
    if (mine !== generation) return;
    speakFrom(index + 1);
  };
  u.onerror = (e) => {
    if (mine !== generation || e.error === "interrupted" || e.error === "canceled") return;
    useSpeech.setState({ playing: false });
  };
  useSpeech.setState({ index, playing: true });
  synth.speak(u);
}

export const useSpeech = create<Speech>()((set, get) => ({
  itemId: null,
  title: "",
  parts: [],
  index: 0,
  playing: false,
  lang: "en-US",
  rate: savedRate(),
  start: (itemId, title, text) => {
    const parts = chunks(`${title}。\n${text}`);
    if (parts.length === 0) return;
    set({ itemId, title, parts, index: 0, lang: LANG_TAGS[guessLang(text)] });
    speakFrom(0);
  },
  toggle: () => {
    const s = get();
    if (!s.itemId) return;
    if (s.playing) {
      generation++;
      window.speechSynthesis.cancel();
      set({ playing: false });
    } else {
      // 从这一段开头接着念（有的引擎暂停后接不上，干脆重念这一段）
      speakFrom(s.index);
    }
  },
  jump: (delta) => {
    const s = get();
    if (!s.itemId) return;
    const next = Math.max(0, Math.min(s.parts.length - 1, s.index + delta));
    if (s.playing) speakFrom(next);
    else set({ index: next });
  },
  stop: () => {
    generation++;
    if (speechSupported()) window.speechSynthesis.cancel();
    set({ itemId: null, title: "", parts: [], index: 0, playing: false });
  },
  setRate: (rate) => {
    try {
      localStorage.setItem("postflow.speechRate", String(rate));
    } catch {
      // 存不了就只管这一次
    }
    set({ rate });
    const s = get();
    if (s.playing) speakFrom(s.index);
  },
}));
