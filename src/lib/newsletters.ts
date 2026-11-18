// 订阅邮件（Newsletter）放到订阅里看：按发件人地址记在同步设置 newsletters 里，
// 这些发件人的邮件就在订阅那边和文章一起看，邮件那边的视图和计数里不再有它们。规则在核心（store.rs 的 newsletter_sql）。

import { useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { api } from "./api";
import { useNewsletters } from "./feedKinds";
import { keys } from "./queries";
import { useUi } from "./store";

export function useNewsletterActions() {
  const qc = useQueryClient();
  const list = useNewsletters();

  const save = useCallback(
    async (addresses: string[]) => {
      await api.setSetting("newsletters", [...new Set(addresses)]);
      void qc.invalidateQueries({ queryKey: keys.nav });
      void qc.invalidateQueries({ queryKey: keys.settings });
      void qc.invalidateQueries({ queryKey: ["items"] });
    },
    [qc],
  );

  const has = useCallback(
    (address: string | null | undefined) => !!address && list.some((n) => n.address === address.trim().toLowerCase()),
    [list],
  );

  /** 这个发件人的信放到订阅里看。 */
  const add = useCallback(
    (address: string) => save([...list.map((n) => n.address), address.trim().toLowerCase()]),
    [list, save],
  );

  /** 放回邮件。正停在这个发件人上的回到「全部文章」。 */
  const remove = useCallback(
    async (address: string) => {
      const target = address.trim().toLowerCase();
      await save(list.map((n) => n.address).filter((a) => a !== target));
      const { scope, setScope } = useUi.getState();
      if (scope.kind === "newsletter" && scope.address === target) setScope({ kind: "view", view: "inbox" });
    },
    [list, save],
  );

  return { list, has, add, remove };
}
