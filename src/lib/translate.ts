// 离线翻译：把正文按段落交给核心翻译，再按原来的结构放回去。
//
// 每个段落拆成纯文字，加上里面链接、粗体这些行内元素覆盖的范围。核心翻译时会算出这些元素在译文里
// 落在哪，这里照着重新包上原来的元素（属性、样式都保留），所以链接在译文里还能点。

import { useQuery } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { create } from "zustand";
import { api, type SourceSegment, type TranslatePlan, type TranslatedSegment } from "./api";
import { useLang } from "./i18n";
import { useUi } from "./store";
import type { Lang } from "./time";
import { RpcError } from "./transport";

// ---- 语言 ----

/** 模型目录里的语言代码换成浏览器认识的语言标签。 */
function bcp47(code: string): string {
  switch (code) {
    case "zh":
      return "zh-Hans";
    case "zh_hant":
      return "zh-Hant";
    case "hbs":
      return "sh";
    default:
      return code.replace("_", "-");
  }
}

/** 语言的显示名称，用界面语言写。 */
export function languageName(code: string, lang: Lang): string {
  try {
    const names = new Intl.DisplayNames([lang === "zh" ? "zh-CN" : "en"], { type: "language" });
    return names.of(bcp47(code)) ?? code;
  } catch {
    return code;
  }
}

/** 翻成哪种语言：设置里选过就用它，否则跟界面语言一致。 */
export function useTranslateTarget(): string {
  const chosen = useUi((s) => s.translateTo);
  const lang = useLang();
  return chosen ?? lang;
}

export function useTranslateLanguages(enabled = true) {
  return useQuery({
    queryKey: ["translate", "languages"],
    queryFn: api.translateLanguages,
    enabled,
    staleTime: 5 * 60_000,
  });
}

// ---- 模型下载进度，来自核心的事件 ----

export interface DownloadState {
  received: number;
  total: number;
  state: "downloading" | "installed" | "failed";
  error?: string;
}

export const useModelDownloads = create<{
  byPair: Record<string, DownloadState>;
  update: (pair: string, state: DownloadState) => void;
}>()((set) => ({
  byPair: {},
  update: (pair, state) => set((s) => ({ byPair: { ...s.byPair, [pair]: state } })),
}));

// ---- 把 HTML 拆成段落，再放回去 ----

const BLOCK = new Set([
  "ADDRESS", "ARTICLE", "ASIDE", "BLOCKQUOTE", "BODY", "CAPTION", "CENTER", "DD", "DETAILS", "DIALOG", "DIV",
  "DL", "DT", "FIELDSET", "FIGCAPTION", "FIGURE", "FOOTER", "FORM", "H1", "H2", "H3", "H4", "H5", "H6",
  "HEADER", "HGROUP", "HR", "LEGEND", "LI", "MAIN", "MENU", "NAV", "OL", "P", "SECTION", "SUMMARY", "TABLE",
  "TBODY", "TD", "TFOOT", "TH", "THEAD", "TR", "UL",
]);

/** 不翻译的内容：代码、样式、表单控件之类。 */
const SKIP = new Set([
  "PRE", "CODE", "KBD", "SAMP", "VAR", "SCRIPT", "STYLE", "NOSCRIPT", "TEMPLATE", "TEXTAREA", "SELECT",
  "SVG", "MATH", "HEAD", "TITLE",
]);

const TEXT_NODE = 3;
const ELEMENT_NODE = 1;

/** 一个段落：原来的那串相邻节点，交给核心的文字，以及每个标记对应的元素。 */
interface Run {
  nodes: Node[];
  segment: SourceSegment;
  elements: Element[];
  leadingSpace: boolean;
  trailingSpace: boolean;
  /** 纯文本邮件里引用行开头的 `> `，不交给模型，翻完放回去。 */
  prefix: string;
}

const hasText = (el: Element) => /\S/.test(el.textContent ?? "");

/** 里面有图片、换行这类没有文字的元素。这样的行内元素不能整个换成文字，要钻进去分别翻。 */
function hasBarrier(el: Element): boolean {
  for (const child of Array.from(el.querySelectorAll("*"))) {
    if (child.tagName === "BR" || !hasText(child)) return true;
  }
  return false;
}

