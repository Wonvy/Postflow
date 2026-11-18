// 播客单集的章节和文字稿（Podcasting 2.0：<podcast:chapters>、<podcast:transcript>）。
// 文件由核心去取（media.text），这里只是把内容读成列表。只有纯函数，方便测试：npm run test:ui

export interface Chapter {
  /** 开始时间，秒。 */
  start: number;
  title: string;
}

/** 文字稿里的一段：有时间的可以点着跳过去。 */
export interface Cue {
  start: number | null;
  speaker?: string;
  text: string;
}

/** 章节 JSON：{ chapters: [{ startTime, title, toc }] }。toc 为 false 的不列出来。 */
export function parseChapters(json: string): Chapter[] {
  let data: unknown;
  try {
    data = JSON.parse(json);
  } catch {
    return [];
  }
  const list = (data as { chapters?: unknown })?.chapters;
  if (!Array.isArray(list)) return [];
  return list
    .map((c) => c as { startTime?: unknown; title?: unknown; toc?: unknown })
    .filter((c) => typeof c.startTime === "number" && Number.isFinite(c.startTime) && c.toc !== false)
    .map((c) => ({ start: Math.max(0, c.startTime as number), title: typeof c.title === "string" ? c.title.trim() : "" }))
    .filter((c) => c.title !== "")
    .sort((a, b) => a.start - b.start);
}

/** 「01:02:03.500」「02:03,5」这样的时间，秒。 */
function clock(text: string): number | null {
  const m = /^(?:(\d+):)?(\d{1,2}):(\d{2})(?:[.,](\d{1,3}))?$/.exec(text.trim());
  if (!m) return null;
  const [, h, min, s, ms] = m;
  return Number(h ?? 0) * 3600 + Number(min) * 60 + Number(s) + (ms ? Number(ms.padEnd(3, "0")) / 1000 : 0);
}

const decode = (text: string) =>
  text
    .replace(/&nbsp;/g, " ")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&amp;/g, "&");

/** VTT、SRT：一段一段，每段有时间行。VTT 的 <v 名字> 是说话的人。 */
function parseTimed(text: string): Cue[] {
  const cues: Cue[] = [];
  for (const block of text.replace(/\r/g, "").split(/\n{2,}/)) {
    const lines = block.split("\n").filter((l) => l.trim() !== "");
    const timing = lines.findIndex((l) => l.includes("-->"));
    if (timing === -1) continue;
    const start = clock(lines[timing]?.split("-->")[0] ?? "");
    let body = lines.slice(timing + 1).join(" ");
    let speaker: string | undefined;
    const voice = /<v(?:\.[^ >]*)?\s+([^>]+)>/.exec(body);
    if (voice) speaker = voice[1]?.trim();
    body = decode(body.replace(/<[^>]+>/g, "")).replace(/\s+/g, " ").trim();
    if (!body) continue;
    const last = cues[cues.length - 1];
    // 同一个人连着说的、不到一句话的，并成一段，读起来不碎
    if (last && last.speaker === speaker && !/[.!?。！？]$/.test(last.text) && last.text.length < 200) {
      last.text = `${last.text} ${body}`;
    } else {
      cues.push({ start, speaker, text: body });
    }
  }
  return cues;
}

/** JSON 文字稿：{ segments: [{ startTime, speaker, body }] }，常常一个词一段，按句子并起来。 */
function parseJsonTranscript(text: string): Cue[] {
  let data: unknown;
  try {
    data = JSON.parse(text);
  } catch {
    return [];
  }
  const segments = (data as { segments?: unknown })?.segments;
  if (!Array.isArray(segments)) return [];
  const cues: Cue[] = [];
  for (const raw of segments) {
    const s = raw as { startTime?: unknown; speaker?: unknown; body?: unknown };
    if (typeof s.body !== "string" || !s.body.trim()) continue;
    const start = typeof s.startTime === "number" ? s.startTime : null;
    const speaker = typeof s.speaker === "string" ? s.speaker : undefined;
    const last = cues[cues.length - 1];
    if (last && last.speaker === speaker && !/[.!?。！？]$/.test(last.text) && last.text.length < 200) {
      last.text = `${last.text} ${s.body.trim()}`.replace(/\s+([,.!?;:，。！？；：])/g, "$1");
    } else {
      cues.push({ start, speaker, text: s.body.trim() });
    }
  }
  return cues;
}

/** HTML、纯文本：没有时间，按段落分开。 */
function parsePlain(text: string, html: boolean): Cue[] {
  const plain = html
    ? decode(text.replace(/<(script|style)[\s\S]*?<\/\1>/gi, "").replace(/<\/(p|div|li|h\d)>|<br\s*\/?>/gi, "\n\n").replace(/<[^>]+>/g, ""))
    : text;
  return plain
    .replace(/\r/g, "")
    .split(/\n{2,}/)
    .map((p) => p.replace(/\s+/g, " ").trim())
    .filter(Boolean)
    .map((p) => ({ start: null, text: p }));
}

export function parseTranscript(text: string, mime: string): Cue[] {
  const type = mime.toLowerCase();
  if (type === "text/vtt" || type.includes("subrip") || type.includes("srt") || /^WEBVTT/.test(text.trimStart())) {
    return parseTimed(text);
  }
  if (type === "application/json") return parseJsonTranscript(text);
  return parsePlain(text, type === "text/html");
}

/** 正在放到的那一段（时间不晚于现在的最后一段）。 */
export function activeIndex<T extends { start: number | null }>(list: readonly T[], position: number): number {
  let found = -1;
  list.forEach((c, i) => {
    if (c.start !== null && c.start <= position + 0.25) found = i;
  });
  return found;
}
