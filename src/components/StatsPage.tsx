// 订阅统计：每个订阅源更新得多勤、内容完不完整，以及我读了多少、读了多久。
//
// 阅读记录来自阅读区（见 lib/reading.ts），只存在本机。

import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { ArrowDown, ArrowUp, ChartColumn, TriangleAlert, X } from "lucide-react";
import { type ReactNode, useEffect, useMemo, useState } from "react";
import { api, type FeedStats, type ReadingDay, type Stats } from "../lib/api";
import { useLang, useT } from "../lib/i18n";
import { type Flag, flagsOf, hoursMinutes, niceMax, perWeek, ratio, type SortKey, sortFeeds } from "../lib/stats";
import { useUi } from "../lib/store";
import { type Lang, shortTime } from "../lib/time";
import { Button, cx, Favicon, ICON, IconButton, Spinner } from "./ui";

const PERIODS = [7, 30, 90] as const;
const DAY = 86_400_000;

type Translate = ReturnType<typeof useT>;

function percent(p: number | null): string {
  return p === null ? "—" : `${Math.round(p * 100)}%`;
}

function duration(t: Translate, ms: number): string {
  if (ms > 0 && ms < 60_000) return t("stats.lessThanMinute");
  const { h, m } = hoursMinutes(ms);
  return h > 0 ? t("stats.hours", { h, m }) : t("stats.minutes", { n: m });
}

/** 后端按本地时区算的「第几天」换成月和日。 */
function dayLabel(day: number, lang: Lang): string {
  return new Intl.DateTimeFormat(lang === "zh" ? "zh-CN" : "en-US", {
    month: "numeric",
    day: "numeric",
    timeZone: "UTC",
  }).format(new Date(day * DAY));
}

export function StatsPage() {
  const t = useT();
  const lang = useLang();
  const setMainView = useUi((s) => s.setMainView);
  const [days, setDays] = useState<number>(30);
  const query = useQuery({
    queryKey: ["stats", days],
    queryFn: () => api.feedStats(days),
    placeholderData: keepPreviousData,
  });
  const stats = query.data;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !document.querySelector("[role=dialog]")) setMainView("list");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setMainView]);

  return (
    <section className="flex min-w-0 flex-1 flex-col bg-bg">
      <header data-tauri-drag-region className="flex h-11 shrink-0 items-center gap-2 border-b border-line pl-4 pr-2">
        <ChartColumn size={15} strokeWidth={1.75} className="text-fg-3" />
        <h1 data-tauri-drag-region className="min-w-0 flex-1 truncate text-[13px] font-semibold">
          {t("stats.title")}
        </h1>
        <IconButton label={t("action.close")} onClick={() => setMainView("list")}>
          <X {...ICON} />
        </IconButton>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {!stats ? (
          <div className="flex h-full items-center justify-center">
            <Spinner />
          </div>
        ) : (
          <div
            className={cx(
              "mx-auto flex max-w-[1080px] flex-col gap-5 px-6 pb-12 pt-5 transition-opacity",
              query.isPlaceholderData && "opacity-60",
            )}
          >
            <div className="flex flex-wrap items-center gap-3">
              <div role="radiogroup" aria-label={t("stats.title")} className="flex rounded-md bg-hover p-0.5">
                {PERIODS.map((n) => (
                  <button
                    key={n}
                    type="button"
                    role="radio"
                    aria-checked={days === n}
                    onClick={() => setDays(n)}
                    className={cx(
                      "h-6 rounded-[5px] px-2.5 text-[12px] transition-colors",
                      days === n ? "bg-surface font-medium text-fg shadow-sm" : "text-fg-3 hover:text-fg-2",
                    )}
                  >
                    {t("stats.days", { n })}
                  </button>
                ))}
              </div>
              <p className="min-w-0 flex-1 text-[12px] text-fg-3">
                {stats.tracking_since === null
                  ? t("stats.trackingNone")
                  : t("stats.tracking", { date: new Date(stats.tracking_since).toLocaleDateString(lang === "zh" ? "zh-CN" : "en-US") })}
              </p>
              {stats.tracking_since !== null && <ClearReading onCleared={() => void query.refetch()} />}
            </div>

            <Tiles stats={stats} />

            <Card title={t("stats.daily")}>
              <DailyChart days={stats.days} />
            </Card>

            <Card title={t("stats.byFeed")}>
              {stats.feeds.length === 0 ? (
                <p className="py-6 text-center text-[13px] text-fg-3">{t("stats.empty")}</p>
              ) : (
                <FeedTable stats={stats} days={days} />
              )}
            </Card>
          </div>
        )}
      </div>
    </section>
  );
}

