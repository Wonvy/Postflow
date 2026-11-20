// 朗读和听新闻的控制：
// - 文章标题下面的「朗读」，念的时候换成一条控制条（暂停、上一句、下一句、下一篇、语速、选项、停止）。
// - 列表底下的小控制条（SpeechBar）：看着别的文章时也能控制。
// - 列表工具栏上的「听这个列表」（ListenButton）：从打开的那篇（或者第一篇）开始，一篇接一篇往下念。
// 每篇念什么、念完接不接着念、标不标已读在控制条的选项菜单和「设置 → 朗读」里选。用什么声音念在控制条上的
// 「换声音」（VoicePicker）里直接换，也可以在设置里选。列表底下的控制条上方显示正在念的这一句（字幕，可以关）。

import {
  Captions,
  Check,
  ChevronsRight,
  Headphones,
  Moon,
  Pause,
  Play,
  Settings2,
  SkipBack,
  SkipForward,
  SlidersHorizontal,
  Square,
  Volume2,
} from "lucide-react";
import { type RefObject, useEffect } from "react";
import type { Item } from "../lib/api";
import { locate } from "../lib/highlightsText";
import { type MessageKey, useT } from "../lib/i18n";
import { useListModel } from "../lib/listModel";
import { LISTEN_CONTENTS, type ListenContent, nextInQueue } from "../lib/listenText";
import { listenItemOf, listenQueueOf, SPEECH_RATES, useReadAloudAvailable, useSpeech } from "../lib/speech";
import { useUi } from "../lib/store";
import { useVoices, voiceOverview } from "../lib/voices";
import { toast } from "./Toast";
import { cx, ICON, IconButton, Menu, MenuItem, MenuLabel, MenuSeparator, MenuSub, Spinner } from "./ui";
import { VoicePicker } from "./VoicePicker";

/** 每种念法的名字，以及标题下面那个按钮上的字。 */
export const CONTENT_LABEL: Record<ListenContent, MessageKey> = {
  full: "listen.content.full",
  summary: "listen.content.summary",
  ai: "listen.content.ai",
  title: "listen.content.title",
};
const BUTTON_LABEL: Record<ListenContent, MessageKey> = {
  full: "speech.read",
  summary: "speech.readSummary",
  ai: "speech.readAi",
  title: "speech.readTitle",
};
const SLEEP_MINUTES = [15, 30, 45, 60] as const;

/** 播放 / 暂停按钮的图标：在准备文字、等本机声音念好这一句时转圈。 */
function PlayIcon({ playing, waiting }: { playing: boolean; waiting: boolean }) {
  if (waiting) return <Spinner size={14} />;
  return playing ? <Pause size={15} fill="currentColor" strokeWidth={0} /> : <Play size={15} fill="currentColor" strokeWidth={0} />;
}

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

const check = (on: boolean) => (on ? <Check size={14} /> : <span className="inline-block size-[14px]" />);

/** 选项：念什么、念完接着念、念完标为已读、定时停止。 */
function ListenMenu() {
  const t = useT();
  const content = useVoices((s) => s.content);
  const autoNext = useVoices((s) => s.autoNext);
  const markRead = useVoices((s) => s.markRead);
  const sleep = useSpeech((s) => s.sleep);
  const aiCount = useSpeech((s) => s.aiCount);
  const prefs = useVoices.getState();
  const { setSleep, restart } = useSpeech.getState();
  return (
    <Menu
      align="end"
      trigger={
        <IconButton label={t("listen.options")}>
          <SlidersHorizontal size={14} strokeWidth={1.75} />
        </IconButton>
      }
    >
      <MenuLabel>{t("listen.content")}</MenuLabel>
      {LISTEN_CONTENTS.map((c) => (
        <MenuItem
          key={c}
          icon={check(c === content)}
          onSelect={() => {
            if (c === content) return;
            prefs.setContent(c);
            // 正在念的这一篇也换成新的念法
            restart();
          }}
        >
          {t(CONTENT_LABEL[c])}
        </MenuItem>
      ))}
      <MenuSeparator />
      <MenuItem icon={check(autoNext)} onSelect={() => prefs.setAutoNext(!autoNext)}>
        {t("listen.autoNext")}
      </MenuItem>
      <MenuItem icon={check(markRead)} onSelect={() => prefs.setMarkRead(!markRead)}>
        {t("listen.markRead")}
      </MenuItem>
      <MenuSub icon={<Moon size={14} />} label={t("listen.sleep")}>
        <MenuItem icon={check(sleep === null)} onSelect={() => setSleep(null)}>
          {t("listen.sleepOff")}
        </MenuItem>
        {SLEEP_MINUTES.map((n) => (
          <MenuItem key={n} icon={<span className="inline-block size-[14px]" />} onSelect={() => setSleep(n)}>
            {t("listen.sleepMinutes", { n })}
          </MenuItem>
        ))}
        <MenuItem icon={check(sleep === "item")} onSelect={() => setSleep("item")}>
          {t("listen.sleepItem")}
        </MenuItem>
      </MenuSub>
      {aiCount > 0 && (
        <>
          <MenuSeparator />
          <MenuLabel>{t("listen.aiCount", { n: aiCount })}</MenuLabel>
        </>
      )}
      <MenuSeparator />
      <MenuItem
        icon={<Settings2 size={14} />}
        onSelect={() => useUi.getState().setDialog({ type: "settings", page: "voices" })}
      >
        {t("listen.settings")}
      </MenuItem>
    </Menu>
  );
}

