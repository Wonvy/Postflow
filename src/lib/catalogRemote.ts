// 内置订阅源目录的取用和在线更新。
//
// 应用自带一份 /catalog/catalog.json。在线更新开着时，每天最多一次让核心去 GitHub（经 jsDelivr）下载最新的
// 同一个文件，读得通、而且比手上的新，就存在这台设备上，之后都用它。下载失败、文件坏了都不影响，继续用手上的。
// 只下载这一个文件，不会把你的订阅或别的信息发出去。

import { useQuery } from "@tanstack/react-query";
import { useEffect } from "react";
import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import { api } from "./api";
import { type Catalog, newerCatalog, parseCatalog } from "./catalog";
import { useUi } from "./store";

const DAY = 86_400_000;
/** 检查失败后过一小时再试，别每次打开都去请求。 */
const RETRY = 3_600_000;

interface RemoteCatalog {
  /** 下载到的、比自带的新的目录。 */
  remote: Catalog | null;
  /** 上次检查的时间。 */
  checkedAt: number;
  /** 正在检查。不保存。 */
  checking: boolean;
}

const useRemote = create<RemoteCatalog>()(
  persist((): RemoteCatalog => ({ remote: null, checkedAt: 0, checking: false }), {
    name: "shibapost.catalog",
    // 存不下（隐私模式、空间满了）时只是下次重新下载，不出错
    storage: createJSONStorage(() => ({
      getItem: (k) => {
        try {
          return localStorage.getItem(k);
        } catch {
          return null;
        }
      },
      setItem: (k, v) => {
        try {
          localStorage.setItem(k, v);
        } catch {
          // 忽略
        }
      },
      removeItem: (k) => {
        try {
          localStorage.removeItem(k);
        } catch {
          // 忽略
        }
      },
    })),
    partialize: (s) => ({ remote: s.remote, checkedAt: s.checkedAt }),
  }),
);

/** 应用自带的目录。 */
function useBundled() {
  return useQuery({
    queryKey: ["catalog", "bundled"],
    queryFn: async () => {
      const res = await fetch("/catalog/catalog.json");
      return parseCatalog(await res.text());
    },
    staleTime: Number.POSITIVE_INFINITY,
    retry: 1,
  });
}

export type CheckResult = "updated" | "current" | "failed";

/** 现在就去检查在线目录。`bundled` 是自带的，用来判断下载到的是不是更新。 */
export async function checkCatalog(bundled: Catalog | null): Promise<CheckResult> {
  const state = useRemote.getState();
  if (state.checking) return "current";
  useRemote.setState({ checking: true });
  try {
    const { text } = await api.catalogFetch();
    const found = parseCatalog(text);
    if (!found) throw new Error("unreadable catalog");
    const current = state.remote && bundled ? newerCatalog(bundled, state.remote) : (state.remote ?? bundled);
    const newer = !current || found.updated > current.updated;
    // 比自带的新才存；不比自带的新时清掉旧的下载（应用更新后自带的可能更新了）
    useRemote.setState({
      remote: bundled && found.updated <= bundled.updated ? null : found,
      checkedAt: Date.now(),
    });
    return newer ? "updated" : "current";
  } catch {
    useRemote.setState({ checkedAt: Date.now() - DAY + RETRY });
    return "failed";
  } finally {
    useRemote.setState({ checking: false });
  }
}

/**
 * 现在用的目录：自带的和下载到的里面新的那份。在线更新开着、离上次检查超过一天时，顺便在后台检查一次。
 * `feeds` 在自带的读出来之前是空的。
 */
export function useCatalog() {
  const bundled = useBundled();
  const online = useUi((s) => s.catalogOnline);
  const remote = useRemote((s) => s.remote);
  const checkedAt = useRemote((s) => s.checkedAt);
  const checking = useRemote((s) => s.checking);
  const local = bundled.data ?? null;

  useEffect(() => {
    if (!online || bundled.isPending) return;
    if (Date.now() - checkedAt < DAY) return;
    void checkCatalog(local);
  }, [online, bundled.isPending, checkedAt, local]);

  const catalog = local ? newerCatalog(local, online ? remote : null) : online ? remote : null;
  return {
    feeds: catalog?.feeds ?? [],
    updated: catalog?.updated ?? null,
    /** 用的是下载到的那份。 */
    fromOnline: !!catalog && catalog === remote,
    loading: bundled.isPending && !catalog,
    checking,
    checkedAt,
    bundled: local,
  };
}
