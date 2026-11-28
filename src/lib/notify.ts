// 新邮件的系统通知，以及图标上的未读数。
//
// 桌面应用用 Tauri 的通知插件；浏览器里用网页通知（第一次会问能不能发通知）。
// 应用在前台时不弹通知：用户正看着，列表里自己就能看到。
//
// 带验证码的邮件另有一套（设置里单独开）：右下角弹一张写着验证码的卡片，应用不在前台时
// 系统通知里也写上验证码。这几封不再算进普通的新邮件通知里。

import { useEffect } from "react";
import type { Nav, Settings } from "./api";
import { splitCodes, useCodeNotice } from "./codeNotice";
import { vipList, vipOnly } from "./vipList";
import { activePack, resolveLang, translate } from "./i18n";
import type { LangPack } from "./langPack";
import { useUi } from "./store";
import { type CoreEvent, isTauri } from "./transport";

export interface NewMailEvent {
  account_id: string;
  /** `address`：发件人地址（小写），判断是不是重要联系人用。 */
  mails: { id: string; from: string; address?: string; subject: string; date: number; code?: string }[];
  total: number;
  /** 全部新邮件的发件人地址（不止列出的几封），从新到旧。 */
  senders?: string[];
}

/** 一条通知的标题和正文：一封时是发件人和主题，几封时是「N 封新邮件」和前几封的主题。 */
export function notification(
  event: NewMailEvent,
  account: string,
  lang: "zh" | "en",
  pack: LangPack | null = null,
): { title: string; body: string } {
  const t = (key: Parameters<typeof translate>[1], vars?: Record<string, string | number>) =>
    translate(lang, key, vars, pack);
  const [first] = event.mails;
  if (event.total === 1 && first) {
    return { title: first.from || account, body: first.subject || t("mail.noSubject") };
  }
  const sep = lang === "zh" ? "：" : ": ";
  const lines = event.mails.map((m) => (m.from ? `${m.from}${sep}` : "") + (m.subject || t("mail.noSubject")));
  const rest = event.total - event.mails.length;
  if (rest > 0) lines.push(t("notify.more", { n: rest }));
  const title = account ? `${t("notify.newMail", { n: event.total })} · ${account}` : t("notify.newMail", { n: event.total });
  return { title, body: lines.join("\n") };
}

async function show(title: string, body: string, onClick: () => void, tag = "shibapost-new-mail") {
  if (isTauri) {
    const n = await import("@tauri-apps/plugin-notification");
    let granted = await n.isPermissionGranted();
    if (!granted) granted = (await n.requestPermission()) === "granted";
    if (granted) n.sendNotification({ title, body });
    return;
  }
  if (!("Notification" in window)) return;
  if (Notification.permission === "default") await Notification.requestPermission();
  if (Notification.permission !== "granted") return;
  const shown = new Notification(title, { body, tag });
  shown.onclick = () => {
    window.focus();
    onClick();
    shown.close();
  };
}

/**
 * 只提醒重要联系人时，把一批新邮件减到他们的来信。核心只带前几封的详情，但给了全部的发件人地址，
 * 所以封数是准的；列出来的是带着详情的那几封里重要联系人的。
 */
export function vipEvent(event: NewMailEvent, vip: readonly string[]): NewMailEvent {
  const mails = vipOnly(event.mails, vip);
  const set = new Set(vip);
  const total = event.senders ? event.senders.filter((a) => set.has(a.trim().toLowerCase())).length : mails.length;
  return { ...event, mails, total: Math.max(total, mails.length) };
}

