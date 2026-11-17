// 把 AI 回答里的 Markdown 显示出来。只认常用的几种写法，直接生成 React 元素，不插入任何 HTML。

import { type ReactNode, Fragment } from "react";
import { openExternal } from "../lib/platform";

/** 行内：**粗体**、*斜体*、`代码`、[链接](https://…)。 */
function inline(text: string, key: string): ReactNode[] {
  const out: ReactNode[] = [];
  const pattern = /(\*\*[^*]+\*\*|__[^_]+__|`[^`]+`|\[[^\]]+\]\(https?:\/\/[^)\s]+\)|\*[^*\s][^*]*\*|_[^_\s][^_]*_)/g;
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
    } else if (token.startsWith("[")) {
      const [, label = "", href = ""] = /^\[([^\]]+)\]\(([^)]+)\)$/.exec(token) ?? [];
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
  | { kind: "code"; text: string };

function blocks(source: string): Block[] {
  const out: Block[] = [];
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i] ?? "";
    const trimmed = line.trim();
    if (trimmed.startsWith("```")) {
      const body: string[] = [];
      for (i++; i < lines.length && !(lines[i] ?? "").trim().startsWith("```"); i++) body.push(lines[i] ?? "");
      out.push({ kind: "code", text: body.join("\n") });
      continue;
    }
    if (!trimmed) continue;
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

export function Markdown({ text, className }: { text: string; className?: string }) {
  return (
    <div className={"flex flex-col gap-2 " + (className ?? "")}>
      {blocks(text).map((block, i) => {
        const key = `b${i}`;
        switch (block.kind) {
          case "h":
            return (
              <p key={key} className="font-semibold text-fg">
                {inline(block.text, key)}
              </p>
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
            return (
              <pre key={key} className="overflow-x-auto rounded-md bg-active p-2 font-mono text-[12px]">
                {block.text}
              </pre>
            );
          default:
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
