// 听新闻：念什么、念完接着念哪一篇。只有纯函数，方便测试：npm run test:ui

/** 每篇念什么。 */
export type ListenContent = "full" | "summary" | "ai" | "title";

export const LISTEN_CONTENTS: readonly ListenContent[] = ["full", "summary", "ai", "title"];

/**
 * AI 总结是写给人看的 Markdown（一句加粗的要点，再加几条列表）：去掉记号，一条一行，念起来是几句话。
 * 没有句末标点的行补一个，句子之间才有停顿。
 */
export function spokenSummary(markdown: string): string {
  return markdown
    .replace(/\r/g, "")
    .split("\n")
    .map((line) =>
      line
        .replace(/^\s{0,3}#{1,6}\s+/, "")
        .replace(/^\s*(?:[-*+•]|\d+[.)])\s+/, "")
        .replace(/\*\*|__|`/g, "")
        .replace(/(^|[^*\w])[*_]([^*_\n]+)[*_](?=[^*\w]|$)/g, "$1$2")
        .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
        .trim(),
    )
    .filter(Boolean)
    .map((line) => (/[。！？.!?；;：:]$/.test(line) ? line : `${line}${/[一-鿿]$/.test(line) ? "。" : "."}`))
    .join("\n");
}

/** 开头的几句：订阅源没给摘要时，念全文的开头代替。 */
export function firstSentences(text: string, count = 3, max = 400): string {
  const sentences = text
    .replace(/\s+/g, " ")
    .trim()
    .split(/(?<=[。！？!?])|(?<=\.)\s+/)
    .map((s) => s.trim())
    .filter(Boolean);
  let out = "";
  for (const s of sentences.slice(0, count)) {
    if (out && out.length + s.length > max) break;
    out = out ? `${out}${/[一-鿿]/.test(s) ? "" : " "}${s}` : s;
  }
  return out.length > max ? `${out.slice(0, max)}…` : out;
}

/** 列表里排在它后面的那一篇；它不在列表里、或者已经是最后一篇时没有。 */
export function nextInQueue<T extends { id: string }>(queue: readonly T[], id: string): T | undefined {
  const at = queue.findIndex((e) => e.id === id);
  return at >= 0 ? queue[at + 1] : undefined;
}

/** 每篇开头念的那一句：接着念下一篇时先报来源，听的人知道换了一篇。 */
export function heading(title: string, source: string, announce: boolean): string {
  const t = title.trim();
  const s = source.trim();
  if (!announce || !s || s === t) return t;
  return /[一-鿿]/.test(s + t) ? `${s}。${t}` : `${s}. ${t}`;
}
