// 「添加订阅 → 发现」：浏览内置的官方订阅源目录，按分类、语言筛选，搜索，一键订阅。
// 新手引导里按分类一次订阅几个也用这里的订阅逻辑。目录数据和排序在 lib/catalog.ts。

import { useQueryClient } from "@tanstack/react-query";
import { ArrowRight, Check, LayoutGrid, List, Plus, Search } from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";
import { api } from "../lib/api";
import {
  feedKey,
  groupCounts,
  PODCAST_GROUPS,
  type PodcastGroup,
  browseCatalog,
  CATALOG_CATEGORIES,
  type CatalogCategory,
  type CatalogFeed,
  type CatalogSection,
  catalogLang,
  type LangFilter,
  sectionCounts,
  subscribedIds,
} from "../lib/catalog";
import { type CheckResult, checkCatalog, useCatalog } from "../lib/catalogRemote";
import { errorText, type MessageKey, type Translate, useLang, useT } from "../lib/i18n";
import { keys, useNav } from "../lib/queries";
import { useUi } from "../lib/store";
import { ApplePodcastResults } from "./PodcastSearch";
import { SettingRow } from "./SettingsParts";
import { toast } from "./Toast";
import { Avatar, Button, cx, IconButton, Select, Spinner, Switch } from "./ui";

/** 分类在界面上的名字，也是按分类订阅时建的文件夹名。 */
export function categoryName(t: Translate, category: CatalogCategory): string {
  return t(`catalog.cat.${category}` as MessageKey);
}

/** 目录源的图标：目录文件里带的图片，没有的显示首字母。 */
export function CatalogIcon({ feed, size = 28 }: { feed: CatalogFeed; size?: number }) {
  const [failed, setFailed] = useState(false);
  if (failed || !feed.icon) return <Avatar name={feed.name} seed={feed.id} size={size} />;
  return (
    <img
      src={feed.icon}
      alt=""
      width={size}
      height={size}
      draggable={false}
      onError={() => setFailed(true)}
      className="shrink-0 rounded-[6px] bg-white object-contain"
      style={{ width: size, height: size }}
    />
  );
}

/**
 * 订阅目录里的源：可以一次订阅好几个（两三个同时进行）。`folderOf` 决定每个源放进哪个文件夹。
 * 订阅过的、正在订阅的都记着，界面据此显示「已订阅」和转圈。
 */
export function useCatalogSubscribe(catalog: readonly CatalogFeed[]) {
  const t = useT();
  const qc = useQueryClient();
  const feeds = useNav().data?.feeds;
  const subscribed = useMemo(() => subscribedIds(feeds ?? [], catalog), [feeds, catalog]);
  const [adding, setAdding] = useState<Set<string>>(new Set());

  const subscribe = async (entries: CatalogFeed[], folderOf: (feed: CatalogFeed) => string) => {
    const todo = entries.filter((e) => !subscribed.has(e.id) && !adding.has(e.id));
    if (todo.length === 0) return;
    setAdding((s) => new Set([...s, ...todo.map((e) => e.id)]));
    const failed: string[] = [];
    let next = 0;
    await Promise.all(
      Array.from({ length: Math.min(3, todo.length) }, async () => {
        while (next < todo.length) {
          const entry = todo[next++];
          if (!entry) break;
          try {
            await api.addFeed(entry.url, folderOf(entry), { title: entry.name });
          } catch (err) {
            failed.push(entry.name);
            if (todo.length === 1) toast(`${entry.name}: ${errorText(t, err)}`);
          } finally {
            setAdding((s) => {
              const n = new Set(s);
              n.delete(entry.id);
              return n;
            });
            void qc.invalidateQueries({ queryKey: keys.nav });
          }
        }
      }),
    );
    void qc.invalidateQueries({ queryKey: ["items"] });
    if (failed.length > 0 && todo.length > 1) toast(t("catalog.someFailed", { names: failed.join("、") }));
  };

  return { subscribed, adding, subscribe };
}

