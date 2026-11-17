// 工具栏的布局：每个按钮放在顶部工具栏、底部状态栏，还是隐藏，以及先后顺序。
// 列表和阅读区各有一套，用户在「自定义工具栏」里改，只存在这台设备上。
// 这里只有纯函数，方便测试：npm run test:ui

export type ToolPlace = "top" | "status" | "hidden";
export type ToolbarId = "list" | "reader";

export interface ToolSlot {
  id: string;
  place: ToolPlace;
}

export type ToolbarLayout = ToolSlot[];

/**
 * 每个工具栏有哪些按钮，以及默认放在哪里。顺序就是默认顺序。
 * 顶部只留常用的操作，显示方式这类开关默认放到状态栏。
 */
export const TOOL_DEFAULTS: Record<ToolbarId, ToolbarLayout> = {
  list: [
    { id: "kindFilter", place: "top" },
    { id: "search", place: "top" },
    { id: "markAllRead", place: "top" },
    { id: "refresh", place: "top" },
    { id: "view", place: "status" },
    { id: "group", place: "status" },
    { id: "translate", place: "status" },
  ],
  reader: [
    { id: "editDraft", place: "top" },
    { id: "deleteDraft", place: "top" },
    { id: "reply", place: "top" },
    { id: "replyAll", place: "top" },
    { id: "forward", place: "top" },
    { id: "label", place: "top" },
    { id: "ai", place: "top" },
    { id: "read", place: "top" },
    { id: "star", place: "top" },
    { id: "archive", place: "top" },
    { id: "original", place: "top" },
    { id: "style", place: "status" },
    { id: "fullText", place: "status" },
    { id: "web", place: "status" },
    { id: "translate", place: "status" },
  ],
};

const PLACES: readonly ToolPlace[] = ["top", "status", "hidden"];

/**
 * 本机存的布局可能来自旧版本：去掉已经没有的按钮、去掉重复的，
 * 新加的按钮按默认位置接在它默认的前一个按钮后面。
 */
export function normalizeLayout(toolbar: ToolbarId, raw: unknown): ToolbarLayout {
  const defaults = TOOL_DEFAULTS[toolbar];
  const known = new Set(defaults.map((d) => d.id));
  const seen = new Set<string>();
  const out: ToolbarLayout = [];
  if (Array.isArray(raw)) {
    for (const entry of raw) {
      if (!entry || typeof entry !== "object") continue;
      const { id, place } = entry as Partial<ToolSlot>;
      if (typeof id !== "string" || !known.has(id) || seen.has(id)) continue;
      seen.add(id);
      out.push({ id, place: PLACES.includes(place as ToolPlace) ? (place as ToolPlace) : "top" });
    }
  }
  defaults.forEach((slot, i) => {
    if (seen.has(slot.id)) return;
    const before = defaults[i - 1]?.id;
    const at = before ? out.findIndex((s) => s.id === before) : -1;
    out.splice(at + 1, 0, { ...slot });
    seen.add(slot.id);
  });
  return out;
}

export function setPlace(layout: ToolbarLayout, id: string, place: ToolPlace): ToolbarLayout {
  return layout.map((s) => (s.id === id ? { ...s, place } : s));
}

/** 把一个按钮挪到 `to` 的位置（拖动排序）。 */
export function moveTo(layout: ToolbarLayout, id: string, to: number): ToolbarLayout {
  const from = layout.findIndex((s) => s.id === id);
  if (from < 0) return layout;
  const next = [...layout];
  const [slot] = next.splice(from, 1);
  if (!slot) return layout;
  next.splice(Math.max(0, Math.min(to, next.length)), 0, slot);
  return next;
}

/** 某个位置上按顺序排的按钮。 */
export function toolsAt(layout: ToolbarLayout, place: ToolPlace): string[] {
  return layout.filter((s) => s.place === place).map((s) => s.id);
}

export function isDefaultLayout(toolbar: ToolbarId, layout: ToolbarLayout): boolean {
  const defaults = TOOL_DEFAULTS[toolbar];
  return layout.length === defaults.length && layout.every((s, i) => s.id === defaults[i]?.id && s.place === defaults[i]?.place);
}