function ClearReading({ onCleared }: { onCleared: () => void }) {
  const t = useT();
  const [armed, setArmed] = useState(false);
  useEffect(() => {
    if (!armed) return;
    const timer = window.setTimeout(() => setArmed(false), 4000);
    return () => window.clearTimeout(timer);
  }, [armed]);
  return (
    <Button
      variant={armed ? "danger" : "ghost"}
      onClick={() => {
        if (!armed) return setArmed(true);
        setArmed(false);
        void api.clearReading().then(onCleared);
      }}
    >
      {armed ? t("stats.clearConfirm") : t("stats.clear")}
    </Button>
  );
}

function Card({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="rounded-lg border border-line bg-surface">
      <h2 className="px-4 pb-1 pt-3 text-[13px] font-semibold">{title}</h2>
      <div className="px-4 pb-4 pt-2">{children}</div>
    </section>
  );
}

function Tiles({ stats }: { stats: Stats }) {
  const t = useT();
  const sum = (pick: (f: FeedStats) => number) => stats.feeds.reduce((n, f) => n + pick(f), 0);
  const opened = sum((f) => f.opened);
  const finished = sum((f) => f.finished);
  const tiles: { label: string; value: string; note?: string }[] = [
    { label: t("stats.tileFeeds"), value: stats.feeds.length.toLocaleString() },
    { label: t("stats.tilePublished"), value: sum((f) => f.published).toLocaleString() },
    { label: t("stats.tileOpened"), value: opened.toLocaleString() },
    {
      label: t("stats.tileFinished"),
      value: finished.toLocaleString(),
      note: opened > 0 ? t("stats.finishedShare", { p: percent(ratio(finished, opened)) }) : undefined,
    },
    { label: t("stats.tileTime"), value: duration(t, sum((f) => f.dwell_ms)) },
  ];
  return (
    <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-5">
      {tiles.map((tile) => (
        <div key={tile.label} className="rounded-lg border border-line bg-surface px-4 py-3">
          <div className="text-[12px] text-fg-3">{tile.label}</div>
          <div className="tabular mt-1 text-[20px] font-semibold leading-tight">{tile.value}</div>
          {tile.note && <div className="mt-0.5 text-[12px] text-fg-3">{tile.note}</div>}
        </div>
      ))}
    </div>
  );
}

