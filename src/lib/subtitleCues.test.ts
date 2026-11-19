import assert from "node:assert/strict";
import { test } from "node:test";
import {
  type Cue,
  coveredPercent,
  cueAt,
  cueKey,
  cueProgress,
  isCovered,
  mergeCues,
  speakerOrder,
  spokenChars,
} from "./subtitleCues.ts";

const cue = (start: number, text: string, end = start + 2): Cue => ({ start, end, text });

test("new cues are merged in order, replacing the redone range", () => {
  const all = [cue(1, "a"), cue(5, "old"), cue(9, "c")];
  const merged = mergeCues(all, 4, 8, [cue(6, "new2"), cue(4.5, "new1")]);
  assert.deepEqual(merged.map((c) => c.text), ["a", "new1", "new2", "c"]);
  // 同一批并两次，结果一样
  const again = mergeCues(merged, 4, 8, [cue(6, "new2"), cue(4.5, "new1")]);
  assert.deepEqual(again, merged);
});

test("cues without a range are simply added, and nothing changes when there is nothing new", () => {
  const all = [cue(1, "a")];
  assert.deepEqual(
    mergeCues(all, undefined, undefined, [cue(3, "b")]).map((c) => c.text),
    ["a", "b"],
  );
  assert.equal(mergeCues(all, undefined, undefined, []), all);
  // 只有范围、没有新句子：这一段原来的句子去掉（这段时间里没人说话）
  assert.deepEqual(mergeCues([cue(1, "a"), cue(5, "b")], 4, 8, []).map((c) => c.text), ["a"]);
});

test("the cue shown follows the playback position", () => {
  const cues = [cue(10, "one", 12), cue(12, "two", 15), cue(30, "three", 33)];
  assert.equal(cueAt(cues, 5), -1);
  assert.equal(cueAt(cues, 9.9), 0, "shown slightly ahead of time");
  assert.equal(cueAt(cues, 11), 0);
  assert.equal(cueAt(cues, 12.5), 1);
  assert.equal(cueAt(cues, 15.5), 1, "lingers a little after the sentence ends");
  assert.equal(cueAt(cues, 20), -1, "gone during a long pause");
  assert.equal(cueAt(cues, 31), 2);
  assert.equal(cueAt([], 3), -1);
});

test("coverage", () => {
  const covered: [number, number][] = [
    [0, 30],
    [60, 100],
  ];
  assert.equal(isCovered(covered, 10), true);
  assert.equal(isCovered(covered, 30), false);
  assert.equal(isCovered(covered, 45), false);
  assert.equal(isCovered(covered, 60), true);
  assert.equal(coveredPercent(covered, 100), 70);
  assert.equal(coveredPercent(covered, null), null);
  assert.equal(coveredPercent([[0, 120]], 100), 100);
});

test("characters light up as they are spoken", () => {
  // 有逐字时间：按它
  const timed: Cue = { start: 10, end: 14, text: "你好朋友", t: [0, 50, 150, 160] };
  assert.equal(spokenChars(timed, 9), 0);
  assert.equal(spokenChars(timed, 10), 1);
  assert.equal(spokenChars(timed, 10.6), 2);
  assert.equal(spokenChars(timed, 11.55), 3);
  assert.equal(spokenChars(timed, 13), 4);
  assert.equal(cueProgress(timed, 10.6), 0.5);
  // 没有：在时长里平均分，但一个字最多 0.32 秒，后面的停顿不拖慢
  const plain: Cue = { start: 0, end: 30, text: "abcd" };
  assert.equal(spokenChars(plain, 0), 1);
  assert.equal(spokenChars(plain, 0.7), 3);
  assert.equal(spokenChars(plain, 2), 4);
  // 逐字时间的个数对不上（比如旧数据）就当没有
  assert.equal(spokenChars({ start: 0, end: 1, text: "ab", t: [0] }, 0.6), 2);
  // 按字符数算，不按 UTF-16
  assert.equal(spokenChars({ start: 0, end: 1, text: "😀😀", t: [0, 50] }, 0.6), 2);
});

test("cue keys match the core", () => {
  assert.equal(cueKey({ start: 12.345, text: "你好" }), "1235|你好");
});

test("speakers are numbered by who talks first, and one-off voices get no number", () => {
  const said = (speaker: number | undefined, i: number): Cue => ({ start: i, end: i + 1, text: "x", speaker });
  // 3 号先开口，然后是 0 号；1 号整期只有一句（插话、笑声），不给编号
  const order = [3, 3, 0, 1, 3, 0, 0, 3, undefined, 0, 3, 0, 3];
  const cues = order.map(said);
  const map = speakerOrder(cues);
  assert.equal(map.get(3), 0);
  assert.equal(map.get(0), 1);
  assert.equal(map.has(1), false);
  // 字幕还很少时，说过一句的也算
  const few = speakerOrder([said(2, 0), said(5, 1)]);
  assert.deepEqual([...few.entries()], [
    [2, 0],
    [5, 1],
  ]);
});
