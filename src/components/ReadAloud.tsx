// 朗读文章：标题下面的「朗读」，念的时候换成一条控制条（暂停、上一句、下一句、语速、停止）。
// 念的是阅读区里现在显示的文字：开着全文就念全文，开着翻译就念译文。

import { Check, Pause, Play, SkipBack, SkipForward, Square, Volume2 } from "lucide-react";
import { type RefObject, useEffect } from "react";
import { locate } from "../lib/highlightsText";
import { useT } from "../lib/i18n";
import { SPEECH_RATES, speechSupported, useSpeech } from "../lib/speech";
import { useUi } from "../lib/store";
import { cx, IconButton, Menu, MenuItem } from "./ui";

/** 正文里第 [start, end) 个字（和 textContent 一样数）的那一段。 */
function rangeAt(root: HTMLElement, start: number, end: number): Range | null {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const range = document.createRange();
  let pos = 0;
  let started = false;
  for (let n = walker.nextNode() as Text | null; n; n = walker.nextNode() as Text | null) {
    const len = n.data.length;
    if (!started && start < pos + len) {
      range.setStart(n, start - pos);
      started = true;
    }
    if (started && end <= pos + len) {
      range.setEnd(n, end - pos);
      return range;
    }
    pos += len;
  }
  return null;
}

export function ReadAloud({ itemId, title, body }: { itemId: string; title: string; body: RefObject<HTMLElement | null> }) {
  const t = useT();
  const mine = useSpeech((s) => s.itemId === itemId);
  const playing = useSpeech((s) => s.playing && s.itemId === itemId);
  const index = useSpeech((s) => s.index);
  const total = useSpeech((s) => s.parts.length);
  const rate = useSpeech((s) => s.rate);
  const { start, toggle, jump, stop, setRate } = useSpeech.getState();

  const part = useSpeech((s) => (s.itemId === itemId ? s.parts[s.index] : undefined));

  // 正文里标出正在念的那一句，并让它留在视野里
  useEffect(() => {
    const root = body.current;
    const css = (globalThis as { CSS?: { highlights?: Map<string, unknown> } }).CSS;
    const registry = css?.highlights;
    const HighlightCtor = (globalThis as { Highlight?: new (...r: Range[]) => unknown }).Highlight;
    if (!registry || !HighlightCtor) return;
    registry.delete("pf-speech");
    if (!root || !part) return;
    const text = root.textContent ?? "";
    const at = locate(text, part);
    if (at < 0) return;
    const range = rangeAt(root, at, at + part.length);
    if (!range) return;
    registry.set("pf-speech", new HighlightCtor(range));
    const rect = range.getBoundingClientRect();
    if (rect.top < 80 || rect.bottom > window.innerHeight - 80) {
      range.startContainer.parentElement?.scrollIntoView({ block: "center", behavior: "smooth" });
    }
    return () => {
      registry.delete("pf-speech");
    };
  }, [part, body]);

  if (!speechSupported()) return null;

  if (!mine) {
    return (
      <>
        <span aria-hidden>·</span>
        <button
          type="button"
          onClick={() => start(itemId, title, body.current?.innerText ?? "")}
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

/** 列表底下的朗读控制条：换了文章也接着念，点标题回到那一篇。 */
export function SpeechBar() {
  const t = useT();
  const itemId = useSpeech((s) => s.itemId);
  const title = useSpeech((s) => s.title);
  const playing = useSpeech((s) => s.playing);
  const index = useSpeech((s) => s.index);
  const total = useSpeech((s) => s.parts.length);
  const openId = useUi((s) => s.openId);
  const { toggle, jump, stop } = useSpeech.getState();
  // 正在看的就是这一篇时，控制条在文章里，这里不重复
  if (!itemId || openId === itemId) return null;
  return (
    <div className="flex shrink-0 items-center gap-1 border-t border-line bg-surface py-1.5 pl-2.5 pr-1.5">
      <button
        type="button"
        onClick={() => useUi.getState().open(itemId)}
        title={title}
        className="flex min-w-0 flex-1 items-center gap-2 rounded-md text-left"
      >
        <Volume2 size={16} strokeWidth={1.75} className="shrink-0 text-accent" />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-[12px] font-medium text-fg">{title}</span>
          <span className="block truncate text-[11px] tabular-nums text-fg-3">{t("speech.progress", { n: index + 1, total })}</span>
        </span>
      </button>
      <IconButton label={playing ? t("speech.pause") : t("speech.play")} onClick={toggle}>
        {playing ? <Pause size={15} fill="currentColor" strokeWidth={0} /> : <Play size={15} fill="currentColor" strokeWidth={0} />}
      </IconButton>
      <IconButton label={t("speech.next")} onClick={() => jump(1)}>
        <SkipForward size={14} strokeWidth={1.75} />
      </IconButton>
      <IconButton label={t("speech.stop")} onClick={stop}>
        <Square size={13} fill="currentColor" strokeWidth={0} />
      </IconButton>
    </div>
  );
}
