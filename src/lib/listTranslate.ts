// 翻译模式下列表里的译文。看得到的条目停下来一会儿后成批去要，要过的不再要。

import { create } from "zustand";
import { api, type MissingModel } from "./api";

export interface ListTranslation {
  from: string;
  title: string;
  summary: string;
}

export const useListTranslations = create<{
  /** 按「语言:条目」记着译文。 */
  byKey: Record<string, ListTranslation>;
  /** 离线翻译还缺的模型，列表上方提示下载。 */
  missing: MissingModel[];
}>()(() => ({ byKey: {}, missing: [] }));

/** 请求过的「方式:语言:条目」。缺模型没翻成的另外记着，模型下好以后再要。 */
const asked = new Set<string>();
const blocked = new Set<string>();
let activeContext: string | null = null;
let contextGeneration = 0;

/** 列表范围、翻译方式或目标语言变了，旧范围缺的模型不该继续挡在当前列表上。 */
export function setListTranslationContext(context: string | null) {
  if (activeContext === context) return;
  activeContext = context;
  contextGeneration++;
  asked.clear();
  blocked.clear();
  useListTranslations.setState({ missing: [] });
}

export function requestListTranslations(ids: string[], to: string, engine: "offline" | "ai", context: string) {
  setListTranslationContext(context);
  const generation = contextGeneration;
  const key = (id: string) => `${engine}:${to}:${id}`;
  const want = ids.filter((id) => !asked.has(key(id))).slice(0, 40);
  if (want.length === 0) return;
  for (const id of want) asked.add(key(id));
  api
    .translateItems(want, to, engine)
    .then(({ items, same, missing }) => {
      const add = Object.fromEntries(Object.entries(items).map(([id, t]) => [`${to}:${id}`, t]));
      useListTranslations.setState((s) => {
        const current = activeContext === context && contextGeneration === generation;
        const pairs = new Map<string, MissingModel>();
        if (current) {
          for (const model of s.missing) pairs.set(model.pair, model);
          for (const model of missing) pairs.set(model.pair, model);
        }
        return {
          byKey: { ...s.byKey, ...add },
          missing: current ? [...pairs.values()] : s.missing,
        };
      });
      // 没翻成、也不是同一种语言的：多半是缺模型，记下来等模型装好再试
      if (activeContext === context && contextGeneration === generation) {
        for (const id of want) {
          if (!items[id] && !same.includes(id)) blocked.add(key(id));
        }
      }
    })
    .catch(() => {
      if (activeContext === context && contextGeneration === generation) {
        for (const id of want) asked.delete(key(id));
      }
    });
}

/** 模型下好了：之前因为缺模型没翻成的重新要。 */
export function retryBlocked() {
  for (const k of blocked) asked.delete(k);
  blocked.clear();
  useListTranslations.setState({ missing: [] });
}
