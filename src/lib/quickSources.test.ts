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

test("Telegram public channels", () => {
  assert.equal(build("telegram", { channel: "durov" }), "https://t.me/s/durov");
  assert.equal(build("telegram", { channel: "@telegram" }), "https://t.me/s/telegram");
  assert.equal(build("telegram", { channel: "https://t.me/s/telegram" }), "https://t.me/s/telegram");
  assert.equal(build("telegram", { channel: "t.me/durov/123" }), "https://t.me/s/durov");
  assert.equal(build("telegram", { channel: "ab" }), null);
});

test("GitLab and Codeberg, including self-hosted", () => {
  assert.equal(build("gitlab", { target: "gitlab-org/gitlab-runner", kind: "releases" }), "https://gitlab.com/gitlab-org/gitlab-runner/-/releases.atom");
  assert.equal(build("gitlab", { target: "https://gitlab.com/a/b/c/-/tree/main", kind: "tags" }), "https://gitlab.com/a/b/c/-/tags?format=atom");
  assert.equal(build("gitlab", { target: "https://git.example.org/team/app", kind: "activity" }), "https://git.example.org/team/app.atom");
  assert.equal(build("gitlab", { target: "@someone", kind: "releases" }), "https://gitlab.com/someone.atom");
  assert.equal(build("codeberg", { target: "forgejo/forgejo", kind: "releases" }), "https://codeberg.org/forgejo/forgejo/releases.rss");
  assert.equal(build("codeberg", { target: "https://gitea.example.com/me/tool.git", kind: "tags" }), "https://gitea.example.com/me/tool/tags.rss");
  assert.equal(build("codeberg", { target: "forgejo", kind: "releases" }), "https://codeberg.org/forgejo.rss");
  assert.equal(build("codeberg", { target: "bad name/x", kind: "releases" }), null);
});

test("DEV, Lobsters, Lemmy, Discourse", () => {
  assert.equal(build("devto", { name: "ben" }), "https://dev.to/feed/ben");
  assert.equal(build("devto", { name: "#rust" }), "https://dev.to/feed/tag/rust");
  assert.equal(build("devto", { name: "https://dev.to/t/webdev" }), "https://dev.to/feed/tag/webdev");
  assert.equal(build("lobsters", { tag: "" }), "https://lobste.rs/rss");
  assert.equal(build("lobsters", { tag: "https://lobste.rs/t/rust" }), "https://lobste.rs/t/rust.rss");
  assert.equal(build("lemmy", { community: "!technology@lemmy.world" }), "https://lemmy.world/feeds/c/technology.xml");
  assert.equal(build("lemmy", { community: "https://lemmy.ml/c/linux" }), "https://lemmy.ml/feeds/c/linux.xml");
  assert.equal(build("lemmy", { community: "technology" }), null);
  assert.equal(build("discourse", { forum: "meta.discourse.org" }), "https://meta.discourse.org/latest.rss");
  assert.equal(build("discourse", { forum: "https://meta.discourse.org/c/support/6" }), "https://meta.discourse.org/c/support/6.rss");
  assert.equal(build("discourse", { forum: "forum" }), null);
});

test("Tumblr, Vimeo, Pinterest, PyPI, Steam, Letterboxd", () => {
  assert.equal(build("tumblr", { blog: "staff" }), "https://staff.tumblr.com/rss");
  assert.equal(build("tumblr", { blog: "https://staff.tumblr.com/post/1" }), "https://staff.tumblr.com/rss");
  assert.equal(build("tumblr", { blog: "https://www.tumblr.com/staff" }), "https://staff.tumblr.com/rss");
  assert.equal(build("vimeo", { user: "staff" }), "https://vimeo.com/staff/videos/rss");
  assert.equal(build("vimeo", { user: "https://vimeo.com/channels/staffpicks" }), "https://vimeo.com/channels/staffpicks/videos/rss");
  assert.equal(build("vimeo", { user: "https://vimeo.com/76979871" }), null);
  assert.equal(build("pinterest", { user: "pinterest" }), "https://www.pinterest.com/pinterest/feed.rss");
  assert.equal(build("pinterest", { user: "https://www.pinterest.com/someone/recipes/" }), "https://www.pinterest.com/someone/recipes.rss");
  assert.equal(build("pypi", { package: "Requests" }), "https://pypi.org/rss/project/requests/releases.xml");
  assert.equal(build("pypi", { package: "https://pypi.org/project/numpy/" }), "https://pypi.org/rss/project/numpy/releases.xml");
  assert.equal(build("steam", { app: "570" }), "https://store.steampowered.com/feeds/news/app/570/");
  assert.equal(build("steam", { app: "https://store.steampowered.com/app/570/Dota_2/" }), "https://store.steampowered.com/feeds/news/app/570/");
  assert.equal(build("steam", { app: "dota" }), null);
  assert.equal(build("letterboxd", { user: "https://letterboxd.com/dave/films/" }), "https://letterboxd.com/dave/rss/");
});
