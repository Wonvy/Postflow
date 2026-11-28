// 主题色（界面里的强调色）：几种预设，也可以自己挑一个颜色。只存在这台设备上，和主题一样。
//
// 每种颜色在浅色、深色界面各用一个深浅：浅色界面用深一点的，白底上的字看得清；深色界面用亮一点的。
// 自己挑的颜色按这个规则自动调深浅，保证文字和背景的对比度够（至少 4.5:1）。
// 这里只有纯函数，方便测试：npm run test:ui

export interface AccentPreset {
  id: string;
  light: string;
  dark: string;
}

/** 预设。第一个（Postflow 紫，和 Logo 一样）是默认。 */
export const ACCENT_PRESETS: AccentPreset[] = [
  { id: "postflow", light: "#7533e8", dark: "#a25bff" },
  { id: "indigo", light: "#5661c8", dark: "#7b86e2" },
  { id: "blue", light: "#2563eb", dark: "#60a5fa" },
  { id: "sky", light: "#0277b3", dark: "#38bdf8" },
  { id: "teal", light: "#0f766e", dark: "#2dd4bf" },
  { id: "green", light: "#15803d", dark: "#4ade80" },
  { id: "orange", light: "#c2410c", dark: "#fb923c" },
  { id: "pink", light: "#be185d", dark: "#f472b6" },
  { id: "red", light: "#c62828", dark: "#f87171" },
  { id: "graphite", light: "#4b5563", dark: "#9ca3af" },
];

export const DEFAULT_ACCENT = "postflow";
/** 纸张质感主题默认使用晴空蓝。 */
export const PAPER_DEFAULT_ACCENT = "sky";

/** 浅色、深色、纸张界面的背景（和 styles.css 里的 --bg / --bg-sidebar 一样），用来算对比度。 */
const LIGHT_BG = "#ffffff";
const LIGHT_SIDEBAR = "#f7f7f8";
const PAPER_BG = "#f1ece2";
const PAPER_SIDEBAR = "#eae3d7";
const DARK_BG = "#0f1012";
const MIN_CONTRAST = 4.5;

type Rgb = [number, number, number];

export function parseHex(hex: string): Rgb | null {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m?.[1]) return null;
  const n = Number.parseInt(m[1], 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

const toHex = ([r, g, b]: Rgb) => `#${[r, g, b].map((c) => Math.round(c).toString(16).padStart(2, "0")).join("")}`;

function luminance([r, g, b]: Rgb): number {
  const f = (c: number) => {
    const v = c / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
}

export function contrast(a: string, b: string): number {
  const ra = parseHex(a);
  const rb = parseHex(b);
  if (!ra || !rb) return 0;
  const [hi, lo] = [luminance(ra), luminance(rb)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}

function toHsl([r, g, b]: Rgb): [number, number, number] {
  const [rr, gg, bb] = [r / 255, g / 255, b / 255];
  const max = Math.max(rr, gg, bb);
  const min = Math.min(rr, gg, bb);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  const h =
    max === rr ? ((gg - bb) / d + (gg < bb ? 6 : 0)) / 6 : max === gg ? ((bb - rr) / d + 2) / 6 : ((rr - gg) / d + 4) / 6;
  return [h, s, l];
}

function fromHsl([h, s, l]: [number, number, number]): Rgb {
  if (s === 0) return [l * 255, l * 255, l * 255];
  const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
  const p = 2 * l - q;
  const ch = (t: number) => {
    let x = t;
    if (x < 0) x += 1;
    if (x > 1) x -= 1;
    if (x < 1 / 6) return p + (q - p) * 6 * x;
    if (x < 1 / 2) return q;
    if (x < 2 / 3) return p + (q - p) * (2 / 3 - x) * 6;
    return p;
  };
  return [ch(h + 1 / 3) * 255, ch(h) * 255, ch(h - 1 / 3) * 255];
}

/** 改亮度，直到在这些背景上的对比度都够。`step` 正数变亮，负数变暗。 */
function adjust(hex: string, backgrounds: string[], step: number): string {
  const rgb = parseHex(hex);
  if (!rgb) return hex;
  const [h, s, start] = toHsl(rgb);
  let l = start;
  for (let i = 0; i < 100; i++) {
    const out = toHex(fromHsl([h, s, l]));
    if (backgrounds.every((bg) => contrast(out, bg) >= MIN_CONTRAST)) return out;
    l = Math.min(1, Math.max(0, l + step));
  }
  return toHex(fromHsl([h, s, l]));
}

function shade(hex: string, by: number): string {
  const rgb = parseHex(hex);
  if (!rgb) return hex;
  const [h, s, l] = toHsl(rgb);
  return toHex(fromHsl([h, s, Math.min(1, Math.max(0, l + by))]));
}

export interface AccentVars {
  accent: string;
  /** 按下、悬停时深一点的。 */
  strong: string;
  /** 淡淡的底色。 */
  soft: string;
}

export type ResolvedTheme = "light" | "dark" | "paper";

/** 选的主题色（预设的 ID，或者 #rrggbb）在这个界面主题下的几个颜色。认不出的当作默认。 */
export function accentVars(choice: string, theme: ResolvedTheme): AccentVars {
  const polarity = theme === "dark" ? "dark" : "light";
  const preset = ACCENT_PRESETS.find((p) => p.id === choice);
  const base = preset
    ? preset[polarity]
    : parseHex(choice)
      ? choice.toLowerCase()
      : (ACCENT_PRESETS[0] as AccentPreset)[polarity];
  const accent =
    theme === "paper"
      ? adjust(base, [PAPER_BG, PAPER_SIDEBAR], -0.01)
      : preset
        ? base
        : theme === "light"
          ? adjust(base, [LIGHT_BG, LIGHT_SIDEBAR], -0.01)
          : adjust(base, [DARK_BG], 0.01);
  const rgb = parseHex(accent) ?? [0, 0, 0];
  return {
    accent,
    strong: shade(accent, -0.06),
    soft: `rgb(${rgb.join(" ")} / ${theme === "dark" ? 0.16 : theme === "paper" ? 0.11 : 0.1})`,
  };
}

/** 选的是哪一个：预设的 ID，或者自己挑的颜色时是 "custom"。 */
export function accentKind(choice: string): string {
  return ACCENT_PRESETS.some((p) => p.id === choice) ? choice : parseHex(choice) ? "custom" : DEFAULT_ACCENT;
}
