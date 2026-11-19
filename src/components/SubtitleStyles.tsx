// 字幕的十一种样子（设置 → 字幕 → 字幕样式，播放条上字幕按钮旁边的菜单也能换）。
//
// - 逐字的效果（逐字高亮、打字机、弹跳、重点强调、粒子）按每个字说出来的时间走：本机识别的字幕
//   有逐字时间；节目自带的文字稿没有，就在这一句的时长里按字数平均分（subtitleCues.ts 的 spokenChars）。
// - 分出了说话人时，这一句用这个人的颜色（styles.css 里的 --spk-0 … --spk-7）。
// - 动效都写在 styles.css 里（.sub-*），系统开了「减少动态效果」时只留颜色变化。

import { type CSSProperties, type ReactNode, useEffect, useState } from "react";
import { playbackTime } from "../lib/player";
import { type Cue, cueKey, cueProgress, spokenChars } from "../lib/subtitleCues";
import type { SubtitleStyle } from "../lib/subtitles";
import { cx } from "./ui";

/** 逐字变化的样子，时钟要细一点。 */
const PER_CHAR: ReadonlySet<SubtitleStyle> = new Set(["karaoke", "typewriter", "pop", "emphasis", "particles"]);

/** 这个样子每秒要更新几次。 */
export function clockRate(style: SubtitleStyle): number {
  return PER_CHAR.has(style) ? 30 : 8;
}

/**
 * 播放到第几秒，放着的时候每秒更新 `fps` 次（播放器的状态一秒才变一次，逐字效果要更细）。
 * 停着的时候就是确切的当前时间。`base` 是播放器状态里的位置，读不到确切时间时用它。
 */
