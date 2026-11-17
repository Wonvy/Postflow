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
