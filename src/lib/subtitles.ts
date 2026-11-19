// 播客字幕：声音放到哪儿，字幕跟到哪儿。
//
// - 节目自己带了有时间的文字稿（Podcasting 2.0）就直接用。
// - 没有的，由核心在本机转写（core/src/subtitles.rs）：从现在放到的地方往后转，识别比播放快，
//   等一小会儿就跟上了。拖到还没转的地方，告诉核心改从那里转。
// - 开没开字幕记在这台设备上。识别模型装在核心那边（设置 → 字幕），网页访问时用的是桌面那台的模型。

import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { create } from "zustand";
import { persist } from "zustand/middleware";
import { api, type SubtitleSnapshot } from "./api";
import { localUrl, useDownloads } from "./episodes";
import { savedPosition, switchToLocal, usePlayer } from "./player";
import { useUi } from "./store";
import { type Cue, isCovered, mergeCues } from "./subtitleCues";
import { parseTranscript } from "./transcript";
import { type CoreEvent, RpcError } from "./transport";

type SubtitlesEvent = Extract<CoreEvent, { type: "subtitles" }>;

/** 字幕的样子。界面上的名字和效果见 SubtitleStyles.tsx。 */
export const SUBTITLE_STYLES = [
  "karaoke",
  "typewriter",
  "pop",
  "glass",
  "bilingual",
  "emphasis",
  "scroll",
  "particles",
  "speaker",
  "wave",
  "minimal",
] as const;
export type SubtitleStyle = (typeof SUBTITLE_STYLES)[number];
type SpeechModelEvent = Extract<CoreEvent, { type: "speech_model" }>;

/** 正在放的这一期的字幕。 */
export interface SubtitleEntry {
  id: string;
  /** 节目自带的文字稿，还是本机转写的。 */
  source: "feed" | "local";
  state: SubtitleSnapshot["state"];
  cues: Cue[];
  covered: [number, number][];
  duration: number | null;
  complete: boolean;
  /** 本机转写用的模型。 */
  model: string;
  /** 说话人的名字（节目自带的文字稿里写了的），按 `Cue.speaker` 的编号排。本机分出来的没有名字。 */
  speakerNames?: string[];
  /** 等这一期下载到本机时的进度。 */
  received: number;
  total: number | null;
  error: string | null;
  message: string | null;
}

interface ModelDownload {
  received: number;
  total: number;
  state: SpeechModelEvent["state"];
  error?: string;
}

interface SubtitleStore {
  /** 开着字幕。记在这台设备上。 */
  on: boolean;
  setOn: (on: boolean) => void;
  /** 字幕和界面语言（或者设置里选的翻译语言）不一样时，自动翻译，原文下面显示译文。 */
  translate: boolean;
  setTranslate: (on: boolean) => void;
  /** 字幕的样子，见 SubtitleStyles.tsx。 */
  style: SubtitleStyle;
  setStyle: (style: SubtitleStyle) => void;
  /** 分出了说话人时，标出是谁在说、每人一种颜色。 */
  speakers: boolean;
  setSpeakers: (on: boolean) => void;
  entry: SubtitleEntry | null;
  /** 开了字幕，但核心那边一个识别模型都没装。 */
  needsModel: boolean;
  /** 识别模型的下载进度。 */
  downloads: Record<string, ModelDownload>;
}

export const useSubtitles = create<SubtitleStore>()(
  persist(
    (set) => ({
      on: false,
      setOn: (on) => set({ on }),
      translate: false,
      setTranslate: (translate) => set({ translate }),
      style: "karaoke",
      setStyle: (style) => set({ style }),
      speakers: true,
      setSpeakers: (speakers) => set({ speakers }),
      entry: null,
      needsModel: false,
      downloads: {},
    }),
    {
      name: "postflow.subtitles",
      partialize: (s) => ({ on: s.on, translate: s.translate, style: s.style, speakers: s.speakers }),
    },
  ),
);

function fromSnapshot(snap: SubtitleSnapshot): SubtitleEntry {
  return {
    id: snap.id,
    source: "local",
    state: snap.state,
    cues: snap.cues,
    covered: snap.covered,
    duration: snap.duration,
    complete: snap.complete,
    model: snap.model,
    received: snap.received,
    total: snap.total,
    error: snap.error,
    message: snap.message,
  };
}

function applyEvent(entry: SubtitleEntry, event: SubtitlesEvent): SubtitleEntry {
  return {
    ...entry,
    state: event.state,
    cues: mergeCues(entry.cues, event.from, event.to, event.cues),
    covered: event.covered,
    duration: event.duration,
    complete: event.complete,
    model: event.model || entry.model,
    received: event.received,
    total: event.total,
    error: event.error ?? null,
    message: event.message ?? null,
  };
}

// 向核心要完整状态的那一会儿收到的事件。回答是在它们之前拍的，回答到了要把它们再并一遍。
let pending: SubtitlesEvent[] | null = null;
// 现在要的是哪一期的字幕：换了一期以后，上一期迟到的回答不要
let wanted: string | null = null;

/** 核心的事件：字幕有了进展。 */
export function onSubtitlesEvent(event: SubtitlesEvent): void {
  if (event.id !== wanted) return;
  pending?.push(event);
  const entry = useSubtitles.getState().entry;
  if (entry?.id === event.id && entry.source === "local") {
    useSubtitles.setState({ entry: applyEvent(entry, event) });
  }
}

/** 核心的事件：识别模型的下载进度。装好以后，等着模型的字幕接着开始。 */
export function onSpeechModelEvent(event: SpeechModelEvent): void {
  useSubtitles.setState((s) => ({
    downloads: {
      ...s.downloads,
      [event.id]: { received: event.received, total: event.total, state: event.state, error: event.error },
    },
  }));
  if (event.state === "installed" && useSubtitles.getState().needsModel && wanted) void startLocal(wanted);
}

