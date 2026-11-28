// 添加订阅里的「常用网站」：这些网站有官方的订阅源（或者 ShibaPost 自己能解析，比如 YouTube），
// 不需要 RSSHub。点图标、填一个名字，就拼出订阅地址，交给正常的预览和订阅流程。
// 每个输入框也接受直接粘贴整个网页地址，从里面取出要的部分。
// 这里只有纯函数，方便测试：npm run test:ui

export type QuickSourceId =
  | "youtube"
  | "github"
  | "reddit"
  | "bluesky"
  | "mastodon"
  | "medium"
  | "substack"
  | "stackoverflow"
  | "hackernews"
  | "v2ex"
  | "arxiv"
  | "telegram"
  | "gitlab"
  | "codeberg"
  | "devto"
  | "lobsters"
  | "lemmy"
  | "discourse"
  | "tumblr"
  | "vimeo"
  | "pinterest"
  | "pypi"
  | "steam"
  | "letterboxd";

export interface QuickField {
  name: string;
  /** 下拉框的选项；没有就是文字输入框。 */
  options?: string[];
  /** 可以不填（比如 V2EX 的节点，不填就是全站最新）。 */
  optional?: boolean;
}

export interface QuickSource {
  id: QuickSourceId;
  /** simple-icons 里的图标名，界面按它取图标。 */
  icon: string;
  fields: QuickField[];
  /** 拼出订阅地址（或者 ShibaPost 能解析的网页地址）。填得不对返回 null。 */
  build: (values: Record<string, string>) => string | null;
}

const clean = (v: string | undefined) => (v ?? "").trim();

