// 「添加订阅 → 播客」：按节目名搜索（苹果播客目录的公开接口，由核心去请求），一键订阅节目自己的 RSS。
// 中文界面在中国区目录里搜，中文节目多；其他语言在美国区搜。

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, Headphones, Plus, Search } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api, type PodcastResult } from "../lib/api";
import { feedKey } from "../lib/catalog";
import { errorText, useLang, useT } from "../lib/i18n";
import { keys, useNav } from "../lib/queries";
import { toast } from "./Toast";
import { Button, cx, Spinner, TextInput } from "./ui";

function Artwork({ src }: { src: string | null }) {
  const [failed, setFailed] = useState(false);
  if (!src || failed) {
    return (
      <span className="flex size-12 shrink-0 items-center justify-center rounded-md bg-hover text-fg-3">
        <Headphones size={20} strokeWidth={1.5} />
      </span>
    );
  }
  return (
    <img
      src={src}
      alt=""
      loading="lazy"
      referrerPolicy="no-referrer"
      onError={() => setFailed(true)}
      className="size-12 shrink-0 rounded-md bg-hover object-cover ring-1 ring-line"
    />
  );
}

export function PodcastSearch({ folder, onDone }: { folder: string; onDone: () => void }) {
  const t = useT();
  const lang = useLang();
  const qc = useQueryClient();
  const feeds = useNav().data?.feeds;
  const [text, setText] = useState("");
  const [term, setTerm] = useState("");
  const [adding, setAdding] = useState<Set<string>>(new Set());

  // 停笔 400 毫秒再搜
  useEffect(() => {
    const timer = setTimeout(() => setTerm(text.trim()), 400);
    return () => clearTimeout(timer);
  }, [text]);

  const country = lang === "zh" ? "cn" : "us";
  const results = useQuery({
    queryKey: ["podcasts", term, country],
    queryFn: () => api.searchPodcasts(term, country),
    enabled: term.length > 0,
    staleTime: 10 * 60_000,
    retry: false,
  });
  const have = useMemo(() => new Set((feeds ?? []).map((f) => feedKey(f.url))), [feeds]);

  const subscribe = async (show: PodcastResult) => {
    setAdding((s) => new Set([...s, show.feed_url]));
    try {
      await api.addFeed(show.feed_url, folder, { title: show.title, icon_url: show.artwork });
      toast(t("podcast.added", { name: show.title }));
      void qc.invalidateQueries({ queryKey: keys.nav });
      void qc.invalidateQueries({ queryKey: ["items"] });
    } catch (err) {
      toast(`${show.title}: ${errorText(t, err)}`);
    } finally {
      setAdding((s) => {
        const next = new Set(s);
        next.delete(show.feed_url);
        return next;
      });
    }
  };

  const list = results.data ?? [];
  return (
    <div className="flex flex-col gap-3 px-4 pb-4">
      <div className="relative">
        <Search size={14} strokeWidth={1.75} className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-fg-3" />
        <TextInput
          autoFocus
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder={t("podcast.searchPlaceholder")}
          aria-label={t("podcast.searchPlaceholder")}
          className="w-full pl-8"
        />
      </div>
      <div className="h-[360px] overflow-y-auto">
        {!term ? (
          <p className="px-1 pt-2 text-[12px] leading-relaxed text-fg-3">{t("podcast.hint")}</p>
        ) : results.isFetching && list.length === 0 ? (
          <div className="flex justify-center py-10 text-fg-3">
            <Spinner size={16} />
          </div>
        ) : results.error ? (
          <p className="px-1 pt-2 text-[12px] text-danger">{errorText(t, results.error)}</p>
        ) : list.length === 0 ? (
          <p className="px-1 pt-2 text-[12px] text-fg-3">{t("podcast.none")}</p>
        ) : (
          <ul className="flex flex-col gap-1">
            {list.map((show) => {
              const subscribed = have.has(feedKey(show.feed_url));
              const busy = adding.has(show.feed_url);
              return (
                <li key={show.feed_url} className="flex items-center gap-3 rounded-md px-1.5 py-1.5 hover:bg-hover">
                  <Artwork src={show.artwork} />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-[13px] font-medium text-fg">{show.title}</div>
                    <div className="truncate text-[12px] text-fg-3">
                      {[show.author, show.genre, show.episodes ? t("podcast.episodes", { n: show.episodes }) : null]
                        .filter(Boolean)
                        .join(" · ")}
                    </div>
                  </div>
                  <Button
                    variant={subscribed ? "ghost" : "secondary"}
                    disabled={subscribed || busy}
                    onClick={() => void subscribe(show)}
                    className={cx("shrink-0", subscribed && "text-fg-3")}
                  >
                    {busy ? <Spinner size={12} /> : subscribed ? <Check size={13} /> : <Plus size={13} />}
                    {subscribed ? t("podcast.subscribed") : t("podcast.subscribe")}
                  </Button>
                </li>
              );
            })}
          </ul>
        )}
      </div>
      <div className="flex items-center justify-between gap-2">
        <span className="text-[11px] text-fg-4">{t("podcast.source")}</span>
        <Button variant="ghost" onClick={onDone}>
          {t("catalog.done")}
        </Button>
      </div>
    </div>
  );
}
