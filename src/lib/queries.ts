// 数据查询与修改。所有修改都先在本地缓存里生效，再发给核心，界面不等待。

import {
  type InfiniteData,
  type QueryClient,
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useEffect, useMemo } from "react";
import {
  api,
  type Cursor,
  type Item,
  type ItemDetail,
  type ItemFilter,
  type ItemPage,
  type ItemPatch,
} from "./api";
import { isArchiveLike, useUi } from "./store";
import { useAiStreams } from "./ai";
import { retryBlocked } from "./listTranslate";
import { useModelDownloads } from "./translate";
import { startOfToday } from "./time";
import { clientId, subscribe } from "./transport";

export const keys = {
  status: ["status"] as const,
  nav: ["nav"] as const,
  items: (filter: ItemFilter) => ["items", filter] as const,
  item: (id: string, allowRemote = false) => ["item", id, allowRemote] as const,
  settings: ["settings"] as const,
  devices: ["devices"] as const,
};

export function useStatus() {
  return useQuery({ queryKey: keys.status, queryFn: api.status });
}

export function useNav() {
  return useQuery({ queryKey: keys.nav, queryFn: () => api.nav(startOfToday()) });
}

export function useItems(filter: ItemFilter) {
  const query = useInfiniteQuery({
    queryKey: keys.items(filter),
    queryFn: ({ pageParam }) => api.listItems(filter, pageParam),
    initialPageParam: undefined as Cursor | undefined,
    getNextPageParam: (last) => last.next ?? undefined,
    // 搜索时一边打字一边换结果：新结果到之前先留着上一次的，列表不闪成空白
    placeholderData: filter.query ? (previous) => previous : undefined,
  });
  const items = useMemo(() => query.data?.pages.flatMap((p) => p.items) ?? [], [query.data]);
  return { ...query, items };
}

export function useItem(id: string | null, allowRemote = false) {
  return useQuery({
    queryKey: keys.item(id ?? "", allowRemote),
    queryFn: () => api.getItem(id as string, allowRemote),
    enabled: id !== null,
    // 邮件正文可能要从服务器取，失败了不要自动重试太多次
    retry: 1,
  });
}

export function usePresets() {
  return useQuery({ queryKey: ["presets"], queryFn: () => api.presets().then((r) => r.presets) });
}

export function useSettings() {
  return useQuery({ queryKey: keys.settings, queryFn: api.settings });
}

export function useDevices() {
  return useQuery({ queryKey: keys.devices, queryFn: api.devices });
}

/** 条目被归档或取消归档后，是否应当从这个列表里消失。 */
function leavesList(filter: ItemFilter, patch: ItemPatch): boolean {
  if (patch.archived === undefined) return false;
  // 星标和搜索「全部」里，归档与否都还在列表里
  if (filter.view === "starred" || filter.view === "all") return false;
  return isArchiveLike(filter.view) ? !patch.archived : patch.archived;
}

/**
 * 把一次状态修改直接应用到缓存里。
 *
 * 注意已读状态变了并不把条目从「未读」列表里移走：
 * 读一篇就少一篇会让列表在眼前跳动，等下次切换视图或有新内容时再整理。
 */
function patchCaches(qc: QueryClient, ids: string[] | "all", filterKey: ItemFilter | null, patch: ItemPatch) {
  const matches = (item: Item) => ids === "all" || ids.includes(item.id);

  for (const [key, data] of qc.getQueriesData<InfiniteData<ItemPage>>({ queryKey: ["items"] })) {
    if (!data) continue;
    const filter = key[1] as ItemFilter;
    // 「全部标为已读」只作用于发起它的那个列表
    if (ids === "all" && JSON.stringify(filter) !== JSON.stringify(filterKey)) continue;
    const remove = leavesList(filter, patch);
    qc.setQueryData<InfiniteData<ItemPage>>(key, {
      ...data,
      pages: data.pages.map((page) => ({
        ...page,
        items: remove
          ? page.items.filter((i) => !matches(i))
          : page.items.map((i) => (matches(i) ? { ...i, ...patch } : i)),
      })),
    });
  }

  if (ids !== "all") {
    for (const id of ids) {
      qc.setQueriesData<ItemDetail>({ queryKey: ["item", id] }, (old) =>
        old ? { ...old, ...patch } : old,
      );
    }
  }
}

export function useSetItems() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ ids, patch }: { ids: string[]; patch: ItemPatch }) => api.setItems(ids, patch),
    onMutate: ({ ids, patch }) => patchCaches(qc, ids, null, patch),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.nav }),
    // 失败了就以核心里的真实状态为准
    onError: () => qc.invalidateQueries(),
  });
}

export function useMarkAllRead() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (filter: ItemFilter) => api.markAllRead(filter),
    onMutate: (filter) => patchCaches(qc, "all", filter, { read: true }),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.nav }),
    onError: () => qc.invalidateQueries(),
  });
}

/** 监听核心事件，让缓存跟上其他设备、其他窗口和后台抓取带来的变化。 */
export function useCoreEvents() {
  const qc = useQueryClient();
  const setRefreshing = useUi((s) => s.setRefreshing);

  useEffect(() => {
    return subscribe((event) => {
      switch (event.type) {
        case "refresh":
          setRefreshing(event.active);
          break;
        case "status":
          void qc.invalidateQueries({ queryKey: keys.status });
          break;
        case "ai":
          useAiStreams.getState().push(event);
          if (event.done) void qc.invalidateQueries({ queryKey: ["ai", "usage"] });
          break;
        case "model":
          useModelDownloads.getState().update(event.pair, {
            received: event.received,
            total: event.total,
            state: event.state,
            error: event.error,
          });
          if (event.state !== "downloading") void qc.invalidateQueries({ queryKey: ["translate"] });
          // 列表里因为缺模型没翻成的，模型装好以后重新翻
          if (event.state === "installed") retryBlocked();
          break;
        case "changed":
          // 自己发起的修改已经乐观更新过了
          if (event.source === clientId) break;
          if (event.scope.includes("feeds") || event.scope.includes("items")) {
            void qc.invalidateQueries({ queryKey: keys.nav });
          }
          if (event.scope.includes("items")) {
            void qc.invalidateQueries({ queryKey: ["items"] });
            void qc.invalidateQueries({ queryKey: ["item"] });
          }
          if (event.scope.includes("settings")) {
            void qc.invalidateQueries({ queryKey: keys.settings });
            void qc.invalidateQueries({ queryKey: ["ai", "providers"] });
          }
          if (event.scope.includes("devices")) {
            void qc.invalidateQueries({ queryKey: keys.devices });
            void qc.invalidateQueries({ queryKey: keys.status });
          }
          break;
      }
    });
  }, [qc, setRefreshing]);
}
