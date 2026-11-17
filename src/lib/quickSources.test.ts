// 常用网站的订阅地址。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { quickInitial, quickSource } from "./quickSources.ts";

const build = (id: Parameters<typeof quickSource>[0], values: Record<string, string>) => quickSource(id).build(values);

test("YouTube takes a handle, a channel or playlist ID, or the page address", () => {
  assert.equal(build("youtube", { channel: "@buckppt" }), "https://www.youtube.com/@buckppt");
  assert.equal(build("youtube", { channel: "buckppt" }), "https://www.youtube.com/@buckppt");
  assert.equal(build("youtube", { channel: "UCBJycsmduvYEL83R_U4JriQ" }), "https://www.youtube.com/channel/UCBJycsmduvYEL83R_U4JriQ");
  assert.equal(build("youtube", { channel: "PLrAXtmErZgOeiKm4sgNOknGvNjby9efdf" }), "https://www.youtube.com/playlist?list=PLrAXtmErZgOeiKm4sgNOknGvNjby9efdf");
  assert.equal(build("youtube", { channel: "youtube.com/@x" }), "https://youtube.com/@x");
  assert.equal(build("youtube", { channel: "" }), null);
});

test("GitHub repositories and users", () => {
  assert.equal(build("github", { target: "DIYgod/RSSHub", kind: "releases" }), "https://github.com/DIYgod/RSSHub/releases.atom");
  assert.equal(build("github", { target: "https://github.com/tauri-apps/tauri.git", kind: "tags" }), "https://github.com/tauri-apps/tauri/tags.atom");
  assert.equal(build("github", { target: "github.com/rust-lang/rust/issues", kind: "commits" }), "https://github.com/rust-lang/rust/commits.atom");
  assert.equal(build("github", { target: "@torvalds", kind: "releases" }), "https://github.com/torvalds.atom");
  assert.equal(build("github", { target: "a/b", kind: "user" }), "https://github.com/a.atom");
  assert.equal(build("github", { target: "bad name/x", kind: "releases" }), null);
});

test("Reddit, Bluesky, Mastodon", () => {
  assert.equal(build("reddit", { subreddit: "r/rust" }), "https://www.reddit.com/r/rust/.rss");
  assert.equal(build("reddit", { subreddit: "https://www.reddit.com/r/selfhosted/top/" }), "https://www.reddit.com/r/selfhosted/.rss");
  assert.equal(build("bluesky", { handle: "@jay.bsky.team" }), "https://bsky.app/profile/jay.bsky.team/rss");
  assert.equal(build("bluesky", { handle: "alice" }), "https://bsky.app/profile/alice.bsky.social/rss");
  assert.equal(build("bluesky", { handle: "bsky.app" }), "https://bsky.app/profile/bsky.app/rss");
  assert.equal(build("bluesky", { handle: "https://bsky.app/profile/bsky.app" }), "https://bsky.app/profile/bsky.app/rss");
  assert.equal(build("mastodon", { account: "@Gargron@mastodon.social" }), "https://mastodon.social/@Gargron.rss");
  assert.equal(build("mastodon", { account: "https://fosstodon.org/@rust" }), "https://fosstodon.org/@rust.rss");
  assert.equal(build("mastodon", { account: "Gargron" }), null);
});

test("Medium, Substack, Stack Overflow, V2EX, arXiv, Hacker News", () => {
  assert.equal(build("medium", { name: "@ev" }), "https://medium.com/feed/@ev");
  assert.equal(build("medium", { name: "netflix-techblog" }), "https://medium.com/feed/netflix-techblog");
  assert.equal(build("medium", { name: "https://medium.com/@ev/some-post" }), "https://medium.com/feed/@ev");
  assert.equal(build("substack", { name: "https://Platformer.substack.com/p/x" }), "https://platformer.substack.com/feed");
  assert.equal(build("substack", { name: "stratechery" }), "https://stratechery.substack.com/feed");
  assert.equal(build("stackoverflow", { tag: "Rust" }), "https://stackoverflow.com/feeds/tag/rust");
  assert.equal(build("stackoverflow", { tag: "https://stackoverflow.com/questions/tagged/c%23" }), "https://stackoverflow.com/feeds/tag/c%23");
  assert.equal(build("v2ex", { node: "" }), "https://www.v2ex.com/index.xml");
  assert.equal(build("v2ex", { node: "https://www.v2ex.com/go/python" }), "https://www.v2ex.com/feed/python.xml");
  assert.equal(build("arxiv", { category: "cs.AI" }), "https://rss.arxiv.org/rss/cs.AI");
  assert.equal(build("arxiv", { category: "https://arxiv.org/list/cs.CL/recent" }), "https://rss.arxiv.org/rss/cs.CL");
  assert.equal(build("arxiv", { category: "https://arxiv.org/abs/2401.00001" }), null);
  assert.equal(build("hackernews", {}), "https://news.ycombinator.com/rss");
});

test("forms start with the first option picked", () => {
  assert.deepEqual(quickInitial(quickSource("github")), { target: "", kind: "releases" });
  assert.deepEqual(quickInitial(quickSource("hackernews")), {});
});
