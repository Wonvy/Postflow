#!/usr/bin/env node
// Regenerates the README screenshots in docs/screenshots/{en,zh}/.
//
//   npm run screenshots
//
// Starts the demo data server and a headless Postflow core with a throwaway data folder,
// then drives the web UI in a locally installed Chrome or Edge. Nothing touches your own data.

import { execFileSync, spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const PORT = 4320;
const BASE = `http://127.0.0.1:${PORT}`;
const FEEDS = "http://127.0.0.1:1145";
const OUT = join(ROOT, "docs", "screenshots");

// Items that should look unread / starred in every screenshot, whatever earlier shots opened.
const UNREAD = new Set([
  "Build #1201 passed on main",
  "Weekly digest: making the inbox a tool again",
  "Q4 budget for review",
  "Dinner on Saturday?",
  "Photos from the trip",
  "Syncing without a server: append-only logs",
  "Quiet interfaces",
  "开源周刊第 48 期：本地优先的软件",
]);
const STARRED = new Set(["Re: Design review notes", "Designing keyboard-first tools"]);

const LABELS = {
  en: { addAccount: "Add mail account" },
  zh: { addAccount: "添加邮箱" },
};

const children = [];
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function start(cmd, args) {
  const child = spawn(cmd, args, { cwd: ROOT, stdio: ["ignore", "ignore", "inherit"] });
  children.push(child);
  return child;
}

async function waitFor(what, check, timeoutMs = 30_000) {
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    try {
      if (await check()) return;
    } catch {
      // not up yet
    }
    await sleep(200);
  }
  throw new Error(`timed out waiting for ${what}`);
}

async function rpc(method, params = {}) {
  const res = await fetch(`${BASE}/api/rpc`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ method, params }),
  });
  const body = await res.json();
  if (body.error) throw new Error(`${method}: ${body.error.code} ${body.error.message}`);
  return body.result;
}

async function allItems() {
  const { items } = await rpc("items.list", { filter: { view: "inbox" }, limit: 200 });
  return items;
}

/** Puts read and starred state back to the same starting point before each screenshot. */
async function resetState() {
  const items = await allItems();
  const ids = (pred) => items.filter(pred).map((i) => i.id);
  await rpc("items.set", { ids: ids((i) => UNREAD.has(i.title)), read: false });
  await rpc("items.set", { ids: ids((i) => !UNREAD.has(i.title)), read: true });
  await rpc("items.set", { ids: ids((i) => STARRED.has(i.title)), starred: true });
  await rpc("items.set", { ids: ids((i) => !STARRED.has(i.title)), starred: false });
}

async function launch() {
  for (const channel of [process.env.SCREENSHOT_BROWSER, "chrome", "msedge"].filter(Boolean)) {
    try {
      return await chromium.launch({ channel });
    } catch {
      // try the next one
    }
  }
  throw new Error("Needs Google Chrome or Microsoft Edge installed (or set SCREENSHOT_BROWSER).");
}

async function shot(browser, lang, name, prefs, act) {
  const context = await browser.newContext({
    viewport: { width: 1280, height: 800 },
    deviceScaleFactor: 2,
    colorScheme: prefs.theme,
    locale: lang === "zh" ? "zh-CN" : "en-US",
  });
  const state = {
    theme: prefs.theme,
    lang,
    density: prefs.density ?? "compact",
    sidebarWidth: 232,
    listWidth: 400,
    collapsedFolders: [],
    kindFilter: null,
    groupModes: { "view:inbox": prefs.groupMode ?? "none" },
  };
  await context.addInitScript((s) => {
    localStorage.setItem("postflow.ui", JSON.stringify({ state: s, version: 0 }));
  }, state);
  const page = await context.newPage();
  await resetState();
  await page.goto(BASE);
  await page.getByRole("option").first().waitFor();
  await act(page);
  await sleep(600);
  const file = join(OUT, lang, `${name}.png`);
  await page.screenshot({ path: file });
  console.log(`  ${lang}/${name}.png`);
  await context.close();
}

