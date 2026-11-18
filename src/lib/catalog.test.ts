// 推荐订阅源目录。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  browseCatalog,
  type Catalog,
  CATALOG_CATEGORIES,
  type CatalogFeed,
  catalogLang,
  feedKey,
  groupCounts,
  newerCatalog,
  parseCatalog,
  sectionCounts,
  starterFeeds,
  subscribedIds,
} from "./catalog.ts";

const FILE = readFileSync(new URL("../../public/catalog/catalog.json", import.meta.url), "utf8");
const CATALOG = parseCatalog(FILE)?.feeds ?? [];

test("the catalog file reads cleanly, with nothing dropped", () => {
  const raw = JSON.parse(FILE) as Catalog;
  assert.equal(raw.schema, 1);
  assert.match(raw.updated, /^\d{4}-\d{2}-\d{2}(T\d{2}:\d{2}:\d{2}Z)?$/);
  // 每个条目都通过了检查：手改文件时写错的条目会在这里被发现
  assert.equal(CATALOG.length, raw.feeds.length);
  const urls = new Set<string>();
  for (const feed of CATALOG) {
    assert.match(feed.id, /^[a-z0-9-]+$/, feed.id);
    assert.ok(!urls.has(feedKey(feed.url)), `duplicate feed ${feed.url}`);
    urls.add(feedKey(feed.url));
  }
  assert.ok(CATALOG.some((f) => f.featured && f.lang === "zh"));
  assert.ok(CATALOG.some((f) => f.featured && f.lang === "en"));
  // 大部分源带图标
  assert.ok(CATALOG.filter((f) => !f.icon).length <= CATALOG.length / 10);
});

test("a downloaded catalog is checked before it's used", () => {
  const good = (id: string): Record<string, unknown> => ({
    id,
    name: id.toUpperCase(),
    url: `https://${id}.example/feed`,
    site: `https://${id}.example`,
    lang: "en",
    category: "tech",
    desc: { zh: "介绍", en: "About" },
  });
  const file = (feeds: unknown[], extra: Record<string, unknown> = {}) =>
    JSON.stringify({ schema: 1, updated: "2027-01-01", feeds, ...extra });

  const parsed = parseCatalog(
    file([
      good("a"),
      { ...good("b"), category: "astrology" },
      { ...good("c"), url: "javascript:alert(1)" },
      { ...good("d"), icon: "https://tracker.example/pixel.png" },
      { ...good("e"), icon: "data:image/png;base64,iVBORw0KGgo=" },
      good("a"),
      "junk",
    ]),
    1,
  );
  assert.deepEqual(parsed?.feeds.map((f) => f.id), ["a", "d", "e"]);
  assert.equal(parsed?.feeds[1]?.icon, undefined, "only embedded images are kept as icons");
  assert.ok(parsed?.feeds[2]?.icon);

  assert.equal(parseCatalog("not json"), null);
  assert.equal(parseCatalog(file([good("a")], { schema: 2 }), 1), null, "a newer format is left alone");
  assert.equal(parseCatalog(file([good("a")])), null, "too few feeds means a broken download");
});

test("the newer catalog wins, the bundled one on a tie", () => {
  const local: Catalog = { schema: 1, updated: "2026-11-18", feeds: [] };
  const newer: Catalog = { schema: 1, updated: "2026-12-01", feeds: [] };
  const older: Catalog = { schema: 1, updated: "2026-10-01", feeds: [] };
  assert.equal(newerCatalog(local, newer), newer);
  assert.equal(newerCatalog(local, older), local);
  assert.equal(newerCatalog(local, { ...local }), local);
  assert.equal(newerCatalog(local, null), local);
  // 同一天改了第二次：带时间的比只有日期的新，晚的比早的新
  const sameDay: Catalog = { schema: 1, updated: "2026-11-18T09:30:00Z", feeds: [] };
  const later: Catalog = { schema: 1, updated: "2026-11-18T17:05:00Z", feeds: [] };
  assert.equal(newerCatalog(local, sameDay), sameDay);
  assert.equal(newerCatalog(sameDay, later), later);
  assert.equal(newerCatalog(later, sameDay), later);
});

test("the interface language picks which feeds come first", () => {
  assert.equal(catalogLang("zh"), "zh");
  assert.equal(catalogLang("zh-TW"), "zh");
  assert.equal(catalogLang("en"), "en");
  assert.equal(catalogLang("pack:ja"), "en");

  const zhFirst = browseCatalog({ section: "news", prefer: "zh", langFilter: "auto" }, CATALOG);
  const firstEn = zhFirst.findIndex((f) => f.lang === "en");
  assert.ok(firstEn > 0, "Chinese feeds come first");
  assert.ok(zhFirst.slice(firstEn).every((f) => f.lang === "en"), "then the English ones, not mixed in");

  const enOnly = browseCatalog({ section: "news", prefer: "zh", langFilter: "en" }, CATALOG);
  assert.ok(enOnly.length > 0 && enOnly.every((f) => f.lang === "en"));
});

