// 字幕的几个纯函数：把新转出来的句子并进已有的、找出现在该显示哪一句、看某个时间转没转过。
// 和核心里的 crates/speech/src/cues.rs 是同一套规则。只有纯函数，方便测试：npm run test:ui

export interface Cue {
  start: number;
  end: number;
  text: string;
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
