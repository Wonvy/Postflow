// 会议邀请卡片：邮件里带着日历邀请时，在正文上面显示时间、地点、组织者和参会人，
// 可以接受、暂定、拒绝（给组织者回一封标准的回复，对方日历里的参会状态跟着变），
// 加入线上会议，或者交给系统的日历应用。取消通知和别人的回复只显示状态。

import { useQueryClient } from "@tanstack/react-query";
import { CalendarClock, CalendarPlus, CalendarX2, Check, HelpCircle, MapPin, Repeat, Users, Video, X } from "lucide-react";
import { useState } from "react";
import type { Invite, ItemDetail } from "../lib/api";
import { api } from "../lib/api";
import { errorText, type MessageKey, useLang, useT } from "../lib/i18n";
import { openExternal, openInvite } from "../lib/platform";
import { keys } from "../lib/queries";
import { toast } from "./Toast";
import { Button, cx, Spinner } from "./ui";

type Status = "ACCEPTED" | "TENTATIVE" | "DECLINED";

const STATUS_LABEL: Record<Status, MessageKey> = {
  ACCEPTED: "invite.accepted",
  TENTATIVE: "invite.tentative",
  DECLINED: "invite.declined",
};

const REPLY_LABEL: Record<Status, MessageKey> = {
  ACCEPTED: "invite.replyAccepted",
  TENTATIVE: "invite.replyTentative",
  DECLINED: "invite.replyDeclined",
};

/** 会议时间：同一天的写成「11月20日 周五 10:00–11:00」，跨天的两头都写全。 */
export function inviteTime(inv: Invite, lang: "zh" | "en"): string {
  const locale = lang === "zh" ? "zh-CN" : "en-US";
  const day = new Intl.DateTimeFormat(locale, { month: "short", day: "numeric", weekday: "short", year: undefined });
  const clock = new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit" });
  if (inv.start === null) {
    if (!inv.start_local) return "";
    // 没有时区：当地时间原样显示
    const [d, t] = inv.start_local.split("T");
    const date = new Date(`${d}T${t ?? "00:00"}`);
    return inv.all_day || !t ? day.format(date) : `${day.format(date)} ${t}`;
  }
  const start = new Date(inv.start);
  if (inv.end === null) return `${day.format(start)} ${clock.format(start)}`;
  const end = new Date(inv.end);
  const sameDay = start.toDateString() === end.toDateString();
  return sameDay
    ? `${day.format(start)} ${clock.format(start)}–${clock.format(end)}`
    : `${day.format(start)} ${clock.format(start)} – ${day.format(end)} ${clock.format(end)}`;
}