/** 核心说来了新邮件时调用。 */
export function notifyNewMail(event: NewMailEvent, nav: Nav | undefined, settings?: Settings) {
  const ui = useUi.getState();
  const account = nav?.accounts.find((a) => a.id === event.account_id);
  const name = account ? account.name || account.email : "";
  const foreground = document.visibilityState === "visible" && document.hasFocus();

  // 验证码提醒：卡片什么时候都弹（正等着验证码呢），系统通知只在应用不在前台时发。
  // 不受「只通知重要联系人」限制：发验证码的都是系统地址
  if (ui.notifyCodes) {
    const { code, rest } = splitCodes(event);
    if (code?.code) {
      const mail = { id: code.id, accountId: event.account_id, code: code.code, from: code.from || name, subject: code.subject };
      useCodeNotice.getState().show(mail);
      if (!foreground) {
        const title = translate(resolveLang(ui.lang), "notify.code", { code: mail.code }, activePack(ui.lang));
        void show(
          title,
          [mail.from, mail.subject].filter(Boolean).join(" · "),
          () => {
            const state = useUi.getState();
            state.setSpace("mail");
            state.setScope({ kind: "account", accountId: mail.accountId });
            state.open(mail.id);
          },
          "shibapost-code",
        ).catch(() => {});
      }
      event = { ...event, ...rest };
    }
  }

  if (!ui.notifyMail) return;
  if (ui.notifyVipOnly) event = vipEvent(event, vipList(settings));
  if (event.total === 0) return;
  // 应用在前台、而且窗口是可见的：不打扰
  if (foreground) return;
  const { title, body } = notification(event, name, resolveLang(ui.lang), activePack(ui.lang));
  const first = event.mails[0]?.id;
  void show(title, body, () => {
    // 浏览器里点通知：打开那个邮箱，单封时直接打开那封
    const state = useUi.getState();
    state.setScope({ kind: "account", accountId: event.account_id });
    if (event.total === 1 && first) state.open(first);
  }).catch(() => {});
}

/** 程序坞、任务栏图标上的未读数（收件箱里的未读邮件）；浏览器里显示在标签页标题上。 */
export function useUnreadBadge(unreadMail: number | undefined) {
  const enabled = useUi((s) => s.badgeUnread);
  useEffect(() => {
    const count = enabled && unreadMail ? unreadMail : 0;
    if (isTauri) {
      void import("@tauri-apps/api/window")
        .then(({ getCurrentWindow }) => getCurrentWindow().setBadgeCount(count > 0 ? count : undefined))
        // Windows 不支持角标数字，安静地算了
        .catch(() => {});
    } else {
      document.title = count > 0 ? `(${count}) ShibaPost` : "ShibaPost";
    }
  }, [enabled, unreadMail]);
}

/** 稍后提醒到时间了：弹通知（应用在前台时也弹，因为这是自己定的提醒）。点通知打开那一条。 */
export function notifyReminders(event: Extract<CoreEvent, { type: "reminders" }>) {
  const ui = useUi.getState();
  const [first] = event.items;
  if (!first) return;
  const t = (key: Parameters<typeof translate>[1], vars?: Record<string, string | number>) =>
    translate(resolveLang(ui.lang), key, vars, activePack(ui.lang));
  const title = event.total === 1 ? t("notify.reminder") : t("notify.reminders", { n: event.total });
  const lines = event.items.map((i) => (i.source_title ? `${i.source_title}: ` : "") + (i.title || t("mail.noSubject")));
  void show(title, lines.join("\n"), () => {
    const state = useUi.getState();
    state.setSpace(first.kind === "mail" ? "mail" : "feeds");
    state.setScope({ kind: "view", view: "inbox" });
    if (event.total === 1) state.open(first.id);
  }).catch(() => {});
}

/**
 * 托盘图标：未读数（macOS 显示在菜单栏图标旁，其他系统在悬停提示里）、菜单文字（界面语言），
 * 以及关窗口时要不要留在后台。只有桌面应用有。
 */
export function useTray(unreadMail: number | undefined) {
  const keepRunning = useUi((s) => s.keepRunning);
  const edgeHide = useUi((s) => s.edgeHide);
  const lang = useUi((s) => s.lang);
  useEffect(() => {
    if (!isTauri) return;
    const unread = unreadMail ?? 0;
    const t = (key: Parameters<typeof translate>[1], vars?: Record<string, string | number>) =>
      translate(resolveLang(lang), key, vars, activePack(lang));
    void import("@tauri-apps/api/core")
      .then(({ invoke }) =>
        invoke("tray_update", {
          update: {
            unread,
            tooltip: unread > 0 ? `ShibaPost · ${t("tray.unread", { n: unread })}` : "ShibaPost",
            unread_label: unread > 0 ? t("tray.unread", { n: unread }) : null,
            show_label: t("tray.open"),
            compose_label: t("tray.compose"),
            refresh_label: t("tray.refresh"),
            settings_label: t("tray.settings"),
            edge_label: t("tray.edgeHide"),
            edge_hide: edgeHide,
            quit_label: t("tray.quit"),
            keep_running: keepRunning,
          },
        }),
      )
      .catch(() => {});
  }, [unreadMail, keepRunning, edgeHide, lang]);
}
