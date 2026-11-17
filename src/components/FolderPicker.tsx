// 添加、编辑订阅时选文件夹：一个输入框，下面弹出最近用过的文件夹和全部文件夹，也能直接输入新名字。

import * as Popover from "@radix-ui/react-popover";
import { Check, ChevronDown, Clock, Folder, FolderPlus, FolderX } from "lucide-react";
import { type KeyboardEvent, type ReactNode, useMemo, useRef, useState } from "react";
import { useT } from "../lib/i18n";
import { useUi } from "../lib/store";
import { cx } from "./ui";

/** 最近的文件夹最多列几个。 */
const RECENT_SHOWN = 4;

interface Option {
  /** 选中后输入框里的值；空字符串表示不放进文件夹。 */
  value: string;
  label: string;
  icon: ReactNode;
  section: "recent" | "all" | "new" | "none";
}

/** 按输入的文字列出可选的文件夹。没输入时最近用过的排在最前面。 */
function optionsFor(query: string, folders: string[], recent: string[], newLabel: (name: string) => string, noneLabel: string): Option[] {
  const q = query.trim().toLowerCase();
  const icon = <Folder size={14} />;
  if (!q) {
    const recents = recent.filter((f) => folders.includes(f)).slice(0, RECENT_SHOWN);
    return [
      ...recents.map((f) => ({ value: f, label: f, icon: <Clock size={14} />, section: "recent" as const })),
      ...folders.filter((f) => !recents.includes(f)).map((f) => ({ value: f, label: f, icon, section: "all" as const })),
      { value: "", label: noneLabel, icon: <FolderX size={14} />, section: "none" },
    ];
  }
  const matches = folders
    .filter((f) => f.toLowerCase().includes(q))
    // 开头就对上的排前面，其次是最近用过的
    .sort((a, b) => {
      const starts = Number(b.toLowerCase().startsWith(q)) - Number(a.toLowerCase().startsWith(q));
      if (starts !== 0) return starts;
      const rank = (f: string) => (recent.includes(f) ? recent.indexOf(f) : recent.length);
      return rank(a) - rank(b);
    })
    .map((f) => ({ value: f, label: f, icon, section: "all" as const }));
  const exact = folders.some((f) => f.toLowerCase() === q);
  const name = query.trim();
  return exact ? matches : [...matches, { value: name, label: newLabel(name), icon: <FolderPlus size={14} />, section: "new" }];
}

