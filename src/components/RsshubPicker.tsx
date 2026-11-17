// 添加订阅里的 RSSHub：搜网站、选路由、把参数填成表单；粘贴网址时提示 RSSHub 能订阅的内容。
// 路由目录由核心下载和搜索（crates/core/src/rsshub.rs），拼订阅地址在 lib/rsshub.ts。

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, ChevronDown, Search, TriangleAlert, Zap } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api } from "../lib/api";
import { errorText, useLang, useT } from "../lib/i18n";
import { keys, useSettings } from "../lib/queries";
import {
  buildFeedUrl,
  DEFAULT_INSTANCE,
  initialValues,
  normalizeInstance,
  plainDescription,
  type RsshubParam,
  type RsshubRoute,
  SETTING_KEY,
} from "../lib/rsshub";
import { cx, Favicon, Field, Select, Spinner, TextInput } from "./ui";

/** 设置里的 RSSHub 实例地址（会同步到其他设备）。 */
export function useRsshubInstance(): [string, (value: string) => Promise<void>] {
  const qc = useQueryClient();
  const settings = useSettings();
  const raw = settings.data?.[SETTING_KEY];
  const instance = normalizeInstance(typeof raw === "string" ? raw : "") ?? DEFAULT_INSTANCE;
  const save = async (value: string) => {
    await api.setSetting(SETTING_KEY, value.trim());
    await qc.invalidateQueries({ queryKey: keys.settings });
  };
  return [instance, save];
}

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

/** 搜索框和结果。选中一条交给 `onPick`。 */
export function RsshubSearch({ onPick }: { onPick: (route: RsshubRoute) => void }) {
  const t = useT();
  const lang = useLang();
  const [query, setQuery] = useState("");
  const [debounced, setDebounced] = useState("");
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

/** 一条路由的参数表单。参数填好后把拼出来的订阅地址交给 `onUrl`（没填完是 null）。 */
export function RsshubForm({
  route,
  found,
  onBack,
  onUrl,
}: {
  route: RsshubRoute;
  /** 从网址里取到的参数值。 */
  found?: Record<string, string>;
  onBack: () => void;
  onUrl: (url: string | null) => void;
}) {
  const t = useT();
  const [instance, saveInstance] = useRsshubInstance();
  const [values, setValues] = useState(() => initialValues(route, found));
  const [instanceDraft, setInstanceDraft] = useState<string | null>(null);
  const [showAbout, setShowAbout] = useState(false);
  const url = useMemo(() => buildFeedUrl(instance, route, values), [instance, route, values]);
  useEffect(() => onUrl(url), [url, onUrl]);

  const set = (name: string, value: string) => setValues((v) => ({ ...v, [name]: value }));
  const about = plainDescription(route.description);

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
            {route.site} <span className="text-fg-3">·</span> {route.name}
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

      {route.params.length === 0 && <p className="text-[12px] text-fg-3">{t("rsshub.noParams")}</p>}
      {route.params.map((p) => (
        <ParamField key={p.name} param={p} value={values[p.name] ?? ""} onChange={(v) => set(p.name, v)} />
      ))}

      <div className="rounded-md border border-line px-3 py-2">
        <div className="flex items-center gap-2 text-[12px] text-fg-3">
          <span className="shrink-0">{t("rsshub.instance")}</span>
          <input
            value={instanceDraft ?? instance}
            onChange={(e) => setInstanceDraft(e.target.value)}
            onBlur={() => {
              if (instanceDraft === null) return;
              const next = normalizeInstance(instanceDraft);
              setInstanceDraft(null);
              if (next && next !== instance) void saveInstance(next);
            }}
            onKeyDown={(e) => e.key === "Enter" && (e.preventDefault(), e.currentTarget.blur())}
            spellCheck={false}
            className="min-w-0 flex-1 rounded bg-transparent px-1 py-0.5 font-mono text-[12px] text-fg-2 outline-none hover:bg-hover focus:bg-hover"
          />
        </div>
        <p className="mt-1 break-all font-mono text-[11px] text-fg-2">{url ?? t("rsshub.fillRequired")}</p>
        {instance === DEFAULT_INSTANCE && (
          <p className="mt-1 text-[11px] leading-relaxed text-fg-3">{t("rsshub.publicHint")}</p>
        )}
      </div>
    </div>
  );
}

function ParamField({ param, value, onChange }: { param: RsshubParam; value: string; onChange: (v: string) => void }) {
  const t = useT();
  const hint = plainDescription(param.description);
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
