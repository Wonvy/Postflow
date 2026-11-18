// ⌘K 命令面板的搜索：按输入的字给命令排序。这里只有纯函数，方便测试：npm run test:ui

import { initials } from "./pinyin.ts";

export interface Command {
  id: string;
  /** 显示的名字。 */
  label: string;
  /** 分组，显示在名字右边（「前往」「操作」「设置」……）。 */
  group: string;
  /** 另外能搜到它的词，比如英文名、网址。 */
  keywords?: string;
  /** 快捷键提示。 */
  shortcut?: string;
  run: () => void;
}

/** 名字以输入开头最好，其次是某个词以它开头、包含它，再次是关键词里有，最后是按顺序出现的字（「xjyj」找「写新邮件」不行，但「新件」能找到「写新邮件」）。 */
function score(command: Command, query: string): number | null {
  const label = command.label.toLowerCase();
  if (label.startsWith(query)) return 0;
  if (label.split(/[\s/·:：-]+/).some((w) => w.startsWith(query))) return 1;
  if (label.includes(query)) return 2;
  if ((command.keywords ?? "").toLowerCase().includes(query)) return 3;
  // 拼音首字母：「ssp」找到「少数派」，「sjx」找到「收件箱」
  if (/^[a-z0-9]+$/.test(query)) {
    const letters = initials(command.label);
    if (letters.startsWith(query)) return 3;
    if (letters.includes(query)) return 3.5;
  }
  let at = 0;
  for (const ch of query) {
    at = label.indexOf(ch, at);
    if (at < 0) return null;
    at += 1;
  }
  return 4;
}

/** 按输入筛选、排序命令；没输入时原样返回。分数一样时保持原来的顺序。 */
export function rankCommands(commands: readonly Command[], input: string): Command[] {
  const query = input.trim().toLowerCase();
  if (!query) return [...commands];
  return commands
    .map((command, i) => ({ command, i, s: score(command, query) }))
    .filter((x): x is { command: Command; i: number; s: number } => x.s !== null)
    .sort((a, b) => a.s - b.s || a.i - b.i)
    .map((x) => x.command);
}
