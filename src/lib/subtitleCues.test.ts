import assert from "node:assert/strict";
import { test } from "node:test";
import { type Cue, coveredPercent, cueAt, isCovered, mergeCues } from "./subtitleCues.ts";

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
