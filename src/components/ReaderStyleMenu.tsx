import * as Popover from "@radix-ui/react-popover";
import { ALargeSmall } from "lucide-react";
import type { ReactNode } from "react";
import { type MessageKey, useT } from "../lib/i18n";
import {
  clampTo,
  formatReaderValue,
  isDefaultStyle,
  READER_DEFAULT,
  READER_RANGE,
  type ReaderFont,
  type ReaderNumber,
} from "../lib/readerStyle";
import { useReaderStyle, useUi } from "../lib/store";
import { ICON, Segmented, Slider, Tip } from "./ui";

const SLIDERS: [ReaderNumber, MessageKey][] = [
  ["size", "reader.fontSize"],
  ["leading", "reader.lineHeight"],
  ["tracking", "reader.letterSpacing"],
  ["spacing", "reader.paragraphSpacing"],
  ["width", "reader.width"],
];

/** 阅读区工具栏上的「Aa」：用滑块改字号、行距、字间距、段间距和宽度，选字体，立即生效。双击滑块回到默认。 */
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
          {SLIDERS.map(([key, label]) => (
            <Row key={key} label={t(label)} value={formatReaderValue(key, style[key])}>
              <Slider
                label={t(label)}
                {...READER_RANGE[key]}
                value={style[key]}
                defaultValue={READER_DEFAULT[key]}
                onChange={(v) => set({ [key]: clampTo(key, v) })}
              />
            </Row>
          ))}
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

/** 一行滑块：上面是名字和当前数值，下面是滑块。 */
function Row({ label, value, children }: { label: string; value: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center justify-between gap-3">
        <span className="text-[13px] text-fg-2">{label}</span>
        <output aria-live="polite" className="tabular text-[12px] text-fg-3">
          {value}
        </output>
      </div>
      {children}
    </div>
  );
}
