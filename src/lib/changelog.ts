// 更新说明：仓库里的 CHANGELOG.md，以及发布流程放进 latest.json 的那几段。
//
// 格式：每个版本一节 `## 0.1.9 — 2026-11-17`，里面分 `### English` 和 `### 中文` 两段列表。
// 「## Unreleased」是还没发布的，不算。这里只有纯函数，方便测试：npm run test:ui

import type { Lang } from "./time";

export interface ChangelogEntry {
  version: string;
  date: string | null;
  en: string;
  zh: string;
}

const VERSION_HEADING = /^##\s+v?(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)\s*(?:[—–-]+\s*(\d{4}-\d{2}-\d{2}))?\s*$/;

/** 读出每个版本的说明，按文件里的顺序（从新到旧）。 */
export function parseChangelog(text: string): ChangelogEntry[] {
  const entries: ChangelogEntry[] = [];
  let current: { version: string; date: string | null; lines: string[] } | null = null;
  const flush = () => {
    if (!current) return;
    const parts: Record<"en" | "zh" | "both", string[]> = { en: [], zh: [], both: [] };
    let target: keyof typeof parts = "both";
    for (const line of current.lines) {
      const sub = /^###\s+(.+?)\s*$/.exec(line);
      if (sub) {
        const name = (sub[1] ?? "").toLowerCase();
        target = name.startsWith("english") ? "en" : name.startsWith("中文") || name.startsWith("chinese") ? "zh" : "both";
        continue;
      }
      parts[target].push(line);
    }
    const join = (lines: string[]) => lines.join("\n").trim();
    const both = join(parts.both);
    entries.push({
      version: current.version,
      date: current.date,
      en: join(parts.en) || both,
      zh: join(parts.zh) || both,
    });
    current = null;
  };
  for (const line of text.split(/\r?\n/)) {
    if (line.startsWith("## ")) {
      flush();
      const m = VERSION_HEADING.exec(line);
      // 「Unreleased」之类不是版本号的标题：它下面的内容不收
      current = m ? { version: m[1] ?? "", date: m[2] ?? null, lines: [] } : null;
      continue;
    }
    current?.lines.push(line);
  }
  flush();
  return entries;
}

/** 比较版本号：按数字比主、次、修订号；带横线的预发布版本排在正式版之前。 */
export function compareVersions(a: string, b: string): number {
  const split = (v: string) => {
    const [core = "", pre = ""] = v.replace(/^v/, "").split("-", 2);
    return { nums: core.split(".").map((n) => Number.parseInt(n, 10) || 0), pre };
  };
  const x = split(a);
  const y = split(b);
  for (let i = 0; i < 3; i++) {
    const d = (x.nums[i] ?? 0) - (y.nums[i] ?? 0);
    if (d !== 0) return d;
  }
  if (x.pre === y.pre) return 0;
  if (!x.pre) return 1;
  if (!y.pre) return -1;
  return x.pre < y.pre ? -1 : 1;
}

/** 从 `from`（不含）到 `to`（含）之间每个版本的说明，从新到旧。跳过了好几个版本时全都列出来。 */
export function entriesBetween(entries: ChangelogEntry[], from: string | null, to: string): ChangelogEntry[] {
  return entries
    .filter((e) => compareVersions(e.version, to) <= 0 && (from === null || compareVersions(e.version, from) > 0))
    .sort((a, b) => compareVersions(b.version, a.version));
}

const CJK = /[\u3000-\u303f\u3400-\u9fff\uff00-\uffef]/;

/**
 * 文件里一条写不下时会折行，续行缩进两格。界面的 Markdown 只认一行一条，这里把续行接回去：
 * 中文接中文不加空格，其他加一个空格。
 */
export function unwrap(text: string): string {
  const out: string[] = [];
  for (const line of text.split("\n")) {
    const prev = out[out.length - 1];
    if (prev !== undefined && prev.trim() !== "" && /^\s{2,}\S/.test(line)) {
      const next = line.trim();
      const tight = CJK.test(prev.slice(-1)) && CJK.test(next.charAt(0));
      out[out.length - 1] = prev + (tight ? "" : " ") + next;
    } else {
      out.push(line);
    }
  }
  return out.join("\n");
}

/** 用界面的语言；没写这种语言时用另一种。折行的条目接成一行。 */
export function entryText(entry: ChangelogEntry, lang: Lang): string {
  return unwrap((lang === "zh" ? entry.zh || entry.en : entry.en || entry.zh).trim());
}
