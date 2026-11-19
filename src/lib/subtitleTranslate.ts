// 字幕自动翻译：字幕和要看的语言（设置里选的翻译语言，没选就是界面语言）不一样时，原文下面显示译文。
//
// - 用设置里选的翻译方式：离线模型（免费，缺模型时先问要不要下载）或者 AI 服务。
// - 只翻播放位置附近的句子，听到哪儿翻到哪儿。
// - 译文存在本机（核心的 subtitles.translations），再听、重开都不用再翻：用 AI 翻译时不会重复花钱。
// - 整个应用只有一处在翻（SubtitleTranslationBridge），播放条、迷你播放器、文字稿都读它的结果。

import { useCallback, useEffect, useRef, useState } from "react";
import { create } from "zustand";
import { api, type TranslatePlan } from "./api";
import { usePlayer } from "./player";
import { cueKey, type Cue } from "./subtitleCues";
import { useSubtitles } from "./subtitles";
import { type TranslateEngine, useModelDownloads, useTranslateEngine, useTranslateTarget } from "./translate";

/** 往前、往后各翻多远（秒）。 */
const BEHIND = 10;
const AHEAD = 90;
/** 一次最多翻几句。 */
const BATCH = 24;
/** 用来认语言的样本取多少句。 */
const SAMPLE = 30;

export type SubtitleTranslation =
  | { status: "off" }
  /** 在认是什么语言、看要不要模型。 */
  | { status: "checking" }
  /** 本来就是要看的语言。 */
  | { status: "same" }
  /** 缺离线翻译模型，等用户同意下载。 */
  | { status: "need"; plan: TranslatePlan; download: () => void }
  | { status: "downloading"; plan: TranslatePlan }
  | { status: "on"; from: string; to: string; entries: Record<string, string>; working: boolean }
  | { status: "error"; error: unknown; retry: () => void };

const OFF: SubtitleTranslation = { status: "off" };

function sampleText(cues: readonly Cue[]): string {
  return cues
    .slice(0, SAMPLE)
    .map((c) => c.text)
    .join("\n");
}

/** 翻一批句子。 */
async function translateBatch(
  id: string,
  from: string,
  to: string,
  engine: TranslateEngine,
  cues: readonly Cue[],
): Promise<Record<string, string>> {
  const input = cues.map((c) => ({ text: c.text, marks: [] as [number, number][] }));
  const { segments } =
    engine === "ai" ? await api.aiTranslate(id, from, to, input) : await api.translateText(from, to, input);
  const out: Record<string, string> = {};
  cues.forEach((c, i) => {
    const text = segments[i]?.text.trim();
    if (text) out[cueKey(c)] = text;
  });
  return out;
}

/**
 * 正在放的这一期字幕的译文。`cues` 是全部字幕（按时间排好），`position` 是播放到第几秒。
 * 关着自动翻译、或者没有字幕时是 off。
 */
