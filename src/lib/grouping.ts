// 列表里「折叠相同邮件」：把同一会话或同一发件人的条目合成一行。
//
// 只在界面上分组，数据本身不变。折叠着的一组在列表里占一行，代表它的是最新的那一条；
// 对这一行做的操作（归档、星标、已读、勾选）作用到组里的每一条。

import type { Item } from "./api";

export type GroupMode = "none" | "thread" | "sender";

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

function keyOf(item: Item, mode: GroupMode): string | null {
  switch (mode) {
    case "none":
      return null;
    case "thread": {
      // 文章没有会话的概念；没有主题的邮件也不硬凑到一起
      if (item.kind !== "mail") return null;
      const subject = threadSubject(item.title);
      return subject ? `t:${item.source_id}:${subject}` : null;
    }
    case "sender":
      if (item.kind === "mail") {
        return item.sender_address ? `s:${item.sender_address.toLowerCase()}` : null;
      }
      return `f:${item.source_id}`;
  }
}

function personOf(item: Item): string {
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
    const person = personOf(item);
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