function isBlockLike(el: Element): boolean {
  if (BLOCK.has(el.tagName)) return true;
  // 行内元素里包着块级元素（邮件里常见：链接包住整个按钮）
  return Array.from(el.querySelectorAll("*")).some((c) => BLOCK.has(c.tagName));
}

/** 只是一个网址或邮箱地址的段落，翻了反而会坏掉。 */
const ONLY_ADDRESS = /^(?:(?:https?:\/\/|www\.)\S+|[^\s@]+@[^\s@]+\.[^\s@]+)$/i;

function makeRun(nodes: Node[]): Run | null {
  let raw = "";
  const marks: [number, number][] = [];
  const elements: Element[] = [];
  for (const node of nodes) {
    let piece = (node.textContent ?? "").replace(/\s+/g, " ");
    // 节点交界处的空白也只留一个
    if (raw.endsWith(" ") && piece.startsWith(" ")) piece = piece.slice(1);
    const start = raw.length;
    raw += piece;
    if (node.nodeType === ELEMENT_NODE) {
      marks.push([start, raw.length]);
      elements.push(node as Element);
    }
  }
  const lead = raw.length - raw.trimStart().length;
  const trimmed = raw.trim();
  const prefix = /^(?:>\s?)+/.exec(trimmed)?.[0] ?? "";
  const text = trimmed.slice(prefix.length);
  if (!/\p{L}/u.test(text) || ONLY_ADDRESS.test(text)) return null;
  const clamp = (n: number) => Math.min(text.length, Math.max(0, n - lead - prefix.length));
  return {
    nodes,
    segment: { text, marks: marks.map(([s, e]) => [clamp(s), clamp(e)]) },
    elements,
    leadingSpace: lead > 0,
    trailingSpace: raw.length > lead + trimmed.length,
    prefix,
  };
}

/**
 * 换行要保留的容器：纯文本邮件（核心生成的 `mail-text`，按 pre-wrap 显示），以及写明了
 * `white-space: pre…` 的元素。解析出来的文档没有渲染，拿不到计算后的样式，只能这样认。
 */
function keepsLines(el: Element): boolean {
  return el.classList.contains("mail-text") || /white-space\s*:\s*pre/i.test(el.getAttribute("style") ?? "");
}

/** 把直接包含的文字节点在换行处切开，换行符单独成一个节点。 */
function splitLines(el: Element) {
  for (const child of Array.from(el.childNodes)) {
    if (child.nodeType !== TEXT_NODE) continue;
    let node = child as Text;
    for (;;) {
      const i = node.data.indexOf("\n");
      if (i === -1) break;
      const rest = node.splitText(i);
      node = rest.data.length > 1 ? rest.splitText(1) : rest;
      if (node === rest) break;
    }
  }
}

const isNewline = (node: Node) => node.nodeType === TEXT_NODE && (node as Text).data === "\n";

/** 收集 `root` 下面所有要翻译的段落。块级元素各自成段，`<br>` 和图片把一段再切开。 */
export function collectRuns(root: Element, runs: Run[] = [], lines = false): Run[] {
  lines = lines || keepsLines(root);
  if (lines) splitLines(root);
  let current: Node[] = [];
  const flush = () => {
    const run = current.length > 0 ? makeRun(current) : null;
    if (run) runs.push(run);
    current = [];
  };
  for (const child of Array.from(root.childNodes)) {
    if (lines && isNewline(child)) {
      // 保留换行的容器里，每一行各翻各的，换行符留在原处
      flush();
      continue;
    }
    if (child.nodeType === TEXT_NODE) {
      current.push(child);
      continue;
    }
    if (child.nodeType !== ELEMENT_NODE) continue;
    const el = child as Element;
    if (SKIP.has(el.tagName)) {
      flush();
    } else if (isBlockLike(el) || (hasText(el) && hasBarrier(el)) || (lines && el.textContent?.includes("\n"))) {
      flush();
      collectRuns(el, runs, lines);
    } else if (!hasText(el)) {
      // 图片、换行留在原处，前后各成一段
      flush();
    } else {
      current.push(el);
    }
  }
  flush();
  return runs;
}

