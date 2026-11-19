// 划线和笔记：在文章或邮件正文里选中一段，点浮出来的「划线」或「笔记」记下来。
// HTML 邮件在沙箱 iframe 里，划线包在 iframe 的文档里，浮出的按钮按 iframe 的位置和缩放换算。
// 划线存在同步日志里（核心的 highlights.*），所有设备都有；位置按引文和前后文记（见 lib/highlightsText.ts）。
// 正文下面列出这篇的全部划线，可以写笔记、删除、复制成 Markdown。

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Copy, Highlighter, NotebookPen, Trash2 } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { api, type Highlight, type ItemDetail } from "../lib/api";
import { locate, quoteAt, toMarkdown } from "../lib/highlightsText";
import { errorText, useT } from "../lib/i18n";
import { copyText } from "../lib/platform";
import { toast } from "./Toast";
import { cx } from "./ui";

const MARK = "pf-hl";

/** 正文里一个位置之前有多少个字（和 textContent 一样数）。 */
function offsetIn(root: HTMLElement, node: Node, offset: number): number {
  const range = root.ownerDocument.createRange();
  range.setStart(root, 0);
  range.setEnd(node, offset);
  return range.toString().length;
}

/** 去掉以前包上的划线标记。 */
function unwrap(root: HTMLElement) {
  for (const mark of Array.from(root.querySelectorAll(`mark.${MARK}`))) {
    mark.replaceWith(...Array.from(mark.childNodes));
  }
  root.normalize();
}

/** 把 [start, end) 这一段文字包上划线标记。跨了好几个元素的，每一小段各包一个。 */
function wrap(root: HTMLElement, start: number, end: number, id: string, note: boolean) {
  const doc = root.ownerDocument;
  const walker = doc.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const nodes: Text[] = [];
  for (let n = walker.nextNode(); n; n = walker.nextNode()) nodes.push(n as Text);
  let pos = 0;
  for (const node of nodes) {
    const len = node.data.length;
    const from = Math.max(start, pos);
    const to = Math.min(end, pos + len);
    if (from < to && node.data.slice(from - pos, to - pos).trim()) {
      let target = node;
      if (from > pos) target = target.splitText(from - pos);
      if (to < pos + len) target.splitText(to - from);
      const mark = doc.createElement("mark");
      mark.className = MARK;
      mark.dataset.hl = id;
      if (note) mark.dataset.note = "1";
      target.replaceWith(mark);
      mark.appendChild(target);
    }
    pos += len;
    if (pos >= end) break;
  }
}

export function useHighlights(itemId: string, enabled = true) {
  return useQuery({
    queryKey: ["highlights", itemId],
    queryFn: () => api.highlights(itemId).then((r) => r.highlights),
    enabled,
    staleTime: 60_000,
  });
}

/** iframe 里的文档没有我们的样式表：放一份划线的样式进去。 */
const FRAME_STYLE =
  "mark.pf-hl{background:rgba(217,160,63,.35);color:inherit;border-radius:2px;cursor:pointer}" +
  "mark.pf-hl[data-note]{border-bottom:2px dotted #d9a03f}";

/**
 * 正文上的划线：包上标记、选中文字时浮出按钮、点划线跳到笔记。`root` 是放正文的那个元素
 * （HTML 邮件是 iframe 里的 body，这时 `frame` 给出 iframe 和缩放），`html` 变了就重新包。
 * `enabled` 为假时（比如正在看译文）不显示也不能划。
 */