/** 定时停止开着时的小月亮，悬停看还剩多久。 */
function SleepMark() {
  const t = useT();
  const sleep = useSpeech((s) => s.sleep);
  if (sleep === null) return null;
  const label =
    sleep === "item" ? t("listen.sleepItem") : t("listen.sleepLeft", { n: Math.max(1, Math.round((sleep - Date.now()) / 60_000)) });
  return (
    <span title={label} aria-label={label} className="inline-flex shrink-0 text-accent">
      <Moon size={13} strokeWidth={1.75} />
    </span>
  );
}

/** 第几句，或者正在准备文字。 */
function Progress() {
  const t = useT();
  const preparing = useSpeech((s) => s.preparing);
  const index = useSpeech((s) => s.index);
  const total = useSpeech((s) => s.parts.length);
  return <>{preparing ? t("listen.preparing") : t("speech.progress", { n: index + 1, total })}</>;
}

/** 正在念的这一句，显示在列表底下的控制条上方。开着字幕、有在念的句子时才有。 */
function Caption() {
  const on = useVoices((s) => s.captions);
  const text = useSpeech((s) => (s.preparing ? "" : (s.parts[s.index] ?? "")));
  if (!on || !text) return null;
  return (
    <p key={text} aria-live="polite" className="anim-fade line-clamp-3 px-3 pt-2 text-[13px] leading-snug text-fg-2">
      {text}
    </p>
  );
}

/** 字幕开关。 */
function CaptionToggle() {
  const t = useT();
  const on = useVoices((s) => s.captions);
  const setOn = useVoices((s) => s.setCaptions);
  return (
    <IconButton label={on ? t("subtitles.turnOff") : t("subtitles.turnOn")} active={on} aria-pressed={on} onClick={() => setOn(!on)}>
      <Captions size={15} strokeWidth={1.75} />
    </IconButton>
  );
}

function useHasNext(): boolean {
  return useSpeech((s) => (s.itemId ? nextInQueue(s.queue, s.itemId) !== undefined : false));
}

