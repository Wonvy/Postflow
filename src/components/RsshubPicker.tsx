// 添加订阅里的 RSSHub：搜网站、选路由、把参数填成表单；粘贴网址时提示 RSSHub 能订阅的内容。
// 路由目录由核心下载和搜索（crates/core/src/rsshub.rs），拼订阅地址在 lib/rsshub.ts。

import { useQuery } from "@tanstack/react-query";
import { ArrowLeft, ChevronDown, Languages, Search, TriangleAlert, Zap } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api } from "../lib/api";
import { errorText, useLang, useT } from "../lib/i18n";
import { formatSize } from "../lib/platform";
import {
  buildFeedUrl,
  directUrl,
  initialValues,
  plainDescription,
  type RsshubParam,
  type RsshubRoute,
} from "../lib/rsshub";
import { useModelDownloads, useTranslateTarget } from "../lib/translate";
import { useRsshubSettings } from "./RsshubServer";
import { Button, cx, Favicon, Field, Select, Spinner, TextInput } from "./ui";

function siteIcon(route: RsshubRoute): string | null {
  return route.site_url ? `https://${route.site_url.replace(/^https?:\/\//, "").split("/")[0]}/favicon.ico` : null;
}

function Badges({ route }: { route: RsshubRoute }) {
  const t = useT();
  const tags = [
    route.require_config && t("rsshub.needsConfig"),
    route.require_puppeteer && t("rsshub.needsBrowser"),
    route.anti_crawler && t("rsshub.antiCrawler"),
  ].filter((x): x is string => Boolean(x));
  if (tags.length === 0) return null;
  return (
    <span className="flex shrink-0 gap-1">
      {tags.map((tag) => (
        <span key={tag} className="rounded bg-warning/15 px-1.5 py-px text-[11px] text-warning">
          {tag}
        </span>
      ))}
    </span>
  );
}

/**
 * 搜索框和结果。选中一条交给 `onPick`。搜索词由外面保管（`query`），
 * 选了路由再返回时还在，结果马上出来，不用重新输入。
 */
