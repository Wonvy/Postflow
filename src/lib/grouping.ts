// 列表里「折叠相同邮件」：把同一会话或同一发件人的条目合成一行。
//
// 只在界面上分组，数据本身不变。折叠着的一组在列表里占一行，代表它的是最新的那一条；
// 对这一行做的操作（归档、星标、已读、勾选）作用到组里的每一条。

import type { Item } from "./api";

export type GroupMode = "none" | "thread" | "sender" | "mailbox";

export interface Group {
  key: string;
  /** 按时间从新到旧。 */
  items: Item[];
  /** 最新的那一条，折叠时在列表里代表整组。 */
  lead: Item;
  unread: number;
  starred: boolean;
  hasAttachments: boolean;
  /** 组里出现过的发件人或来源，去重后按出现顺序。 */
  people: string[];
}

export type Row =
  | { kind: "item"; item: Item; child: boolean }
  | { kind: "group"; group: Group; expanded: boolean };

/** 列表里能停留、能打开的位置。折叠的组算一个，成员是组里所有条目。 */
export interface Stop {
  id: string;
  members: string[];
}

// 回复、转发的前缀，各种语言的写法都去掉，可能叠好几层：「Re: 回复：Fwd: 主题」
const PREFIX = /^\s*(re|fw|fwd|aw|sv|antw|tr|rif|回复|答复|回覆|转发|轉寄|转寄)\s*(\[\d+\]|\(\d+\))?\s*[:：]\s*/i;

/** 会话主题：去掉回复、转发前缀，忽略大小写和多余空白。 */
export function threadSubject(title: string): string {
  let subject = title.trim();
  for (;;) {
    const next = subject.replace(PREFIX, "");
    if (next === subject) break;
    subject = next;
  }
  return subject.replace(/\s+/g, " ").trim().toLowerCase();
}

/** 比较文章链接用的形式：不管 http/https、www、结尾的斜杠、#后面的部分和 utm_ 之类的跟踪参数。 */
export function articleUrlKey(url: string): string | null {
  let u: URL;
  try {
    u = new URL(url.trim());
  } catch {
    return null;
  }
  if (u.protocol !== "http:" && u.protocol !== "https:") return null;
  for (const name of [...u.searchParams.keys()]) {
    if (/^(utm_|mc_|fbclid$|gclid$|ref$|ref_src$|spm$|from$|source$)/i.test(name)) u.searchParams.delete(name);
  }
  const query = u.searchParams.toString();
  const host = u.hostname.toLowerCase().replace(/^www\./, "");
  const path = u.pathname.replace(/\/+$/, "");
  return `${host}${path}${query ? `?${query}` : ""}`;
}

/** 标题够长时才拿来认重复，免得「周报」「Weekly update」这种通用标题把不相干的文章并在一起。 */
const DUPLICATE_TITLE_MIN = 12;

/**
 * 同一篇文章被几个订阅源转发：链接指向同一个网页，或者标题一字不差（够长的标题），只显示一次。
 * 折成一组，展开能看到每个源的那一条。
 */
export function duplicateKey(item: Item): string | null {
  if (item.kind !== "article") return null;
  const url = item.url ? articleUrlKey(item.url) : null;
  if (url) return `d:u:${url}`;
  const title = item.title.replace(/\s+/g, " ").trim().toLowerCase();
  return [...title].length >= DUPLICATE_TITLE_MIN ? `d:t:${title}` : null;
}

function keyOf(item: Item, mode: GroupMode): string | null {
  switch (mode) {
    case "none":
      return duplicateKey(item);
    case "thread": {
      // 文章没有会话的概念，只把重复的并在一起；没有主题的邮件也不硬凑到一起
      if (item.kind !== "mail") return duplicateKey(item);
      const subject = threadSubject(item.title);
      return subject ? `t:${item.source_id}:${subject}` : null;
    }
    case "sender":
      if (item.kind === "mail") {
        return item.sender_address ? `s:${item.sender_address.toLowerCase()}` : null;
      }
      return `f:${item.source_id}`;
    case "mailbox":
      // 邮件按所属邮箱合并；订阅文章没有邮箱，只把重复的并在一起。
      return item.kind === "mail" ? `a:${item.source_id}` : duplicateKey(item);
  }
}

function personOf(item: Item, mode: GroupMode): string {
  if (mode === "mailbox" && item.kind === "mail") return item.source_title || item.source_id;
  return item.kind === "mail" ? item.author || item.sender_address || "" : item.source_title;
}

/** 分组。组的顺序按组里最新一条出现的位置，列表本身已经从新到旧排好。 */
export function groupItems(items: Item[], mode: GroupMode): Group[] {
  const groups: Group[] = [];
  const byKey = new Map<string, Group>();
  for (const item of items) {
    const key = keyOf(item, mode) ?? `i:${item.id}`;
    let group = byKey.get(key);
    if (!group) {
      group = { key, items: [], lead: item, unread: 0, starred: false, hasAttachments: false, people: [] };
      byKey.set(key, group);
      groups.push(group);
    }
    group.items.push(item);
    if (!item.read) group.unread += 1;
    if (item.starred) group.starred = true;
    if (item.has_attachments) group.hasAttachments = true;
    const person = personOf(item, mode);
    if (person && !group.people.includes(person)) group.people.push(person);
  }
  return groups;
}

export function rowsOf(groups: Group[], expanded: ReadonlySet<string>): Row[] {
  const rows: Row[] = [];
  for (const group of groups) {
    if (group.items.length === 1) {
      rows.push({ kind: "item", item: group.lead, child: false });
    } else if (expanded.has(group.key)) {
      rows.push({ kind: "group", group, expanded: true });
      for (const item of group.items) rows.push({ kind: "item", item, child: true });
    } else {
      rows.push({ kind: "group", group, expanded: false });
    }
  }
  return rows;
}

/** 列表行对应的停留位置。展开的组头只是标题，不能停留。 */
export function stopOf(row: Row): Stop | null {
  if (row.kind === "item") return { id: row.item.id, members: [row.item.id] };
  if (row.expanded) return null;
  return { id: row.group.lead.id, members: row.group.items.map((i) => i.id) };
}
