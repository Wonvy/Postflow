// RSSHub：把路由的参数填进路径，拼出订阅地址。路由目录和搜索在核心里（crates/feed/src/rsshub.rs）。
// 这里只有纯函数，方便测试：npm run test:ui

/** 用户没设置时用的实例。公共实例对阅读器有访问限制，设置里建议换成自建的。 */
export const DEFAULT_INSTANCE = "https://rsshub.app";
export const SETTING_KEY = "rsshub_instance";
/** 实例设了 ACCESS_KEY 时的访问密钥，订阅地址后面带上 `?key=`。 */
export const KEY_SETTING = "rsshub_key";

/** 测试实例的结果，和核心里的 InstanceCheck 对应。 */
export type InstanceCheck =
  | { state: "ok"; items: number }
  | { state: "unreachable"; detail: string }
  | { state: "challenge" | "unauthorized" | "not_rsshub" | "error"; status: number };

/** 部署指南：中文界面看中文版。 */
export function guideUrl(lang: string): string {
  const base = "https://github.com/Wonvy/Postflow/blob/main/docs/";
  return lang === "zh" ? `${base}rsshub.zh-CN.md` : `${base}rsshub.md`;
}

export interface RsshubParam {
  name: string;
  description: string;
  optional: boolean;
  options: { value: string; label: string }[];
  default: string | null;
  multi_segment: boolean;
}

export interface RsshubRoute {
  namespace: string;
  site: string;
  site_url: string | null;
  name: string;
  path: string;
  example: string;
  description: string;
  params: RsshubParam[];
  categories: string[];
  require_config: boolean;
  require_puppeteer: boolean;
  anti_crawler: boolean;
}

/** 实例地址：补上 https://、去掉末尾的斜杠。不像网址的返回 null。 */
export function normalizeInstance(input: string | null | undefined): string | null {
  const raw = (input ?? "").trim() || DEFAULT_INSTANCE;
  // 没写协议时：本机、局域网 IP、带端口的地址多半是自己部署的，用 http；其他的用 https
  const local = /^(localhost|\d{1,3}(\.\d{1,3}){3}|\[[0-9a-f:]+\]|[^/]*\.local)(:\d+)?(\/|$)|^[^/]+:\d+(\/|$)/i.test(raw);
  const withScheme = /^https?:\/\//i.test(raw) ? raw : `${local ? "http" : "https"}://${raw}`;
  try {
    const url = new URL(withScheme);
    if (!url.hostname.includes(".") && url.hostname !== "localhost") return null;
    return `${url.origin}${url.pathname.replace(/\/+$/, "")}`;
  } catch {
    return null;
  }
}

function encodeValue(value: string, multi: boolean): string {
  return multi ? value.split("/").map(encodeURIComponent).join("/") : encodeURIComponent(value);
}

/**
 * 把参数填进路径模板（如 `/issue/:user/:repo/:state?`），拼出完整的订阅地址。
 * 必填的参数没填返回 null。可省的参数没填时，它和后面的部分都不写。
 */
export function buildFeedUrl(
  instance: string,
  route: Pick<RsshubRoute, "namespace" | "path">,
  values: Record<string, string>,
  key = "",
): string | null {
  const parts: string[] = [];
  for (const segment of route.path.split("/").filter(Boolean)) {
    if (!segment.startsWith(":")) {
      parts.push(segment);
      continue;
    }
    const optional = segment.endsWith("?");
    const body = segment.slice(1).replace(/\?$/, "");
    const brace = body.indexOf("{");
    const name = brace >= 0 ? body.slice(0, brace) : body;
    const multi = brace >= 0 && /\.\+|\.\*/.test(body.slice(brace));
    const value = (values[name] ?? "").trim();
    if (!value) {
      if (optional) break;
      return null;
    }
    parts.push(encodeValue(value, multi));
  }
  const query = key.trim() ? `?key=${encodeURIComponent(key.trim())}` : "";
  return `${instance}/${route.namespace}${parts.length ? `/${parts.join("/")}` : ""}${query}`;
}

/** 表单的初始值：radar 从网址里取到的值优先，其次是 RSSHub 给的默认值。 */
export function initialValues(route: RsshubRoute, found: Record<string, string> = {}): Record<string, string> {
  return Object.fromEntries(route.params.map((p) => [p.name, found[p.name] ?? p.default ?? ""]));
}

/** 参数说明是 Markdown 写的，表单里只显示纯文字：去掉链接语法、代码标记和多余的空行。 */
export function plainDescription(text: string): string {
  return text
    .replace(/:::\s*\w*/g, "")
    .replace(/!\[[^\]]*\]\([^)]*\)/g, "")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/\*\*([^*]+)\*\*/g, "$1")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

/**
 * 有些网站 Postflow 自己就能订阅，不需要 RSSHub（YouTube 的频道和播放列表）。
 * 能的话返回该网站的网页地址，交给「网址」那边直接订阅；不能返回 null。
 */
export function directUrl(route: Pick<RsshubRoute, "namespace" | "path">, values: Record<string, string>): string | null {
  if (route.namespace !== "youtube") return null;
  const v = (name: string) => (values[name] ?? "").trim();
  const head = route.path.split("/").filter(Boolean)[0] ?? "";
  const username = v("username");
  const id = v("id");
  switch (head) {
    case "user":
    case "c":
      if (!username) return null;
      if (username.startsWith("@")) return `https://www.youtube.com/${username}`;
      return head === "c" ? `https://www.youtube.com/c/${encodeURIComponent(username)}` : `https://www.youtube.com/@${username}`;
    case "channel":
      return id ? `https://www.youtube.com/channel/${encodeURIComponent(id)}` : null;
    case "playlist":
      return id ? `https://www.youtube.com/playlist?list=${encodeURIComponent(id)}` : null;
    default:
      return null;
  }
}

/** 订阅地址出错时，从核心的错误信息里取出 HTTP 状态码（「server returned HTTP 403」）。 */
export function httpStatusOf(message: string): number | null {
  const m = /HTTP (\d{3})/.exec(message);
  return m ? Number(m[1]) : null;
}
