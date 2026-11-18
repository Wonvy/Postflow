// 内置订阅源目录。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { test } from "node:test";
import {
  browseCatalog,
  CATALOG,
  CATALOG_CATEGORIES,
  type CatalogFeed,
  catalogLang,
  feedKey,
  sectionCounts,
  starterFeeds,
  subscribedIds,
} from "./catalog.ts";

test("catalog data is well formed", () => {
  const ids = new Set<string>();
  const urls = new Set<string>();
  for (const feed of CATALOG) {
    assert.match(feed.id, /^[a-z0-9-]+$/, feed.id);
    assert.ok(!ids.has(feed.id), `duplicate id ${feed.id}`);
    ids.add(feed.id);
    assert.ok(!urls.has(feedKey(feed.url)), `duplicate feed ${feed.url}`);
    urls.add(feedKey(feed.url));
    assert.match(feed.url, /^https?:\/\//);
    assert.match(feed.site, /^https?:\/\//);
    assert.ok(CATALOG_CATEGORIES.includes(feed.category), `${feed.id}: ${feed.category}`);
    assert.ok(feed.lang === "zh" || feed.lang === "en");
    assert.ok(feed.name.trim() && feed.desc.zh.trim() && feed.desc.en.trim(), `${feed.id} needs a name and both descriptions`);
  }
  // 两种语言都有推荐，第一次打开不会是空的
  assert.ok(CATALOG.some((f) => f.featured && f.lang === "zh"));
  assert.ok(CATALOG.some((f) => f.featured && f.lang === "en"));
});

test("most catalog entries ship an icon", () => {
  const missing = CATALOG.filter((f) => !existsSync(new URL(`../../public/catalog/icons/${f.id}.png`, import.meta.url)));
  assert.ok(missing.length <= CATALOG.length / 10, `missing icons: ${missing.map((f) => f.id).join(", ")}`);
});

test("the interface language picks which feeds come first", () => {
  assert.equal(catalogLang("zh"), "zh");
  assert.equal(catalogLang("zh-TW"), "zh");
  assert.equal(catalogLang("en"), "en");
  assert.equal(catalogLang("pack:ja"), "en");

  const zhFirst = browseCatalog({ section: "news", prefer: "zh", langFilter: "auto" });
  const firstEn = zhFirst.findIndex((f) => f.lang === "en");
  assert.ok(firstEn > 0, "Chinese feeds come first");
  assert.ok(zhFirst.slice(firstEn).every((f) => f.lang === "en"), "then the English ones, not mixed in");

  const enOnly = browseCatalog({ section: "news", prefer: "zh", langFilter: "en" });
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
  const byName = browseCatalog({ section: "featured", prefer: "zh", langFilter: "auto", search: "少数派" });
  assert.equal(byName[0]?.id, "sspai");
  const byHost = browseCatalog({ section: "news", prefer: "en", langFilter: "auto", search: "github.blog" });
  assert.deepEqual(byHost.map((f) => f.id), ["github-blog"]);
  const byEnglishDesc = browseCatalog({ section: "news", prefer: "en", langFilter: "auto", search: "official rust" });
  assert.deepEqual(byEnglishDesc.map((f) => f.id), ["rust-blog"]);
  assert.deepEqual(browseCatalog({ section: "news", prefer: "en", langFilter: "auto", search: "zzzz-nothing" }), []);
});

test("already subscribed feeds are recognised despite small URL differences", () => {
  assert.equal(feedKey("HTTPS://www.Example.com/feed/"), "example.com/feed");
  const ids = subscribedIds([{ url: "http://sspai.com/feed/" }, { url: "https://unrelated.example/rss" }]);
  assert.ok(ids.has("sspai"));
  assert.equal(ids.size, 1);
});

test("starter feeds take a few per chosen category, in the interface language when there are any", () => {
  const zh = starterFeeds(["tech", "design"], "zh", 3);
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
});
