// 订阅源刷新失败时给人看的说明。核心记下了错误的种类（Feed.error_code，见 shibapost_feed::Error::code）
// 和原始的英文信息；以前记下的错误没有种类，从原始信息里认。只有纯函数，方便测试：npm run test:ui

import type { MessageKey } from "./i18n";

type Translate = (key: MessageKey, vars?: Record<string, string | number>) => string;

/** 从原始信息认出错误的种类（核心以前没记种类）。 */
function guessCode(error: string): string | null {
  if (error.startsWith("the server refused")) return "feed_refused";
  if (error.startsWith("not a feed")) return "not_a_feed";
  if (error.startsWith("server returned HTTP")) return "http_status";
  if (error.startsWith("request failed")) return "network";
  if (error.startsWith("response is larger")) return "too_large";
  if (error.startsWith("invalid url")) return "invalid_url";
  return null;
}

/**
 * 一句看得懂的说明。`error` 是核心记下的原始信息，`code` 是错误的种类（没有时从原始信息里认）。
 * 认不出来的照原样给。
 */
export function feedErrorText(t: Translate, error: string, code?: string | null): string {
  switch (code ?? guessCode(error)) {
    case "feed_refused": {
      const title = /saying "(.*)"$/s.exec(error)?.[1]?.trim();
      return title ? t("feedError.refused", { title }) : t("feedError.refusedPage");
    }
    case "http_status": {
      const status = Number(/HTTP (\d{3})/.exec(error)?.[1] ?? 0);
      if (status === 404 || status === 410) return t("feedError.gone", { status });
      if (status === 401 || status === 403) return t("feedError.denied", { status });
      if (status === 429) return t("feedError.tooMany");
      if (status >= 500) return t("feedError.server", { status });
      return status ? t("feedError.status", { status }) : t("error.http_status");
    }
    case "network":
      return /timed out/i.test(error) ? t("feedError.timeout") : t("feedError.network");
    case "not_a_feed":
      return t("feedError.notAFeed");
    case "too_large":
      return t("feedError.tooLarge");
    case "invalid_url":
      return t("feedError.invalidUrl");
    default:
      return error;
  }
}
