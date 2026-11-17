import * as Popover from "@radix-ui/react-popover";
import { ALargeSmall, Minus, Plus } from "lucide-react";
import type { ReactNode } from "react";
import { useT } from "../lib/i18n";
import {
  isDefaultStyle,
  READER_DEFAULT,
  READER_SIZE_MAX,
  READER_SIZE_MIN,
  type ReaderFont,
  type ReaderLeading,
  type ReaderWidth,
} from "../lib/readerStyle";
import { useReaderStyle, useUi } from "../lib/store";
import { ICON, Segmented, Tip } from "./ui";

/** 阅读区工具栏上的「Aa」：改字号、行距、字体和宽度，立即生效。 */
export function ReaderStyleMenu() {
  const t = useT();
  const style = useReaderStyle();
  const set = useUi((s) => s.setReaderStyle);

  return (
    <Popover.Root>
      <Tip label={t("reader.style")} shortcut="+ −">
        <Popover.Trigger asChild>
          <button
            type="button"
            aria-label={t("reader.style")}
            className="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-fg-2 transition-colors hover:bg-hover hover:text-fg data-[state=open]:bg-hover data-[state=open]:text-fg"
          >
            <ALargeSmall {...ICON} />
          </button>
        </Popover.Trigger>
      </Tip>
      <Popover.Portal>
        <Popover.Content
          align="end"
          sideOffset={4}
          aria-label={t("reader.style")}
          className="anim-pop z-50 flex w-[304px] flex-col gap-3 rounded-lg bg-surface p-3 shadow-pop outline-none"
        >
          <Line label={t("reader.fontSize")}>
            <div className="flex items-center gap-1">
              <Step
                label={t("reader.smaller")}
                disabled={style.size <= READER_SIZE_MIN}
                onClick={() => set({ size: style.size - 1 })}
              >
                <Minus size={14} strokeWidth={2} />
              </Step>
              <output aria-live="polite" className="tabular w-9 text-center text-[13px] text-fg">
                {style.size}
              </output>
              <Step
                label={t("reader.larger")}
                disabled={style.size >= READER_SIZE_MAX}
                onClick={() => set({ size: style.size + 1 })}
              >
                <Plus size={14} strokeWidth={2} />
              </Step>
            </div>
          </Line>
          <Line label={t("reader.lineHeight")}>
            <Segmented<ReaderLeading>
              value={style.leading}
              onChange={(leading) => set({ leading })}
              options={[
                { value: "compact", label: t("reader.leadingCompact") },
                { value: "normal", label: t("reader.leadingNormal") },
                { value: "relaxed", label: t("reader.leadingRelaxed") },
              ]}
            />
          </Line>
          <Line label={t("reader.font")}>
            <Segmented<ReaderFont>
              value={style.font}
              onChange={(font) => set({ font })}
              options={[
                { value: "sans", label: t("reader.fontSans") },
                { value: "serif", label: t("reader.fontSerif") },
              ]}
            />
          </Line>
          <Line label={t("reader.width")}>
            <Segmented<ReaderWidth>
              value={style.width}
              onChange={(width) => set({ width })}
              options={[
                { value: "narrow", label: t("reader.widthNarrow") },
                { value: "normal", label: t("reader.widthNormal") },
                { value: "wide", label: t("reader.widthWide") },
              ]}
            />
          </Line>
          <div className="flex items-center justify-between gap-3 border-t border-line pt-2.5">
            <span className="text-[11px] leading-snug text-fg-3">{t("reader.styleHint")}</span>
            <button
              type="button"
              disabled={isDefaultStyle(style)}
              onClick={() => set(READER_DEFAULT)}
              className="shrink-0 rounded px-1.5 py-0.5 text-[12px] text-accent hover:bg-hover disabled:pointer-events-none disabled:text-fg-4"
            >
              {t("reader.styleReset")}
            </button>
          </div>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

function Line({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-3">
      <span className="text-[13px] text-fg-2">{label}</span>
      {children}
    </div>
  );
}

function Step({
  label,
  disabled,
  onClick,
  children,
}: {
  label: string;
  disabled: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
      className="inline-flex size-7 items-center justify-center rounded-md border border-line-strong text-fg-2 transition-colors hover:bg-hover hover:text-fg disabled:pointer-events-none disabled:opacity-40"
    >
      {children}
    </button>
  );
}