export function RsshubSearch({
  query,
  onQuery,
  onPick,
}: {
  query: string;
  onQuery: (query: string) => void;
  onPick: (route: RsshubRoute) => void;
}) {
  const t = useT();
  const lang = useLang();
  const setQuery = onQuery;
  const [debounced, setDebounced] = useState(() => query.trim());
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(query.trim()), 250);
    return () => clearTimeout(timer);
  }, [query]);
  const search = useQuery({
    queryKey: ["rsshub", "search", debounced, lang],
    queryFn: () => api.rsshubSearch(debounced, lang === "zh"),
    enabled: debounced.length > 0,
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  });

  return (
    <div className="flex flex-col gap-2">
      <div className="relative">
        <Search size={14} className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-fg-3" />
        <TextInput
          autoFocus
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={t("rsshub.searchPlaceholder")}
          className="pl-8 pr-8"
        />
        {search.isFetching && (
          <span className="absolute right-2.5 top-1/2 -translate-y-1/2 text-fg-3">
            <Spinner size={13} />
          </span>
        )}
      </div>
      {search.isError && (
        <p className="flex items-center gap-2 text-[12px] text-danger">
          <TriangleAlert size={13} className="shrink-0" />
          {errorText(t, search.error)}
        </p>
      )}
      {!debounced && <p className="text-[12px] leading-relaxed text-fg-3">{t("rsshub.searchHint")}</p>}
      {search.data && search.data.routes.length === 0 && (
        <p className="text-[12px] text-fg-3">{t("rsshub.noResults", { query: debounced })}</p>
      )}
      {search.data && search.data.routes.length > 0 && (
        <ul className="flex max-h-[300px] flex-col gap-px overflow-y-auto rounded-md border border-line p-1">
          {search.data.routes.map((r) => (
            <li key={`${r.namespace}${r.path}`}>
              <button
                type="button"
                onClick={() => onPick(r)}
                className="flex w-full items-center gap-2.5 rounded px-2 py-1.5 text-left hover:bg-hover"
              >
                <Favicon src={siteIcon(r)} size={16} />
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate text-fg">
                    {r.site} <span className="text-fg-3">·</span> {r.name}
                  </span>
                  <span className="truncate font-mono text-[11px] text-fg-3">
                    /{r.namespace}
                    {r.path}
                  </span>
                </span>
                <Badges route={r} />
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/**
 * RSSHub 的说明大多是英文（或中文）写的。和界面要的语言不一样时，用离线翻译翻一下：
 * 路由名、路由说明、每个参数的说明。没装对应的模型时返回要下载的模型，由界面问用户要不要下。
 */
function useRouteTranslation(route: RsshubRoute) {
  const target = useTranslateTarget();
  const texts = useMemo(
    () => [route.name, plainDescription(route.description), ...route.params.map((p) => plainDescription(p.description))],
    [route],
  );
  const sample = texts.filter(Boolean).join("\n").slice(0, 2000);
  const downloads = useModelDownloads((s) => s.byPair);
  const plan = useQuery({
    queryKey: ["rsshub", "plan", sample, target],
    queryFn: () => api.translatePlan(sample, target),
    enabled: sample.trim().length > 0,
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  });
  const missing = plan.data?.missing ?? [];
  const installing = missing.some((m) => downloads[m.pair]?.state === "downloading");
  const installed = missing.length > 0 && missing.every((m) => downloads[m.pair]?.state === "installed");
  // 模型刚装好：重新看一下能不能翻
  useEffect(() => {
    if (installed) void plan.refetch();
  }, [installed, plan.refetch]);
  const ready = plan.data && !plan.data.same && plan.data.from && missing.length === 0;
  const translated = useQuery({
    queryKey: ["rsshub", "translated", route.namespace, route.path, target],
    queryFn: async () => {
      const p = plan.data;
      if (!p?.from) return null;
      const { segments } = await api.translateText(
        p.from,
        p.to,
        texts.map((text) => ({ text, marks: [] })),
      );
      return segments.map((seg, i) => (texts[i] ? seg.text : ""));
    },
    enabled: Boolean(ready),
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  });
  return {
    /** 第 i 段的译文，没有就是原文。0 是路由名，1 是路由说明，之后是各参数的说明。 */
    text: (i: number) => translated.data?.[i] || texts[i] || "",
    translating: translated.isFetching,
    translatedNow: Boolean(translated.data),
    missing: plan.data?.same ? [] : missing,
    installing,
  };
}

/** 一条路由的参数表单。参数填好后把拼出来的订阅地址交给 `onUrl`（没填完是 null）。 */
export function RsshubForm({
  route,
  found,
  onBack,
  onUrl,
  onDirect,
}: {
  route: RsshubRoute;
  /** 从网址里取到的参数值。 */
  found?: Record<string, string>;
  onBack: () => void;
  onUrl: (url: string | null) => void;
  /** Postflow 自己就能订阅这个网站时，改用网页地址直接订阅。 */
  onDirect: (pageUrl: string) => void;
}) {
  const t = useT();
  const tr = useRouteTranslation(route);
  const { instance, key } = useRsshubSettings();
  const [values, setValues] = useState(() => initialValues(route, found));
  const [showAbout, setShowAbout] = useState(false);
  const url = useMemo(() => buildFeedUrl(instance, route, values, key), [instance, route, values, key]);
  useEffect(() => onUrl(url), [url, onUrl]);

  const set = (name: string, value: string) => setValues((v) => ({ ...v, [name]: value }));
  const about = tr.text(1);
  const direct = directUrl(route, values);

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-start gap-2">
        <button
          type="button"
          aria-label={t("rsshub.back")}
          title={t("rsshub.back")}
          onClick={onBack}
          className="mt-0.5 inline-flex size-6 shrink-0 items-center justify-center rounded text-fg-3 hover:bg-hover hover:text-fg"
        >
          <ArrowLeft size={14} />
        </button>
        <Favicon src={siteIcon(route)} size={16} />
        <div className="min-w-0 flex-1">
          <p className="truncate font-medium text-fg">
            {route.site} <span className="text-fg-3">·</span> {tr.text(0)}
          </p>
          {about && (
            <button
              type="button"
              onClick={() => setShowAbout((s) => !s)}
              className="mt-0.5 flex items-center gap-1 text-[12px] text-fg-3 hover:text-fg-2"
            >
              {t("rsshub.about")}
              <ChevronDown size={12} className={cx("transition-transform", showAbout && "rotate-180")} />
            </button>
          )}
        </div>
        <Badges route={route} />
      </div>
      {showAbout && (
        <p className="max-h-32 select-text overflow-y-auto whitespace-pre-line rounded-md bg-hover/60 px-3 py-2 text-[12px] leading-relaxed text-fg-2">
          {about}
        </p>
      )}

      {route.namespace === "youtube" && (
        <div className="flex items-center gap-3 rounded-md bg-accent-soft px-3 py-2 text-[12px] leading-relaxed text-fg-2">
          <span className="min-w-0 flex-1">{t("rsshub.youtubeDirect")}</span>
          <Button disabled={!direct} onClick={() => direct && onDirect(direct)}>
            {t("rsshub.subscribeDirect")}
          </Button>
        </div>
      )}

      {(tr.missing.length > 0 || tr.translating || tr.translatedNow) && (
        <p className="flex items-center gap-1.5 text-[11px] text-fg-3">
          <Languages size={12} className="shrink-0" />
          {tr.translating ? (
            t("rsshub.translating")
          ) : tr.translatedNow ? (
            t("rsshub.translated")
          ) : tr.installing ? (
            t("rsshub.installingModel")
          ) : (
            <>
              {t("rsshub.untranslated")}
              <button
                type="button"
                onClick={() => void api.downloadModels(tr.missing.map((m) => m.pair))}
                className="text-accent hover:underline"
              >
                {t("rsshub.installModel", { size: formatSize(tr.missing.reduce((n, m) => n + m.size, 0)) })}
              </button>
            </>
          )}
        </p>
      )}

      {route.params.length === 0 && <p className="text-[12px] text-fg-3">{t("rsshub.noParams")}</p>}
      {route.params.map((p, i) => (
        <ParamField
          key={p.name}
          param={p}
          hint={tr.text(i + 2)}
          value={values[p.name] ?? ""}
          onChange={(v) => set(p.name, v)}
        />
      ))}
    </div>
  );
}

function ParamField({
  param,
  hint,
  value,
  onChange,
}: {
  param: RsshubParam;
  /** 参数说明（可能是译文）。 */
  hint: string;
  value: string;
  onChange: (v: string) => void;
}) {
  const t = useT();
  const [expanded, setExpanded] = useState(false);
  const long = hint.length > 140 || hint.includes("\n");
  const label = param.optional ? `${param.name} · ${t("rsshub.optional")}` : param.name;
  return (
    <Field label={label}>
      {param.options.length > 0 ? (
        <Select value={value} onChange={(e) => onChange(e.target.value)}>
          {param.optional && <option value="">{t("rsshub.notSet")}</option>}
          {!param.optional && !param.options.some((o) => o.value === value) && <option value="">—</option>}
          {param.options.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label === o.value ? o.label : `${o.label} (${o.value})`}
            </option>
          ))}
        </Select>
      ) : (
        <TextInput
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={param.default ?? (param.optional ? t("rsshub.notSet") : "")}
          spellCheck={false}
        />
      )}
      {hint && (
        <p className={cx("mt-1 whitespace-pre-line text-[12px] leading-relaxed text-fg-3", !expanded && long && "line-clamp-2")}>
          {hint}
        </p>
      )}
      {long && (
        <button type="button" onClick={() => setExpanded((e) => !e)} className="mt-0.5 text-[12px] text-accent hover:underline">
          {expanded ? t("rsshub.less") : t("rsshub.more")}
        </button>
      )}
    </Field>
  );
}

/** 粘贴的是普通网页时，RSSHub 能订阅它的哪些内容。点一条进入表单。 */
export function RadarSuggestions({
  url,
  onPick,
}: {
  url: string;
  onPick: (route: RsshubRoute, values: Record<string, string>) => void;
}) {
  const t = useT();
  const lang = useLang();
  const radar = useQuery({
    queryKey: ["rsshub", "radar", url, lang],
    queryFn: () => api.rsshubRadar(url, lang === "zh"),
    enabled: /^https?:\/\/\S+\.\S+/i.test(url),
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  });
  const matches = radar.data?.matches ?? [];
  if (matches.length === 0) return null;
  return (
    <div>
      <p className="mb-1.5 flex items-center gap-1.5 text-[12px] font-medium text-fg-2">
        <Zap size={12} className="text-accent" />
        {t("rsshub.radarTitle")}
      </p>
      <ul className="flex flex-col gap-px rounded-md border border-line p-1">
        {matches.map((m) => (
          <li key={`${m.route.namespace}${m.route.path}`}>
            <button
              type="button"
              onClick={() => onPick(m.route, m.values)}
              className="flex w-full items-center gap-2.5 rounded px-2 py-1.5 text-left hover:bg-hover"
            >
              <Favicon src={siteIcon(m.route)} size={16} />
              <span className="min-w-0 flex-1 truncate text-fg">
                {m.route.site} <span className="text-fg-3">·</span> {m.route.name}
              </span>
              <span className="truncate text-[12px] text-fg-3">
                {m.route.params.flatMap((p) => (m.values[p.name] ? [m.values[p.name]] : [])).join(" / ")}
              </span>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
