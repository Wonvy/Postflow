// 播完一期后接着放哪一期。只有纯函数，方便测试：npm run test:ui

export interface QueueEntry {
  id: string;
  /** 发布时间，毫秒。 */
  published: number;
  /** 开始放的时候已经标了已读（听完过）。 */
  read: boolean;
}

/**
 * 下一期：
 * - 平常：列表里排在它下面的那一条（列表新的在前，所以通常是更早的一期）。
 * - 「从旧到新」（catchUp）：比它新的里面最早的那一期，跳过已经听完的（列表里标了已读的，
 *   以及这次打开以后放完的 `played`）。用来从头追一个节目。
 */
export function nextEntry<T extends QueueEntry>(
  queue: readonly T[],
  finishedId: string,
  catchUp: boolean,
  played: ReadonlySet<string> = new Set(),
): T | undefined {
  const at = queue.findIndex((e) => e.id === finishedId);
  if (!catchUp) return at >= 0 ? queue[at + 1] : undefined;
  const finished = queue[at];
  if (!finished) return undefined;
  let best: T | undefined;
  for (const e of queue) {
    if (e.id === finishedId || e.read || played.has(e.id)) continue;
    // 同一时间发布的按列表里的位置排：列表新的在前，排在后面的算更早
    const newer =
      e.published > finished.published ||
      (e.published === finished.published && queue.indexOf(e) < at);
    if (!newer) continue;
    if (!best || e.published < best.published || (e.published === best.published && queue.indexOf(e) > queue.indexOf(best))) {
      best = e;
    }
  }
  return best;
}