const open = (page, title) => page.getByRole("option").filter({ hasText: title }).first().click();

async function main() {
  console.log("Building the web UI and the demo binaries…");
  execFileSync("npm", ["run", "build"], { cwd: ROOT, stdio: ["ignore", "ignore", "inherit"] });
  execFileSync("cargo", ["build", "-q", "-p", "postflow-web", "-p", "postflow-testkit", "--bins"], {
    cwd: ROOT,
    stdio: "inherit",
  });

  const tmp = mkdtempSync(join(tmpdir(), "postflow-shots-"));
  try {
    start(join(ROOT, "target/debug/demo-server"), []);
    await waitFor("demo server", async () => (await fetch(`${FEEDS}/cn/atom.xml`)).ok);
    start(join(ROOT, "target/debug/postflow-web"), [
      "--local-dir", join(tmp, "local"),
      "--port", String(PORT),
      "--static", join(ROOT, "dist"),
      "--memory-secrets",
    ]);
    await waitFor("postflow core", async () => (await fetch(BASE)).ok);

    console.log("Loading demo data…");
    await rpc("app.setup", { data_dir: join(tmp, "data"), device_name: "MacBook Pro" });
    for (const [email, name, port] of [
      ["work@example.com", "Work", 1143],
      ["me@example.org", "Personal", 1144],
    ]) {
      await rpc("accounts.add", {
        email, name, password: "demo",
        imap_host: "127.0.0.1", imap_port: port, imap_security: "none", username: email,
      });
    }
    for (const [path, folder] of [
      ["field-notes/atom.xml", "Engineering"],
      ["weekly/atom.xml", "Design"],
      ["cn/atom.xml", ""],
    ]) {
      await rpc("feeds.add", { url: `${FEEDS}/${path}`, folder });
    }
    await waitFor("mail to sync", async () => (await allItems()).filter((i) => i.kind === "mail").length >= 9);

    const browser = await launch();
    try {
      for (const lang of ["en", "zh"]) {
        console.log(`Screenshots (${lang}):`);
        mkdirSync(join(OUT, lang), { recursive: true });
        const L = LABELS[lang];

        // 按发件人折叠：同一发件人的通知、同一订阅源的文章各合成一行
        await shot(browser, lang, "inbox", { theme: "dark", groupMode: "sender" }, async (page) => {
          await open(page, "Weekly digest");
          await page.locator("main iframe").waitFor();
        });

        await shot(browser, lang, "mail", { theme: "light", density: "comfortable" }, async (page) => {
          await page.locator("aside").getByText("Work", { exact: true }).click();
          await open(page, "Q4 budget for review");
          await page.locator("main section li").first().waitFor();
        });

        await shot(browser, lang, "article", { theme: "dark" }, async (page) => {
          await page.locator("aside").getByText("Field Notes", { exact: true }).click();
          await open(page, "Syncing without a server");
          await page.locator("main .article pre").waitFor();
        });

        // 第一步：所有服务商的图标，下面是已经添加的邮箱
        await shot(browser, lang, "add-account", { theme: "dark" }, async (page) => {
          await page.getByRole("button", { name: L.addAccount }).first().click();
          await page.getByRole("dialog").locator("section").first().waitFor();
        });

        await shot(browser, lang, "selection", { theme: "light", density: "comfortable" }, async (page) => {
          const modifier = process.platform === "darwin" ? "Meta" : "Control";
          for (const title of ["Q4 budget for review", "Photos from the trip", "Quiet interfaces"]) {
            await page.getByRole("option").filter({ hasText: title }).first().click({ modifiers: [modifier] });
          }
          await page.getByRole("toolbar").waitFor();
        });
      }
    } finally {
      await browser.close();
    }
  } finally {
    for (const child of children) child.kill();
    rmSync(tmp, { recursive: true, force: true });
  }
}

main().catch((err) => {
  for (const child of children) child.kill();
  console.error(err);
  process.exit(1);
});
