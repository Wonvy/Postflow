// 验证码提醒：收到带验证码的新邮件时，右下角弹出一张卡片（components/CodeNotice.tsx），
// 应用不在前台时另发一条写着验证码的系统通知（lib/notify.ts）。
// 验证码是核心收信时从主题和正文里找出来的（crates/mail/src/code.rs），随新邮件事件带过来。
// 开关在设置 → 通用里，默认关着，只记在这台设备上。

import { create } from "zustand";

export interface CodeMail {
  /** 邮件的条目 ID，点卡片时打开它。 */
  id: string;
  accountId: string;
  code: string;
  from: string;
  subject: string;
}

/** 卡片留多久：验证码一般几分钟就过期，过后留着也没用。 */
export const CODE_NOTICE_MS = 3 * 60_000;

interface CodeNoticeStore {
  /** 现在显示的验证码。新的来了换掉旧的。 */
  current: CodeMail | null;
  show: (mail: CodeMail) => void;
  dismiss: () => void;
}

export const useCodeNotice = create<CodeNoticeStore>()((set) => ({
  current: null,
  show: (current) => set({ current }),
  dismiss: () => set({ current: null }),
}));

interface MailWithCode {
  id: string;
  from: string;
  address?: string;
  subject: string;
  code?: string;
}

/**
 * 把一批新邮件分成两部分：带验证码的（最新的那封用来提醒），和剩下照常通知的。
 * `senders` 是全部新邮件的发件人地址，带验证码的那几封也从里面去掉，剩下的封数才对得上。
 */
export function splitCodes<M extends MailWithCode>(event: {
  mails: M[];
  total: number;
  senders?: string[];
}): { code: M | null; rest: { mails: M[]; total: number; senders?: string[] } } {
  const coded = event.mails.filter((m) => m.code);
  if (coded.length === 0) return { code: null, rest: event };
  let senders = event.senders;
  if (senders) {
    senders = [...senders];
    for (const m of coded) {
      const at = m.address ? senders.indexOf(m.address) : -1;
      if (at >= 0) senders.splice(at, 1);
    }
  }
  return {
    code: coded[0] ?? null,
    rest: {
      mails: event.mails.filter((m) => !m.code),
      total: Math.max(0, event.total - coded.length),
      senders,
    },
  };
}
