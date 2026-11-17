// 侧栏「订阅」一组的顶层顺序：文件夹和不在文件夹里的订阅源可以交错排列。
//
// 顶层顺序记在同步的设置 `folder_order` 里。字符串是文件夹名（旧版本只认这种），
// `{ "feed": id }` 是文件夹外的订阅源。旧版本读到时只取字符串，照样能用，
// 只是文件夹外的源会排在所有文件夹后面。
//
// 这里只有纯函数，方便测试：npm run test:ui

import type { Feed } from "./api";

export type TopEntry = { kind: "folder"; name: string } | { kind: "feed"; id: string };
export type SavedEntry = string | { feed: string };

type FeedLike = Pick<Feed, "id" | "folder">;

function savedFolders(saved: unknown): string[] {
  return Array.isArray(saved) ? saved.filter((x): x is string => typeof x === "string" && x.trim() !== "") : [];
}

/** 文件夹的显示顺序：设置里记着的在前（包括空文件夹），其余按名称排在后面。 */
export function orderFolders(feeds: FeedLike[], saved: unknown): string[] {
  const used = [...new Set(feeds.map((f) => f.folder).filter(Boolean))].sort((a, b) => a.localeCompare(b));
  return [...new Set([...savedFolders(saved), ...used])];
}

/**
 * 顶层的实际顺序。设置里记着的在前；已经不存在、或者已经放进文件夹的源跳过；
 * 没记着的文件夹接在后面，没记着的文件夹外的源（比如刚订阅的）排在最后。
 */
export function topLevel(feeds: FeedLike[], saved: unknown): TopEntry[] {
  const loose = feeds.filter((f) => !f.folder).map((f) => f.id);
  const looseSet = new Set(loose);
  const out: TopEntry[] = [];
  const seenFolders = new Set<string>();
  const seenFeeds = new Set<string>();
  for (const x of Array.isArray(saved) ? saved : []) {
    if (typeof x === "string") {
      if (x.trim() !== "" && !seenFolders.has(x)) {
        seenFolders.add(x);
        out.push({ kind: "folder", name: x });
      }
    } else if (x && typeof x === "object" && "feed" in x) {
      const id = (x as { feed: unknown }).feed;
      if (typeof id === "string" && looseSet.has(id) && !seenFeeds.has(id)) {
        seenFeeds.add(id);
        out.push({ kind: "feed", id });
      }
    }
  }
  for (const name of orderFolders(feeds, saved)) {
    if (!seenFolders.has(name)) out.push({ kind: "folder", name });
  }
  for (const id of loose) {
    if (!seenFeeds.has(id)) out.push({ kind: "feed", id });
  }
  return out;
}

export function serializeTop(entries: TopEntry[]): SavedEntry[] {
  return entries.map((e) => (e.kind === "folder" ? e.name : { feed: e.id }));
}

export function sameEntry(a: TopEntry, b: TopEntry): boolean {
  return a.kind === "folder" ? b.kind === "folder" && a.name === b.name : b.kind === "feed" && a.id === b.id;
}

/**
 * 把 `moved` 挪到 `anchor` 前面或后面。`anchor` 为空时放到最前（before）或最后（after）。
 * `moved` 原来不在顶层（比如从文件夹里拖出来的源）也行。
 */
export function moveTop(
  entries: TopEntry[],
  moved: TopEntry,
  anchor: TopEntry | null,
  position: "before" | "after",
): TopEntry[] {
  const rest = entries.filter((e) => !sameEntry(e, moved));
  if (!anchor) return position === "before" ? [moved, ...rest] : [...rest, moved];
  const at = rest.findIndex((e) => sameEntry(e, anchor));
  if (at === -1) return [...rest, moved];
  rest.splice(position === "before" ? at : at + 1, 0, moved);
  return rest;
}

/** 新建的文件夹排在最后一个文件夹后面；还没有文件夹时排在最前。 */
export function insertFolder(entries: TopEntry[], name: string): TopEntry[] {
  if (entries.some((e) => e.kind === "folder" && e.name === name)) return entries;
  let last = -1;
  entries.forEach((e, i) => {
    if (e.kind === "folder") last = i;
  });
  const out = [...entries];
  out.splice(last + 1, 0, { kind: "folder", name });
  return out;
}

/** 删掉文件夹：里面的源留在文件夹原来的位置，按原来的顺序。 */
export function removeFolder(entries: TopEntry[], name: string, feedIds: string[]): TopEntry[] {
  const inside = new Set(feedIds);
  const freed: TopEntry[] = feedIds.map((id) => ({ kind: "feed", id }));
  const out: TopEntry[] = [];
  for (const e of entries) {
    if (e.kind === "folder" && e.name === name) out.push(...freed);
    else if (!(e.kind === "feed" && inside.has(e.id))) out.push(e);
  }
  return out;
}

/** 改名。新名字已经有了就并进那个文件夹，原来的位置不再保留。 */
export function renameFolder(entries: TopEntry[], from: string, to: string): TopEntry[] {
  const merged = entries.some((e) => e.kind === "folder" && e.name === to);
  return entries.flatMap((e): TopEntry[] =>
    e.kind === "folder" && e.name === from ? (merged ? [] : [{ kind: "folder", name: to }]) : [e],
  );
}