export function FolderPicker({
  value,
  onChange,
  folders,
  placeholder,
  autoFocus,
}: {
  value: string;
  onChange: (value: string) => void;
  folders: string[];
  placeholder: string;
  autoFocus?: boolean;
}) {
  const t = useT();
  const recent = useUi((s) => s.recentFolders);
  const [open, setOpen] = useState(false);
  // 打开时按输入过滤；刚选完、或者打开时输入框里就是一个已有的文件夹，就列出全部
  const [query, setQuery] = useState<string | null>(null);
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const anchor = useRef<HTMLDivElement>(null);

  const options = useMemo(
    () =>
      optionsFor(query ?? "", folders, recent, (name) => t("folderPicker.new", { name }), t("folderPicker.none")),
    [folders, query, recent, t],
  );

  const show = () => {
    setQuery(null);
    setActive(Math.max(0, options.findIndex((o) => o.value === value)));
    setOpen(true);
  };
  const choose = (option: Option) => {
    onChange(option.value);
    setQuery(null);
    setOpen(false);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (!open) return show();
      const step = e.key === "ArrowDown" ? 1 : -1;
      setActive((i) => (i + step + options.length) % options.length);
    } else if (e.key === "Enter" && open) {
      // 选中高亮的那一项，不提交表单
      const option = options[active];
      if (option) {
        e.preventDefault();
        choose(option);
      }
    } else if (e.key === "Escape" && open) {
      // 只关下拉，不关对话框
      e.preventDefault();
      e.stopPropagation();
      setOpen(false);
    }
  };

  const heading = (option: Option, i: number) => {
    const prev = options[i - 1];
    if (prev?.section === option.section || option.section === "new") return null;
    if (option.section === "none") return <div className="mx-1 my-1 h-px bg-line" />;
    const label = option.section === "recent" ? t("folderPicker.recent") : t("folderPicker.all");
    return <div className="px-2 pb-1 pt-1.5 text-[11px] font-medium text-fg-3">{label}</div>;
  };

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Anchor asChild>
        <div ref={anchor} className="relative">
          <input
            ref={input}
            type="text"
            role="combobox"
            aria-expanded={open}
            aria-autocomplete="list"
            autoComplete="off"
            autoCorrect="off"
            spellCheck={false}
            autoFocus={autoFocus}
            data-autofocus={autoFocus ? "" : undefined}
            value={value}
            placeholder={placeholder}
            onChange={(e) => {
              onChange(e.target.value);
              setQuery(e.target.value);
              setActive(0);
              setOpen(true);
            }}
            onFocus={(e) => {
              // 选中原来的名字，直接打字就是换一个
              e.currentTarget.select();
              show();
            }}
            onClick={() => !open && show()}
            onKeyDown={onKeyDown}
            className="h-8 w-full rounded-md border border-line-strong bg-bg pl-8 pr-8 text-[13px] text-fg outline-none transition-colors placeholder:text-fg-3 focus:border-accent focus:ring-1 focus:ring-accent"
          />
          <Folder
            size={14}
            strokeWidth={1.75}
            className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-fg-3"
          />
          <button
            type="button"
            tabIndex={-1}
            aria-label={t("folderPicker.open")}
            onClick={() => {
              if (open) setOpen(false);
              else {
                show();
                input.current?.focus();
              }
            }}
            className="absolute right-1 top-1/2 flex size-6 -translate-y-1/2 items-center justify-center rounded text-fg-3 hover:bg-hover hover:text-fg"
          >
            <ChevronDown size={14} className={cx("transition-transform", open && "rotate-180")} />
          </button>
        </div>
      </Popover.Anchor>
      <Popover.Portal>
        <Popover.Content
          align="start"
          sideOffset={4}
          collisionPadding={8}
          // 焦点留在输入框里，可以接着打字
          onOpenAutoFocus={(e) => e.preventDefault()}
          onCloseAutoFocus={(e) => e.preventDefault()}
          onInteractOutside={(e) => {
            if (anchor.current?.contains(e.target as Node)) e.preventDefault();
          }}
          className="anim-pop z-50 max-h-[min(280px,var(--radix-popover-content-available-height))] w-[var(--radix-popper-anchor-width)] overflow-y-auto rounded-lg bg-surface p-1 shadow-pop outline-none"
        >
          <div role="listbox" aria-label={t("add.folder")}>
            {options.map((option, i) => (
              <div key={`${option.section}:${option.value}`}>
                {heading(option, i)}
                <div
                  role="option"
                  aria-selected={i === active}
                  onMouseEnter={() => setActive(i)}
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => choose(option)}
                  className={cx(
                    "flex h-7 cursor-default items-center gap-2 rounded-md px-2 text-[13px]",
                    i === active ? "bg-active text-fg" : "text-fg",
                    option.section === "new" && "text-accent",
                  )}
                >
                  <span className={cx("flex size-4 shrink-0 items-center justify-center", option.section !== "new" && "text-fg-3")}>
                    {option.icon}
                  </span>
                  <span className={cx("min-w-0 flex-1 truncate", option.section === "none" && "text-fg-2")}>
                    {option.label}
                  </span>
                  {option.section !== "new" && option.value === value.trim() && (
                    <Check size={14} className="shrink-0 text-fg-2" />
                  )}
                </div>
              </div>
            ))}
          </div>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
