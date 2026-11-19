// 字幕的几个纯函数：把新转出来的句子并进已有的、找出现在该显示哪一句、看某个时间转没转过。
// 和核心里的 crates/speech/src/cues.rs 是同一套规则。只有纯函数，方便测试：npm run test:ui

export interface Cue {
  start: number;
  end: number;
  text: string;
  /** 每个字从这句开始算起第几个百分之一秒说出来（唱词效果用）。没有就按字数平均分。 */
  t?: number[];
  /** 第几个说话的人（从 0 数）。 */
  speaker?: number;
}

/** 一句字幕的身份：开始时间加原文。译文按它存。和核心那边一样。 */
export function cueKey(cue: Pick<Cue, "start" | "text">): string {
  return `${Math.round(cue.start * 100)}|${cue.text}`;
}

/** 没有逐字时间时，一个字最多算这么久：句子后面的停顿不拖慢唱词。 */
const CHAR_SECONDS = 0.32;

/**
 * 这一句已经说到第几个字（唱词效果）：已经开始说的字数。有逐字时间按它算，没有就在这句的时长里按字数平均分。
 */
export function spokenChars(cue: Cue, position: number): number {
  const count = Array.from(cue.text).length;
  const elapsed = position - cue.start;
  if (elapsed < 0) return 0;
  if (cue.t && cue.t.length === count) {
    let n = 0;
    while (n < count && (cue.t[n] as number) / 100 <= elapsed) n++;
    return n;
  }
  const span = Math.min(Math.max(cue.end - cue.start, 0), count * CHAR_SECONDS);
  if (span <= 0) return count;
  return Math.min(count, Math.floor((elapsed / span) * count) + 1);
}

/** 字幕少于这么多句时，只说过一句的人也算。 */
const FEW_CUES = 12;

/**
 * 本机分出来的说话人在界面上的编号（从 0 数）：按第一次开口的先后排，这样主要的两个人总是 1 号、2 号。
 * 整期只说过一句的不给编号（多半是笑声、插话或者分错了），这一句就不标是谁；字幕还很少时都算。
 */
export function speakerOrder(cues: readonly Cue[]): Map<number, number> {
  const count = new Map<number, number>();
  for (const c of cues) if (c.speaker !== undefined) count.set(c.speaker, (count.get(c.speaker) ?? 0) + 1);
  const strict = cues.length >= FEW_CUES;
  const order = new Map<number, number>();
  for (const c of cues) {
    const s = c.speaker;
    if (s === undefined || order.has(s)) continue;
    if (strict && (count.get(s) ?? 0) < 2) continue;
    order.set(s, order.size);
  }
  return order;
}

/** 这一句说到哪儿了（0–1），进度条用。 */
export function cueProgress(cue: Cue, position: number): number {
  const count = Array.from(cue.text).length;
  if (count === 0) return 1;
  return Math.max(0, Math.min(1, spokenChars(cue, position) / count));
}

/**
 * 把新转出来的几句并进已有的字幕：落在 `[from, to)` 里的旧句子先去掉（重转的那一段），再按时间排好。
 * 同一批句子并两次，结果和并一次一样。
 */
export function mergeCues(all: readonly Cue[], from: number | undefined, to: number | undefined, fresh: readonly Cue[]): Cue[] {
  if (fresh.length === 0 && (from === undefined || to === undefined || to <= from)) return all as Cue[];
  const starts = new Set(fresh.map((c) => c.start));
  const kept = all.filter((c) => {
    if (starts.has(c.start)) return false;
    return from === undefined || to === undefined || c.start < from || c.start >= to;
  });
  return [...kept, ...fresh].sort((a, b) => a.start - b.start);
}

/** 句子说完以后还留在屏幕上这么久（秒），除非下一句已经开始。 */
const LINGER = 1.2;
/** 提前这么一点显示：播放位置是每秒报几次的，不提前会显得字幕慢半拍。 */
const LEAD = 0.25;

/** 现在该显示第几句；没有就是 -1。`cues` 按开始时间排好。 */
export function cueAt(cues: readonly Cue[], position: number): number {
  let lo = 0;
  let hi = cues.length - 1;
  let found = -1;
  const at = position + LEAD;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if ((cues[mid] as Cue).start <= at) {
      found = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  if (found === -1) return -1;
  return position <= (cues[found] as Cue).end + LINGER ? found : -1;
}

/** 第 `position` 秒转没转过。 */
export function isCovered(covered: readonly (readonly [number, number])[], position: number): boolean {
  return covered.some(([a, b]) => a <= position + 0.05 && position < b);
}

/** 转完了百分之多少。不知道总长度时是 null。 */
export function coveredPercent(covered: readonly (readonly [number, number])[], duration: number | null | undefined): number | null {
  if (!duration || duration <= 0) return null;
  const done = covered.reduce((sum, [a, b]) => sum + Math.max(0, Math.min(b, duration) - Math.max(a, 0)), 0);
  return Math.max(0, Math.min(100, Math.floor((done / duration) * 100)));
}
