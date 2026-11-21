// 「添加订阅 → 播客」搜索时，目录里的播客下面接着列出苹果播客目录里搜到的节目（公开接口，由核心去请求），
// 一键订阅节目自己的 RSS。中文界面在中国区目录里搜，中文节目多；其他语言在美国区搜。目录里已经列出来的不重复。

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, Headphones, Plus } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api, type PodcastResult } from "../lib/api";
import { feedKey } from "../lib/catalog";
import { errorText, useLang, useT } from "../lib/i18n";
import { keys, useNav } from "../lib/queries";
import { toast } from "./Toast";
import { Button, Spinner } from "./ui";

function Artwork({ src }: { src: string | null }) {
  const [failed, setFailed] = useState(false);
  if (!src || failed) {
    return (
      <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-hover text-fg-3">
        <Headphones size={16} strokeWidth={1.5} />
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
      className="size-8 shrink-0 rounded-md bg-hover object-cover ring-1 ring-line"
    />
  );
}

export function ApplePodcastResults({
  search,
  folder,
  listed,
}: {
  /** 搜索框里的字，停笔 400 毫秒再搜。 */
  search: string;
  folder: string;
  /** 上面目录里已经列出来的订阅地址（feedKey），这里不再重复。 */
  listed: ReadonlySet<string>;
}) {
  const t = useT();
  const lang = useLang();
  const qc = useQueryClient();
  const feeds = useNav().data?.feeds;
  const [term, setTerm] = useState("");
  const [adding, setAdding] = useState<Set<string>>(new Set());

  useEffect(() => {
    const timer = setTimeout(() => setTerm(search.trim()), 400);
    return () => clearTimeout(timer);
  }, [search]);

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
      await qc.invalidateQueries({ queryKey: keys.nav });
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

  if (!term) return null;
  const list = (results.data ?? []).filter((show) => !listed.has(feedKey(show.feed_url)));
  return (
    <section className="mt-3">
      <h3 className="mb-1 flex items-center gap-2 px-2 text-[12px] font-medium text-fg-3">
        {t("podcast.fromApple")}
        {results.isFetching && <Spinner size={11} />}
      </h3>
      {results.error ? (
        <p className="px-2 py-2 text-[12px] text-danger">{errorText(t, results.error)}</p>
      ) : !results.isFetching && list.length === 0 ? (
        <p className="px-2 py-2 text-[12px] text-fg-3">{t("podcast.none")}</p>
      ) : (
        <ul className="flex flex-col gap-px">
          {list.map((show) => {
            const subscribed = have.has(feedKey(show.feed_url));
            const busy = adding.has(show.feed_url);
            return (
              <li key={show.feed_url} className="flex items-center gap-3 rounded-md px-2 py-2 hover:bg-hover">
                <Artwork src={show.artwork} />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-[13px] font-medium text-fg">{show.title}</div>
                  <div className="truncate text-[12px] text-fg-3">
                    {[show.author, show.genre, show.episodes ? t("podcast.episodes", { n: show.episodes }) : null]
                      .filter(Boolean)
                      .join(" · ")}
                  </div>
                </div>
                {subscribed ? (
                  <span className="flex h-7 shrink-0 items-center gap-1 px-2 text-[12px] text-fg-3">
                    <Check size={13} strokeWidth={2} />
                    {t("podcast.subscribed")}
                  </span>
                ) : (
                  <Button className="h-7 shrink-0 px-2.5" disabled={busy} onClick={() => void subscribe(show)}>
                    {busy ? <Spinner size={12} /> : <Plus size={13} strokeWidth={2} />}
                    {busy ? t("feed.subscribing") : t("podcast.subscribe")}
                  </Button>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