/** 把粘贴进来的整个地址拆成路径的各段；不是地址就返回 null。 */
function pathOf(input: string, host: RegExp): string[] | null {
  const text = clean(input);
  // 有协议，或者「域名/路径」才算地址；只有一个域名的（比如 Bluesky 的用户名 bsky.app）不算
  if (!/^https?:\/\//i.test(text) && !/^[^/\s]+\.[^/\s]+\/./i.test(text)) return null;
  try {
    const url = new URL(/^https?:\/\//i.test(text) ? text : `https://${text}`);
    if (!host.test(url.hostname)) return null;
    return url.pathname.split("/").filter(Boolean).map(decodeURIComponent);
  } catch {
    return null;
  }
}

const SAFE = /^[\w.-]+$/;

/** 粘贴的任意网址（自建的 GitLab、Forgejo、论坛）：返回主机名和路径各段；不是网址返回 null。 */
function anyUrl(input: string): { host: string; parts: string[] } | null {
  const text = clean(input);
  if (!/^https?:\/\//i.test(text) && !/^[^/\s]+\.[^/\s]+\/./i.test(text)) return null;
  try {
    const url = new URL(/^https?:\/\//i.test(text) ? text : `https://${text}`);
    return { host: url.host, parts: url.pathname.split("/").filter(Boolean).map(decodeURIComponent) };
  } catch {
    return null;
  }
}

/** 「owner/repo」或粘贴的仓库地址，拆成主机名和路径；`/-/` 后面（GitLab 的子页面）不要。 */
function repoPath(input: string, defaultHost: string): { host: string; parts: string[] } | null {
  const url = anyUrl(input);
  const host = url?.host ?? defaultHost;
  let parts = url?.parts ?? clean(input).replace(/^@/, "").split("/").filter(Boolean);
  const dash = parts.indexOf("-");
  if (dash >= 0) parts = parts.slice(0, dash);
  parts = parts.map((p) => p.replace(/\.git$/, ""));
  return parts.length > 0 && parts.every((p) => SAFE.test(p)) ? { host, parts } : null;
}

export const QUICK_SOURCES: QuickSource[] = [
  {
    id: "youtube",
    icon: "siYoutube",
    fields: [{ name: "channel" }],
    build: ({ channel }) => {
      const v = clean(channel);
      if (!v) return null;
      if (/^https?:\/\//i.test(v) || /^(www\.|m\.)?youtube\.com\//i.test(v)) {
        return /^https?:\/\//i.test(v) ? v : `https://${v}`;
      }
      if (/^UC[\w-]{20,}$/.test(v)) return `https://www.youtube.com/channel/${v}`;
      if (/^PL[\w-]{10,}$/.test(v)) return `https://www.youtube.com/playlist?list=${v}`;
      const handle = v.replace(/^@/, "");
      return /^[\w.-]+$/.test(handle) ? `https://www.youtube.com/@${handle}` : null;
    },
  },
  {
    id: "github",
    icon: "siGithub",
    fields: [{ name: "target" }, { name: "kind", options: ["releases", "commits", "tags", "user"] }],
    build: ({ target, kind }) => {
      const parts = pathOf(target ?? "", /(^|\.)github\.com$/i) ?? clean(target).replace(/^@/, "").split("/").filter(Boolean);
      const [owner, repo] = parts;
      if (!owner || !SAFE.test(owner)) return null;
      if (kind === "user" || !repo) return `https://github.com/${owner}.atom`;
      const name = repo.replace(/\.git$/, "");
      if (!SAFE.test(name)) return null;
      const what = kind === "commits" || kind === "tags" ? kind : "releases";
      return `https://github.com/${owner}/${name}/${what}.atom`;
    },
  },
  {
    id: "reddit",
    icon: "siReddit",
    fields: [{ name: "subreddit" }],
    build: ({ subreddit }) => {
      const parts = pathOf(subreddit ?? "", /(^|\.)reddit\.com$/i);
      const name = parts ? (parts[0] === "r" ? parts[1] : undefined) : clean(subreddit).replace(/^\/?r\//i, "");
      return name && /^\w+$/.test(name) ? `https://www.reddit.com/r/${name}/.rss` : null;
    },
  },
  {
    id: "bluesky",
    icon: "siBluesky",
    fields: [{ name: "handle" }],
    build: ({ handle }) => {
      const parts = pathOf(handle ?? "", /(^|\.)bsky\.app$/i);
      const v = parts ? (parts[0] === "profile" ? parts[1] : undefined) : clean(handle).replace(/^@/, "");
      if (!v) return null;
      // 只写了用户名时补上默认的 .bsky.social
      const full = v.includes(".") || v.startsWith("did:") ? v : `${v}.bsky.social`;
      return /^[\w.:-]+$/.test(full) ? `https://bsky.app/profile/${full}/rss` : null;
    },
  },
  {
    id: "mastodon",
    icon: "siMastodon",
    fields: [{ name: "account" }],
    build: ({ account }) => {
      const v = clean(account);
      // https://mastodon.social/@user
      const url = /^https?:\/\/([^/]+)\/@([\w.]+)/i.exec(v);
      if (url) return `https://${url[1]}/@${url[2]}.rss`;
      // @user@mastodon.social
      const handle = /^@?([\w.]+)@([\w.-]+\.[a-z]{2,})$/i.exec(v);
      return handle ? `https://${handle[2]}/@${handle[1]}.rss` : null;
    },
  },
  {
    id: "medium",
    icon: "siMedium",
    fields: [{ name: "name" }],
    build: ({ name }) => {
      const parts = pathOf(name ?? "", /(^|\.)medium\.com$/i);
      const v = parts ? parts[0] : clean(name);
      if (!v) return null;
      const sub = /^([\w-]+)\.medium\.com$/i.exec(v);
      if (sub) return `https://medium.com/feed/@${sub[1]}`;
      return /^@?[\w.-]+$/.test(v) ? `https://medium.com/feed/${v}` : null;
    },
  },
  {
    id: "substack",
    icon: "siSubstack",
    fields: [{ name: "name" }],
    build: ({ name }) => {
      const v = clean(name);
      const host = /^(?:https?:\/\/)?([\w-]+)\.substack\.com/i.exec(v);
      const n = host ? host[1] : v;
      return n && /^[\w-]+$/.test(n) ? `https://${n.toLowerCase()}.substack.com/feed` : null;
    },
  },
  {
    id: "stackoverflow",
    icon: "siStackoverflow",
    fields: [{ name: "tag" }],
    build: ({ tag }) => {
      const parts = pathOf(tag ?? "", /(^|\.)stackoverflow\.com$/i);
      const v = parts ? (parts[0] === "questions" && parts[1] === "tagged" ? parts[2] : undefined) : clean(tag);
      return v && /^[\w.#+-]+$/.test(v) ? `https://stackoverflow.com/feeds/tag/${encodeURIComponent(v.toLowerCase())}` : null;
    },
  },
  {
    id: "hackernews",
    icon: "siYcombinator",
    fields: [],
    build: () => "https://news.ycombinator.com/rss",
  },
  {
    id: "v2ex",
    icon: "siV2ex",
    fields: [{ name: "node", optional: true }],
    build: ({ node }) => {
      const parts = pathOf(node ?? "", /(^|\.)v2ex\.com$/i);
      const v = parts ? (parts[0] === "go" ? parts[1] : undefined) : clean(node);
      if (!v) return "https://www.v2ex.com/index.xml";
      return /^[\w-]+$/.test(v) ? `https://www.v2ex.com/feed/${v}.xml` : null;
    },
  },
  {
    id: "arxiv",
    icon: "siArxiv",
    fields: [{ name: "category" }],
    build: ({ category }) => {
      const parts = pathOf(category ?? "", /(^|\.)arxiv\.org$/i);
      // arxiv.org/list/cs.CL/recent → cs.CL
      const v = parts ? (parts[0] === "list" ? parts[1] : undefined) : clean(category);
      return v && /^[a-z-]+(\.[A-Za-z-]+)?$/.test(v) ? `https://rss.arxiv.org/rss/${v}` : null;
    },
  },
  {
    // 没有订阅源：ShibaPost 自己读频道的公开网页（t.me/s/频道名）
    id: "telegram",
    icon: "siTelegram",
    fields: [{ name: "channel" }],
    build: ({ channel }) => {
      const parts = pathOf(channel ?? "", /^(t\.me|telegram\.me)$/i);
      const v = parts ? (parts[0] === "s" ? parts[1] : parts[0]) : clean(channel).replace(/^@/, "");
      return v && /^\w{4,}$/.test(v) ? `https://t.me/s/${v}` : null;
    },
  },
  {
    // 也接受自建 GitLab 的地址
    id: "gitlab",
    icon: "siGitlab",
    fields: [{ name: "target" }, { name: "kind", options: ["releases", "tags", "activity", "user"] }],
    build: ({ target, kind }) => {
      const repo = repoPath(target ?? "", "gitlab.com");
      if (!repo) return null;
      const base = `https://${repo.host}/${repo.parts.join("/")}`;
      if (kind === "user" || repo.parts.length === 1) return `https://${repo.host}/${repo.parts[0]}.atom`;
      if (kind === "tags") return `${base}/-/tags?format=atom`;
      if (kind === "activity") return `${base}.atom`;
      return `${base}/-/releases.atom`;
    },
  },
  {
    // Forgejo、Gitea 的地址都一样，自建的也行
    id: "codeberg",
    icon: "siCodeberg",
    fields: [{ name: "target" }, { name: "kind", options: ["releases", "tags", "activity", "user"] }],
    build: ({ target, kind }) => {
      const repo = repoPath(target ?? "", "codeberg.org");
      if (!repo) return null;
      const [owner, name] = repo.parts;
      if (kind === "user" || !name) return `https://${repo.host}/${owner}.rss`;
      const base = `https://${repo.host}/${owner}/${name}`;
      if (kind === "tags") return `${base}/tags.rss`;
      if (kind === "activity") return `${base}.rss`;
      return `${base}/releases.rss`;
    },
  },
  {
    id: "devto",
    icon: "siDevdotto",
    fields: [{ name: "name" }],
    build: ({ name }) => {
      const parts = pathOf(name ?? "", /(^|\.)dev\.to$/i);
      if (parts) {
        if (parts[0] === "t" && parts[1] && /^\w+$/.test(parts[1])) return `https://dev.to/feed/tag/${parts[1]}`;
        return parts[0] && /^[\w-]+$/.test(parts[0]) ? `https://dev.to/feed/${parts[0]}` : null;
      }
      const v = clean(name);
      if (v.startsWith("#")) return /^#\w+$/.test(v) ? `https://dev.to/feed/tag/${v.slice(1)}` : null;
      const user = v.replace(/^@/, "");
      return /^[\w-]+$/.test(user) ? `https://dev.to/feed/${user}` : null;
    },
  },
  {
    id: "lobsters",
    icon: "siLobsters",
    fields: [{ name: "tag", optional: true }],
    build: ({ tag }) => {
      const parts = pathOf(tag ?? "", /(^|\.)lobste\.rs$/i);
      const v = parts ? (parts[0] === "t" ? parts[1] : undefined) : clean(tag);
      if (!v) return "https://lobste.rs/rss";
      return /^[\w-]+$/.test(v) ? `https://lobste.rs/t/${v}.rss` : null;
    },
  },
  {
    id: "lemmy",
    icon: "siLemmy",
    fields: [{ name: "community" }],
    build: ({ community }) => {
      const url = anyUrl(community ?? "");
      if (url) {
        return url.parts[0] === "c" && url.parts[1] && /^\w+$/.test(url.parts[1])
          ? `https://${url.host}/feeds/c/${url.parts[1]}.xml`
          : null;
      }
      // !technology@lemmy.world
      const handle = /^!?(\w+)@([\w.-]+\.[a-z]{2,})$/i.exec(clean(community));
      return handle ? `https://${handle[2]}/feeds/c/${handle[1]}.xml` : null;
    },
  },
  {
    id: "discourse",
    icon: "siDiscourse",
    fields: [{ name: "forum" }],
    build: ({ forum }) => {
      // 只写了域名（meta.discourse.org）也行
      const v = clean(forum);
      const url = anyUrl(/^https?:\/\//i.test(v) || v.includes("/") ? v : `https://${v}`);
      if (!url || !url.host.includes(".")) return null;
      // 某个分类：/c/support/6 → /c/support/6.rss
      if (url.parts[0] === "c" && url.parts.length >= 2 && url.parts.every((p) => /^[\w-]+$/.test(p))) {
        return `https://${url.host}/${url.parts.join("/")}.rss`;
      }
      return `https://${url.host}/latest.rss`;
    },
  },
  {
    id: "tumblr",
    icon: "siTumblr",
    fields: [{ name: "blog" }],
    build: ({ blog }) => {
      const v = clean(blog);
      const sub = /^(?:https?:\/\/)?(?!www\.)([\w-]+)\.tumblr\.com/i.exec(v);
      const parts = sub ? null : pathOf(v, /(^|\.)tumblr\.com$/i);
      const name = sub ? sub[1] : parts ? parts[0] : v.replace(/^@/, "");
      return name && /^[\w-]+$/.test(name) ? `https://${name.toLowerCase()}.tumblr.com/rss` : null;
    },
  },
  {
    id: "vimeo",
    icon: "siVimeo",
    fields: [{ name: "user" }],
    build: ({ user }) => {
      const parts = pathOf(user ?? "", /(^|\.)vimeo\.com$/i);
      if (parts?.[0] === "channels") {
        return parts[1] && /^[\w-]+$/.test(parts[1]) ? `https://vimeo.com/channels/${parts[1]}/videos/rss` : null;
      }
      const v = parts ? parts[0] : clean(user).replace(/^@/, "");
      return v && /^[\w-]+$/.test(v) && !/^\d+$/.test(v) ? `https://vimeo.com/${v}/videos/rss` : null;
    },
  },
  {
    id: "pinterest",
    icon: "siPinterest",
    fields: [{ name: "user" }],
    build: ({ user }) => {
      const parts = pathOf(user ?? "", /(^|\.)pinterest\.[a-z.]+$/i) ?? clean(user).replace(/^@/, "").split("/").filter(Boolean);
      const [name, board] = parts;
      if (!name || !/^[\w.-]+$/.test(name)) return null;
      if (board && /^[\w-]+$/.test(board)) return `https://www.pinterest.com/${name}/${board}.rss`;
      return `https://www.pinterest.com/${name}/feed.rss`;
    },
  },
  {
    id: "pypi",
    icon: "siPypi",
    fields: [{ name: "package" }],
    build: ({ package: pkg }) => {
      const parts = pathOf(pkg ?? "", /(^|\.)pypi\.org$/i);
      const v = parts ? (parts[0] === "project" ? parts[1] : undefined) : clean(pkg);
      return v && /^[\w.-]+$/.test(v) ? `https://pypi.org/rss/project/${v.toLowerCase()}/releases.xml` : null;
    },
  },
  {
    id: "steam",
    icon: "siSteam",
    fields: [{ name: "app" }],
    build: ({ app }) => {
      const parts = pathOf(app ?? "", /(^|\.)(steampowered|steamcommunity)\.com$/i);
      const v = parts ? parts[parts.indexOf("app") + 1] : clean(app);
      return v && /^\d+$/.test(v) ? `https://store.steampowered.com/feeds/news/app/${v}/` : null;
    },
  },
  {
    id: "letterboxd",
    icon: "siLetterboxd",
    fields: [{ name: "user" }],
    build: ({ user }) => {
      const parts = pathOf(user ?? "", /(^|\.)letterboxd\.com$/i);
      const v = parts ? parts[0] : clean(user).replace(/^@/, "");
      return v && /^\w+$/.test(v) ? `https://letterboxd.com/${v}/rss/` : null;
    },
  },
];

export function quickSource(id: QuickSourceId): QuickSource {
  return QUICK_SOURCES.find((s) => s.id === id) ?? (QUICK_SOURCES[0] as QuickSource);
}

/** 表单的初始值：下拉框选第一项，其他为空。 */
export function quickInitial(source: QuickSource): Record<string, string> {
  return Object.fromEntries(source.fields.map((f) => [f.name, f.options?.[0] ?? ""]));
}