/** 每天的阅读时长：一天一根柱子。悬停看那天读了几篇、来了几篇新文章。 */
function DailyChart({ days }: { days: ReadingDay[] }) {
  const t = useT();
  const lang = useLang();
  const [hover, setHover] = useState<number | null>(null);
  const minutes = days.map((d) => d.dwell_ms / 60_000);
  const top = niceMax(Math.max(10, ...minutes));
  const ticks = [top, top / 2, 0];
  const n = days.length;
  const thin = n > 45;
  const hovered = hover === null ? null : days[hover];
  const labelDays = n > 0 ? [0, Math.floor((n - 1) / 2), n - 1] : [];

  return (
    <div>
      <div className="relative h-[168px] pl-9" onPointerLeave={() => setHover(null)}>
        {ticks.map((v) => (
          <div
            key={v}
            className="pointer-events-none absolute left-9 right-0 border-t border-line"
            style={{ top: `${(1 - v / top) * 100}%` }}
          >
            <span className="tabular absolute -left-9 w-7 -translate-y-1/2 text-right text-[11px] text-fg-3">{v}</span>
          </div>
        ))}
        <div className="absolute inset-y-0 left-9 right-0 flex items-end gap-[2px]" aria-hidden>
          {days.map((d, i) => (
            <div
              key={d.day}
              onPointerEnter={() => setHover(i)}
              className="flex h-full min-w-0 flex-1 items-end justify-center"
            >
              <div
                className={cx(
                  "w-full max-w-[24px] bg-accent transition-opacity",
                  thin ? "rounded-t-[2px]" : "rounded-t-[4px]",
                  hover !== null && hover !== i && "opacity-50",
                )}
                style={{ height: `${(d.dwell_ms / 60_000 / top) * 100}%`, minHeight: d.dwell_ms > 0 ? 2 : 0 }}
              />
            </div>
          ))}
        </div>
        {hovered && hover !== null && (
          <div
            className={cx(
              "pointer-events-none absolute top-0 z-10 rounded-md border border-line bg-surface px-2.5 py-1.5 text-[12px] shadow-pop",
              hover < n * 0.2 ? "translate-x-0" : hover > n * 0.8 ? "-translate-x-full" : "-translate-x-1/2",
            )}
            style={{ left: `calc(2.25rem + (100% - 2.25rem) * ${(hover + 0.5) / n})` }}
          >
            <div className="tabular font-semibold text-fg">{duration(t, hovered.dwell_ms)}</div>
            <div className="whitespace-nowrap text-fg-3">
              {dayLabel(hovered.day, lang)} · {t("stats.dailyTip", { n: hovered.opened, m: hovered.published })}
            </div>
          </div>
        )}
      </div>
      <div className="relative ml-9 mt-1.5 h-4 text-[11px] text-fg-3">
        {labelDays.map((i, k) => (
          <span
            key={i}
            className={cx(
              "absolute",
              k === 0 ? "left-0" : k === labelDays.length - 1 ? "right-0" : "-translate-x-1/2",
            )}
            style={k === 1 ? { left: `${((i + 0.5) / n) * 100}%` } : undefined}
          >
            {dayLabel(days[i]?.day ?? 0, lang)}
          </span>
        ))}
      </div>
      {/* 读屏用的表格 */}
      <table className="sr-only">
        <caption>{t("stats.daily")}</caption>
        <tbody>
          {days.map((d) => (
            <tr key={d.day}>
              <th scope="row">{dayLabel(d.day, lang)}</th>
              <td>{duration(t, d.dwell_ms)}</td>
              <td>{t("stats.dailyTip", { n: d.opened, m: d.published })}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

const FLAG_LABEL: Record<Flag, "stats.flagError" | "stats.flagStale" | "stats.flagSummary" | "stats.flagUnread"> = {
  error: "stats.flagError",
  stale: "stats.flagStale",
  summary: "stats.flagSummary",
  unread: "stats.flagUnread",
};

function FeedTable({ stats, days }: { stats: Stats; days: number }) {
  const t = useT();
  const lang = useLang();
  const setScope = useUi((s) => s.setScope);
  const [sort, setSort] = useState<{ key: SortKey; desc: boolean }>({ key: "dwell", desc: true });
  const rows = useMemo(() => sortFeeds(stats.feeds, sort.key, sort.desc), [stats.feeds, sort]);

  const Th = ({ k, label, help, align = "right" }: { k: SortKey; label: string; help?: string; align?: "left" | "right" }) => {
    const on = sort.key === k;
    return (
      <th
        scope="col"
        aria-sort={on ? (sort.desc ? "descending" : "ascending") : "none"}
        className={cx("px-2 py-2 font-normal", align === "right" ? "text-right" : "text-left")}
      >
        <button
          type="button"
          title={help}
          onClick={() => setSort(on ? { key: k, desc: !sort.desc } : { key: k, desc: k !== "title" })}
          className={cx("inline-flex items-center gap-0.5 hover:text-fg", on && "text-fg")}
        >
          {label}
          {on && (sort.desc ? <ArrowDown size={11} /> : <ArrowUp size={11} />)}
        </button>
      </th>
    );
  };

  return (
    <div className="-mx-2 overflow-x-auto">
      <table className="w-full min-w-[760px] border-collapse text-[13px]">
        <thead>
          <tr className="border-b border-line text-[12px] text-fg-3">
            <Th k="title" label={t("stats.colFeed")} align="left" />
            <Th k="rate" label={t("stats.colRate")} align="left" />
            <Th k="last" label={t("stats.colLast")} />
            <Th k="full" label={t("stats.colFull")} help={t("stats.helpFull")} />
            <Th k="opened" label={t("stats.colOpened")} help={t("stats.helpOpened")} />
            <Th k="finished" label={t("stats.colFinished")} help={t("stats.helpFinished")} />
            <Th k="dwell" label={t("stats.colTime")} help={t("stats.helpTime")} />
          </tr>
        </thead>
        <tbody>
          {rows.map((f) => {
            const flags = flagsOf(f, stats);
            const rate = perWeek(f.published, days);
            return (
              <tr key={f.id} className="border-b border-line last:border-b-0 hover:bg-hover">
                <td className="max-w-[280px] px-2 py-2 align-top">
                  <button
                    type="button"
                    onClick={() => setScope({ kind: "feed", feedId: f.id })}
                    className="flex min-w-0 max-w-full items-center gap-2 text-left hover:underline"
                  >
                    <Favicon src={f.icon_url} />
                    <span className="truncate font-medium">{f.title || f.url}</span>
                  </button>
                  {flags.length > 0 && (
                    <div className="mt-1 flex flex-wrap gap-1 pl-6">
                      {flags.map((flag) => (
                        <span
                          key={flag}
                          className={cx(
                            "inline-flex items-center gap-1 rounded px-1.5 py-px text-[11px]",
                            flag === "error" ? "bg-hover text-warning" : "bg-hover text-fg-2",
                          )}
                          title={flag === "error" ? (f.error ?? undefined) : undefined}
                        >
                          {flag === "error" && <TriangleAlert size={11} strokeWidth={2} />}
                          {t(FLAG_LABEL[flag])}
                        </span>
                      ))}
                    </div>
                  )}
                </td>
                <td className="px-2 py-2 align-top">
                  <div className="flex items-center gap-2">
                    <Sparkline weekly={f.weekly} />
                    <span className="tabular whitespace-nowrap text-fg-2">
                      {f.published === 0
                        ? t("stats.noPosts")
                        : t("stats.perWeek", { n: rate < 10 ? rate.toFixed(1) : Math.round(rate) })}
                    </span>
                  </div>
                </td>
                <td className="tabular whitespace-nowrap px-2 py-2 text-right align-top text-fg-2">
                  {f.last_published === null ? "—" : shortTime(f.last_published, lang, stats.now)}
                </td>
                <td className="px-2 py-2 align-top">
                  <Meter value={f.full_share} />
                </td>
                <td className="px-2 py-2 align-top">
                  <Meter value={ratio(f.opened, f.published)} text={f.published > 0 ? `${f.opened}/${f.published}` : "—"} />
                </td>
                <td className="tabular whitespace-nowrap px-2 py-2 text-right align-top text-fg-2">
                  {percent(ratio(f.finished, f.opened))}
                </td>
                <td className="tabular whitespace-nowrap px-2 py-2 text-right align-top">
                  {f.dwell_ms > 0 ? duration(t, f.dwell_ms) : <span className="text-fg-4">—</span>}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

/** 每周发了几篇的小柱图。最近一周用强调色。 */
function Sparkline({ weekly }: { weekly: number[] }) {
  const t = useT();
  const max = Math.max(1, ...weekly);
  return (
    <div
      role="img"
      aria-label={`${t("stats.weekly", { n: weekly.length })}: ${weekly.join(", ")}`}
      title={weekly.join(" · ")}
      className="flex h-5 shrink-0 items-end gap-[2px]"
    >
      {weekly.map((n, i) => (
        <div
          key={i}
          className={cx(
            "w-[4px] rounded-t-[1px]",
            n === 0 ? "bg-line-strong" : i === weekly.length - 1 ? "bg-accent" : "bg-fg-4",
          )}
          style={{ height: n === 0 ? 1 : `${(n / max) * 100}%`, minHeight: n > 0 ? 2 : 0 }}
        />
      ))}
    </div>
  );
}

/** 一个比例：数字加一条细条。底色是同一个颜色的浅色。 */
function Meter({ value, text }: { value: number | null; text?: string }) {
  return (
    <div className="flex items-center justify-end gap-2">
      <span className="tabular whitespace-nowrap text-fg-2">{text ?? percent(value)}</span>
      <div className="h-1.5 w-12 shrink-0 overflow-hidden rounded-full bg-accent-soft">
        {value !== null && <div className="h-full rounded-full bg-accent" style={{ width: `${value * 100}%` }} />}
      </div>
    </div>
  );
}
