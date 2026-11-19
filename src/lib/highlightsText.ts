// 划线在正文里的位置：按引文和前后几个字（W3C 的 TextQuoteSelector）记，正文排版变了也找得回来。
// 只有纯函数（不碰 DOM），方便测试：npm run test:ui

/** 前后各记多少个字。 */
export const CONTEXT = 32;

/** 选中的那一段，加上它前后的几个字。`start` 是它在整段文字里的位置。 */
export function quoteAt(text: string, start: number, end: number): { quote: string; prefix: string; suffix: string } {
  return {
    quote: text.slice(start, end),
    prefix: text.slice(Math.max(0, start - CONTEXT), start),
    suffix: text.slice(end, end + CONTEXT),
  };
}

/** 两段文字从末尾（`fromEnd`）或开头起有几个字一样。 */
function overlap(a: string, b: string, fromEnd: boolean): number {
  let n = 0;
  const len = Math.min(a.length, b.length);
  while (n < len) {
    const x = fromEnd ? a[a.length - 1 - n] : a[n];
    const y = fromEnd ? b[b.length - 1 - n] : b[n];
    if (x !== y) break;
    n++;
  }
  return n;
}

/**
 * 在整段文字里找回这段引文：出现不止一次时，挑前后文最吻合的那一处。找不到返回 -1。
 * 空白多了少了不影响（正文里的换行、缩进常常变）。
 */
export function locate(text: string, quote: string, prefix = "", suffix = ""): number {
  if (!quote) return -1;
  const hits: number[] = [];
  for (let at = text.indexOf(quote); at !== -1; at = text.indexOf(quote, at + 1)) hits.push(at);
  if (hits.length === 0) {
    // 空白不一样：都压成一个空格再找，再换算回原来的位置
    const squash = (s: string) => s.replace(/\s+/g, " ");
    const map: number[] = [];
    let flat = "";
    for (let i = 0; i < text.length; i++) {
      const c = text[i] as string;
      if (/\s/.test(c)) {
        if (flat.endsWith(" ")) continue;
        flat += " ";
      } else flat += c;
      map.push(i);
    }
    const at = flat.indexOf(squash(quote).trim());
    return at === -1 ? -1 : (map[at] ?? -1);
  }
  if (hits.length === 1) return hits[0] as number;
  let best = hits[0] as number;
  let score = -1;
  for (const at of hits) {
    const s = overlap(text.slice(0, at), prefix, true) + overlap(text.slice(at + quote.length), suffix, false);
    if (s > score) {
      score = s;
      best = at;
    }
  }
  return best;
}

/** 一段划线导出成 Markdown 的引用，带上笔记。 */
export function toMarkdown(
  groups: { title: string; url: string | null; source: string; highlights: { quote: string; note: string }[] }[],
): string {
  return groups
    .map((g) => {
      const head = g.url ? `## [${g.title}](${g.url})` : `## ${g.title}`;
      const lines = [head];
      if (g.source) lines.push(`*${g.source}*`);
      for (const h of g.highlights) {
        lines.push("", ...h.quote.trim().split("\n").map((l) => `> ${l}`));
        if (h.note.trim()) lines.push("", h.note.trim());
      }
      return lines.join("\n");
    })
    .join("\n\n");
}