/** 看起来像一个网址：带点、没有空格。 */
const looksLikeUrl = (text: string) => /^\S+\.\S+$/.test(text.trim());

export function DiscoverPane({
  startFolder,
  onUrl,
  onDone,
  podcasts = false,
}: {
  /** 从文件夹的菜单打开时，都放进这个文件夹。 */
  startFolder: string;
  /** 输入或粘贴的是网址：换到「网址」页订阅。 */
  onUrl: (url: string) => void;
  onDone: () => void;
  /**
   * 「添加订阅 → 播客」：只看目录里的播客，左边是「推荐」和播客的小类（新闻、科技……）；
   * 搜索时目录里的在前，后面接着是苹果播客目录里搜到的。
   */
  podcasts?: boolean;
}) {
  const t = useT();
  const lang = useLang();
  const prefer = catalogLang(lang);
  const [query, setQuery] = useState("");
  const [section, setSection] = useState<CatalogSection>("featured");
  // 播客再按小类看，「全部」是 null
  const [group, setGroup] = useState<PodcastGroup | null>(null);
  const [langFilter, setLangFilter] = useState<LangFilter>("auto");
  const [byCategory, setByCategory] = useState(true);
  const catalog = useCatalog();
  const { subscribed, adding, subscribe } = useCatalogSubscribe(catalog.feeds);
  const view = useUi((s) => s.catalogView);
  const setView = useUi((s) => s.setCatalogView);

  const search = query.trim();
  // 播客页只看目录里的播客
  const pool = useMemo(
    () => (podcasts ? catalog.feeds.filter((f) => f.category === "podcasts") : catalog.feeds),
    [podcasts, catalog.feeds],
  );
  const counts = sectionCounts(langFilter, pool);
  const groups = groupCounts(langFilter, pool);
  const shownGroups = PODCAST_GROUPS.filter((g) => (groups[g] ?? 0) > 0);
  // 左边那一列：发现页是「推荐」和各个分类，播客页是「推荐」和播客的小类
  const nav: { key: string; label: string; count: number }[] = podcasts
    ? [
        { key: "featured", label: t("catalog.featured"), count: counts.featured ?? 0 },
        ...shownGroups.map((g) => ({ key: g, label: t(`catalog.group.${g}`), count: groups[g] ?? 0 })),
      ].filter((n) => n.count > 0)
    : (["featured", ...CATALOG_CATEGORIES] as CatalogSection[])
        .filter((s) => (counts[s] ?? 0) > 0)
        .map((s) => ({
          key: s,
          label: s === "featured" ? t("catalog.featured") : categoryName(t, s),
          count: counts[s] ?? 0,
        }));
  const picked = podcasts ? (group ?? "featured") : section;
  const current = nav.some((n) => n.key === picked) ? picked : (nav[0]?.key ?? "featured");
  const showGroups = !podcasts && current === "podcasts" && !search && shownGroups.length > 1;
  const activeGroup = showGroups && group && shownGroups.includes(group) ? group : undefined;
  const list = podcasts
    ? browseCatalog(
        current === "featured" || search
          ? { section: search ? "podcasts" : "featured", prefer, langFilter, search }
          : { section: "podcasts", prefer, langFilter, group: current as PodcastGroup },
        pool,
      )
    : browseCatalog({ section: current as CatalogSection, prefer, langFilter, search, group: activeGroup }, pool);
  const left = list.filter((f) => !subscribed.has(f.id) && !adding.has(f.id));
  const folderOf = (feed: CatalogFeed) => startFolder || (byCategory ? categoryName(t, feed.category) : "");
  const desc = (feed: CatalogFeed) => (prefer === "zh" ? feed.desc.zh : feed.desc.en);
  const langBadge = (feed: CatalogFeed) => (
    <span className="shrink-0 rounded border border-line px-1 text-[10px] leading-[14px] text-fg-3">
      {feed.lang === "zh" ? t("catalog.langZh") : "EN"}
    </span>
  );

  return (
    <div className="flex h-[min(540px,calc(76vh-96px))] flex-col max-md:h-[calc(100dvh-140px)]">
      <div className="flex shrink-0 flex-wrap items-center gap-2 px-4 pb-3">
        <div className="relative min-w-[200px] flex-1">
          <Search size={14} strokeWidth={1.75} className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-fg-3" />
          <input
            data-autofocus=""
            // biome-ignore lint/a11y/noAutofocus: 打开对话框就能直接搜
            autoFocus
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onPaste={(e) => {
              // 粘贴进来的是网址：直接去「网址」页读它
              const text = e.clipboardData.getData("text").trim();
              if (!looksLikeUrl(text) || !/[/:]/.test(text)) return;
              e.preventDefault();
              onUrl(text);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && looksLikeUrl(search) && list.length === 0) {
                e.preventDefault();
                onUrl(search);
              }
            }}
            placeholder={podcasts ? t("podcast.searchPlaceholder") : t("catalog.search")}
            aria-label={podcasts ? t("podcast.searchPlaceholder") : t("catalog.search")}
            autoComplete="off"
            spellCheck={false}
            className="h-9 w-full rounded-md border border-line-strong bg-bg pl-8 pr-2.5 text-[13px] text-fg outline-none placeholder:text-fg-3 focus:border-accent focus:ring-1 focus:ring-accent"
          />
        </div>
        <div className="w-[136px] shrink-0">
          <Select
            aria-label={t("catalog.language")}
            value={langFilter}
            onChange={(e) => setLangFilter(e.target.value as LangFilter)}
            className="h-9"
          >
            <option value="auto">{t("catalog.langAll")}</option>
            <option value="zh">{t("catalog.langZh")}</option>
            <option value="en">{t("catalog.langEn")}</option>
          </Select>
        </div>
        <div className="flex shrink-0 items-center">
          <IconButton label={t("catalog.viewGrid")} active={view === "grid"} onClick={() => setView("grid")}>
            <LayoutGrid size={15} strokeWidth={1.75} />
          </IconButton>
          <IconButton label={t("catalog.viewList")} active={view === "list"} onClick={() => setView("list")}>
            <List size={15} strokeWidth={1.75} />
          </IconButton>
        </div>
      </div>

      <div className="flex min-h-0 flex-1 border-t border-line max-md:flex-col">
        {/* 分类；手机上是一排，左右滑动 */}
        <nav
          aria-label={t("catalog.categories")}
          className="w-[152px] shrink-0 overflow-y-auto border-r border-line p-1.5 no-scrollbar max-md:flex max-md:w-auto max-md:gap-1 max-md:overflow-x-auto max-md:overflow-y-hidden max-md:border-b max-md:border-r-0"
        >
          {nav.map((n) => (
            <button
              key={n.key}
              type="button"
              aria-current={!search && n.key === current ? "true" : undefined}
              onClick={() => {
                if (podcasts) {
                  setGroup(n.key === "featured" ? null : (n.key as PodcastGroup));
                } else {
                  setSection(n.key as CatalogSection);
                  setGroup(null);
                }
                setQuery("");
              }}
              className={cx(
                "flex h-7 w-full shrink-0 items-center justify-between gap-2 rounded-md px-2 text-left text-[13px] transition-colors max-md:w-auto touch:h-10",
                !search && n.key === current ? "bg-active text-fg" : "text-fg-2 hover:bg-hover hover:text-fg",
              )}
            >
              <span className="truncate">{n.label}</span>
              <span className="tabular shrink-0 text-[11px] text-fg-3">{n.count}</span>
            </button>
          ))}
        </nav>

        {/* 源 */}
        <div className="min-w-0 flex-1 overflow-y-auto p-1.5">
          {/* 播客的小类：新闻、科技、商业…… */}
          {showGroups && (
            <div role="group" aria-label={t("catalog.podcastGroups")} className="mb-1.5 flex flex-wrap gap-1 px-0.5">
              {[null, ...shownGroups].map((g) => (
                <button
                  key={g ?? "all"}
                  type="button"
                  aria-pressed={(activeGroup ?? null) === g}
                  onClick={() => setGroup(g)}
                  className={cx(
                    "h-6 rounded-full border px-2.5 text-[12px] transition-colors touch:h-9 touch:px-3",
                    (activeGroup ?? null) === g
                      ? "border-accent bg-accent-soft text-accent"
                      : "border-line-strong text-fg-2 hover:bg-hover hover:text-fg",
                  )}
                >
                  {g ? t(`catalog.group.${g}`) : t("feedKind.all")}
                  {g && <span className="tabular ml-1 text-fg-3">{groups[g]}</span>}
                </button>
              ))}
            </div>
          )}
          {looksLikeUrl(search) && (
            <button
              type="button"
              onClick={() => onUrl(search)}
              className="mb-1 flex w-full items-center gap-2 rounded-md border border-dashed border-line-strong px-2.5 py-2 text-left text-[13px] text-fg-2 hover:bg-hover hover:text-fg"
            >
              <span className="min-w-0 flex-1 truncate">{t("catalog.useUrl", { url: search })}</span>
              <ArrowRight size={14} className="shrink-0" />
            </button>
          )}
          {catalog.loading ? (
            <div className="flex justify-center py-10 text-fg-3">
              <Spinner size={16} />
            </div>
          ) : list.length === 0 ? (
            // 播客页搜索时，目录里没有的还会去苹果播客目录里找，不用先说「没有」
            podcasts && search ? null : (
              <p className="px-3 py-8 text-center text-[13px] leading-relaxed text-fg-3">{t("catalog.empty")}</p>
            )
          ) : view === "grid" ? (
            <ul className="grid grid-cols-[repeat(auto-fill,minmax(140px,1fr))] gap-1.5 p-0.5">
              {list.map((feed) => (
                <li key={feed.id}>
                  <GridCard
                    feed={feed}
                    desc={desc(feed)}
                    badge={langFilter === "auto" && feed.lang !== prefer ? langBadge(feed) : null}
                    done={subscribed.has(feed.id)}
                    busy={adding.has(feed.id)}
                    onSubscribe={() => void subscribe([feed], folderOf)}
                  />
                </li>
              ))}
            </ul>
          ) : (
            <ul className="flex flex-col gap-px">
              {list.map((feed) => {
                const done = subscribed.has(feed.id);
                const busy = adding.has(feed.id);
                return (
                  <li key={feed.id} className="group flex items-center gap-3 rounded-md px-2 py-2 hover:bg-hover">
                    <CatalogIcon feed={feed} size={32} />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-1.5">
                        <span className="truncate text-[13px] font-medium text-fg" title={feed.site}>
                          {feed.name}
                        </span>
                        {langFilter === "auto" && feed.lang !== prefer && langBadge(feed)}
                        {search && (
                          <span className="shrink-0 text-[11px] text-fg-3">{categoryName(t, feed.category)}</span>
                        )}
                      </div>
                      <p className="truncate text-[12px] text-fg-3">{desc(feed)}</p>
                    </div>
                    <SubscribeButton done={done} busy={busy} onClick={() => void subscribe([feed], folderOf)} />
                  </li>
                );
              })}
            </ul>
          )}
          {podcasts && search && !looksLikeUrl(search) && (
            <ApplePodcastResults
              search={search}
              folder={startFolder || (byCategory ? categoryName(t, "podcasts") : "")}
              listed={new Set(pool.map((f) => feedKey(f.url)))}
            />
          )}
        </div>
      </div>

      <div className="flex shrink-0 items-center gap-3 border-t border-line px-4 py-3">
        {startFolder ? (
          <span className="min-w-0 truncate text-[12px] text-fg-3">{t("catalog.intoFolder", { folder: startFolder })}</span>
        ) : (
          <label className="flex min-w-0 items-center gap-2 text-[12px] text-fg-2">
            <Switch checked={byCategory} onChange={setByCategory} label={t("catalog.byCategory")} />
            <span className="truncate">{t("catalog.byCategory")}</span>
          </label>
        )}
        <div className="ml-auto flex shrink-0 items-center gap-2">
          {!search && left.length > 1 && (
            <Button onClick={() => void subscribe(left, folderOf)}>
              {t("catalog.subscribeAll", { n: left.length })}
            </Button>
          )}
          <Button variant="primary" onClick={onDone}>
            {t("catalog.done")}
          </Button>
        </div>
      </div>
    </div>
  );
}

