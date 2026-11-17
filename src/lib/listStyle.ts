// 中间一栏列表的样式：字号、行间距、内边距，显示哪些部分。和阅读设置一样只存在这台设备上。
//
// 列表是虚拟滚动的，每一行的高度要事先算出来，所以行高都由这里的数字决定，不靠浏览器排版。
// 这里只有纯函数，方便测试：npm run test:ui

export interface ListStyle {
  /** 标题字号。摘要和卡片视图里的标题再大 1px。 */
  titleSize: number;
  /** 摘要和来源那一行的字号。 */
  summarySize: number;
  /** 行与行之间的空隙。 */
  gap: number;
  /** 每一行里面的上下留白；左右留白比它多 2px。 */
  padding: number;
  /** 摘要视图里摘要显示几行。 */
  summaryLines: number;
  showSummary: boolean;
  showIcon: boolean;
  /** 摘要视图的缩略图和卡片的封面图。 */
  showImage: boolean;
}

export const LIST_STYLE_DEFAULT: ListStyle = {
  titleSize: 13,
  summarySize: 12,
  gap: 1,
  padding: 8,
  summaryLines: 2,
  showSummary: true,
  showIcon: true,
  showImage: true,
};

/** 每一项能调的范围。 */
export const LIST_STYLE_RANGE = {
  titleSize: [11, 18],
  summarySize: [10, 16],
  gap: [0, 12],
  padding: [2, 16],
  summaryLines: [1, 4],
} as const satisfies Partial<Record<keyof ListStyle, readonly [number, number]>>;

type Ranged = keyof typeof LIST_STYLE_RANGE;

const clampTo = (key: Ranged, value: number) => {
  const [min, max] = LIST_STYLE_RANGE[key];
  return Math.min(max, Math.max(min, Math.round(value)));
};

/** 本机存的设置可能是旧版本留下的、缺字段的，补成完整的。 */
export function normalizeListStyle(raw: unknown): ListStyle {
  const r = (raw && typeof raw === "object" ? raw : {}) as Partial<Record<keyof ListStyle, unknown>>;
  const num = (key: Ranged) => {
    const v = r[key];
    return typeof v === "number" && Number.isFinite(v) ? clampTo(key, v) : LIST_STYLE_DEFAULT[key];
  };
  const bool = (key: "showSummary" | "showIcon" | "showImage") =>
    typeof r[key] === "boolean" ? (r[key] as boolean) : LIST_STYLE_DEFAULT[key];
  return {
    titleSize: num("titleSize"),
    summarySize: num("summarySize"),
    gap: num("gap"),
    padding: num("padding"),
    summaryLines: num("summaryLines"),
    showSummary: bool("showSummary"),
    showIcon: bool("showIcon"),
    showImage: bool("showImage"),
  };
}

export function isDefaultListStyle(style: ListStyle): boolean {
  return (Object.keys(LIST_STYLE_DEFAULT) as (keyof ListStyle)[]).every((k) => style[k] === LIST_STYLE_DEFAULT[k]);
}

/** 一行文字占的高度。 */
export function lineHeight(size: number): number {
  return Math.round(size * 1.4);
}

/** 摘要视图里缩略图的边长。 */
export const THUMB_SIZE = 72;
/** 卡片上方来源那一行的高度。 */
const CARD_SOURCE = 20;
/** 摘要视图和卡片里，标题、摘要、来源之间的空隙。 */
const STACK_GAP = 4;
const CARD_STACK_GAP = 6;

export interface ListMetrics {
  title: { size: number; line: number };
  summary: { size: number; line: number };
  /** 摘要视图和卡片里的标题。 */
  bigTitle: { size: number; line: number };
  padX: number;
  padY: number;
  gap: number;
  /** 每种行的高度，不含行间距。 */
  compact: number;
  comfortable: number;
  summaryRow: number;
  /** 卡片下半部分文字区的高度。 */
  cardText: number;
  /** 卡片之间的空隙。 */
  cardGap: number;
}

export function listMetrics(style: ListStyle): ListMetrics {
  const title = { size: style.titleSize, line: lineHeight(style.titleSize) };
  const summary = { size: style.summarySize, line: lineHeight(style.summarySize) };
  const bigTitle = { size: style.titleSize + 1, line: lineHeight(style.titleSize + 1) };
  const padY = style.padding;
  // 来源那一行里有 14px 的图标，比字矮的时候以图标为准
  const sourceLine = Math.max(summary.line, style.showIcon ? 16 : 0);

  const compact = Math.max(title.line, style.showIcon ? 16 : 0) + padY * 2;
  const comfortable = title.line + STACK_GAP + sourceLine + padY * 2;

  const summaryText =
    bigTitle.line +
    (style.showSummary ? STACK_GAP + summary.line * style.summaryLines : 0) +
    STACK_GAP +
    sourceLine;
  const summaryRow = Math.max(summaryText, style.showImage ? THUMB_SIZE : 0) + (padY + 2) * 2;

  const cardText =
    (padY + 2) * 2 +
    CARD_SOURCE +
    CARD_STACK_GAP +
    bigTitle.line * 2 +
    (style.showSummary ? CARD_STACK_GAP + summary.line * 2 : 0);

  return {
    title,
    summary,
    bigTitle,
    padX: style.padding + 2,
    padY,
    gap: style.gap,
    compact,
    comfortable,
    summaryRow,
    cardText,
    cardGap: 9 + style.gap,
  };
}