export function HighlightLayer({
  item,
  root,
  frame,
  html,
  enabled,
  onFocusNote,
}: {
  item: ItemDetail;
  root: HTMLElement | null;
  frame?: { el: HTMLIFrameElement | null; zoom: number };
  html: string;
  enabled: boolean;
  onFocusNote: (id: string) => void;
}) {
  const body = { current: root };
  const focusRef = useRef(onFocusNote);
  focusRef.current = onFocusNote;
  const frameRef = useRef(frame);
  frameRef.current = frame;
  const t = useT();
  const qc = useQueryClient();
  const list = useHighlights(item.id, enabled);
  const [bar, setBar] = useState<{ x: number; y: number; start: number; end: number } | null>(null);

  // 包上划线
  useEffect(() => {
    if (!root) return;
    const doc = root.ownerDocument;
    if (doc !== document && !doc.getElementById("pf-hl-style")) {
      const style = doc.createElement("style");
      style.id = "pf-hl-style";
      style.textContent = FRAME_STYLE;
      doc.head?.appendChild(style);
    }
    unwrap(root);
    if (!enabled) return;
    const text = root.textContent ?? "";
    for (const h of list.data ?? []) {
      const at = locate(text, h.quote, h.prefix, h.suffix);
      if (at >= 0) wrap(root, at, at + h.quote.length, h.id, !!h.note.trim());
    }
  }, [root, html, list.data, enabled]);

  // 点正文里的划线：跳到下面它的笔记
  useEffect(() => {
    if (!root || !enabled) return;
    const onClick = (e: Event) => {
      const mark = (e.target as { closest?: (s: string) => HTMLElement | null } | null)?.closest?.("mark[data-hl]");
      const id = mark?.dataset.hl;
      if (id && !root.ownerDocument.getSelection()?.toString()) focusRef.current(id);
    };
    root.addEventListener("click", onClick);
    return () => root.removeEventListener("click", onClick);
  }, [root, enabled]);

  // 选中正文里的一段：浮出按钮
  useEffect(() => {
    if (!enabled || !root) return;
    const doc = root.ownerDocument;
    const onUp = () => {
      setTimeout(() => {
        const sel = doc.getSelection();
        if (!root || !sel || sel.rangeCount === 0 || sel.isCollapsed) {
          setBar(null);
          return;
        }
        const range = sel.getRangeAt(0);
        if (!root.contains(range.commonAncestorContainer) || !range.toString().trim()) {
          setBar(null);
          return;
        }
        const r = range.getBoundingClientRect();
        // iframe 里的位置：加上 iframe 在窗口里的位置，乘上缩放
        const f = frameRef.current;
        const box = f?.el?.getBoundingClientRect();
        const z = f?.zoom ?? 1;
        const rect = box ? { left: box.left + r.left * z, top: box.top + r.top * z, width: r.width * z } : r;
        setBar({
          x: rect.left + rect.width / 2,
          y: rect.top,
          start: offsetIn(root, range.startContainer, range.startOffset),
          end: offsetIn(root, range.endContainer, range.endOffset),
        });
      }, 0);
    };
    const onScroll = () => setBar(null);
    const docs = doc === document ? [document] : [document, doc];
    for (const d of docs) {
      d.addEventListener("mouseup", onUp);
      d.addEventListener("keyup", onUp);
      d.addEventListener("scroll", onScroll, true);
    }
    return () => {
      for (const d of docs) {
        d.removeEventListener("mouseup", onUp);
        d.removeEventListener("keyup", onUp);
        d.removeEventListener("scroll", onScroll, true);
      }
    };
  }, [root, enabled]);

  const add = async (withNote: boolean) => {
    const root = body.current;
    if (!root || !bar) return;
    // 两头的空白不算
    const text = root.textContent ?? "";
    let { start, end } = bar;
    while (start < end && /\s/.test(text[start] ?? "")) start++;
    while (end > start && /\s/.test(text[end - 1] ?? "")) end--;
    setBar(null);
    root.ownerDocument.getSelection()?.removeAllRanges();
    if (start >= end) return;
    try {
      const saved = await api.addHighlight({
        id: Math.random().toString(36).slice(2, 12),
        item: item.id,
        ...quoteAt(text, start, end),
        note: "",
      });
      qc.setQueryData<Highlight[]>(["highlights", item.id], (old) => [...(old ?? []), saved]);
      void qc.invalidateQueries({ queryKey: ["highlights"] });
      if (withNote) setTimeout(() => onFocusNote(saved.id), 50);
    } catch (err) {
      toast(errorText(t, err));
    }
  };

  if (!bar) return null;
  return createPortal(
    <div
      role="toolbar"
      aria-label={t("highlight.toolbar")}
      className="anim-pop fixed z-50 flex -translate-x-1/2 -translate-y-full gap-0.5 rounded-lg bg-surface p-1 shadow-pop"
      style={{ left: bar.x, top: bar.y - 6 }}
      onMouseDown={(e) => e.preventDefault()}
    >
      <button
        type="button"
        onClick={() => void add(false)}
        className="inline-flex h-7 items-center gap-1 rounded-md px-2 text-[12px] text-fg-2 hover:bg-hover hover:text-fg"
      >
        <Highlighter size={14} />
        {t("highlight.add")}
      </button>
      <button
        type="button"
        onClick={() => void add(true)}
        className="inline-flex h-7 items-center gap-1 rounded-md px-2 text-[12px] text-fg-2 hover:bg-hover hover:text-fg"
      >
        <NotebookPen size={14} />
        {t("highlight.note")}
      </button>
    </div>,
    document.body,
  );
}

