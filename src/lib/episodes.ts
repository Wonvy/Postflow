// 下载播客单集离线听。下载在核心里进行（core/src/episodes.rs），这里看进度、给播放器本机文件的地址。
// 文件只在这台设备上：网页端从 /api/episodes/<ID> 取，桌面应用直接读本机目录（Tauri 的 asset 协议）。

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect } from "react";
import { api, type EpisodeList } from "./api";
import { setLocalFiles } from "./player";
import { useUi } from "./store";
import { isTauri } from "./transport";

const KEY = ["episodes"] as const;

/** 本机文件给 <audio> 用的地址。 */
function localUrl(id: string, file: string): string {
  if (isTauri) {
    const internals = (window as unknown as { __TAURI_INTERNALS__?: { convertFileSrc?: (p: string, s: string) => string } })
      .__TAURI_INTERNALS__;
    const convert = internals?.convertFileSrc;
    if (convert) return convert(file, "asset");
  }
  return `/api/episodes/${encodeURIComponent(id)}`;
}

export function useDownloads() {
  const query = useQuery({
    queryKey: KEY,
    queryFn: api.episodesList,
    // 有正在下的就每秒看一次进度
    refetchInterval: (q) => {
      const data = q.state.data as EpisodeList | undefined;
      const busy = Object.values(data?.downloading ?? {}).some((p) => !p.error);
      return busy ? 1000 : false;
    },
    staleTime: 30_000,
  });
  return query.data ?? { downloaded: [], downloading: {} };
}

export function useDownloadActions() {
  const qc = useQueryClient();
  const keep = useUi((s) => s.episodesKeep);
  const download = useCallback(
    async (id: string) => {
      await api.downloadEpisode(id, keep);
      void qc.invalidateQueries({ queryKey: KEY });
    },
    [keep, qc],
  );
  const remove = useCallback(
    async (id: string) => {
      await api.removeEpisode(id);
      void qc.invalidateQueries({ queryKey: KEY });
    },
    [qc],
  );
  return { download, remove };
}

/** 把下好的单集告诉播放器：放这一期时用本机文件，不再联网。 */
export function DownloadsBridge() {
  const { downloaded } = useDownloads();
  useEffect(() => {
    setLocalFiles(Object.fromEntries(downloaded.map((d) => [d.id, localUrl(d.id, d.file)])));
  }, [downloaded]);
  return null;
}
