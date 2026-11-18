// 内置订阅源目录的取用：按分类、语言筛选，按界面语言排序，搜索，判断是否已经订阅过。
// 数据在 catalogData.ts。这里只有纯函数，方便测试：npm run test:ui

import { CATALOG, type CatalogCategory, type CatalogFeed } from "./catalogData.ts";

export { CATALOG, CATALOG_CATEGORIES, type CatalogCategory, type CatalogFeed } from "./catalogData.ts";

/** 目录里的内容语言。 */
export type CatalogLang = CatalogFeed["lang"];

/** 语言筛选：跟随界面语言（同语言的排在前面，其他语言的也显示），或者只看某一种。 */
export type LangFilter = "auto" | CatalogLang;

/** 界面语言对应的目录语言：中文界面优先中文源，其他语言都优先英文源。 */
export function catalogLang(uiLang: string): CatalogLang {
  return uiLang.toLowerCase().startsWith("zh") ? "zh" : "en";
}

/** 目录里的分类，或者「推荐」。 */
export type CatalogSection = CatalogCategory | "featured";

export interface CatalogQuery {
  section: CatalogSection;
  /** 界面语言（`catalogLang` 的结果）：「跟随」时同语言的排在前面。 */
  prefer: CatalogLang;
  langFilter: LangFilter;
  /** 搜索词。有词时在全部分类里搜，不管 `section`。 */
  search?: string;
}

const matchesLang = (feed: CatalogFeed, filter: LangFilter) => filter === "auto" || feed.lang === filter;

/** 搜索要比较的文字：名字、介绍、网站域名。 */
function haystack(feed: CatalogFeed): string {
  return [feed.name, feed.desc.zh, feed.desc.en, hostOf(feed.site), feed.id].join("\n").toLowerCase();
}

/** 按条件取出目录里的源：界面语言的排在前面，然后推荐的在前，其余按目录里的顺序。 */
export function browseCatalog(query: CatalogQuery, catalog: readonly CatalogFeed[] = CATALOG): CatalogFeed[] {
  const words = (query.search ?? "").toLowerCase().split(/\s+/).filter(Boolean);
  const picked = catalog.filter((feed) => {
    if (!matchesLang(feed, query.langFilter)) return false;
    if (words.length > 0) {
      const text = haystack(feed);
      return words.every((w) => text.includes(w));
    }
    return query.section === "featured" ? feed.featured === true : feed.category === query.section;
  });
  const rank = (feed: CatalogFeed) => (feed.lang === query.prefer ? 0 : 2) + (feed.featured ? 0 : 1);
  return picked
    .map((feed, i) => ({ feed, i }))
    .sort((a, b) => rank(a.feed) - rank(b.feed) || a.i - b.i)
    .map((x) => x.feed);
}

/** 每个分类在这个语言筛选下有几个源，用来隐藏空的分类。 */
export function sectionCounts(
  langFilter: LangFilter,
  catalog: readonly CatalogFeed[] = CATALOG,
): Record<CatalogSection, number> {
  const counts = { featured: 0 } as Record<CatalogSection, number>;
  for (const feed of catalog) {
    if (!matchesLang(feed, langFilter)) continue;
    counts[feed.category] = (counts[feed.category] ?? 0) + 1;
    if (feed.featured) counts.featured += 1;
  }
  return counts;
}

function hostOf(url: string): string {
  try {
    return new URL(url).host.replace(/^www\./, "");
  } catch {
    return url;
  }
}

/** 比较订阅地址用的形式：不管 http/https、www、末尾的斜杠和大小写。 */
export function feedKey(url: string): string {
  return url
    .trim()
    .toLowerCase()
    .replace(/^https?:\/\//, "")
    .replace(/^www\./, "")
    .replace(/\/+$/, "");
}

/** 已经订阅了的目录源的 ID。 */
export function subscribedIds(
  feeds: readonly { url: string }[],
  catalog: readonly CatalogFeed[] = CATALOG,
): Set<string> {
  const have = new Set(feeds.map((f) => feedKey(f.url)));
  return new Set(catalog.filter((c) => have.has(feedKey(c.url))).map((c) => c.id));
}

/**
 * 新手引导里按分类一次订阅几个：每个分类先取界面语言的推荐源，不够再补同语言的其他源，最多 `per` 个。
 * 界面语言在这个分类里一个都没有时，用另一种语言的。
 */
export function starterFeeds(
  categories: readonly CatalogCategory[],
  prefer: CatalogLang,
  per = 4,
  catalog: readonly CatalogFeed[] = CATALOG,
): CatalogFeed[] {
  const out: CatalogFeed[] = [];
  for (const category of categories) {
    const inCategory = catalog.filter((f) => f.category === category);
    const own = inCategory.filter((f) => f.lang === prefer);
    const pool = own.length > 0 ? own : inCategory;
    const sorted = [...pool.filter((f) => f.featured), ...pool.filter((f) => !f.featured)];
    out.push(...sorted.slice(0, per));
  }
  return out;
}

/** 图标随应用一起发布，在 public/catalog/icons 下；下载不到图标的源没有文件，界面显示首字母。 */
export function catalogIcon(feed: CatalogFeed): string {
  return `/catalog/icons/${feed.id}.png`;
}
