// 阅读区的排版：字号、行距、字间距、段间距、字体和版心宽度。和主题一样只存在这台设备上，不同步。
//
// 文章和纯文本邮件用这些设置排版；HTML 邮件有自己的样式，只跟着字号整体缩放。
// 数值都用滑块调，范围和步长见下面的 READER_RANGE。
// 这里只有纯函数，方便测试：npm run test:ui

export type ReaderFont = "sans" | "serif";

export interface ReaderStyle {
  /** 字号，px。 */
  size: number;
  /** 行距，字号的倍数。 */
  leading: number;
  /** 字间距，em。 */
  tracking: number;
  /** 段落之间的空，em。 */
  spacing: number;
  font: ReaderFont;
  /** 文章的版心宽度，px；邮件再宽 40px。 */
  width: number;
}

export type ReaderNumber = Exclude<keyof ReaderStyle, "font">;

export interface Range {
  min: number;
  max: number;
  step: number;
}

export const READER_RANGE: Record<ReaderNumber, Range> = {
  size: { min: 12, max: 24, step: 1 },
  leading: { min: 1.3, max: 2.4, step: 0.01 },
  tracking: { min: -0.02, max: 0.12, step: 0.005 },
  spacing: { min: 0.4, max: 2, step: 0.05 },
  width: { min: 480, max: 1100, step: 20 },
};

export const READER_SIZE_MIN = READER_RANGE.size.min;
export const READER_SIZE_MAX = READER_RANGE.size.max;
export const READER_DEFAULT: ReaderStyle = {
  size: 15,
  leading: 1.72,
  tracking: 0,
  spacing: 1.15,
  font: "sans",
  width: 720,
};

/** 平板上的默认：拿在手里读，字大一点。 */
export const READER_TABLET: ReaderStyle = { ...READER_DEFAULT, size: 17 };

/** 这台设备的默认排版。 */
export function readerDefault(tablet: boolean): ReaderStyle {
  return tablet ? READER_TABLET : READER_DEFAULT;
}

/** 以前行距和宽度是三档选项，旧版本存下来的是这些名字。 */
const OLD_LEADING: Record<string, number> = { compact: 1.55, normal: 1.72, relaxed: 1.95 };
const OLD_WIDTH: Record<string, number> = { narrow: 600, normal: 720, wide: 900 };

/** 系统自带的衬线字体：先西文，再中文（macOS 宋体、思源宋体、Windows 宋体）。 */
const SERIF =
  '"Iowan Old Style", Charter, Georgia, "Songti SC", "Noto Serif CJK SC", "Source Han Serif SC", "Noto Serif SC", SimSun, serif';

/** 限制在范围内，并对齐到步长（顺带去掉浮点误差）。 */
export function clampTo(key: ReaderNumber, value: number): number {
  const { min, max, step } = READER_RANGE[key];
  const snapped = Math.round((value - min) / step) * step + min;
  return Math.round(Math.min(max, Math.max(min, snapped)) * 1000) / 1000;
}

export function clampSize(size: number): number {
  return clampTo("size", size);
}

function num(key: ReaderNumber, value: unknown, old?: Record<string, number>): number {
  if (typeof value === "number" && Number.isFinite(value)) return clampTo(key, value);
  if (typeof value === "string" && old?.[value] !== undefined) return old[value];
  return READER_DEFAULT[key];
}

/** 本机存的设置可能是旧版本留下的、缺字段的，补成完整的。 */
export function normalizeReaderStyle(raw: unknown): ReaderStyle {
  const r = (raw && typeof raw === "object" ? raw : {}) as Partial<Record<keyof ReaderStyle, unknown>>;
  return {
    size: num("size", r.size),
    leading: num("leading", r.leading, OLD_LEADING),
    tracking: num("tracking", r.tracking),
    spacing: num("spacing", r.spacing),
    font: r.font === "serif" ? "serif" : "sans",
    width: num("width", r.width, OLD_WIDTH),
  };
}

export function isDefaultStyle(style: ReaderStyle, base: ReaderStyle = READER_DEFAULT): boolean {
  return (Object.keys(base) as (keyof ReaderStyle)[]).every((k) => style[k] === base[k]);
}

/** 正文用的 CSS 变量，见 styles.css 里的 `.article`。 */
export function readerVars(style: ReaderStyle): Record<string, string> {
  return {
    "--reader-size": `${style.size}px`,
    "--reader-leading": String(style.leading),
    "--reader-tracking": `${style.tracking}em`,
    "--reader-spacing": `${style.spacing}em`,
    "--reader-font": style.font === "serif" ? SERIF : "var(--font-sans)",
  };
}

export function readerWidth(style: ReaderStyle, kind: "article" | "mail"): number {
  return style.width + (kind === "mail" ? 40 : 0);
}

/** 标题跟着正文一起变大变小：默认字号下文章标题 26px，邮件标题 22px。 */
export function titleSize(style: ReaderStyle, kind: "article" | "mail"): number {
  return Math.round((style.size * (kind === "mail" ? 22 : 26)) / READER_DEFAULT.size);
}

/** HTML 邮件整体缩放的比例。 */
export function mailZoom(style: ReaderStyle): number {
  return Math.round((style.size / READER_DEFAULT.size) * 1000) / 1000;
}

/** 滑块旁边显示的数值。 */
export function formatReaderValue(key: ReaderNumber, value: number): string {
  switch (key) {
    case "size":
    case "width":
      return `${value}`;
    case "leading":
      return value.toFixed(2);
    case "tracking":
      return `${value > 0 ? "+" : ""}${value.toFixed(3)}`;
    case "spacing":
      return value.toFixed(2);
  }
}
