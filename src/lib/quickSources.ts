// 添加订阅里的「常用网站」：这些网站有官方的订阅源（或者 Postflow 自己能解析，比如 YouTube），
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
  | "arxiv";

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
  /** 拼出订阅地址（或者 Postflow 能解析的网页地址）。填得不对返回 null。 */
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
];

export function quickSource(id: QuickSourceId): QuickSource {
  return QUICK_SOURCES.find((s) => s.id === id) ?? (QUICK_SOURCES[0] as QuickSource);
}

/** 表单的初始值：下拉框选第一项，其他为空。 */
export function quickInitial(source: QuickSource): Record<string, string> {
  return Object.fromEntries(source.fields.map((f) => [f.name, f.options?.[0] ?? ""]));
}
