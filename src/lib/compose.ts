// 回复、全部回复、转发时，信的收件人、主题和正文怎么预先填好。不碰界面，方便测试。

import type { AttachedFile, Mailbox, Thread } from "./api";

/** 写新邮件、回复、全部回复、转发，或者接着写一封草稿。 */
export type ComposeMode = "new" | "reply" | "reply-all" | "forward" | "draft";

/** 回复、转发要用到的原信内容（`mail.compose_context` 的返回）。 */
export interface ReplyContext {
  account_id: string;
  message_id: string | null;
  subject: string;
  /** Unix 毫秒。 */
  date: number | null;
  from: Mailbox[];
  reply_to: Mailbox[];
  to: Mailbox[];
  cc: Mailbox[];
  /** 原信正文的文字，保留分行。 */
  text: string;
  /** 回复时接上讨论用的信头。 */
  thread: Thread;
  /** 转发时原信的附件（已经交给核心了）。 */
  attachments: AttachedFile[];
}

/** 写信窗口里一开始填好的内容。 */
export interface Prefill {
  to: string[];
  cc: string[];
  subject: string;
  text: string;
}

/** 一个收件人写成一格里显示的样子：有名字时是 `名字 <地址>`。 */
export function formatAddress(m: Mailbox): string {
  const address = m.address?.trim() ?? "";
  const name = m.name?.trim().replace(/"/g, "") ?? "";
  if (!address) return name;
  if (!name || name.toLowerCase() === address.toLowerCase()) return address;
  // 名字里有逗号、尖括号这些符号时加引号，免得被拆成两个收件人
  return /[,;<>@()[\]:\\]/.test(name) ? `"${name}" <${address}>` : `${name} <${address}>`;
}

/** 取出一格里的地址部分，比较是不是同一个人用。 */
export function addressOf(entry: string): string {
  const m = entry.match(/<([^<>]*)>\s*$/);
  return (m ? m[1]! : entry).trim().toLowerCase();
}

/** 看起来像一个邮件地址。和核心里的检查一致，只用来在界面上提前标出写错的。 */
export function isValidAddress(entry: string): boolean {
  const address = addressOf(entry);
  const at = address.lastIndexOf("@");
  if (at <= 0 || at === address.length - 1) return false;
  const local = address.slice(0, at);
  const domain = address.slice(at + 1);
  const bad = /[\s<>()[\],;:"\\@]/;
  return (
    !bad.test(local) &&
    !bad.test(domain) &&
    domain.includes(".") &&
    !domain.startsWith(".") &&
    !domain.endsWith(".") &&
    address.length <= 254
  );
}

/**
 * 把一段文字拆成几个收件人：逗号、分号、换行分隔。引号和尖括号里的逗号不算，
 * 所以 `"Lovelace, Ada" <ada@example.com>` 是一个人。
 */
export function splitAddresses(input: string): string[] {
  const out: string[] = [];
  let current = "";
  let quoted = false;
  let angled = false;
  for (const c of input) {
    if (c === '"') quoted = !quoted;
    else if (c === "<" && !quoted) angled = true;
    else if (c === ">" && !quoted) angled = false;
    if (!quoted && !angled && (c === "," || c === ";" || c === "\n" || c === "，" || c === "；")) {
      if (current.trim()) out.push(current.trim());
      current = "";
      continue;
    }
    current += c;
  }
  if (current.trim()) out.push(current.trim());
  return out;
}

const REPLY_PREFIX = /^\s*(re|aw|sv|antw|回复|答复|回覆)\s*(\[\d+\])?\s*[:：]/i;
const FORWARD_PREFIX = /^\s*(fwd?|wg|tr|转发|轉寄|转寄)\s*(\[\d+\])?\s*[:：]/i;

export function replySubject(subject: string): string {
  const s = subject.trim();
  return REPLY_PREFIX.test(s) ? s : `Re: ${s}`;
}

export function forwardSubject(subject: string): string {
  const s = subject.trim();
  return FORWARD_PREFIX.test(s) ? s : `Fwd: ${s}`;
}

/** 用「>」引用原信：已经引用过的行再加一层时不加空格，和常见邮件程序一样。 */
export function quote(text: string): string {
  return text
    .split("\n")
    .map((line) => (line.startsWith(">") ? `>${line}` : line ? `> ${line}` : ">"))
    .join("\n");
}

function dedupe(list: Mailbox[], exclude: Set<string>): Mailbox[] {
  const out: Mailbox[] = [];
  for (const m of list) {
    const key = m.address?.trim().toLowerCase();
    if (!key || exclude.has(key)) continue;
    exclude.add(key);
    out.push(m);
  }
  return out;
}

/**
 * 回复给谁。`self` 是收到这封信的邮箱的地址，全部回复时把它去掉。
 * 回复自己发出的信时，回给原来的收件人。
 */
export function replyRecipients(
  ctx: ReplyContext,
  all: boolean,
  self: string[],
): { to: Mailbox[]; cc: Mailbox[] } {
  const mine = new Set(self.map((a) => a.trim().toLowerCase()));
  const fromMe = ctx.from.some((m) => m.address && mine.has(m.address.trim().toLowerCase()));
  const primary = fromMe ? ctx.to : ctx.reply_to.length > 0 ? ctx.reply_to : ctx.from;
  if (!all) {
    const to = dedupe(primary, new Set(fromMe ? mine : []));
    return { to: to.length > 0 ? to : dedupe(primary, new Set()), cc: [] };
  }
  const seen = new Set(mine);
  let to = dedupe(fromMe ? ctx.to : [...primary, ...ctx.to], seen);
  // 只有自己一个人的讨论：至少回给原发件人
  if (to.length === 0) to = dedupe(primary, new Set());
  const cc = dedupe(ctx.cc, seen);
  return { to, cc };
}

function when(date: number | null, lang: "zh" | "en"): string {
  if (date === null) return "";
  return new Intl.DateTimeFormat(lang === "zh" ? "zh-CN" : "en-US", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(date));
}

function who(m: Mailbox | undefined): string {
  if (!m) return "";
  return m.name?.trim() || m.address?.trim() || "";
}

/** 回复时引文上面那一行：「某时某人写道：」。 */
export function attribution(ctx: ReplyContext, lang: "zh" | "en"): string {
  const name = who(ctx.from[0]);
  const date = when(ctx.date, lang);
  if (lang === "zh") {
    if (date && name) return `${date}，${name} 写道：`;
    return name ? `${name} 写道：` : "原信：";
  }
  if (date && name) return `On ${date}, ${name} wrote:`;
  return name ? `${name} wrote:` : "Original message:";
}

/** 转发时正文上面那几行：原信的发件人、时间、主题和收件人。 */
export function forwardHeader(ctx: ReplyContext, lang: "zh" | "en"): string {
  const list = (ms: Mailbox[]) => ms.map(formatAddress).join(", ");
  const rows: [string, string][] =
    lang === "zh"
      ? [
          ["发件人", list(ctx.from)],
          ["日期", when(ctx.date, lang)],
          ["主题", ctx.subject],
          ["收件人", list(ctx.to)],
          ["抄送", list(ctx.cc)],
        ]
      : [
          ["From", list(ctx.from)],
          ["Date", when(ctx.date, lang)],
          ["Subject", ctx.subject],
          ["To", list(ctx.to)],
          ["Cc", list(ctx.cc)],
        ];
  const sep = lang === "zh" ? "：" : ": ";
  const title = lang === "zh" ? "---------- 转发的邮件 ----------" : "---------- Forwarded message ----------";
  return [title, ...rows.filter(([, v]) => v).map(([k, v]) => `${k}${sep}${v}`)].join("\n");
}

/** 按方式预先填好收件人、主题和正文。光标放在最前面，引文在下面。 */
export function prefill(mode: ComposeMode, ctx: ReplyContext | null, self: string[], lang: "zh" | "en"): Prefill {
  if (mode === "new" || mode === "draft" || !ctx) return { to: [], cc: [], subject: "", text: "" };
  if (mode === "forward") {
    return {
      to: [],
      cc: [],
      subject: forwardSubject(ctx.subject),
      text: `\n\n${forwardHeader(ctx, lang)}\n\n${ctx.text}`,
    };
  }
  const { to, cc } = replyRecipients(ctx, mode === "reply-all", self);
  const quoted = ctx.text ? `\n${quote(ctx.text)}` : "";
  return {
    to: to.map(formatAddress),
    cc: cc.map(formatAddress),
    subject: replySubject(ctx.subject),
    text: `\n\n${attribution(ctx, lang)}${quoted}`,
  };
}

/** 签名加在正文里的样子：常见的「-- 」分隔线，下面是签名。没有签名时什么都不加。 */
export function signatureBlock(signature: string): string {
  const sig = signature.replace(/\s+$/, "");
  return sig ? `\n\n-- \n${sig}` : "";
}

/**
 * 正文里放进签名：写新邮件时在最后，回复、转发时在引文上面（`tail` 是预先填好的引文那一段）。
 */
export function addSignature(text: string, signature: string, tail: string): string {
  const block = signatureBlock(signature);
  if (!block) return text;
  if (tail && text.endsWith(tail)) return text.slice(0, text.length - tail.length) + block + tail;
  return text + block;
}

/**
 * 换了发信的邮箱：把正文里旧邮箱的签名换成新邮箱的。旧签名被改动过就不碰它，免得改掉用户写的字；
 * 原来没有签名时，按 `addSignature` 的位置加上。
 */
export function swapSignature(text: string, from: string, to: string, tail: string): string {
  const old = signatureBlock(from);
  const next = signatureBlock(to);
  if (old === next) return text;
  if (!old) return addSignature(text, to, tail);
  const at = text.lastIndexOf(old, tail && text.endsWith(tail) ? text.length - tail.length : undefined);
  if (at < 0) return text;
  return text.slice(0, at) + next + text.slice(at + old.length);
}
