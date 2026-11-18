// 网页访问（桌面应用在本机端口上提供界面）的状态和设置。服务在 src-tauri/src/web_access.rs。
// 设置页和侧栏底部的手机图标共用这里的查询，改了一处另一处跟着变。只有桌面应用里有。

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { isTauri } from "./transport";

/** 一块网卡和它的地址。 */
export interface LanAddr {
  name: string;
  ip: string;
}

export interface WebAccessStatus {
  enabled: boolean;
  running: boolean;
  port: number;
  lan: boolean;
  local_url: string | null;
  lan_url: string | null;
  /** 这台电脑的各块网卡，最像局域网的在前面。 */
  lan_addresses: LanAddr[];
  /** 链接和二维码用的是哪块网卡。 */
  lan_interface: string | null;
  error: string | null;
}

export type WebAccessUpdate = {
  enabled?: boolean;
  port?: number;
  lan?: boolean;
  lan_interface?: string;
  new_key?: boolean;
};

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

const KEY = ["webAccess"] as const;

export function useWebAccess() {
  return useQuery({
    queryKey: KEY,
    queryFn: () => invoke<WebAccessStatus>("web_access_status"),
    enabled: isTauri,
    // 网卡和地址会变（换了 Wi-Fi、开关 VPN），隔一会儿重新看
    staleTime: 15_000,
    refetchInterval: 60_000,
  });
}

/** 改设置，返回新的状态并更新共用的查询。 */
export function useUpdateWebAccess() {
  const qc = useQueryClient();
  return async (change: WebAccessUpdate): Promise<WebAccessStatus> => {
    const status = await invoke<WebAccessStatus>("web_access_update", { update: change });
    qc.setQueryData(KEY, status);
    return status;
  };
}
