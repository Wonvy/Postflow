// AI 请求的流式结果。核心通过事件把每一段文字推过来，这里按请求 ID 攒起来。

import { create } from "zustand";
import type { CoreEvent } from "./transport";

export interface AiStream {
  text: string;
  done: boolean;
  result?: Record<string, unknown>;
  error?: string;
  message?: string;
}

type AiEvent = Extract<CoreEvent, { type: "ai" }>;

export const useAiStreams = create<{
  byId: Record<string, AiStream>;
  push: (event: AiEvent) => void;
  forget: (id: string) => void;
}>()((set) => ({
  byId: {},
  push: (e) =>
    set((s) => {
      const current = s.byId[e.request_id] ?? { text: "", done: false };
      return {
        byId: {
          ...s.byId,
          [e.request_id]: {
            text: current.text + (e.delta ?? ""),
            done: current.done || e.done,
            result: e.result ?? current.result,
            error: e.error ?? current.error,
            message: e.message ?? current.message,
          },
        },
      };
    }),
  forget: (id) =>
    set((s) => {
      const { [id]: _, ...rest } = s.byId;
      return { byId: rest };
    }),
}));

/** 一个请求的进展。还没收到任何东西时是空的。 */
export function useAiStream(id: string | null): AiStream | undefined {
  return useAiStreams((s) => (id ? s.byId[id] : undefined));
}

// ---- 列表里的一句话摘要 ----

/** 按「语言:条目」记着已经拿到的摘要。 */
export const useAbstracts = create<{ byKey: Record<string, string> }>()(() => ({ byKey: {} }));

/** 这次打开应用期间请求过的条目，不论成败都不再重复请求。 */
const asked = new Set<string>();

/** 为这些条目要摘要。已有的、请求过的跳过；一次最多 24 条。 */
export function requestAbstracts(ids: string[], lang: string) {
  const want = ids.filter((id) => !asked.has(`${lang}:${id}`)).slice(0, 24);
  if (want.length === 0) return;
  for (const id of want) asked.add(`${lang}:${id}`);
  void import("./api").then(({ api }) =>
    api
      .aiAbstracts(want, lang)
      .then(({ abstracts }) => {
        const add = Object.fromEntries(Object.entries(abstracts).map(([id, text]) => [`${lang}:${id}`, text]));
        useAbstracts.setState((s) => ({ byKey: { ...s.byKey, ...add } }));
      })
      .catch(() => {
        // 没配置服务、网络不通：列表照常显示原文开头
      }),
  );
}
