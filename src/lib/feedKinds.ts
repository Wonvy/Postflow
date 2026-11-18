// 订阅源的几类：全部、文章、视频、播客。侧栏订阅那边顶上一排图标切换，列表和侧栏都只显示这一类。
// 每个源的类型由核心按内容判断（一半以上带音频是播客，视频网站或带视频的是视频），用户可以在「编辑订阅」里改。

import { FileText, Headphones, Layers, type LucideIcon, Newspaper, Play } from "lucide-react";
import { useMemo } from "react";
import type { ContentKind, Feed, FeedKind, Newsletter } from "./api";
import type { MessageKey } from "./i18n";
import { useNav } from "./queries";
import { type MediaList, useUi } from "./store";

export const FEED_KINDS: FeedKind[] = ["article", "video", "podcast"];

export const FEED_KIND_ICON: Record<ContentKind | "all", LucideIcon> = {
  all: Layers,
  article: FileText,
  video: Play,
  podcast: Headphones,
  newsletter: Newspaper,
};

export const FEED_KIND_LABEL: Record<ContentKind | "all", MessageKey> = {
  all: "feedKind.all",
  article: "feedKind.article",
  video: "feedKind.video",
  podcast: "feedKind.podcast",
  newsletter: "feedKind.newsletter",
};

/** 订阅里出现了哪几类：订阅源的类型，有放到订阅里看的订阅邮件时再加上 Newsletter。 */
export function kindsIn(feeds: readonly Pick<Feed, "kind">[], newsletters: readonly unknown[] = []): ContentKind[] {
  const kinds: ContentKind[] = FEED_KINDS.filter((k) => feeds.some((f) => f.kind === k));
  if (newsletters.length > 0) kinds.push("newsletter");
  return kinds;
}

/** 现在只看哪一类。选的那一类已经没有了（比如刚退订）时当作全部。 */
export function resolveFeedKind(
  chosen: ContentKind | "all",
  feeds: readonly Pick<Feed, "kind">[],
  newsletters: readonly unknown[] = [],
): ContentKind | "all" {
  if (chosen === "all") return "all";
  const present = kindsIn(feeds, newsletters);
  return present.length > 1 && present.includes(chosen) ? chosen : "all";
}

const NO_NEWSLETTERS: Newsletter[] = [];

export function useNewsletters(): Newsletter[] {
  return useNav().data?.newsletters ?? NO_NEWSLETTERS;
}

export function useFeedKind(): ContentKind | "all" {
  const chosen = useUi((s) => s.feedKind);
  const feeds = useNav().data?.feeds;
  const newsletters = useNewsletters();
  return useMemo(() => resolveFeedKind(chosen, feeds ?? [], newsletters), [chosen, feeds, newsletters]);
}

/**
 * 当前列表是视频还是播客的列表（它们的视图和文章分开记）：订阅那边选了视频、播客这一类，
 * 或者打开的是一个视频、播客订阅源。其他（文章、邮件、全部）是 undefined。
 */
export function useListMedia(): MediaList {
  const scope = useUi((s) => s.scope);
  const space = useUi((s) => s.space);
  const kind = useFeedKind();
  const feeds = useNav().data?.feeds;
  if (space !== "feeds") return undefined;
  const k = scope.kind === "feed" ? feeds?.find((f) => f.id === scope.feedId)?.kind : kind;
  return k === "video" || k === "podcast" ? k : undefined;
}
