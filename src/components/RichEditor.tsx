// 写信窗口里带格式的正文：粗体、斜体、下划线、删除线、链接、列表、引用。
//
// 用浏览器自带的可编辑区域，格式命令走 execCommand。这里不管粘贴进来的东西安不安全：
// 发出去和存草稿之前，核心会按允许的格式清理一遍。

import {
  Bold,
  Italic,
  Link,
  List,
  ListOrdered,
  RemoveFormatting,
  Strikethrough,
  TextQuote,
  Underline,
} from "lucide-react";
import {
  type ClipboardEvent,
  type KeyboardEvent,
  type ReactNode,
  type Ref,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react";
import { escapeHtml } from "../lib/compose";
import { useT } from "../lib/i18n";
import { isMac } from "../lib/platform";
import { cx, Tip } from "./ui";

export interface RichEditorHandle {
  /** 光标放到最前面。 */
  focusStart: () => void;
  /** 换掉签名块（`signatureHtml` 生成的）；给空字符串就去掉。原来没有签名时插在开头两行后面。 */
  setSignature: (html: string) => void;
}

const MOD = isMac ? "⌘" : "Ctrl+";

export function RichEditor({
  initialHtml,
  onChange,
  onPaste,
  label,
  editorRef,
}: {
  initialHtml: string;
  onChange: (html: string, text: string) => void;
  onPaste: (e: ClipboardEvent<HTMLDivElement>) => void;
  label: string;
  editorRef: Ref<RichEditorHandle>;
}) {
  const t = useT();
  const box = useRef<HTMLDivElement>(null);
  const [active, setActive] = useState<Record<string, boolean>>({});
  const [linking, setLinking] = useState(false);
  const [url, setUrl] = useState("");
  const saved = useRef<Range | null>(null);

  const emit = () => {
    const el = box.current;
    if (el) onChange(el.innerHTML, plainText(el));
  };

  // 内容只在挂上时放一次，之后由浏览器自己管，免得每次输入都重写、光标乱跳
  useEffect(() => {
    if (!box.current) return;
    box.current.innerHTML = initialHtml;
    emit();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useImperativeHandle(editorRef, () => ({
    focusStart: () => {
      const el = box.current;
      if (!el) return;
      el.focus();
      const range = document.createRange();
      range.setStart(el.firstChild ?? el, 0);
      range.collapse(true);
      const sel = window.getSelection();
      sel?.removeAllRanges();
      sel?.addRange(range);
      el.scrollTop = 0;
    },
    setSignature: (html) => {
      const el = box.current;
      if (!el) return;
      const current = el.querySelector("[data-signature]");
      if (current) {
        if (html) current.outerHTML = html;
        else current.remove();
      } else if (html) {
        const anchor = el.children[2] ?? null;
        const holder = document.createElement("div");
        holder.innerHTML = html;
        const node = holder.firstElementChild;
        if (node) el.insertBefore(node, anchor);
      }
      emit();
    },
  }));

  // 光标所在处的格式，工具栏按钮据此亮起
  useEffect(() => {
    const update = () => {
      const el = box.current;
      const sel = window.getSelection();
      if (!el || !sel || !el.contains(sel.anchorNode)) return;
      setActive({
        bold: document.queryCommandState("bold"),
        italic: document.queryCommandState("italic"),
        underline: document.queryCommandState("underline"),
        strikeThrough: document.queryCommandState("strikeThrough"),
        insertUnorderedList: document.queryCommandState("insertUnorderedList"),
        insertOrderedList: document.queryCommandState("insertOrderedList"),
      });
    };
    document.addEventListener("selectionchange", update);
    return () => document.removeEventListener("selectionchange", update);
  }, []);

  const run = (command: string, value?: string) => {
    box.current?.focus();
    document.execCommand(command, false, value);
    emit();
  };

  const openLink = () => {
    const sel = window.getSelection();
    saved.current = sel && sel.rangeCount > 0 ? sel.getRangeAt(0).cloneRange() : null;
    setUrl("");
    setLinking(true);
  };

  const applyLink = () => {
    const raw = url.trim();
    setLinking(false);
    if (!raw) return;
    const href = /^(https?:|mailto:)/i.test(raw) ? raw : raw.includes("@") && !raw.includes("/") ? `mailto:${raw}` : `https://${raw}`;
    box.current?.focus();
    const sel = window.getSelection();
    if (saved.current && sel) {
      sel.removeAllRanges();
      sel.addRange(saved.current);
    }
    if (!sel || sel.isCollapsed) {
      document.execCommand("insertHTML", false, `<a href="${escapeHtml(href)}">${escapeHtml(raw)}</a>`);
    } else {
      document.execCommand("createLink", false, href);
    }
    emit();
  };

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    // ⌘K 加链接，和常见编辑器一样
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
      e.preventDefault();
      openLink();
    }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-9 shrink-0 items-center gap-0.5 border-b border-line px-2">
        <ToolButton label={t("format.bold")} shortcut={MOD + "B"} active={active.bold} onClick={() => run("bold")}>
          <Bold size={14} strokeWidth={2.25} />
        </ToolButton>
        <ToolButton label={t("format.italic")} shortcut={MOD + "I"} active={active.italic} onClick={() => run("italic")}>
          <Italic size={14} strokeWidth={2.25} />
        </ToolButton>
        <ToolButton
          label={t("format.underline")}
          shortcut={MOD + "U"}
          active={active.underline}
          onClick={() => run("underline")}
        >
          <Underline size={14} strokeWidth={2.25} />
        </ToolButton>
        <ToolButton label={t("format.strike")} active={active.strikeThrough} onClick={() => run("strikeThrough")}>
          <Strikethrough size={14} strokeWidth={2.25} />
        </ToolButton>
        <span aria-hidden className="mx-1 h-4 w-px bg-line" />
        <ToolButton label={t("format.link")} shortcut={MOD + "K"} active={linking} onClick={openLink}>
          <Link size={14} strokeWidth={2.25} />
        </ToolButton>
        <ToolButton
          label={t("format.bullets")}
          active={active.insertUnorderedList}
          onClick={() => run("insertUnorderedList")}
        >
          <List size={14} strokeWidth={2.25} />
        </ToolButton>
        <ToolButton
          label={t("format.numbers")}
          active={active.insertOrderedList}
          onClick={() => run("insertOrderedList")}
        >
          <ListOrdered size={14} strokeWidth={2.25} />
        </ToolButton>
        <ToolButton label={t("format.quote")} onClick={() => run("formatBlock", "blockquote")}>
          <TextQuote size={14} strokeWidth={2.25} />
        </ToolButton>
        <ToolButton label={t("format.clear")} onClick={() => run("removeFormat")}>
          <RemoveFormatting size={14} strokeWidth={2.25} />
        </ToolButton>
        {linking && (
          <input
            autoFocus
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                applyLink();
              } else if (e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                setLinking(false);
                box.current?.focus();
              }
            }}
            onBlur={() => setLinking(false)}
            placeholder={t("format.linkPlaceholder")}
            spellCheck={false}
            className="ml-1 h-7 min-w-0 flex-1 rounded-md border border-line-strong bg-bg px-2 text-[12px] text-fg outline-none focus:border-accent"
          />
        )}
      </div>
      <div
        ref={box}
        role="textbox"
        aria-multiline
        aria-label={label}
        contentEditable
        suppressContentEditableWarning
        onInput={emit}
        onPaste={onPaste}
        onKeyDown={onKeyDown}
        className="rich-editor min-h-0 flex-1 overflow-y-auto px-4 py-3 text-[14px] leading-relaxed text-fg outline-none"
      />
    </div>
  );
}