export function usePlaybackClock(running: boolean, base: number, fps: number): number {
  const [time, setTime] = useState(base);
  useEffect(() => {
    if (!running) {
      setTime(playbackTime() ?? base);
      return;
    }
    let frame = 0;
    let last = -1;
    const tick = () => {
      const now = playbackTime() ?? base;
      const step = Math.floor(now * fps);
      if (step !== last) {
        last = step;
        setTime(now);
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [running, base, fps]);
  return time;
}

/** 预览用：一直在 0 到 `period` 秒之间循环的时钟。 */
export function useLoopClock(period: number, fps = 30, running = true): number {
  const [time, setTime] = useState(0);
  useEffect(() => {
    if (!running) return;
    const start = performance.now();
    let frame = 0;
    let last = -1;
    const tick = () => {
      const now = ((performance.now() - start) / 1000) % period;
      const step = Math.floor(now * fps);
      if (step !== last) {
        last = step;
        setTime(now);
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [period, fps, running]);
  return time;
}

interface Glyph {
  ch: string;
  /** 在这一句里是第几个字（按码点数）。 */
  i: number;
  space: boolean;
}

/** 拆成一个个字。连着的拉丁字母、数字算一个词，包起来不让中间断行；中文每个字都能断。 */
function tokenize(text: string): Glyph[][] {
  const out: Glyph[][] = [];
  let i = 0;
  for (const match of text.matchAll(/[A-Za-z0-9À-ɏЀ-ӿ'’-]+|[\s\S]/gu)) {
    const word = Array.from(match[0]).map((ch) => ({ ch, i: i++, space: /\s/u.test(ch) }));
    out.push(word);
  }
  return out;
}

/** 一句话，每个字一个 span：已经说出来的带 data-on，正在说的带 data-now。`caret` 时在说到的地方放个光标。 */
function Glyphs({ text, spoken, now, caret }: { text: string; spoken: number; now: number; caret?: boolean }) {
  const words = tokenize(text);
  const total = words.reduce((n, w) => n + w.length, 0);
  const caretEl = <span key="caret" className="sub-caret" aria-hidden />;
  const glyph = (g: Glyph) => (
    <span
      key={g.i}
      className={g.space ? "sub-ws" : "sub-char"}
      data-on={g.i < spoken ? "" : undefined}
      data-now={g.i === now ? "" : undefined}
    >
      {g.ch}
    </span>
  );
  return (
    <>
      {words.map((w) => {
        const first = w[0] as Glyph;
        const parts: ReactNode[] = [];
        for (const g of w) {
          if (caret && g.i === spoken) parts.push(caretEl);
          parts.push(glyph(g));
        }
        return w.length > 1 ? (
          <span key={`w${first.i}`} className="whitespace-nowrap">
            {parts}
          </span>
        ) : (
          parts
        );
      })}
      {caret && spoken >= total && caretEl}
    </>
  );
}

/** 背后往上飘的光点：位置、大小、快慢都是固定算出来的，每次渲染一样。 */
const SPARKS = Array.from({ length: 10 }, (_, i) => ({
  left: `${(i * 37 + 9) % 100}%`,
  "--s": `${2 + (i % 3)}px`,
  "--d": `${3.4 + (i % 4) * 0.8}s`,
  "--delay": `${((i * 0.61) % 3.2).toFixed(2)}s`,
  "--dx": `${((i % 5) - 2) * 7}px`,
}));

function Waves() {
  // 两个波长：宽 200%，往左移一半正好接上
  const path = "M0 10 Q 25 0 50 10 T 100 10 T 150 10 T 200 10 V 20 H 0 Z";
  return (
    <>
      <svg className="sub-wave-a" viewBox="0 0 200 20" preserveAspectRatio="none" aria-hidden>
        <path d={path} />
      </svg>
      <svg className="sub-wave-b" viewBox="0 0 200 20" preserveAspectRatio="none" aria-hidden>
        <path d={path} />
      </svg>
    </>
  );
}

export interface SubtitleSpeaker {
  /** 第几个人（从 0 数），决定颜色。 */
  index: number;
  name: string;
}

export interface SubtitleViewProps {
  style: SubtitleStyle;
  cues: readonly Cue[];
  /** 现在显示第几句；没有是 -1。 */
  index: number;
  /** 播放到第几秒。 */
  time: number;
  /** 这一句的译文。 */
  translation?: string;
  /** 这一句是谁说的（开着「区分说话人」、也分出来了才有）。 */
  speaker?: SubtitleSpeaker | null;
  /** 迷你播放器里的一小行。 */
  compact?: boolean;
  /** 没有句子可显示时显示的（在等什么）。 */
  placeholder?: ReactNode;
  /** 译文那里的说明（缺模型、出错、正在翻译）。 */
  note?: ReactNode;
  className?: string;
}

/** 说话人的颜色变量。 */
export function speakerVars(speaker: SubtitleSpeaker | null | undefined): CSSProperties | undefined {
  if (!speaker) return undefined;
  return { "--sub-color": `var(--spk-${speaker.index % 8})` } as CSSProperties;
}

export function SubtitleView({
  style,
  cues,
  index,
  time,
  translation,
  speaker,
  compact = false,
  placeholder,
  note,
  className,
}: SubtitleViewProps) {
  const cue = index >= 0 ? cues[index] : undefined;
  const count = cue ? Array.from(cue.text).length : 0;
  const spoken = cue ? spokenChars(cue, time) : 0;
  // 正在说的那个字：这一句还没说完时，最后说出来的那个
  const now = cue && spoken > 0 && spoken < count && time <= cue.end ? spoken - 1 : -1;
  const vars = speakerVars(speaker);
  const dot = speaker ? <span className="sub-dot-speaker" title={speaker.name} aria-hidden /> : null;
  const key = cue ? cueKey(cue) : "none";
  const glyphs = cue ? <Glyphs text={cue.text} spoken={spoken} now={now} caret={style === "typewriter"} /> : null;

  if (compact) {
    return (
      <div
        className={cx("sub px-3 pt-1.5 text-center text-[12px] leading-snug text-fg touch:text-[13px]", PER_CHAR.has(style) && `sub-${style}`, className)}
        style={vars}
        aria-live="off"
        title={cue?.text}
      >
        {cue ? (
          <div key={key} className={cx("line-clamp-2", !PER_CHAR.has(style) && "sub-fade")}>
            {dot}
            {PER_CHAR.has(style) ? glyphs : cue.text}
          </div>
        ) : (
          <span className="text-fg-3">{placeholder ?? " "}</span>
        )}
        {cue && translation && <div className="truncate text-[11px] text-fg-2">{translation}</div>}
      </div>
    );
  }

  const status = (content: ReactNode) => <span className="text-[12px] text-fg-3">{content}</span>;
  const below = (
    <>
      {cue && translation && style !== "bilingual" && (
        <div key={`tr:${key}`} className="sub-enter mt-1 text-[13px] leading-snug text-fg-2">
          {translation}
        </div>
      )}
      {note && <div className="mt-1 text-[11px] text-fg-3">{note}</div>}
    </>
  );
  const frame = "mt-2 flex min-h-[44px] flex-col items-center justify-center px-2 text-center leading-snug text-fg";

  switch (style) {
    case "karaoke":
    case "typewriter":
    case "pop":
    case "emphasis":
      return (
        <div className={cx("sub", `sub-${style}`, frame, className)} style={vars} aria-live="off">
          {cue ? (
            <div
              key={key}
              className={cx(
                "select-text",
                style === "emphasis" ? "text-[18px]" : "text-[16px]",
                style === "pop" ? "font-semibold" : style === "karaoke" && "font-medium",
              )}
            >
              {dot}
              {glyphs}
            </div>
          ) : (
            status(placeholder)
          )}
          {below}
        </div>
      );

    case "particles":
      return (
        <div className={cx("sub sub-particles overflow-hidden py-1", frame, className)} style={vars} aria-live="off">
          {SPARKS.map((s) => (
            <span key={s.left} className="sub-spark" style={s as CSSProperties} aria-hidden />
          ))}
          {cue ? (
            <div key={key} className="relative select-text text-[16px]">
              {dot}
              {glyphs}
            </div>
          ) : (
            status(placeholder)
          )}
          {below}
        </div>
      );

    case "glass":
    case "wave":
      return (
        <div className={cx("sub mt-2", `sub-${style}`, className)} style={vars} aria-live="off">
          {style === "wave" && <Waves />}
          <div
            className={cx(
              "relative flex min-h-[52px] flex-col items-center justify-center px-4 py-2.5 text-center leading-snug text-fg",
              style === "glass" && "sub-glass-panel",
            )}
          >
            {cue ? (
              <div key={key} className="sub-enter select-text text-[15px]">
                {dot}
                {cue.text}
              </div>
            ) : (
              status(placeholder)
            )}
            {below}
            {style === "glass" && cue && (
              <div className="absolute inset-x-4 bottom-1.5">
                <div className="sub-bar" style={{ transform: `scaleX(${cueProgress(cue, time)})` }} />
              </div>
            )}
          </div>
        </div>
      );

    case "bilingual":
      return (
        <div className={cx("sub sub-bilingual", frame, className)} style={vars} aria-live="off">
          {cue ? (
            <div key={key} className="sub-enter select-text">
              <div className="text-[15px] text-fg">
                {dot}
                {cue.text}
              </div>
              {translation && (
                <>
                  <div className="sub-rule" aria-hidden />
                  <div className="text-[14px]" style={{ color: "var(--sub-color)" }}>
                    {translation}
                  </div>
                </>
              )}
            </div>
          ) : (
            status(placeholder)
          )}
          {note && <div className="mt-1 text-[11px] text-fg-3">{note}</div>}
        </div>
      );

    case "scroll": {
      const prev = index > 0 ? cues[index - 1] : undefined;
      const next = index >= 0 ? cues[index + 1] : undefined;
      return (
        <div className={cx("sub sub-scroll mt-1 px-2 text-center leading-snug", className)} style={vars} aria-live="off">
          {cue ? (
            <div key={key} className="sub-scroll-track flex flex-col gap-1 py-1">
              <div className="sub-scroll-side truncate">{prev?.text ?? " "}</div>
              <div className="select-text text-[15px] text-fg">
                {dot}
                {cue.text}
              </div>
              {translation && <div className="text-[13px] text-fg-2">{translation}</div>}
              <div className="sub-scroll-side truncate">{next?.text ?? " "}</div>
            </div>
          ) : (
            <div className="flex min-h-[44px] items-center justify-center">{status(placeholder)}</div>
          )}
          {note && <div className="pb-1 text-[11px] text-fg-3">{note}</div>}
        </div>
      );
    }

    case "speaker":
      return (
        <div className={cx("sub mt-2 flex min-h-[44px] flex-col justify-center px-2", className)} style={vars} aria-live="off">
          {cue ? (
            // 标签按人换：同一个人接着说时不重新滑进来，换了人才滑
            <div className={cx("select-text text-left", speaker && "sub-speaker-line pl-3")}>
              {speaker && (
                <div key={`s:${speaker.index}`} className="sub-slide mb-1">
                  <span className="sub-tag">{speaker.name}</span>
                </div>
              )}
              <div key={key} className="sub-enter text-[15px] leading-snug text-fg">
                {cue.text}
              </div>
              {translation && (
                <div key={`tr:${key}`} className="sub-enter mt-0.5 text-[13px] leading-snug text-fg-2">
                  {translation}
                </div>
              )}
            </div>
          ) : (
            <div className="text-center">{status(placeholder)}</div>
          )}
          {note && <div className="mt-1 text-[11px] text-fg-3">{note}</div>}
        </div>
      );

    case "minimal":
      return (
        <div className={cx("sub sub-minimal", frame, className)} style={vars} aria-live="off">
          {cue ? (
            <div key={key} className="sub-fade select-text text-[14px]">
              {dot}
              {cue.text}
            </div>
          ) : (
            status(placeholder)
          )}
          {below}
        </div>
      );
  }
}