/** 让核心开始（或者接着）转写这一期。 */
export async function startLocal(id: string): Promise<void> {
  const player = usePlayer.getState();
  const position = player.current?.id === id ? player.position : savedPosition(id);
  const mine: SubtitlesEvent[] = [];
  pending = mine;
  try {
    const snap = await api.subtitlesStart(id, position, useUi.getState().episodesKeep);
    if (wanted !== id) return;
    let entry = fromSnapshot(snap);
    for (const event of mine) entry = applyEvent(entry, event);
    useSubtitles.setState({ entry, needsModel: false });
  } catch (err) {
    if (wanted !== id) return;
    if (err instanceof RpcError && err.code === "speech_not_installed") {
      useSubtitles.setState({ entry: null, needsModel: true });
      return;
    }
    useSubtitles.setState({
      needsModel: false,
      entry: {
        id,
        source: "local",
        state: "failed",
        cues: [],
        covered: [],
        duration: null,
        complete: false,
        model: "",
        received: 0,
        total: null,
        error: err instanceof RpcError ? err.code : "unknown",
        message: err instanceof Error ? err.message : String(err),
      },
    });
  } finally {
    if (pending === mine) pending = null;
  }
}

/** 删掉这一期本机转写的字幕，开着字幕的话重新转（换了模型以后用）。 */
export async function redoLocal(id: string): Promise<void> {
  await api.subtitlesRemove(id);
  const s = useSubtitles.getState();
  if (s.entry?.id === id && s.entry.source === "local") useSubtitles.setState({ entry: null });
  if (s.on && wanted === id) await startLocal(id);
}

/** 拖动超过这么多秒算跳转（正常播放时位置是一点点往前走的）。 */
const JUMP = 3;

/**
 * 把播放器和字幕接起来：开着字幕时，放哪一期就准备哪一期的字幕；关了字幕、关了播放器就让核心停下
 * （转了多少存多少，下次接着来）。整个应用里放一个。
 */
export function SubtitlesBridge() {
  const qc = useQueryClient();
  const on = useSubtitles((s) => s.on);
  const id = usePlayer((s) => s.current?.id ?? null);
  const transcript = usePlayer((s) => s.current?.media.transcript);
  const url = transcript?.url;
  const mime = transcript?.mime ?? "";

  useEffect(() => {
    if (!on || !id) {
      wanted = null;
      useSubtitles.setState({ entry: null, needsModel: false });
      return;
    }
    wanted = id;
    useSubtitles.setState({ entry: null, needsModel: false });
    let cancelled = false;
    if (url) {
      // 节目自带的文字稿有时间就用它；没有时间（纯文本）对不上声音，还是本机转
      api
        .mediaText(url)
        .then((r) => parseTranscript(r.text, mime))
        .then((parsed) => {
          if (cancelled) return;
          const timed = parsed.filter((c): c is typeof c & { start: number } => c.start !== null);
          if (timed.length < parsed.length / 2 || timed.length === 0) {
            void startLocal(id);
            return;
          }
          // 文字稿里写了谁在说：每个名字一个编号，和本机分出来的说话人一样上色
          const speakerNames: string[] = [];
          const cues: Cue[] = timed.map((c, i) => {
            let speaker: number | undefined;
            if (c.speaker) {
              speaker = speakerNames.indexOf(c.speaker);
              if (speaker < 0) speaker = speakerNames.push(c.speaker) - 1;
            }
            return { start: c.start, end: timed[i + 1]?.start ?? c.start + 8, text: c.text, speaker };
          });
          useSubtitles.setState({
            entry: {
              id,
              source: "feed",
              state: "idle",
              cues,
              covered: [],
              duration: null,
              complete: true,
              model: "",
              speakerNames,
              received: 0,
              total: null,
              error: null,
              message: null,
            },
          });
        })
        .catch(() => {
          if (!cancelled) void startLocal(id);
        });
    } else {
      void startLocal(id);
    }
    return () => {
      cancelled = true;
      // 核心那边存下已经转的部分；这一期「有没有本机字幕」的查询也该更新了
      void api
        .subtitlesStop(id)
        .catch(() => {})
        .finally(() => void qc.invalidateQueries({ queryKey: ["subtitles", id] }));
    };
  }, [on, id, url, mime, qc]);

  // 转写会先把这一期下载到本机。下载的进展让「下载离线听」那个按钮也知道；
  // 下好以后换成放本机文件：字幕是照着这份文件转的，和它才对得准（见 switchToLocal）。
  const state = useSubtitles((s) => (s.entry?.id === id && s.entry.source === "local" ? s.entry.state : null));
  useEffect(() => {
    if (state) void qc.invalidateQueries({ queryKey: ["episodes"] });
  }, [state, qc]);
  const { downloaded } = useDownloads();
  const file = id ? downloaded.find((d) => d.id === id)?.file : undefined;
  useEffect(() => {
    if (on && id && file && state) switchToLocal(id, localUrl(id, file));
  }, [on, id, file, state]);

  // 拖到了还没转的地方：告诉核心改从那里转
  useEffect(() => {
    if (!on || !id) return;
    let last = usePlayer.getState().position;
    return usePlayer.subscribe((s) => {
      if (s.current?.id !== id) return;
      const jumped = Math.abs(s.position - last) > JUMP;
      last = s.position;
      if (!jumped) return;
      const entry = useSubtitles.getState().entry;
      if (!entry || entry.id !== id || entry.source !== "local" || entry.complete) return;
      if (isCovered(entry.covered, s.position)) return;
      if (entry.state === "failed") return;
      void startLocal(id);
    });
  }, [on, id]);

  return null;
}
