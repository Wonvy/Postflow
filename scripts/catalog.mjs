#!/usr/bin/env node
// Maintains the built-in feed catalog: public/catalog/catalog.json, the one file with every feed and its icon.
// The app ships it, and installed apps download the newest one from GitHub once a day, so editing this file
// and pushing it to main updates everyone's list.
//
//   node scripts/catalog.mjs check            # is every feed still up, and when did it last post?
//   node scripts/catalog.mjs icons            # add icons for feeds that have none
//   node scripts/catalog.mjs icons --force    # download all icons again
//   node scripts/catalog.mjs icons sspai v2ex # only these
//   node scripts/catalog.mjs touch            # set "updated" to now after editing by hand
//
// Icons are 64×64 PNGs stored as base64 in the file, so browsing the catalog never contacts the sites.
// Resizing uses `sips`, which comes with macOS. A feed without an icon shows its first letter.

import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const FILE = join(ROOT, "public", "catalog", "catalog.json");
const catalog = JSON.parse(readFileSync(FILE, "utf8"));
const CATALOG = catalog.feeds;

/**
 * Writes the file back with the current time as "updated" (UTC, to the second), so installed apps pick it up even
 * if the file changes twice in a day. Apps compare it as text, and a full time sorts after an older date-only value.
 */
function save() {
  catalog.updated = `${new Date().toISOString().slice(0, 19)}Z`;
  writeFileSync(FILE, `${JSON.stringify(catalog, null, 2)}\n`);
  console.log(`Saved ${FILE} (updated ${catalog.updated}).`);
}
// Same as the app (crates/feed/src/client.rs sends "ShibaPost/<version>"), so sites that treat readers differently
// answer the way they will in ShibaPost
const FEED_UA = `ShibaPost/${JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8")).version}`;
const PAGE_UA =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";
const STALE_DAYS = 180;

async function get(url, ua, timeoutMs = 30_000) {
  let last;
  for (let attempt = 0; attempt < 3; attempt++) {
    try {
      return await fetch(url, { headers: { "user-agent": ua }, redirect: "follow", signal: AbortSignal.timeout(timeoutMs) });
    } catch (err) {
      last = err;
    }
  }
  throw last;
}

async function pool(items, size, fn) {
  const out = new Array(items.length);
  let next = 0;
  await Promise.all(
    Array.from({ length: size }, async () => {
      while (next < items.length) {
        const i = next++;
        out[i] = await fn(items[i]);
      }
    }),
  );
  return out;
}

async function checkOne(feed) {
  try {
    const res = await get(feed.url, FEED_UA);
    const text = await res.text();
    const isFeed = /<(rss|feed|rdf:RDF)[\s>]/i.test(text.slice(0, 5000));
    const items = (text.match(/<(item|entry)[\s>]/gi) ?? []).length;
    const dates = [...text.matchAll(/<(pubDate|updated|published|dc:date)>([^<]+)</gi)]
      .map((m) => Date.parse(m[2].trim()))
      .filter((n) => !Number.isNaN(n) && n <= Date.now() + 86_400_000);
    const newest = dates.length ? Math.max(...dates) : null;
    const problems = [];
    if (res.status !== 200) problems.push(`HTTP ${res.status}`);
    else if (!isFeed) problems.push("not a feed");
    else if (items === 0) problems.push("no entries");
    if (newest && Date.now() - newest > STALE_DAYS * 86_400_000) problems.push("stale");
    if (res.url !== feed.url) problems.push(`moved to ${res.url}`);
    return { feed, newest, problems };
  } catch (err) {
    return { feed, newest: null, problems: [String(err.cause?.code ?? err.name ?? err)] };
  }
}

async function check() {
  const results = await pool(CATALOG, 12, checkOne);
  let bad = 0;
  for (const { feed, newest, problems } of results) {
    const date = newest ? new Date(newest).toISOString().slice(0, 10) : "no dates";
    if (problems.length) bad++;
    console.log(`${problems.length ? "!!" : "ok"} ${feed.id.padEnd(22)} ${date}  ${problems.join("; ")}`);
  }
  console.log(`\n${CATALOG.length - bad} fine, ${bad} to look at.`);
  if (results.some((r) => r.problems.some((p) => !p.startsWith("moved") && p !== "stale"))) process.exitCode = 1;
}

