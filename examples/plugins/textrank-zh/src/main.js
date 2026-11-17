// TextRank keywords and summary for Chinese articles and mail. See docs/plugins.md for the plugin API.
//
// Above the body of a Chinese article or message, shows its keywords and the few sentences that
// best sum it up, computed on this device with TextRank (textrank4zh-ts): offline, free, nothing
// leaves the computer. A reader toolbar button turns it on and off.
//
// Shows: api.ui.addReaderSection (drawing into the reader), a toolbar toggle, storage, and bundling a
// library into one file (npm run build:plugins).

import { TextRankKeyword, TextRankSentence } from "../vendor/textrank4zh-ts.mjs";
// The stop list that ships with textrank4zh-ts (data/stopwords.txt, MIT)
import PACKAGE_STOP_WORDS from "../vendor/stopwords.json";

// The library's segmenter has no part-of-speech tags (the Python original drops these words by tag),
// so common low-information words are listed here as well.
const EXTRA_STOP_WORDS = `一个 一些 一种 一样 一直 一次 一定 已经 没有 无法 不同 更多 更加 必须 主要 同时 其他 其中
这样 那样 这是 那是 什么 怎么 如何 为什么 因为 所以 但是 而且 还是 就是 不是 只要 可能 需要 进行 通过 现在 目前 时候
问题 方面 情况 东西 事情 非常 很多 许多 一下 一起 自己 他们 她们 它们 我们 你们 大家 这里 那里 这些 那些 其实 只是 然后
而是 还有 以及 之后 之前 以后 以前 能够 应该 觉得 认为 表示 成为 出现 发现 开始 继续 包括 例如 比如 使用 这种 那种
每个 所有 任何 一般 可以 不会 不能 还要 只有 都是 就会 也是 第一 第二 今天 今年 去年 明年
the and for that with this are was you not have but from they`.split(/\s+/);
const STOP_WORDS = [...new Set([...PACKAGE_STOP_WORDS, ...EXTRA_STOP_WORDS])];

/** Paragraphs that are site furniture, not content. */
const BOILERPLATE = /版权声明|转载|原文链接|文档信息|往年回顾|（完）|阅读原文|点击关注|扫码/;

const ICON = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M4 6h10M4 12h7M4 18h10"/><circle cx="18" cy="9" r="2"/><circle cx="18" cy="17" r="2"/><path d="M18 11v4"/></svg>`;

const KEYWORDS = 8;
const SENTENCES = 3;
/** Shorter texts don't need a summary. */
const MIN_CHARS = 180;
/** Very long texts are cut to keep it quick. */
const MAX_CHARS = 20000;
const CJK = /[一-鿿]/g;

/** Mostly Chinese, and long enough to be worth summing up. */
function chineseText(text) {
  const cjk = text.match(CJK)?.length ?? 0;
  return cjk >= MIN_CHARS * 0.6 && cjk / Math.max(1, text.replace(/\s/g, "").length) > 0.3;
}

/**
 * The text to analyze, one paragraph per line: the sentence splitter also breaks at line ends,
 * so headings and list items don't run into the next paragraph.
 */
function textOf(item) {
  const html = item.mail ? item.mail.body : item.full_text || item.content || item.summary || "";
  const doc = new DOMParser().parseFromString(html, "text/html");
  // Code, tables and captions aren't prose
  for (const el of doc.querySelectorAll("script, style, pre, code, noscript, figure, table")) el.remove();
  const blocks = [...doc.querySelectorAll("p, li, h1, h2, h3, h4, h5, h6, blockquote, dd")]
    .filter((el) => !el.querySelector("p, li"))
    .map((el) => el.textContent.replace(/\s+/g, " ").trim());
  // Plain-text mail and pages without paragraphs: fall back to the lines of the whole text
  const lines = blocks.length > 0 ? blocks : (doc.body?.innerText ?? doc.body?.textContent ?? "").split(/\n+/);
  return lines
    .map((line) => line.replace(/\s+/g, " ").trim())
    .filter((line) => line && !BOILERPLATE.test(line))
    .join("\n")
    .slice(0, MAX_CHARS);
}

/** Keywords and key sentences, kept per item so reopening is instant. */
const cache = new Map();

