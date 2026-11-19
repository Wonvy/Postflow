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
