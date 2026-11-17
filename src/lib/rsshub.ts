// RSSHub：把路由的参数填进路径，拼出订阅地址。路由目录和搜索在核心里（crates/feed/src/rsshub.rs）。
// 这里只有纯函数，方便测试：npm run test:ui

/** 用户没设置时用的实例。公共实例对阅读器有访问限制，设置里建议换成自建的。 */
export const DEFAULT_INSTANCE = "https://rsshub.app";
export const SETTING_KEY = "rsshub_instance";

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
  const withScheme = /^https?:\/\//i.test(raw) ? raw : `https://${raw}`;
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
  return `${instance}/${route.namespace}${parts.length ? `/${parts.join("/")}` : ""}`;
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