async function analyze(item, text) {
  const key = `${item.id}:${item.full_text ? "full" : "feed"}`;
  if (cache.has(key)) return cache.get(key);
  const words = new TextRankKeyword({ stopWords: STOP_WORDS });
  const sentences = new TextRankSentence({ stopWords: STOP_WORDS });
  // The async versions split the work into small slices so the window stays responsive
  const a = await words.analyzeAsync(text, { lower: true, window: 2 });
  const b = await sentences.analyzeAsync(text, { lower: true, source: "all_filters" });
  if (!a.ok) throw new Error(a.error?.message ?? "keyword analysis failed");
  if (!b.ok) throw new Error(b.error?.message ?? "sentence analysis failed");
  const result = {
    keywords: words.getKeywords(KEYWORDS, 2).map((k) => k.word),
    // Back in reading order, so the summary reads naturally
    sentences: sentences
      .getKeySentences(SENTENCES, 8)
      .sort((x, y) => x.index - y.index)
      .map((s) => s.sentence.trim()),
  };
  if (cache.size > 200) cache.delete(cache.keys().next().value);
  cache.set(key, result);
  return result;
}

const STYLE = `
.textrank-card { border: 1px solid var(--line); border-radius: 8px; padding: 10px 12px; font-size: 13px; line-height: 1.65; }
.textrank-head { display: flex; align-items: center; gap: 6px; font-size: 12px; font-weight: 500; color: var(--fg-3); margin-bottom: 6px; }
.textrank-chips { display: flex; flex-wrap: wrap; gap: 4px; margin-bottom: 8px; }
.textrank-chip { padding: 1px 8px; border-radius: 999px; background: var(--accent-soft); color: var(--accent); font-size: 12px; }
.textrank-list { margin: 0; padding-left: 18px; color: var(--fg-2); }
.textrank-list li { margin: 2px 0; }
.textrank-note { color: var(--fg-3); font-size: 12px; }
`;

export default {
  onload(api) {
    const zh = () => api.lang() === "zh";
    const text = (cn, en) => (zh() ? cn : en);
    const enabled = () => api.storage.get("enabled") !== false;

    api.ui.addStyle(STYLE);

    const section = {
      id: "summary",
      async render(item, el) {
        const body = textOf(item);
        if (!chineseText(body)) return;

        const card = document.createElement("div");
        card.className = "textrank-card";
        const head = document.createElement("div");
        head.className = "textrank-head";
        head.textContent = text("关键词与摘要 · TextRank", "Keywords and summary · TextRank");
        const note = document.createElement("div");
        note.className = "textrank-note";
        note.textContent = text("正在分析…", "Analyzing…");
        card.append(head, note);
        el.append(card);

        const { keywords, sentences } = await analyze(item, body);
        note.remove();
        if (keywords.length > 0) {
          const chips = document.createElement("div");
          chips.className = "textrank-chips";
          for (const word of keywords) {
            const chip = document.createElement("span");
            chip.className = "textrank-chip";
            chip.textContent = word;
            chips.append(chip);
          }
          card.append(chips);
        }
        if (sentences.length > 0) {
          const list = document.createElement("ul");
          list.className = "textrank-list";
          for (const sentence of sentences) {
            const li = document.createElement("li");
            li.textContent = sentence;
            list.append(li);
          }
          card.append(list);
        }
      },
    };
    // Turning it off removes the section; turning it on adds it back, and the reader redraws either way
    let removeSection = enabled() ? api.ui.addReaderSection(section) : null;

    api.ui.addToolbarButton({
      id: "toggle",
      toolbar: "reader",
      label: text("显示关键词与摘要（TextRank）", "Show keywords and summary (TextRank)"),
      icon: ICON,
      onClick() {
        const next = !enabled();
        api.storage.set("enabled", next);
        if (next && !removeSection) removeSection = api.ui.addReaderSection(section);
        if (!next && removeSection) {
          removeSection();
          removeSection = null;
        }
        api.ui.toast(next ? text("已显示关键词与摘要", "Keywords and summary on") : text("已隐藏关键词与摘要", "Keywords and summary off"));
      },
    });
  },
};
