// 新邮件的系统通知，以及图标上的未读数。
//
// 桌面应用用 Tauri 的通知插件；浏览器里用网页通知（第一次会问能不能发通知）。
// 应用在前台时不弹通知：用户正看着，列表里自己就能看到。

import { useEffect } from "react";
import type { Nav } from "./api";
import { resolveLang, translate } from "./i18n";
import { useUi } from "./store";
import { isTauri } from "./transport";

export interface NewMailEvent {
  account_id: string;
  mails: { id: string; from: string; subject: string; date: number }[];
  total: number;
}

/** 一条通知的标题和正文：一封时是发件人和主题，几封时是「N 封新邮件」和前几封的主题。 */
export function notification(event: NewMailEvent, account: string, lang: "zh" | "en"): { title: string; body: string } {
  const t = (key: Parameters<typeof translate>[1], vars?: Record<string, string | number>) => translate(lang, key, vars);
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

async function show(title: string, body: string, onClick: () => void) {
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
  const shown = new Notification(title, { body, tag: "postflow-new-mail" });
  shown.onclick = () => {
    window.focus();
    onClick();
    shown.close();
  };
}

/** 核心说来了新邮件时调用。 */
export function notifyNewMail(event: NewMailEvent, nav: Nav | undefined) {
  const ui = useUi.getState();
  if (!ui.notifyMail || event.total === 0) return;
  // 应用在前台、而且窗口是可见的：不打扰
  if (document.visibilityState === "visible" && document.hasFocus()) return;
  const account = nav?.accounts.find((a) => a.id === event.account_id);
  const name = account ? account.name || account.email : "";
  const { title, body } = notification(event, name, resolveLang(ui.lang));
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
      document.title = count > 0 ? `(${count}) Postflow` : "Postflow";
    }
  }, [enabled, unreadMail]);
}