/** 「订阅」按钮，订阅过的换成「已订阅」。 */
function SubscribeButton({
  done,
  busy,
  onClick,
  wide = false,
}: {
  done: boolean;
  busy: boolean;
  onClick: () => void;
  wide?: boolean;
}) {
  const t = useT();
  if (done) {
    return (
      <span
        className={cx(
          "flex h-7 shrink-0 items-center justify-center gap-1 px-2 text-[12px] text-fg-3",
          wide && "w-full",
        )}
      >
        <Check size={13} strokeWidth={2} />
        {t("catalog.subscribed")}
      </span>
    );
  }
  return (
    <Button className={cx("h-7 shrink-0 px-2.5", wide && "w-full justify-center")} disabled={busy} onClick={onClick}>
      {busy ? <Spinner size={12} /> : <Plus size={13} strokeWidth={2} />}
      {t("catalog.subscribe")}
    </Button>
  );
}

/** 图标视图里的一张卡片：大图标、名字、两行介绍，下面是订阅按钮。 */
function GridCard({
  feed,
  desc,
  badge,
  done,
  busy,
  onSubscribe,
}: {
  feed: CatalogFeed;
  desc: string;
  badge: ReactNode;
  done: boolean;
  busy: boolean;
  onSubscribe: () => void;
}) {
  return (
    <div className="flex h-full flex-col items-center gap-1.5 rounded-lg border border-line px-2.5 pb-2.5 pt-3 text-center transition-colors hover:bg-hover">
      <CatalogIcon feed={feed} size={40} />
      <div className="flex w-full min-w-0 items-center justify-center gap-1">
        <span className="truncate text-[13px] font-medium text-fg" title={feed.site}>
          {feed.name}
        </span>
        {badge}
      </div>
      <p className="line-clamp-2 min-h-[32px] w-full text-[12px] leading-4 text-fg-3">{desc}</p>
      <div className="mt-auto w-full pt-0.5">
        <SubscribeButton done={done} busy={busy} onClick={onSubscribe} wide />
      </div>
    </div>
  );
}

/** 设置 → 订阅：推荐订阅源目录的在线更新开关，现在用的版本，以及「立即检查」。 */
export function CatalogOnlineSetting() {
  const t = useT();
  const online = useUi((s) => s.catalogOnline);
  const setOnline = useUi((s) => s.setCatalogOnline);
  const catalog = useCatalog();
  const [result, setResult] = useState<CheckResult | null>(null);
  const status =
    result === "updated"
      ? t("catalog.checkUpdated")
      : result === "current"
        ? t("catalog.checkCurrent")
        : result === "failed"
          ? t("catalog.checkFailed")
          : null;
  return (
    <SettingRow
      label={t("catalog.online")}
      hint={
        <>
          {t("catalog.onlineHint")}
          {catalog.updated && (
            <span className="mt-1 block">
              {t("catalog.version", { date: catalog.updated, n: catalog.feeds.length })}
              {status && ` · ${status}`}
            </span>
          )}
        </>
      }
    >
      {online && (
        <Button
          busy={catalog.checking}
          onClick={() => void checkCatalog(catalog.bundled).then(setResult)}
        >
          {t("catalog.checkNow")}
        </Button>
      )}
      <Switch label={t("catalog.online")} checked={online} onChange={setOnline} />
    </SettingRow>
  );
}