export function ReadAloud({
  item,
  title,
  body,
  full = false,
  loadingFull = false,
}: {
  item: Item;
  title: string;
  body: RefObject<HTMLElement | null>;
  /** 阅读区看的是全文。 */
  full?: boolean;
  /** 全文还在取：这时显示着的只是订阅源给的那段，不能拿来念。 */
  loadingFull?: boolean;
}) {
  const t = useT();
  const itemId = item.id;
  const mine = useSpeech((s) => s.itemId === itemId);
  const playing = useSpeech((s) => s.playing && s.itemId === itemId);
  const waiting = useSpeech((s) => (s.waiting || s.preparing) && s.itemId === itemId);
  const available = useReadAloudAvailable();
  const rate = useSpeech((s) => s.rate);
  const content = useVoices((s) => s.content);
  const hasNext = useHasNext();
  const { items } = useListModel();
  const { start, toggle, jump, next, stop, setRate } = useSpeech.getState();

  const part = useSpeech((s) => (s.itemId === itemId ? s.parts[s.index] : undefined));

  // 看看核心那边装没装本机声音：没有系统语音的地方（Linux）装了也能朗读
  useEffect(() => {
    void voiceOverview().catch(() => undefined);
  }, []);

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

  if (!available) return null;

  if (!mine) {
    return (
      <>
        <span aria-hidden>·</span>
        <button
          type="button"
          onClick={() =>
            // 念的是显示着的标题和正文（开着翻译就是译文）；念完按列表往下念
            start(
              { ...listenItemOf(item), title, full },
              loadingFull ? null : (body.current?.innerText ?? ""),
              listenQueueOf(items),
            )
          }
          className="inline-flex items-center gap-1 rounded-md px-1 text-fg-3 transition-colors hover:text-fg"
        >
          <Volume2 size={13} strokeWidth={1.75} />
          {t(BUTTON_LABEL[content])}
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
      <IconButton label={waiting ? t("voices.preparing") : playing ? t("speech.pause") : t("speech.play")} onClick={toggle}>
        <PlayIcon playing={playing} waiting={waiting} />
      </IconButton>
      <IconButton label={t("speech.previous")} onClick={() => jump(-1)}>
        <SkipBack size={14} strokeWidth={1.75} />
      </IconButton>
      <IconButton label={t("speech.next")} onClick={() => jump(1)}>
        <SkipForward size={14} strokeWidth={1.75} />
      </IconButton>
      {hasNext && (
        <IconButton label={t("listen.nextArticle")} onClick={next}>
          <ChevronsRight size={15} strokeWidth={1.75} />
        </IconButton>
      )}
      <span className="ml-1 min-w-0 flex-1 truncate text-[12px] tabular-nums text-fg-3" aria-live="polite">
        <Progress />
      </span>
      <SleepMark />
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
          <MenuItem key={r} icon={check(r === rate)} onSelect={() => setRate(r)}>
            <span className="tabular-nums">{r}×</span>
          </MenuItem>
        ))}
      </Menu>
      <VoicePicker />
      <ListenMenu />
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
  const source = useSpeech((s) => s.source);
  const playing = useSpeech((s) => s.playing);
  const waiting = useSpeech((s) => s.waiting || s.preparing);
  const openId = useUi((s) => s.openId);
  const hasNext = useHasNext();
  const { toggle, jump, next, stop } = useSpeech.getState();
  // 正在看的就是这一篇时，控制条在文章里，这里不重复
  if (!itemId || openId === itemId) return null;
  return (
    <div className="flex shrink-0 flex-col border-t border-line bg-surface">
      <Caption />
      <div className="flex items-center gap-1 py-1.5 pl-2.5 pr-1.5">
        <button
          type="button"
          onClick={() => useUi.getState().open(itemId)}
          title={source ? `${source} · ${title}` : title}
          className="flex min-w-0 flex-1 items-center gap-2 rounded-md text-left"
        >
          <Volume2 size={16} strokeWidth={1.75} className="shrink-0 text-accent" />
          <span className="min-w-0 flex-1">
            <span className="block truncate text-[12px] font-medium text-fg">{title}</span>
            <span className="block truncate text-[11px] tabular-nums text-fg-3">
              {source && `${source} · `}
              <Progress />
            </span>
          </span>
        </button>
        <SleepMark />
        <IconButton label={waiting ? t("voices.preparing") : playing ? t("speech.pause") : t("speech.play")} onClick={toggle}>
          <PlayIcon playing={playing} waiting={waiting} />
        </IconButton>
        {hasNext ? (
          <IconButton label={t("listen.nextArticle")} onClick={next}>
            <ChevronsRight size={15} strokeWidth={1.75} />
          </IconButton>
        ) : (
          <IconButton label={t("speech.next")} onClick={() => jump(1)}>
            <SkipForward size={14} strokeWidth={1.75} />
          </IconButton>
        )}
        <CaptionToggle />
        <VoicePicker />
        <ListenMenu />
        <IconButton label={t("speech.stop")} onClick={stop}>
          <Square size={13} fill="currentColor" strokeWidth={0} />
        </IconButton>
      </div>
    </div>
  );
}

/** 列表工具栏上的「听这个列表」：从打开的那篇（不在列表里就从第一篇）开始，一篇接一篇往下念。 */
export function ListenButton({ items }: { items: readonly Item[] }) {
  const t = useT();
  const available = useReadAloudAvailable();
  const listening = useSpeech((s) => s.itemId !== null && s.queue.length > 0);
  useEffect(() => {
    void voiceOverview().catch(() => undefined);
  }, []);
  if (!available) return null;
  const listen = () => {
    // 正在听：再点一下就停
    if (listening) {
      useSpeech.getState().stop();
      return;
    }
    const queue = listenQueueOf(items);
    if (queue.length === 0) {
      toast(t("listen.empty"));
      return;
    }
    const openId = useUi.getState().openId;
    const first = queue.find((e) => e.id === openId) ?? queue[0];
    if (first) useSpeech.getState().start(first, null, queue, true);
  };
  return (
    <IconButton label={listening ? t("listen.stopList") : t("listen.fromHere")} active={listening} aria-pressed={listening} onClick={listen}>
      <Headphones {...ICON} />
    </IconButton>
  );
}