/** 正文下面这篇的全部划线和笔记。 */
export function HighlightList({
  item,
  focus,
  enabled,
}: {
  item: ItemDetail;
  /** 要聚焦笔记的那一段（点了正文里的划线，或者刚点了「笔记」）。 */
  focus: { id: string; n: number } | null;
  enabled: boolean;
}) {
  const t = useT();
  const qc = useQueryClient();
  const list = useHighlights(item.id, enabled);
  const notes = useRef(new Map<string, HTMLTextAreaElement>());

  useEffect(() => {
    if (!focus) return;
    const el = notes.current.get(focus.id);
    el?.scrollIntoView({ block: "center", behavior: "smooth" });
    el?.focus();
  }, [focus]);

  const save = useCallback(
    (h: Highlight, note: string) => {
      if (note === h.note) return;
      qc.setQueryData<Highlight[]>(["highlights", item.id], (old) => old?.map((x) => (x.id === h.id ? { ...x, note } : x)));
      void api.setHighlightNote(h.id, note).catch((err) => toast(errorText(t, err)));
    },
    [item.id, qc, t],
  );

  const remove = (h: Highlight) => {
    qc.setQueryData<Highlight[]>(["highlights", item.id], (old) => old?.filter((x) => x.id !== h.id));
    void api.removeHighlight(h.id).catch((err) => toast(errorText(t, err)));
  };

  const highlights = list.data ?? [];
  if (!enabled || highlights.length === 0) return null;
  const copy = () => {
    const md = toMarkdown([
      { title: item.title, url: item.url, source: item.source_title, highlights },
    ]);
    void copyText(md).then(() => toast(t("highlight.copied")));
  };
  return (
    <section aria-label={t("highlight.section")} className="mt-12 border-t border-line pt-5 text-[13px]">
      <div className="mb-3 flex items-center gap-2">
        <h2 className="flex-1 font-medium text-fg-2">{t("highlight.count", { n: highlights.length })}</h2>
        <button
          type="button"
          onClick={copy}
          className="inline-flex h-7 items-center gap-1 rounded-md px-2 text-[12px] text-fg-3 hover:bg-hover hover:text-fg"
        >
          <Copy size={13} />
          {t("highlight.copy")}
        </button>
      </div>
      <ul className="flex flex-col gap-3">
        {highlights.map((h) => (
          <li key={h.id} className="group">
            <div className="flex items-start gap-2">
              <blockquote className="min-w-0 flex-1 whitespace-pre-wrap border-l-2 border-warning pl-3 leading-relaxed text-fg-2">
                {h.quote}
              </blockquote>
              <button
                type="button"
                aria-label={t("highlight.remove")}
                onClick={() => remove(h)}
                className="shrink-0 rounded-md p-1 text-fg-3 opacity-0 hover:bg-hover hover:text-danger focus:opacity-100 group-hover:opacity-100"
              >
                <Trash2 size={13} />
              </button>
            </div>
            <textarea
              ref={(el) => {
                if (el) notes.current.set(h.id, el);
                else notes.current.delete(h.id);
              }}
              defaultValue={h.note}
              placeholder={t("highlight.notePlaceholder")}
              aria-label={t("highlight.noteLabel")}
              rows={h.note ? Math.min(6, h.note.split("\n").length + 1) : 1}
              onBlur={(e) => save(h, e.target.value)}
              className={cx(
                "mt-1.5 ml-[14px] w-[calc(100%-14px)] resize-y rounded-md border border-transparent bg-transparent px-2 py-1 text-[13px] leading-relaxed text-fg outline-none placeholder:text-fg-4 hover:border-line focus:border-accent focus:bg-surface",
              )}
            />
          </li>
        ))}
      </ul>
    </section>
  );
}
