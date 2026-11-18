// 章节和文字稿的读法。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { activeIndex, parseChapters, parseTranscript } from "./transcript.ts";

test("chapters", () => {
  const json = JSON.stringify({
    version: "1.2.0",
    chapters: [
      { startTime: 120.5, title: "News" },
      { startTime: 0, title: "Intro" },
      { startTime: 60, title: "Ad", toc: false },
      { startTime: "x", title: "Broken" },
    ],
  });
  assert.deepEqual(parseChapters(json), [
    { start: 0, title: "Intro" },
    { start: 120.5, title: "News" },
  ]);
  assert.deepEqual(parseChapters("nope"), []);
});

test("VTT with speakers, joined into sentences", () => {
  const vtt = `WEBVTT

00:00:01.000 --> 00:00:03.000
<v Ann>Hello and

00:00:03.000 --> 00:00:05.000
<v Ann>welcome back.

01:02:03.500 --> 01:02:05.000
<v Bob>Thanks &amp; hi!`;
  assert.deepEqual(parseTranscript(vtt, "text/vtt"), [
    { start: 1, speaker: "Ann", text: "Hello and welcome back." },
    { start: 3723.5, speaker: "Bob", text: "Thanks & hi!" },
  ]);
});

test("SRT, JSON and HTML transcripts", () => {
  const srt = "1\n00:00:02,500 --> 00:00:04,000\nFirst line.\n\n2\n00:00:05,000 --> 00:00:06,000\nSecond.";
  assert.deepEqual(
    parseTranscript(srt, "application/x-subrip").map((c) => [c.start, c.text]),
    [
      [2.5, "First line."],
      [5, "Second."],
    ],
  );
  const json = JSON.stringify({
    segments: [
      { startTime: 0, speaker: "A", body: "Hi" },
      { startTime: 0.4, speaker: "A", body: "there." },
      { startTime: 1, speaker: "B", body: "Yo" },
    ],
  });
  assert.deepEqual(parseTranscript(json, "application/json"), [
    { start: 0, speaker: "A", text: "Hi there." },
    { start: 1, speaker: "B", text: "Yo" },
  ]);
  assert.deepEqual(parseTranscript("<p>One</p><p>Two &amp; three</p>", "text/html"), [
    { start: null, text: "One" },
    { start: null, text: "Two & three" },
  ]);
});

test("the cue being played", () => {
  const cues = [{ start: 0 }, { start: 10 }, { start: null }, { start: 20 }];
  assert.equal(activeIndex(cues, 0), 0);
  assert.equal(activeIndex(cues, 15), 1);
  assert.equal(activeIndex(cues, 25), 3);
});