test("featured feeds lead within a language, and the Featured section has only those", () => {
  const sample: CatalogFeed[] = [
    { id: "a", name: "A", url: "https://a.example/feed", site: "https://a.example", lang: "en", category: "tech", desc: { zh: "甲", en: "A" } },
    { id: "b", name: "B", url: "https://b.example/feed", site: "https://b.example", lang: "en", category: "tech", desc: { zh: "乙", en: "B" }, featured: true },
    { id: "c", name: "C", url: "https://c.example/feed", site: "https://c.example", lang: "zh", category: "tech", desc: { zh: "丙", en: "C" } },
  ];
  assert.deepEqual(
    browseCatalog({ section: "tech", prefer: "en", langFilter: "auto" }, sample).map((f) => f.id),
    ["b", "a", "c"],
  );
  assert.deepEqual(
    browseCatalog({ section: "featured", prefer: "en", langFilter: "auto" }, sample).map((f) => f.id),
    ["b"],
  );
  const counts = sectionCounts("zh", sample);
  assert.equal(counts.tech, 1);
  assert.equal(counts.featured, 0);
});

test("search looks across every category by name, description and site", () => {
  const byName = browseCatalog({ section: "featured", prefer: "zh", langFilter: "auto", search: "少数派" }, CATALOG);
  assert.equal(byName[0]?.id, "sspai");
  const byHost = browseCatalog({ section: "news", prefer: "en", langFilter: "auto", search: "github.blog" }, CATALOG);
  assert.deepEqual(byHost.map((f) => f.id), ["github-blog"]);
  const byEnglishDesc = browseCatalog({ section: "news", prefer: "en", langFilter: "auto", search: "official rust" }, CATALOG);
  assert.deepEqual(byEnglishDesc.map((f) => f.id), ["rust-blog"]);
  assert.deepEqual(browseCatalog({ section: "news", prefer: "en", langFilter: "auto", search: "zzzz-nothing" }, CATALOG), []);
});

test("already subscribed feeds are recognised despite small URL differences", () => {
  assert.equal(feedKey("HTTPS://www.Example.com/feed/"), "example.com/feed");
  const ids = subscribedIds([{ url: "http://sspai.com/feed/" }, { url: "https://unrelated.example/rss" }], CATALOG);
  assert.ok(ids.has("sspai"));
  assert.equal(ids.size, 1);
});

test("starter feeds take a few per chosen category, in the interface language when there are any", () => {
  const zh = starterFeeds(["tech", "design"], "zh", CATALOG, 3);
  const tech = zh.filter((f) => f.category === "tech");
  assert.equal(tech.length, 3);
  assert.ok(tech.every((f) => f.lang === "zh"));
  assert.ok(tech[0]?.featured, "featured first");
  // 中文里没有设计类的源时，用英文的
  const design = zh.filter((f) => f.category === "design");
  assert.ok(design.length > 0);
  if (!CATALOG.some((f) => f.category === "design" && f.lang === "zh")) {
    assert.ok(design.every((f) => f.lang === "en"));
  }
  assert.ok(CATALOG_CATEGORIES.length === 11);
});

test("podcasts are split into groups, and every podcast has one", () => {
  const podcasts = CATALOG.filter((f) => f.category === "podcasts");
  assert.ok(podcasts.length >= 50);
  assert.ok(podcasts.every((f) => f.group), podcasts.filter((f) => !f.group).map((f) => f.id).join(", "));
  assert.ok(CATALOG.filter((f) => f.category !== "podcasts").every((f) => !f.group));
  const counts = groupCounts("zh", CATALOG);
  assert.ok((counts.tech ?? 0) >= 5);
  const tech = browseCatalog({ section: "podcasts", prefer: "zh", langFilter: "auto", group: "tech" }, CATALOG);
  assert.ok(tech.length >= 5 && tech.every((f) => f.group === "tech"));
  // 搜索时不管小类
  const found = browseCatalog({ section: "podcasts", prefer: "zh", langFilter: "auto", group: "sports", search: "硅谷" }, CATALOG);
  assert.ok(found.some((f) => f.group === "tech"));
  // 不认识的小类不要
  const parsed = parseCatalog(
    JSON.stringify({
      schema: 1,
      updated: "2026-01-01",
      feeds: [
        { id: "a", name: "A", url: "https://a.example/rss", site: "https://a.example", lang: "zh", category: "podcasts", group: "music", desc: { zh: "a", en: "a" } },
      ],
    }),
    1,
  );
  assert.equal(parsed?.feeds[0]?.group, undefined);
});
