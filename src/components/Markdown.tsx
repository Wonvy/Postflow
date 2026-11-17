// 把 AI 回答、更新说明和随应用带的文档里的 Markdown 显示出来。只认常用的几种写法，直接生成 React 元素，
// 不插入任何 HTML。

import { Check, Copy } from "lucide-react";
import { type ReactNode, Fragment, useState } from "react";
import { useT } from "../lib/i18n";
import { copyText, openExternal } from "../lib/platform";

/** 行内：**粗体**、*斜体*、`代码`、[链接](https://…)、<https://…>。指向别的文件或本页标题的链接只显示文字。 */
function inline(text: string, key: string): ReactNode[] {
  const out: ReactNode[] = [];
  const pattern =
    /(\*\*[^*]+\*\*|__[^_]+__|`[^`]+`|\[[^\]]+\]\([^)\s]+\)|<https?:\/\/[^>\s]+>|\*[^*\s][^*]*\*|_[^_\s][^_]*_)/g;
  let last = 0;
  let n = 0;
  for (const match of text.matchAll(pattern)) {
    const token = match[0];
    const at = match.index ?? 0;
    if (at > last) out.push(text.slice(last, at));
    const k = `${key}-${n++}`;
    if (token.startsWith("**") || token.startsWith("__")) {
      out.push(<strong key={k}>{inline(token.slice(2, -2), k)}</strong>);
    } else if (token.startsWith("`")) {
      out.push(
        <code key={k} className="rounded bg-active px-1 py-px font-mono text-[0.92em]">
          {token.slice(1, -1)}
        </code>,
      );
    } else if (token.startsWith("[") || token.startsWith("<")) {
      const [, label = "", href = ""] = token.startsWith("<")
        ? [token, token.slice(1, -1), token.slice(1, -1)]
        : (/^\[([^\]]+)\]\(([^)]+)\)$/.exec(token) ?? []);
      if (!/^https?:\/\//i.test(href)) {
        out.push(<Fragment key={k}>{inline(label, k)}</Fragment>);
      } else {
        out.push(
          <a
            key={k}
            href={href}
            onClick={(e) => {
              e.preventDefault();
              void openExternal(href);
            }}
            className="text-accent underline-offset-2 hover:underline"
          >
            {label}
          </a>,
        );
      }
    } else {
      out.push(<em key={k}>{inline(token.slice(1, -1), k)}</em>);
    }
    last = at + token.length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

type Block =
  | { kind: "p"; lines: string[] }
  | { kind: "h"; level: number; text: string }
  | { kind: "ul" | "ol"; items: string[] }
  | { kind: "code"; text: string }
  | { kind: "quote"; lines: string[] }
  | { kind: "table"; head: string[]; rows: string[][] };

/** 表格的一行拆成各格。 */
function cells(line: string): string[] {
  return line
    .trim()
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split("|")
    .map((c) => c.trim());
}

function blocks(source: string): Block[] {
  const out: Block[] = [];
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i] ?? "";
    const trimmed = line.trim();
    if (trimmed.startsWith("```")) {
      // 列表里缩进的代码块：去掉和开头一样的缩进
      const indent = line.length - line.trimStart().length;
      const body: string[] = [];
      for (i++; i < lines.length && !(lines[i] ?? "").trim().startsWith("```"); i++) {
        const l = lines[i] ?? "";
        body.push(l.slice(Math.min(indent, l.length - l.trimStart().length)));
      }
      out.push({ kind: "code", text: body.join("\n") });
      continue;
    }
    if (!trimmed) continue;
    // 表格：表头、分隔行（| --- |），再是各行
    if (trimmed.startsWith("|") && /^\|?\s*:?-{3,}/.test((lines[i + 1] ?? "").trim())) {
      const head = cells(trimmed);
      const rows: string[][] = [];
      for (i += 2; i < lines.length && (lines[i] ?? "").trim().startsWith("|"); i++) rows.push(cells(lines[i] ?? ""));
      i--;
      out.push({ kind: "table", head, rows });
      continue;
    }
    if (trimmed.startsWith(">")) {
      const text = trimmed.replace(/^>\s?/, "");
      const prev = out[out.length - 1];
      if (prev && prev.kind === "quote") prev.lines.push(text);
      else out.push({ kind: "quote", lines: [text] });
      continue;
    }
    const heading = /^(#{1,4})\s+(.*)$/.exec(trimmed);
    if (heading) {
      out.push({ kind: "h", level: heading[1]?.length ?? 1, text: heading[2] ?? "" });
      continue;
    }
    const bullet = /^[-*•]\s+(.*)$/.exec(trimmed);
    const numbered = /^\d+[.)、]\s*(.*)$/.exec(trimmed);
    if (bullet || numbered) {
      const kind = bullet ? "ul" : "ol";
      const text = (bullet ?? numbered)?.[1] ?? "";
      const prev = out[out.length - 1];
      if (prev && prev.kind === kind) prev.items.push(text);
      else out.push({ kind, items: [text] });
      continue;
    }
    const prev = out[out.length - 1];
    // 紧挨着的几行是同一段
    if (prev && prev.kind === "p" && (lines[i - 1] ?? "").trim()) prev.lines.push(trimmed);
    else out.push({ kind: "p", lines: [trimmed] });
  }
  return out;
}

