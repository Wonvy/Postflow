// 发件人卡片：点阅读区里的发件人弹出来。和这个人的往来（收到几封、发给他几封、最近几封）、
// 写信、看他的全部来信、设为重要联系人。数字来自邮件历史，不需要通讯录。

import * as Popover from "@radix-ui/react-popover";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Copy, Search, SquarePen, Star } from "lucide-react";
import { type ReactNode, useState } from "react";
import { api, type Nav } from "../lib/api";
import { useLang, useT } from "../lib/i18n";
import { copyText } from "../lib/platform";
import { keys } from "../lib/queries";
import { useUi } from "../lib/store";
import { shortTime } from "../lib/time";
import { useVip } from "../lib/vip";
import { SenderAvatar } from "./SourceIcon";
import { toast } from "./Toast";
import { Button, cx, Spinner } from "./ui";

export function SenderCard({
  name,
  address,
  accountId,
  children,
}: {
  name: string;
  address: string;
  /** 这封信是哪个邮箱收到的：写信时默认从它发。 */
  accountId: string;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild>{children}</Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align="start"
          sideOffset={6}
          collisionPadding={8}
          className="anim-pop z-50 w-[320px] rounded-lg bg-surface p-3 shadow-pop outline-none"
        >
          <CardBody name={name} address={address} accountId={accountId} close={() => setOpen(false)} />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

function CardBody({
  name,
  address,
  accountId,
  close,
}: {
  name: string;
  address: string;
  accountId: string;
  close: () => void;
}) {
  const t = useT();
  const lang = useLang();
  const zh = lang === "zh" ? "zh" : "en";
  const qc = useQueryClient();
  const vip = useVip();
  const profile = useQuery({
    queryKey: ["sender", address.toLowerCase()],
    queryFn: () => api.senderProfile(address),
    staleTime: 30_000,
  });
  const p = profile.data;
  const shownName = p?.name || name || address;
  const important = vip.has(address);


  const writeTo = () => {
    close();
    if (!qc.getQueryData<Nav>(keys.nav)?.accounts.length) return;
    useUi.getState().setDialog({ type: "compose", mode: "new", accountId, to: address });
  };

  const allMail = () => {
    close();
    const ui = useUi.getState();
    ui.setSpace("mail");
    // 搜索框打开时记着自己的文字：先关上，再带着新的条件打开
    ui.closeSearch();
    setTimeout(() => {
      const s = useUi.getState();
      s.setSearchAll(true);
      s.setSearchQuery(`from:${address}`);
      s.openSearch();
    }, 0);
  };

  return (
    <div className="flex flex-col gap-3 text-[13px]">
      <div className="flex items-center gap-3">
        <SenderAvatar name={shownName} address={address} size={40} />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5">
            <span className="truncate font-medium text-fg">{shownName}</span>
            {important && <Star size={12} className="shrink-0 fill-warning text-warning" aria-label={t("sender.vip")} />}
          </div>
          <button
            type="button"
            title={t("sender.copy")}
            onClick={() => void copyText(address).then(() => toast(t("sender.copied")))}
            className="group flex max-w-full items-center gap-1 text-[12px] text-fg-3 hover:text-fg-2"
          >
            <span className="truncate">{address}</span>
            <Copy size={11} className="shrink-0 opacity-0 group-hover:opacity-100" />
          </button>
        </div>
      </div>

      {!p ? (
        <div className="flex justify-center py-2 text-fg-3">
          <Spinner size={13} />
        </div>
      ) : (
        <>
          <p className="text-[12px] leading-relaxed text-fg-2">
            {t("sender.received", { n: p.received })}
            {p.unread > 0 && ` · ${t("sender.unread", { n: p.unread })}`}
            {p.sent > 0 && ` · ${t("sender.sent", { n: p.sent })}`}
            {p.first_at !== null && p.received > 1 && (
              <>
                <br />
                <span className="text-fg-3">
                  {t("sender.since", { date: new Date(p.first_at).toLocaleDateString(lang === "zh" ? "zh-CN" : "en-US") })}
                </span>
              </>
            )}
          </p>
          {p.recent.length > 0 && (
            <ul className="-mx-1.5 flex flex-col">
              {p.recent.map((m) => (
                <li key={m.id}>
                  <button
                    type="button"
                    onClick={() => {
                      close();
                      useUi.getState().open(m.id);
                    }}
                    className={cx(
                      "flex w-full items-baseline gap-2 rounded-md px-1.5 py-1 text-left hover:bg-hover",
                      m.read ? "text-fg-2" : "font-medium text-fg",
                    )}
                  >
                    <span className="min-w-0 flex-1 truncate text-[12px]">{m.title || t("mail.noSubject")}</span>
                    <span className="shrink-0 text-[11px] tabular-nums font-normal text-fg-3">
                      {shortTime(m.published_at, zh)}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </>
      )}

      <div className="flex flex-wrap gap-1.5">
        <Button className="h-7 gap-1.5 px-2 text-[12px]" onClick={writeTo}>
          <SquarePen size={13} />
          {t("sender.write")}
        </Button>
        <Button className="h-7 gap-1.5 px-2 text-[12px]" onClick={allMail}>
          <Search size={13} />
          {t("sender.all")}
        </Button>
        <Button
          variant={important ? "primary" : "secondary"}
          className="h-7 gap-1.5 px-2 text-[12px]"
          aria-pressed={important}
          onClick={() => void vip.toggle(address)}
          title={t("sender.vipHint")}
        >
          <Star size={13} className={cx(important && "fill-current")} />
          {t(important ? "sender.vipOn" : "sender.vipOff")}
        </Button>
      </div>
    </div>
  );
}
