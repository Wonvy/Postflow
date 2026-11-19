// 快速回复模板的纯函数（不依赖界面），见 templates.ts。用 Node 自带的测试工具测：npm run test:ui

export const TEMPLATES_SETTING = "mail_templates";

/** 一段常用回复。`name` 是菜单里显示的名字，`body` 是插进正文的纯文字。 */
export interface Template {
  id: string;
  name: string;
  body: string;
}

/** 设置里存的模板，坏掉的条目丢掉。 */
export function templateList(settings: Record<string, unknown> | undefined): Template[] {
  const raw = settings?.[TEMPLATES_SETTING];
  if (!Array.isArray(raw)) return [];
  return raw.filter(
    (t): t is Template =>
      !!t && typeof t === "object" && typeof t.id === "string" && typeof t.name === "string" && typeof t.body === "string",
  );
}

/** 新模板的名字：正文第一行，太长截断。 */
export function templateName(body: string, max = 24): string {
  const first = body.trim().split("\n")[0]?.trim() ?? "";
  return first.length > max ? `${first.slice(0, max)}…` : first;
}

/** 把文字插到纯文本正文的光标处，返回新正文和插入后光标的位置。 */
export function insertAt(text: string, at: number, insert: string): { text: string; caret: number } {
  const pos = Math.max(0, Math.min(at, text.length));
  return { text: text.slice(0, pos) + insert + text.slice(pos), caret: pos + insert.length };
}

/** 「Mira Chen <mira@x.com>」里的名字；只有地址时用 @ 前面那段。 */
export function personName(recipient: string): string {
  const r = recipient.trim();
  const lt = r.indexOf("<");
  const name = (lt > 0 ? r.slice(0, lt) : "").trim().replace(/^"|"$/g, "");
  if (name) return name;
  const address = (lt >= 0 ? r.slice(lt + 1).replace(/>.*$/, "") : r).trim();
  return address.split("@")[0] ?? address;
}

/**
 * 模板里的占位：`{name}`（第一个收件人的名字）、`{first}`（名字的第一个词）、`{me}`（我的名字）、
 * `{date}`（今天）。中文写法 `{名字}`、`{名}`、`{我}`、`{日期}` 也行。没有收件人时 `{name}` 留空。
 */
export function fillTemplate(
  body: string,
  ctx: { to: readonly string[]; me: string; now: Date; lang: "zh" | "en" },
): string {
  const name = ctx.to[0] ? personName(ctx.to[0]) : "";
  // 中文名不拆；西文取第一个词
  const first = /[一-鿿]/.test(name) ? name : (name.split(/\s+/)[0] ?? "");
  const date = ctx.now.toLocaleDateString(ctx.lang === "zh" ? "zh-CN" : "en-US", {
    year: "numeric",
    month: "long",
    day: "numeric",
  });
  const values: Record<string, string> = {
    name,
    名字: name,
    first,
    名: first,
    me: ctx.me,
    我: ctx.me,
    date,
    日期: date,
  };
  return body.replace(/\{([^{}\s]{1,8})\}/g, (whole, key: string) => values[key.toLowerCase()] ?? values[key] ?? whole);
}