export function InviteCard({ item }: { item: ItemDetail }) {
  const t = useT();
  const lang = useLang() === "zh" ? "zh" : "en";
  const qc = useQueryClient();
  const mail = item.mail;
  const inv = mail?.invite;
  const [busy, setBusy] = useState<Status | null>(null);
  const [answered, setAnswered] = useState<string | null>(mail?.rsvp ?? null);
  if (!mail || !inv) return null;

  const cancelled = inv.method === "CANCEL";
  const isReply = inv.method === "REPLY";
  const canAnswer = inv.method === "REQUEST" && !!inv.organizer?.address;
  const organizer = inv.organizer?.name || inv.organizer?.address;
  const going = inv.attendees.filter((a) => a.status === "ACCEPTED").length;

  const answer = async (status: Status) => {
    setBusy(status);
    try {
      await api.rsvp(item.id, status);
      setAnswered(status);
      toast(t("invite.sent", { status: t(STATUS_LABEL[status]) }));
      void qc.invalidateQueries({ queryKey: keys.item(item.id) });
    } catch (err) {
      toast(errorText(t, err));
    } finally {
      setBusy(null);
    }
  };

  const Icon = cancelled ? CalendarX2 : CalendarClock;
  return (
    <div className={cx("mb-6 rounded-xl border bg-surface px-4 py-3", cancelled ? "border-danger/40" : "border-line")}>
      <div className="flex items-start gap-3">
        <Icon size={20} strokeWidth={1.5} className={cx("mt-0.5 shrink-0", cancelled ? "text-danger" : "text-accent")} />
        <div className="min-w-0 flex-1">
          <p className={cx("font-medium text-fg", cancelled && "line-through decoration-danger/60")}>
            {inv.summary || t("invite.untitled")}
          </p>
          {cancelled && <p className="text-[12px] text-danger">{t("invite.cancelled")}</p>}
          {isReply && (
            <p className="text-[12px] text-fg-2">
              {inv.attendees
                .map((a) => t(REPLY_LABEL[a.status as Status] ?? "invite.replied", { name: a.name || a.address }))
                .join(" · ") || t("invite.replyGeneric")}
            </p>
          )}
          <div className="mt-1 flex flex-col gap-0.5 text-[12px] text-fg-2">
            <span className="flex flex-wrap items-center gap-x-1.5">
              {inviteTime(inv, lang)}
              {(inv.recurring || inv.occurrence) && (
                <span className="inline-flex items-center gap-0.5 text-fg-3">
                  <Repeat size={11} />
                  {t(inv.occurrence ? "invite.occurrence" : "invite.recurring")}
                </span>
              )}
            </span>
            {inv.location && (
              <span className="flex min-w-0 items-center gap-1">
                <MapPin size={12} className="shrink-0 text-fg-3" />
                <span className="truncate">{inv.location}</span>
              </span>
            )}
            {!isReply && (organizer || inv.attendees.length > 0) && (
              <span className="flex min-w-0 items-center gap-1 text-fg-3">
                <Users size={12} className="shrink-0" />
                <span className="truncate">
                  {organizer && t("invite.organizer", { name: organizer })}
                  {inv.attendees.length > 0 &&
                    `${organizer ? " · " : ""}${t("invite.people", { n: inv.attendees.length, going })}`}
                </span>
              </span>
            )}
          </div>
        </div>
      </div>

      {!isReply && (
        <div className="mt-3 flex flex-wrap items-center gap-1.5">
          {canAnswer &&
            !cancelled &&
            (["ACCEPTED", "TENTATIVE", "DECLINED"] as const).map((s) => {
              const IconFor = s === "ACCEPTED" ? Check : s === "TENTATIVE" ? HelpCircle : X;
              const chosen = answered === s;
              return (
                <Button
                  key={s}
                  variant={chosen ? "primary" : "secondary"}
                  aria-pressed={chosen}
                  disabled={busy !== null}
                  className="h-7 gap-1.5 px-2.5 text-[12px]"
                  onClick={() => void answer(s)}
                >
                  {busy === s ? <Spinner size={12} /> : <IconFor size={13} />}
                  {t(s === "ACCEPTED" ? "invite.accept" : s === "TENTATIVE" ? "invite.maybe" : "invite.decline")}
                </Button>
              );
            })}
          {inv.url && !cancelled && (
            <Button className="h-7 gap-1.5 px-2.5 text-[12px]" onClick={() => void openExternal(inv.url ?? "")}>
              <Video size={13} />
              {t("invite.join")}
            </Button>
          )}
          <Button
            variant="ghost"
            className="h-7 gap-1.5 px-2.5 text-[12px]"
            onClick={() => void openInvite(item.id).catch((err) => toast(errorText(t, err)))}
          >
            <CalendarPlus size={13} />
            {t(cancelled ? "invite.openCalendar" : "invite.addCalendar")}
          </Button>
          {answered && !busy && (
            <span className="ml-1 text-[12px] text-fg-3">{t("invite.youAnswered", { status: t(STATUS_LABEL[answered as Status] ?? "invite.accepted") })}</span>
          )}
        </div>
      )}
    </div>
  );
}
