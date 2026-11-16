// 邮箱服务商的图标。
//
// 有开源图标的用 Simple Icons（CC0 协议）里的品牌图形；没有的（比如微软、网易，它们不允许或没有收录）
// 用品牌色加一两个字做成方块，一眼也能认出来。

import { Mail } from "lucide-react";
import {
  siAlibabacloud,
  siGmail,
  siGmx,
  siGoogle,
  siIcloud,
  siMaildotru,
  siQq,
  siZoho,
  type SimpleIcon,
} from "simple-icons";
import type { Account, Preset } from "../lib/api";

type Glyph = { kind: "logo"; path: string; color: string } | { kind: "text"; text: string; color: string };

const logo = (icon: SimpleIcon): Glyph => ({ kind: "logo", path: icon.path, color: `#${icon.hex}` });
const text = (t: string, color: string): Glyph => ({ kind: "text", text: t, color });

const GLYPHS: Record<string, Glyph> = {
  gmail: logo(siGmail),
  workspace: logo(siGoogle),
  icloud: logo(siIcloud),
  icloud_custom: logo(siIcloud),
  qq: logo(siQq),
  zoho: logo(siZoho),
  zoho_custom: logo(siZoho),
  gmx: logo(siGmx),
  mailru: logo(siMaildotru),
  aliyun: logo(siAlibabacloud),
  qiye_aliyun: logo(siAlibabacloud),
  outlook: text("O", "#0F6CBD"),
  m365: text("M", "#0F6CBD"),
  yahoo: text("Y!", "#6001D2"),
  "163": text("163", "#D43C33"),
  "126": text("126", "#1E8E5A"),
  yeah: text("yeah", "#D43C33"),
  "188": text("188", "#B8862B"),
  qiye163: text("网易", "#D43C33"),
  exmail: text("腾", "#0052D9"),
  feishu: text("飞", "#3370FF"),
  lark: text("L", "#3370FF"),
  sina: text("新", "#E6162D"),
  sohu: text("搜", "#E8A317"),
  "139": text("139", "#0085D0"),
  fastmail: text("F", "#0067B9"),
  fastmail_custom: text("F", "#0067B9"),
  yandex: text("Я", "#FC3F1D"),
  aol: text("Aol", "#2D2D2D"),
  "263": text("263", "#E60012"),
};

export function hasProviderIcon(id: string | null | undefined): boolean {
  return Boolean(id && GLYPHS[id]);
}

/** 圆角方块里的服务商图标。不认识的服务商显示一个通用的信封。 */
export function ProviderIcon({ id, size = 16 }: { id: string | null | undefined; size?: number }) {
  const glyph = id ? GLYPHS[id] : undefined;
  const radius = Math.max(3, Math.round(size * 0.24));
  const box = { width: size, height: size, borderRadius: radius };

  if (!glyph) {
    return (
      <span
        aria-hidden
        className="inline-flex shrink-0 items-center justify-center bg-active text-fg-2"
        style={box}
      >
        <Mail size={Math.round(size * 0.62)} strokeWidth={1.75} />
      </span>
    );
  }
  if (glyph.kind === "logo") {
    // 品牌图形放在白底上，深浅主题里都清楚
    return (
      <span
        aria-hidden
        className="inline-flex shrink-0 items-center justify-center bg-white ring-1 ring-inset ring-black/10"
        style={box}
      >
        <svg viewBox="0 0 24 24" width={Math.round(size * 0.66)} height={Math.round(size * 0.66)}>
          <path d={glyph.path} fill={glyph.color} />
        </svg>
      </span>
    );
  }
  const chars = Array.from(glyph.text).length;
  const fontSize = Math.round(size * (chars >= 4 ? 0.3 : chars === 3 ? 0.36 : chars === 2 ? 0.44 : 0.56));
  return (
    <span
      aria-hidden
      className="inline-flex shrink-0 items-center justify-center font-bold leading-none text-white"
      style={{ ...box, background: glyph.color, fontSize, letterSpacing: "-0.02em" }}
    >
      {glyph.text}
    </span>
  );
}

/** 已添加的邮箱属于哪个服务商：先按服务器地址认，认不出再按邮箱域名。 */
export function providerOf(account: Pick<Account, "email" | "imap_host">, presets: Preset[] | undefined): string | null {
  if (!presets) return null;
  const host = account.imap_host.trim().toLowerCase();
  const domain = account.email.split("@")[1]?.toLowerCase() ?? "";
  const byHost = presets.filter((p) => p.imap_host === host);
  const byDomain = presets.find((p) => p.domains.includes(domain));
  // 同一台服务器可能对应好几个预设（Gmail 和 Google Workspace），用域名挑更贴切的那个
  const pick = byHost.find((p) => p.domains.includes(domain)) ?? byHost.find((p) => p.domains.length === 0) ?? byHost[0];
  return (pick ?? byDomain)?.id ?? null;
}

const ENGLISH_NAMES: Record<string, string> = {
  qq: "QQ Mail",
  "163": "NetEase 163",
  "126": "NetEase 126",
  yeah: "NetEase Yeah",
  "188": "NetEase 188",
  sina: "Sina Mail",
  sohu: "Sohu Mail",
  "139": "139 Mail",
  aliyun: "Aliyun Mail",
  exmail: "Tencent Exmail",
  qiye163: "NetEase Enterprise",
  qiye_aliyun: "Alibaba Mail",
  feishu: "Feishu Mail",
  "263": "263 Mail",
};

/** 服务商的显示名称。中文服务商在英文界面里用英文名。 */
export function providerName(preset: Pick<Preset, "id" | "name">, lang: "zh" | "en"): string {
  return lang === "en" ? (ENGLISH_NAMES[preset.id] ?? preset.name) : preset.name;
}
