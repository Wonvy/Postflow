// 订阅源的几类：全部、文章、视频、播客。侧栏订阅那边顶上一排图标切换，列表和侧栏都只显示这一类。
// 每个源的类型由核心按内容判断（一半以上带音频是播客，视频网站或带视频的是视频），用户可以在「编辑订阅」里改。

import { FileText, Headphones, Layers, type LucideIcon, Play } from "lucide-react";
import { useMemo } from "react";
import type { Feed, FeedKind } from "./api";
import type { MessageKey } from "./i18n";
import { useNav } from "./queries";
import { useUi } from "./store";

export const FEED_KINDS: FeedKind[] = ["article", "video", "podcast"];

export const FEED_KIND_ICON: Record<FeedKind | "all", LucideIcon> = {
  all: Layers,
  article: FileText,
  video: Play,
  podcast: Headphones,
};

export const FEED_KIND_LABEL: Record<FeedKind | "all", MessageKey> = {
  all: "feedKind.all",
  article: "feedKind.article",
  video: "feedKind.video",
  podcast: "feedKind.podcast",
};

/** 订阅源里出现了哪几类。 */
export function kindsIn(feeds: readonly Pick<Feed, "kind">[]): FeedKind[] {
  return FEED_KINDS.filter((k) => feeds.some((f) => f.kind === k));
}

/** 现在只看哪一类。选的那一类已经没有源了（比如刚退订）时当作全部。 */
export function resolveFeedKind(chosen: FeedKind | "all", feeds: readonly Pick<Feed, "kind">[]): FeedKind | "all" {
  if (chosen === "all") return "all";
  const present = kindsIn(feeds);
  return present.length > 1 && present.includes(chosen) ? chosen : "all";
}

export function useFeedKind(): FeedKind | "all" {
  const chosen = useUi((s) => s.feedKind);
  const feeds = useNav().data?.feeds;
  return useMemo(() => resolveFeedKind(chosen, feeds ?? []), [chosen, feeds]);
}
