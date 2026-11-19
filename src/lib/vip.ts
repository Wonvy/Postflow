// 重要联系人：按发件人地址记在同步设置 vip_senders 里。发件人卡片里加、减，
// 设置里可以让新邮件通知只提醒他们的来信（见 notify.ts）。

import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";
import { api } from "./api";
import { keys, useSettings } from "./queries";

import { VIP_SETTING, vipList } from "./vipList";

export { VIP_SETTING, vipList, vipOnly } from "./vipList";

export function useVip() {
  const qc = useQueryClient();
  const settings = useSettings();
  const list = useMemo(() => vipList(settings.data as Record<string, unknown> | undefined), [settings.data]);
  const has = useCallback((address: string | null | undefined) => !!address && list.includes(address.trim().toLowerCase()), [list]);
  const save = useCallback(
    async (next: string[]) => {
      await api.setSetting(VIP_SETTING, [...new Set(next)]);
      void qc.invalidateQueries({ queryKey: keys.settings });
    },
    [qc],
  );
  const toggle = useCallback(
    (address: string) => {
      const a = address.trim().toLowerCase();
      return save(list.includes(a) ? list.filter((x) => x !== a) : [...list, a]);
    },
    [list, save],
  );
  const remove = useCallback((address: string) => save(list.filter((x) => x !== address)), [list, save]);
  return { list, has, toggle, remove };
}
