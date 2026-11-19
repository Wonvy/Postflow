// 朗读文章：标题下面的「朗读」，念的时候换成一条控制条（暂停、上一句、下一句、语速、停止）。
// 念的是阅读区里现在显示的文字：开着全文就念全文，开着翻译就念译文。

import { Check, Pause, Play, SkipBack, SkipForward, Square, Volume2 } from "lucide-react";
import { type RefObject, useEffect } from "react";
import { useT } from "../lib/i18n";
import { SPEECH_RATES, speechSupported, useSpeech } from "../lib/speech";
import { cx, IconButton, Menu, MenuItem } from "./ui";

export function ReadAloud({ itemId, title, body }: { itemId: string; title: string; body: RefObject<HTMLElement | null> }) {
  const t = useT();
  const mine = useSpeech((s) => s.itemId === itemId);
  const playing = useSpeech((s) => s.playing && s.itemId === itemId);
  const index = useSpeech((s) => s.index);
  const total = useSpeech((s) => s.parts.length);
  const rate = useSpeech((s) => s.rate);
  const { start, toggle, jump, stop, setRate } = useSpeech.getState();

  // 换一篇（或者关掉阅读区）就停
  useEffect(
    () => () => {
      if (useSpeech.getState().itemId === itemId) useSpeech.getState().stop();
    },
    [itemId],
  );

  if (!speechSupported()) return null;

  if (!mine) {
    return (
      <>
        <span aria-hidden>·</span>
        <button
          type="button"
          onClick={() => start(itemId, `${title}。\n${body.current?.innerText ?? ""}`)}
          className="inline-flex items-center gap-1 rounded-md px-1 text-fg-3 transition-colors hover:text-fg"
        >
          <Volume2 size={13} strokeWidth={1.75} />
          {t("speech.read")}
        </button>
      </>
    );
  }

  return (
    <div
      role="group"
      aria-label={t("speech.controls")}
      className="flex basis-full items-center gap-0.5 rounded-lg border border-line bg-surface px-1.5 py-1 text-fg-2"
    >
      <IconButton label={playing ? t("speech.pause") : t("speech.play")} onClick={toggle}>
        {playing ? <Pause size={15} fill="currentColor" strokeWidth={0} /> : <Play size={15} fill="currentColor" strokeWidth={0} />}
      </IconButton>
      <IconButton label={t("speech.previous")} onClick={() => jump(-1)}>
        <SkipBack size={14} strokeWidth={1.75} />
      </IconButton>
      <IconButton label={t("speech.next")} onClick={() => jump(1)}>
        <SkipForward size={14} strokeWidth={1.75} />
      </IconButton>
      <span className="ml-1 min-w-0 flex-1 truncate text-[12px] tabular-nums text-fg-3" aria-live="polite">
        {t("speech.progress", { n: index + 1, total })}
      </span>
      <Menu
        align="end"
        trigger={
          <button
            type="button"
            aria-label={t("speech.rate")}
            className={cx("h-7 rounded-md px-1.5 text-[12px] font-medium tabular-nums hover:bg-hover hover:text-fg")}
          >
            {rate}×
          </button>
        }
      >
        {SPEECH_RATES.map((r) => (
          <MenuItem key={r} icon={r === rate ? <Check size={14} /> : <span className="inline-block size-[14px]" />} onSelect={() => setRate(r)}>
            <span className="tabular-nums">{r}×</span>
          </MenuItem>
        ))}
      </Menu>
      <IconButton label={t("speech.stop")} onClick={stop}>
        <Square size={13} fill="currentColor" strokeWidth={0} />
      </IconButton>
    </div>
  );
}
