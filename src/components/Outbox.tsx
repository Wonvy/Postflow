// 定时发送：侧栏邮件那边「定时发送」一行（有等着发的才显示），点开列出来，可以立即发送或取消（存回草稿）。
// 存放和发送在核心（crates/core/src/compose/outbox.rs）。

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Clock } from "lucide-react";
import { useState } from "react";
import { api, type OutboxItem } from "../lib/api";
import { errorText, useLang, useT } from "../lib/i18n";
import { keys, useNav } from "../lib/queries";
import { useUi } from "../lib/store";
import { snoozeTime } from "./Snooze";
import { toast } from "./Toast";
import { Button, cx, ICON, Modal } from "./ui";

export function useOutbox() {
  return useQuery({ queryKey: ["outbox"], queryFn: api.outbox, staleTime: 30_000, refetchInterval: 60_000 });
}

/** 侧栏里的一行，跟其他文件夹一个样子。 */
export function OutboxRow() {
  const t = useT();
  const outbox = useOutbox();
  const setDialog = useUi((s) => s.setDialog);
  const n = outbox.data?.length ?? 0;
  if (n === 0) return null;
  const failed = outbox.data?.some((i) => i.error);
  return (
    <button
      type="button"
      onClick={() => setDialog({ type: "outbox" })}
      className="flex h-7 items-center gap-2 rounded-md pl-2 pr-1.5 text-left text-fg-2 hover:bg-hover hover:text-fg"
    >
      <span className="flex size-4 shrink-0 items-center justify-center text-fg-3">
        <Clock {...ICON} />
      </span>
      <span className="min-w-0 flex-1 truncate">{t("outbox.title")}</span>
      <span className={cx("tabular shrink-0 px-1 text-[12px]", failed ? "text-danger" : "text-fg-3")}>{n}</span>
    </button>
  );
}

function Entry({ item }: { item: OutboxItem }) {
  const t = useT();
  const lang = useLang();
  const qc = useQueryClient();
  const accounts = useNav().data?.accounts ?? [];
  const [busy, setBusy] = useState<"send" | "cancel" | null>(null);
  const account = accounts.find((a) => a.id === item.account_id);
  const run = async (what: "send" | "cancel") => {
    setBusy(what);
    try {
      if (what === "send") await api.outboxSend(item.id);
      else await api.unschedule(item.id);
      toast(what === "send" ? t("compose.sent") : t("compose.draftSaved", { time: snoozeTime(Date.now(), lang) }));
    } catch (err) {
      toast(errorText(t, err));
    } finally {
      setBusy(null);
      void qc.invalidateQueries({ queryKey: ["outbox"] });
      void qc.invalidateQueries({ queryKey: ["items"] });
      void qc.invalidateQueries({ queryKey: keys.nav });
    }
  };
  return (
    <li className="flex flex-col gap-1.5 rounded-lg border border-line p-3">
      <div className="flex items-baseline gap-2">
        <span className="min-w-0 flex-1 truncate font-medium text-fg">{item.subject.trim() || t("outbox.noSubject")}</span>
        <span className="shrink-0 text-[12px] text-fg-3">{snoozeTime(item.send_at, lang)}</span>
      </div>
      <p className="truncate text-[12px] text-fg-3">
        {t("outbox.to", { names: item.to.join(", ") })}
        {account && ` · ${account.name || account.email}`}
      </p>
      {item.error && <p className="text-[12px] text-danger">{t("outbox.failed", { error: item.error })}</p>}
      <div className="flex justify-end gap-2 pt-1">
        <Button variant="ghost" busy={busy === "cancel"} disabled={busy !== null} onClick={() => void run("cancel")}>
          {t("outbox.cancel")}
        </Button>
        <Button busy={busy === "send"} disabled={busy !== null} onClick={() => void run("send")}>
          {t("outbox.sendNow")}
        </Button>
      </div>
    </li>
  );
}

export function OutboxDialog() {
  const t = useT();
  const dialog = useUi((s) => s.dialog);
  const setDialog = useUi((s) => s.setDialog);
  const outbox = useOutbox();
  const items = outbox.data ?? [];
  return (
    <Modal open={dialog?.type === "outbox"} onClose={() => setDialog(null)} title={t("outbox.title")} width={480}>
      <div className="flex flex-col gap-3 p-4">
        <p className="text-[12px] leading-relaxed text-fg-3">{t("outbox.hint")}</p>
        {items.length === 0 ? (
          <p className="py-6 text-center text-[13px] text-fg-3">{t("outbox.empty")}</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {items.map((item) => (
              <Entry key={item.id} item={item} />
            ))}
          </ul>
        )}
      </div>
    </Modal>
  );
}
