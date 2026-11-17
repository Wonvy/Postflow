// 导入的界面语言，存在这台设备的浏览器存储里（和主题、语言选择一样不同步）。
// 文件格式和检查规则见 langPack.ts。

import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import type { LangPack } from "./langPack";

interface LangPacksState {
  packs: LangPack[];
  /** 同代码的会被替换：导入新版本的文件就是更新。 */
  add: (pack: LangPack) => void;
  remove: (code: string) => void;
}

export const useLangPacks = create<LangPacksState>()(
  persist(
    (set) => ({
      packs: [],
      add: (pack) => set((s) => ({ packs: [...s.packs.filter((p) => p.code !== pack.code), pack] })),
      remove: (code) => set((s) => ({ packs: s.packs.filter((p) => p.code !== code) })),
    }),
    { name: "postflow.languages", storage: createJSONStorage(() => localStorage) },
  ),
);

export function findPack(code: string | null): LangPack | null {
  if (!code) return null;
  return useLangPacks.getState().packs.find((p) => p.code === code) ?? null;
}
