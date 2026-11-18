// 内置订阅源目录：网站自己提供的官方订阅源，按分类和语言整理，「添加订阅 → 发现」里浏览、一键订阅。
// 只收官方源，不收 RSSHub 路由（公共实例有限流和防盗链，不适合给所有人默认用）。
//
// 数据只有一个文件：public/catalog/catalog.json，订阅源和图标（base64）都在里面。
// 它随应用一起发布，也是在线更新的来源：应用每天最多检查一次 GitHub 上的这个文件（见 catalogRemote.ts）。
// 改完运行 `node scripts/catalog.mjs check` 检查每个源，`node scripts/catalog.mjs icons` 补上新源的图标。
//
// 这里只有纯函数，方便测试：npm run test:ui

export type CatalogCategory =
  | "news"
  | "tech"
  | "dev"
  | "design"
  | "science"
  | "business"
  | "gaming"
  | "culture"
  | "life"
  | "blogs";

export const CATALOG_CATEGORIES: CatalogCategory[] = [
  "news",
  "tech",
  "dev",
  "design",
  "science",
  "business",
  "gaming",
  "culture",
  "life",
  "blogs",
];

export interface CatalogFeed {
  id: string;
  /** 网站自己的名字。 */
  name: string;
  /** 订阅地址。 */
  url: string;
  /** 网站首页。 */
  site: string;
  /** 内容的语言。 */
  lang: "zh" | "en";
  category: CatalogCategory;
  /** 一句话介绍，界面是中文时显示 zh，其他语言显示 en。 */
  desc: { zh: string; en: string };
  /** 放在「推荐」里，新手引导按分类订阅时也先选这些。 */
  featured?: boolean;
  /** 图标，`data:image/png;base64,…`。没有的显示首字母。 */
  icon?: string;
}

export interface Catalog {
  /** 文件格式的版本。以后改了格式就加一，旧版应用不认新格式时继续用自己带的目录。 */
  schema: number;
  /**
   * 最后修改的时间：UTC 的 YYYY-MM-DDTHH:MM:SSZ（早期只有日期 YYYY-MM-DD）。按文字比较，
   * 在线的比本机的新才用在线的；同一天的完整时间排在只有日期的后面。
   */
  updated: string;
  feeds: CatalogFeed[];
}

/** 这个版本的应用认得的文件格式。 */
export const CATALOG_SCHEMA = 1;

/** 内容的语言。 */
export type CatalogLang = CatalogFeed["lang"];

/** 语言筛选：跟随界面语言（同语言的排在前面，其他语言的也显示），或者只看某一种。 */
export type LangFilter = "auto" | CatalogLang;

const isText = (v: unknown): v is string => typeof v === "string" && v.trim() !== "";
const isHttp = (v: unknown): v is string => isText(v) && /^https?:\/\/\S+$/i.test(v);

/**
 * 读目录文件。格式不认识、或者有用的条目太少（多半是下载坏了）时返回 null，调用方继续用原来的。
 * 单个条目缺字段、分类不认识的直接跳过，不影响其他条目。
 */
export function parseCatalog(text: string, minFeeds = 20): Catalog | null {
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch {
    return null;
  }
  if (!raw || typeof raw !== "object") return null;
  const { schema, updated, feeds } = raw as Record<string, unknown>;
  if (schema !== CATALOG_SCHEMA || !isText(updated) || !Array.isArray(feeds)) return null;
  const seen = new Set<string>();
  const out: CatalogFeed[] = [];
  for (const entry of feeds) {
    if (!entry || typeof entry !== "object") continue;
    const f = entry as Record<string, unknown>;
    const desc = f.desc as Record<string, unknown> | undefined;
    if (
      !isText(f.id) ||
      seen.has(f.id) ||
      !isText(f.name) ||
      !isHttp(f.url) ||
      !isHttp(f.site) ||
      (f.lang !== "zh" && f.lang !== "en") ||
      !CATALOG_CATEGORIES.includes(f.category as CatalogCategory) ||
      !desc ||
      !isText(desc.zh) ||
      !isText(desc.en)
    ) {
      continue;
    }
    seen.add(f.id);
    out.push({
      id: f.id,
      name: f.name,
      url: f.url,
      site: f.site,
      lang: f.lang,
      category: f.category as CatalogCategory,
      desc: { zh: desc.zh, en: desc.en },
      ...(f.featured === true ? { featured: true } : {}),
      // 只认 png、jpeg、webp 的 data 地址，别的（比如外部链接、svg）不显示
      ...(typeof f.icon === "string" && /^data:image\/(png|jpeg|webp);base64,[A-Za-z0-9+/=]+$/.test(f.icon)
        ? { icon: f.icon }
        : {}),
    });
  }
  return out.length >= minFeeds ? { schema: CATALOG_SCHEMA, updated: updated.trim(), feeds: out } : null;
}

/** 两份目录用哪一份：日期新的；一样新时用本机的。 */
export function newerCatalog(local: Catalog, remote: Catalog | null): Catalog {
  return remote && remote.updated > local.updated ? remote : local;
}

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
export function browseCatalog(query: CatalogQuery, feeds: readonly CatalogFeed[]): CatalogFeed[] {
  const words = (query.search ?? "").toLowerCase().split(/\s+/).filter(Boolean);
  const picked = feeds.filter((feed) => {
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
export function sectionCounts(langFilter: LangFilter, feeds: readonly CatalogFeed[]): Record<CatalogSection, number> {
  const counts = { featured: 0 } as Record<CatalogSection, number>;
  for (const feed of feeds) {
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
export function subscribedIds(subscriptions: readonly { url: string }[], feeds: readonly CatalogFeed[]): Set<string> {
  const have = new Set(subscriptions.map((f) => feedKey(f.url)));
  return new Set(feeds.filter((c) => have.has(feedKey(c.url))).map((c) => c.id));
}

/**
 * 新手引导里按分类一次订阅几个：每个分类先取界面语言的推荐源，不够再补同语言的其他源，最多 `per` 个。
 * 界面语言在这个分类里一个都没有时，用另一种语言的。
 */
export function starterFeeds(
  categories: readonly CatalogCategory[],
  prefer: CatalogLang,
  feeds: readonly CatalogFeed[],
  per = 4,
): CatalogFeed[] {
  const out: CatalogFeed[] = [];
  for (const category of categories) {
    const inCategory = feeds.filter((f) => f.category === category);
    const own = inCategory.filter((f) => f.lang === prefer);
    const pool = own.length > 0 ? own : inCategory;
    const sorted = [...pool.filter((f) => f.featured), ...pool.filter((f) => !f.featured)];
    out.push(...sorted.slice(0, per));
  }
  return out;
}