/**
 * 带格式正文的纯文本版，给只看纯文本的邮件程序：列表前面加「- 」或「1. 」，引用每行前面加「> 」，
 * 链接后面写上地址。
 */
export function plainText(root: Node): string {
  let text = "";
  const breakLine = () => {
    if (text && !text.endsWith("\n")) text += "\n";
  };
  const walk = (node: Node) => {
    if (node.nodeType === Node.TEXT_NODE) {
      text += (node.textContent ?? "").replace(/[ \t\r\n]+/g, " ");
      return;
    }
    if (!(node instanceof HTMLElement)) return;
    const children = () => node.childNodes.forEach(walk);
    switch (node.tagName) {
      case "BR":
        text += "\n";
        return;
      case "UL":
      case "OL": {
        breakLine();
        let n = 0;
        for (const li of Array.from(node.children)) {
          n += 1;
          text += node.tagName === "OL" ? `${n}. ` : "- ";
          li.childNodes.forEach(walk);
          breakLine();
        }
        return;
      }
      case "BLOCKQUOTE": {
        breakLine();
        const inner = plainText(node).replace(/\s+$/, "");
        text += inner
          .split("\n")
          .map((line) => (line ? `> ${line}` : ">"))
          .join("\n");
        breakLine();
        return;
      }
      case "A": {
        const before = text.length;
        children();
        const label = text.slice(before);
        const href = node.getAttribute("href") ?? "";
        const shown = href.replace(/^mailto:/i, "");
        if (shown && shown !== label.trim()) text += ` <${shown}>`;
        return;
      }
      case "DIV":
      case "P":
      case "H1":
      case "H2":
      case "H3":
      case "H4":
      case "PRE":
      case "LI":
        breakLine();
        children();
        breakLine();
        return;
      default:
        children();
    }
  };
  root.childNodes.forEach(walk);
  return text
    .split("\n")
    // 签名的分隔线按惯例是「-- 」，末尾的空格要留着
    .map((line) => (line === "-- " ? line : line.replace(/\s+$/, "")))
    .join("\n")
    .replace(/^\n+/, "")
    .replace(/\n{3,}/g, "\n\n")
    .replace(/\s+$/, "");
}

function ToolButton({
  label,
  shortcut,
  active,
  onClick,
  children,
}: {
  label: string;
  shortcut?: string;
  active?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <Tip label={label} shortcut={shortcut} side="top">
      <button
        type="button"
        aria-label={label}
        aria-pressed={active}
        // 按下时不抢焦点，选中的文字还在
        onMouseDown={(e) => e.preventDefault()}
        onClick={onClick}
        className={cx(
          "inline-flex size-7 items-center justify-center rounded-md transition-colors",
          active ? "bg-active text-fg" : "text-fg-3 hover:bg-hover hover:text-fg",
        )}
      >
        {children}
      </button>
    </Tip>
  );
}
