// 阅读区的排版：字号、行距、字体和版心宽度。和主题一样只存在这台设备上，不同步。
//
// 文章和纯文本邮件用这些设置排版；HTML 邮件有自己的样式，只跟着字号整体缩放。
// 这里只有纯函数，方便测试：npm run test:ui

export type ReaderFont = "sans" | "serif";
export type ReaderLeading = "compact" | "normal" | "relaxed";
export type ReaderWidth = "narrow" | "normal" | "wide";

export interface ReaderStyle {
  size: number;
  leading: ReaderLeading;
  font: ReaderFont;
  width: ReaderWidth;
}

export const READER_SIZE_MIN = 12;
export const READER_SIZE_MAX = 24;
export const READER_DEFAULT: ReaderStyle = { size: 15, leading: "normal", font: "sans", width: "normal" };

const LEADING: Record<ReaderLeading, number> = { compact: 1.55, normal: 1.72, relaxed: 1.95 };
/** 文章的版心宽度；邮件的信头信息多一些，再宽 40px。 */
const WIDTH: Record<ReaderWidth, number> = { narrow: 600, normal: 720, wide: 900 };
/** 系统自带的衬线字体：先西文，再中文（macOS 宋体、思源宋体、Windows 宋体）。 */
const SERIF =
  '"Iowan Old Style", Charter, Georgia, "Songti SC", "Noto Serif CJK SC", "Source Han Serif SC", "Noto Serif SC", SimSun, serif';

const pick = <T extends string>(value: unknown, allowed: Record<T, unknown>, fallback: T): T =>
  typeof value === "string" && value in allowed ? (value as T) : fallback;

export function clampSize(size: number): number {
  return Math.min(READER_SIZE_MAX, Math.max(READER_SIZE_MIN, Math.round(size)));
}

/** 本机存的设置可能是旧版本留下的、缺字段的，补成完整的。 */
export function normalizeReaderStyle(raw: unknown): ReaderStyle {
  const r = (raw && typeof raw === "object" ? raw : {}) as Partial<Record<keyof ReaderStyle, unknown>>;
  return {
    size: typeof r.size === "number" && Number.isFinite(r.size) ? clampSize(r.size) : READER_DEFAULT.size,
    leading: pick(r.leading, LEADING, READER_DEFAULT.leading),
    font: pick(r.font, { sans: 1, serif: 1 }, READER_DEFAULT.font),
    width: pick(r.width, WIDTH, READER_DEFAULT.width),
  };
}

export function isDefaultStyle(style: ReaderStyle): boolean {
  return (Object.keys(READER_DEFAULT) as (keyof ReaderStyle)[]).every((k) => style[k] === READER_DEFAULT[k]);
}

/** 正文用的 CSS 变量，见 styles.css 里的 `.article`。 */
export function readerVars(style: ReaderStyle): Record<string, string> {
  return {
    "--reader-size": `${style.size}px`,
    "--reader-leading": String(LEADING[style.leading]),
    "--reader-font": style.font === "serif" ? SERIF : "var(--font-sans)",
  };
}

export function readerWidth(style: ReaderStyle, kind: "article" | "mail"): number {
  return WIDTH[style.width] + (kind === "mail" ? 40 : 0);
}

/** 标题跟着正文一起变大变小：默认字号下文章标题 26px，邮件标题 22px。 */
export function titleSize(style: ReaderStyle, kind: "article" | "mail"): number {
  return Math.round((style.size * (kind === "mail" ? 22 : 26)) / READER_DEFAULT.size);
}

/** HTML 邮件整体缩放的比例。 */
export function mailZoom(style: ReaderStyle): number {
  return Math.round((style.size / READER_DEFAULT.size) * 1000) / 1000;
}