/** 复制一个元素的外壳。只有一个子元素一路包下去时（`<a><span style>…</span></a>`），整串外壳都复制。 */
function shell(el: Element): { outer: Element; inner: Element } {
  const outer = el.cloneNode(false) as Element;
  let inner = outer;
  let source = el;
  for (;;) {
    const kids = Array.from(source.childNodes).filter(
      (n) => !(n.nodeType === TEXT_NODE && !/\S/.test(n.textContent ?? "")),
    );
    const only = kids[0];
    if (kids.length !== 1 || only?.nodeType !== ELEMENT_NODE) break;
    source = only as Element;
    const copy = source.cloneNode(false) as Element;
    inner.appendChild(copy);
    inner = copy;
  }
  return { outer, inner };
}

/** 用译文换掉一个段落原来的节点。 */
function applyRun(run: Run, out: TranslatedSegment) {
  const first = run.nodes[0];
  const parent = first?.parentNode;
  const doc = first?.ownerDocument;
  if (!first || !parent || !doc) return;
  const space = run.leadingSpace ? " " : "";
  const text = space + run.prefix + out.text + (run.trailingSpace ? " " : "");
  const shift = space.length + run.prefix.length;
  const fragment = doc.createDocumentFragment();
  let pos = 0;
  for (const [start, end, id] of [...out.marks].sort((a, b) => a[0] - b[0])) {
    const el = run.elements[id];
    // 从段首开始的元素把引用前缀也包进去
    const [s, e] = [start === 0 ? space.length : start + shift, end + shift];
    if (!el || s < pos || e <= s) continue;
    if (s > pos) fragment.append(text.slice(pos, s));
    const { outer, inner } = shell(el);
    inner.textContent = text.slice(s, e);
    fragment.append(outer);
    pos = e;
  }
  if (pos < text.length) fragment.append(text.slice(pos));
  parent.insertBefore(fragment, first);
  for (const node of run.nodes) node.parentNode?.removeChild(node);
}

/** 要翻译的正文。邮件是完整的 HTML 文档，文章和纯文本邮件是一段 HTML 片段。 */
export interface TranslateSource {
  title: string;
  html: string;
  document: boolean;
}

function parse(source: TranslateSource): Document {
  const html = source.document ? source.html : `<!doctype html><html><head></head><body>${source.html}</body></html>`;
  return new DOMParser().parseFromString(html, "text/html");
}

/** 用来识别语言的一段文字：标题加正文开头。 */
function sampleOf(source: TranslateSource): string {
  const body = (parse(source).body?.textContent ?? "").replace(/\s+/g, " ").trim();
  return `${source.title}\n${body.slice(0, 3000)}`;
}

export async function translateSource(source: TranslateSource, from: string, to: string) {
  const doc = parse(source);
  const runs = doc.body ? collectRuns(doc.body) : [];
  const { segments } = await api.translateText(from, to, [
    { text: source.title, marks: [] },
    ...runs.map((r) => r.segment),
  ]);
  const [title, ...rest] = segments;
  rest.forEach((out, i) => {
    const run = runs[i];
    if (run) applyRun(run, out);
  });
  doc.documentElement.lang = bcp47(to);
  const html = source.document ? `<!doctype html>\n${doc.documentElement.outerHTML}` : doc.body.innerHTML;
  return { title: title?.text || source.title, html };
}

// ---- 阅读面板用的状态 ----

export type TranslationPhase =
  | { kind: "off" }
  | { kind: "checking" }
  /** 缺模型，等用户同意下载。 */
  | { kind: "need"; plan: TranslatePlan }
  | { kind: "downloading"; plan: TranslatePlan }
  | { kind: "translating" }
  | { kind: "on"; from: string; title: string; html: string }
  /** 原文已经是目标语言。 */
  | { kind: "same"; from: string }
  | { kind: "error"; error: unknown };

interface Done {
  from: string;
  to: string;
  forHtml: string;
  title: string;
  html: string;
}