/** 把折行的几行接成一段：中文之间直接接上，其他地方加一个空格。 */
function joinLines(lines: string[]): string {
  return lines.reduce((out, line) => {
    if (!out) return line;
    const cjk = /[\u3000-\u9fff\uff00-\uffef]$/.test(out) && /^[\u3000-\u9fff\uff00-\uffef]/.test(line);
    return out + (cjk ? "" : " ") + line;
  }, "");
}

/**
 * `wrapped`：源文本是按固定宽度折行的文档（比如 docs/ 里的说明），段落里的换行不算换行。
 * 默认每个换行都保留（AI 的回答、更新说明）。
 */
export function Markdown({ text, className, wrapped = false }: { text: string; className?: string; wrapped?: boolean }) {
  return (
    <div className={"flex flex-col gap-2 " + (className ?? "")}>
      {blocks(text).map((block, i) => {
        const key = `b${i}`;
        switch (block.kind) {
          case "h":
            return (
              <p
                key={key}
                className={
                  "font-semibold text-fg " +
                  (block.level === 1 ? "text-[1.25em]" : block.level === 2 ? "mt-2 text-[1.1em]" : "")
                }
              >
                {inline(block.text, key)}
              </p>
            );
          case "quote":
            return (
              <p key={key} className="border-l-2 border-line-strong pl-3 text-fg-3">
                {inline(joinLines(block.lines), key)}
              </p>
            );
          case "table":
            return (
              <div key={key} className="overflow-x-auto">
                <table className="w-full border-collapse text-left text-[0.92em]">
                  <thead>
                    <tr>
                      {block.head.map((c, j) => (
                        <th key={j} className="border-b border-line-strong px-2 py-1 font-medium text-fg">
                          {inline(c, `${key}-h${j}`)}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {block.rows.map((row, r) => (
                      <tr key={r}>
                        {row.map((c, j) => (
                          <td key={j} className="border-b border-line px-2 py-1 align-top">
                            {inline(c, `${key}-${r}-${j}`)}
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            );
          case "ul":
          case "ol": {
            const List = block.kind === "ul" ? "ul" : "ol";
            return (
              <List key={key} className={(block.kind === "ul" ? "list-disc" : "list-decimal") + " flex flex-col gap-1 pl-5"}>
                {block.items.map((item, j) => (
                  <li key={`${key}-${j}`}>{inline(item, `${key}-${j}`)}</li>
                ))}
              </List>
            );
          }
          case "code":
            return <CodeBlock key={key} text={block.text} />;
          default:
            if (wrapped) return <p key={key}>{inline(joinLines(block.lines), key)}</p>;
            return (
              <p key={key}>
                {block.lines.map((line, j) => (
                  <Fragment key={`${key}-${j}`}>
                    {j > 0 && <br />}
                    {inline(line, `${key}-${j}`)}
                  </Fragment>
                ))}
              </p>
            );
        }
      })}
    </div>
  );
}

/** 代码块，右上角可以一键拷贝（文档里的命令）。 */
function CodeBlock({ text }: { text: string }) {
  const t = useT();
  const [copied, setCopied] = useState(false);
  return (
    <div className="group relative">
      <pre className="select-text overflow-x-auto rounded-md bg-active p-2 pr-8 font-mono text-[12px]">{text}</pre>
      <button
        type="button"
        aria-label={t("action.copy")}
        title={t("action.copy")}
        onClick={() => {
          void copyText(text).then(() => {
            setCopied(true);
            setTimeout(() => setCopied(false), 1500);
          });
        }}
        className="absolute right-1.5 top-1.5 inline-flex size-6 items-center justify-center rounded text-fg-3 opacity-0 transition-opacity hover:bg-hover hover:text-fg group-hover:opacity-100 focus-visible:opacity-100"
      >
        {copied ? <Check size={13} /> : <Copy size={13} />}
      </button>
    </div>
  );
}