/** Icon candidates from a page's <link> tags, best first. */
function iconLinks(html, base) {
  const links = [];
  for (const tag of html.match(/<link\b[^>]*>/gi) ?? []) {
    const rel = /\brel=["']?([^"'>]+)/i.exec(tag)?.[1]?.toLowerCase() ?? "";
    const href = /\bhref=["']?([^"'\s>]+)/i.exec(tag)?.[1];
    if (!href || !/\bicon\b/.test(rel)) continue;
    const size = Number(/\bsizes=["']?(\d+)/i.exec(tag)?.[1] ?? 0);
    let url;
    try {
      url = new URL(href.replace(/&amp;/g, "&"), base).href;
    } catch {
      continue;
    }
    // bitmaps first (svg goes through QuickLook); apple-touch-icon is usually a crisp 180px png
    const score = (/\.svg(\?|$)/i.test(url) ? -1000 : 0) + (rel.includes("apple-touch") ? 500 : 0) + Math.min(size, 256);
    links.push({ url, score });
  }
  return links.sort((a, b) => b.score - a.score).map((l) => l.url);
}

function toPng(bytes, tmp, svg) {
  const out = join(tmp, "icon.png");
  convert(bytes, out, tmp, svg);
  return `data:image/png;base64,${readFileSync(out).toString("base64")}`;
}

function convert(bytes, out, tmp, svg) {
  if (svg) {
    // sips can't read svg; QuickLook renders it to a png thumbnail
    const src = join(tmp, "icon.svg");
    writeFileSync(src, bytes);
    execFileSync("qlmanage", ["-t", "-s", "128", "-o", tmp, src], { stdio: "ignore" });
    execFileSync("sips", ["-s", "format", "png", "-Z", "64", join(tmp, "icon.svg.png"), "--out", out], { stdio: "ignore" });
    return;
  }
  const src = join(tmp, "src");
  writeFileSync(src, bytes);
  execFileSync("sips", ["-s", "format", "png", "-Z", "64", src, "--out", out], { stdio: "ignore" });
}

async function iconFor(feed, tmp) {
  const tries = [];
  try {
    const page = await get(feed.site, PAGE_UA);
    tries.push(...iconLinks(await page.text(), page.url));
  } catch {
    // fall back to the usual places
  }
  tries.push(new URL("/apple-touch-icon.png", feed.site).href, new URL("/favicon.ico", feed.site).href);
  for (const url of tries) {
    try {
      const res = await get(url, PAGE_UA);
      const type = res.headers.get("content-type") ?? "";
      if (!res.ok || /text\/html/i.test(type)) continue;
      const bytes = Buffer.from(await res.arrayBuffer());
      if (bytes.length < 100) continue;
      feed.icon = toPng(bytes, tmp, /svg/i.test(type) || /\.svg(\?|$)/i.test(url));
      return url;
    } catch {
      // try the next one
    }
  }
  return null;
}

async function icons(args) {
  const force = args.includes("--force");
  const only = args.filter((a) => !a.startsWith("--"));
  const todo = CATALOG.filter((f) => (only.length === 0 || only.includes(f.id)) && (force || only.length > 0 || !f.icon));
  const missing = [];
  await pool(todo, 6, async (feed) => {
    const tmp = mkdtempSync(join(tmpdir(), "shibapost-icon-"));
    try {
      const from = await iconFor(feed, tmp);
      console.log(`${from ? "ok" : "!!"} ${feed.id.padEnd(22)} ${from ?? "no icon found"}`);
      if (!from) missing.push(feed.id);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });
  if (missing.length) {
    console.log(`\nNo icon for: ${missing.join(", ")}. The app shows the first letter for these.`);
  }
  if (todo.length > missing.length) save();
}

const [command, ...rest] = process.argv.slice(2);
if (command === "check") await check();
else if (command === "icons") await icons(rest);
else if (command === "touch") save();
else {
  console.error("Usage: node scripts/catalog.mjs check | icons [--force] [id…] | touch");
  process.exit(1);
}