export function useTranslation(source: TranslateSource | null) {
  const to = useTranslateTarget();
  const [phase, setPhase] = useState<TranslationPhase>({ kind: "off" });
  const done = useRef<Done | null>(null);
  // 每次开始翻译加一，过时的结果直接丢掉
  const generation = useRef(0);
  const latest = useRef(source);
  latest.current = source;

  const translate = useCallback(
    async (from: string) => {
      const src = latest.current;
      if (!src) return;
      const gen = ++generation.current;
      setPhase({ kind: "translating" });
      try {
        const out = await translateSource(src, from, to);
        if (gen !== generation.current) return;
        done.current = { from, to, forHtml: src.html, ...out };
        setPhase({ kind: "on", from, ...out });
      } catch (error) {
        if (gen === generation.current) setPhase({ kind: "error", error });
      }
    },
    [to],
  );

  // 正文还没加载完就按了翻译，等它到了再开始
  const pending = useRef(false);

  const start = useCallback(async () => {
    const src = latest.current;
    if (!src) {
      pending.current = true;
      setPhase({ kind: "checking" });
      return;
    }
    pending.current = false;
    const cached = done.current;
    if (cached && cached.forHtml === src.html && cached.to === to) {
      setPhase({ kind: "on", from: cached.from, title: cached.title, html: cached.html });
      return;
    }
    const gen = ++generation.current;
    setPhase({ kind: "checking" });
    try {
      const plan = await api.translatePlan(sampleOf(src), to);
      if (gen !== generation.current) return;
      if (!plan.from) {
        setPhase({ kind: "error", error: new RpcError("translate_unknown_language", "") });
      } else if (plan.same) {
        setPhase({ kind: "same", from: plan.from });
      } else if (plan.missing.length > 0) {
        setPhase({ kind: "need", plan });
      } else {
        await translate(plan.from);
      }
    } catch (error) {
      if (gen === generation.current) setPhase({ kind: "error", error });
    }
  }, [to, translate]);

  const download = useCallback((plan: TranslatePlan) => {
    const { update } = useModelDownloads.getState();
    for (const m of plan.missing) update(m.pair, { received: 0, total: 0, state: "downloading" });
    setPhase({ kind: "downloading", plan });
    api.downloadModels(plan.missing.map((m) => m.pair)).catch((error) => setPhase({ kind: "error", error }));
  }, []);

  // 下载完了接着翻译；有一个失败就报错
  const downloads = useModelDownloads((s) => s.byPair);
  useEffect(() => {
    if (phase.kind !== "downloading") return;
    const states = phase.plan.missing.map((m) => downloads[m.pair]);
    const failed = states.find((s) => s?.state === "failed");
    if (failed) {
      setPhase({ kind: "error", error: new RpcError(failed.error ?? "translate_download_failed", "") });
    } else if (states.every((s) => s?.state === "installed") && phase.plan.from) {
      void translate(phase.plan.from);
    }
  }, [downloads, phase, translate]);

  useEffect(() => {
    if (source && pending.current) void start();
  }, [source, start]);

  // 正文变了（显示了图片、换成全文）而译文还开着，就重新翻译
  useEffect(() => {
    if (phase.kind === "on" && source && done.current && done.current.forHtml !== source.html) {
      void start();
    }
  }, [phase.kind, source, start]);

  // 换了目标语言而译文还开着
  useEffect(() => {
    if (phase.kind === "on" && done.current && done.current.to !== to) void start();
  }, [phase.kind, start, to]);

  const off = useCallback(() => {
    generation.current++;
    pending.current = false;
    setPhase({ kind: "off" });
  }, []);

  /** 翻译按钮和 T 键：没开就开始，开着就回到原文。 */
  const toggle = useCallback(() => {
    if (phase.kind === "off" || phase.kind === "error" || phase.kind === "same") void start();
    else if (phase.kind === "checking" && pending.current) off();
    else if (phase.kind === "need") download(phase.plan);
    else if (phase.kind === "on") off();
  }, [download, off, phase, start]);

  return { phase, to, toggle, download, off, retry: start };
}