export function useSubtitleTranslation(id: string | null, cues: readonly Cue[], position: number): SubtitleTranslation {
  const on = useSubtitles((s) => s.translate);
  const to = useTranslateTarget();
  const engine = useTranslateEngine();
  const [state, setState] = useState<SubtitleTranslation>(OFF);
  // 认出来的原文语言、要不要模型，每期只看一次
  const [from, setFrom] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const entries = useRef<Record<string, string>>({});
  const busy = useRef(false);
  const failed = useRef(new Set<string>());
  const enough = cues.length >= 3;

  // 再试一次：之前没翻成的句子也重新翻
  const retry = useCallback(() => {
    failed.current = new Set();
    setAttempt((n) => n + 1);
  }, []);

  // 换了一期、换了语言：重新来，先读存下来的译文
  useEffect(() => {
    entries.current = {};
    failed.current = new Set();
    setFrom(null);
    if (!on || !id) {
      setState(OFF);
      return;
    }
    let cancelled = false;
    void api
      .subtitleTranslations(id, to)
      .then((r) => {
        if (!cancelled) entries.current = { ...r.entries, ...entries.current };
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [on, id, to]);

  // 字幕够几句了：认语言，离线翻译时看缺不缺模型
  useEffect(() => {
    if (!on || !id || !enough || from !== null) return;
    let cancelled = false;
    setState({ status: "checking" });
    const sample = sampleText(cues);
    (async () => {
      const plan = await api.translatePlan(sample, to);
      if (cancelled) return;
      if (!plan.from) throw new Error("unknown language");
      if (plan.same) {
        setState({ status: "same" });
        return;
      }
      if (engine === "offline" && plan.missing.length > 0) {
        const download = () => {
          const { update } = useModelDownloads.getState();
          for (const m of plan.missing) update(m.pair, { received: 0, total: 0, state: "downloading" });
          setState({ status: "downloading", plan });
          api.downloadModels(plan.missing.map((m) => m.pair)).catch((error) => setState({ status: "error", error, retry }));
        };
        setState({ status: "need", plan, download });
        return;
      }
      setFrom(plan.from);
    })().catch((error) => {
      if (!cancelled) setState({ status: "error", error, retry });
    });
    return () => {
      cancelled = true;
    };
    // cues 只在「够不够几句」变的时候要；之后的新句子不用重新认语言
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [on, id, enough, from, to, engine, attempt, retry]);

  // 模型下好了，接着翻
  const downloads = useModelDownloads((s) => s.byPair);
  useEffect(() => {
    if (state.status !== "downloading") return;
    const states = state.plan.missing.map((m) => downloads[m.pair]);
    const bad = states.find((s) => s?.state === "failed");
    if (bad) setState({ status: "error", error: { code: bad.error ?? "translate_download_failed" }, retry });
    else if (states.every((s) => s?.state === "installed") && state.plan.from) setFrom(state.plan.from);
  }, [downloads, state, retry]);

  // 翻播放位置附近还没翻的句子，一次一批
  const tick = Math.floor(position / 5);
  useEffect(() => {
    if (!on || !id || !from) return;
    const near = cues.filter(
      (c) =>
        c.start >= position - BEHIND &&
        c.start <= position + AHEAD &&
        !entries.current[cueKey(c)] &&
        !failed.current.has(cueKey(c)),
    );
    const show = (working: boolean) =>
      setState({ status: "on", from, to, entries: { ...entries.current }, working });
    if (near.length === 0 || busy.current) {
      show(busy.current);
      return;
    }
    busy.current = true;
    show(true);
    const batch = near.slice(0, BATCH);
    void translateBatch(id, from, to, engine, batch)
      .then((done) => {
        for (const c of batch) if (!done[cueKey(c)]) failed.current.add(cueKey(c));
        entries.current = { ...entries.current, ...done };
        if (Object.keys(done).length > 0) void api.saveSubtitleTranslations(id, to, done).catch(() => {});
        busy.current = false;
        if (useSubtitles.getState().translate) show(false);
      })
      .catch((error) => {
        // 这一批先不翻了，等用户点「再试一次」：AI 服务出错时不反复重试花钱
        for (const c of batch) failed.current.add(cueKey(c));
        busy.current = false;
        setState({ status: "error", error, retry });
      });
    // 位置每 5 秒看一次就够了；新转出来的句子（cues 变了）也要看
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [on, id, from, to, engine, tick, cues, attempt]);

  return on ? state : OFF;
}

const NO_CUES: Cue[] = [];

/** 正在放的这一期字幕的翻译情况。由 SubtitleTranslationBridge 更新。 */
export const useSubtitleTranslationState = create<{ id: string | null; value: SubtitleTranslation }>()(() => ({
  id: null,
  value: OFF,
}));

/** 这一期（`id`）这一句的译文；没有就是 undefined。 */
export function translationOf(value: SubtitleTranslation, cue: Pick<Cue, "start" | "text">): string | undefined {
  return value.status === "on" ? value.entries[cueKey(cue)] : undefined;
}

/** 某一期的翻译情况：不是正在放的那一期时是 off。 */
export function useTranslationFor(id: string | null): SubtitleTranslation {
  return useSubtitleTranslationState((s) => (id && s.id === id ? s.value : OFF));
}

/** 把正在放的这一期的字幕翻起来。整个应用里放一个。 */
export function SubtitleTranslationBridge() {
  const entry = useSubtitles((s) => s.entry);
  const id = entry?.id ?? null;
  const position = usePlayer((s) => (id && s.current?.id === id ? s.position : 0));
  const value = useSubtitleTranslation(id, entry?.cues ?? NO_CUES, position);
  useEffect(() => {
    useSubtitleTranslationState.setState({ id, value });
  }, [id, value]);
  return null;
}
