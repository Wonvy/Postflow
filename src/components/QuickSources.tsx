// 添加订阅里的「常用网站」：一排有官方订阅源的网站图标，点一个、填一个名字就能订阅，不需要 RSSHub。
// 网站列表和订阅地址的拼法在 lib/quickSources.ts。

import { ArrowLeft } from "lucide-react";
import {
  siArxiv,
  siBluesky,
  siGithub,
  siMastodon,
  siMedium,
  siReddit,
  siStackoverflow,
  siSubstack,
  siV2ex,
  siYcombinator,
  siYoutube,
  siTelegram,
  siGitlab,
  siCodeberg,
  siDevdotto,
  siLobsters,
  siLemmy,
  siDiscourse,
  siTumblr,
  siVimeo,
  siPinterest,
  siPypi,
  siSteam,
  siLetterboxd,
  type SimpleIcon,
} from "simple-icons";
import { useEffect, useMemo, useState } from "react";
import { type MessageKey, useT } from "../lib/i18n";
import { QUICK_SOURCES, type QuickSource, type QuickSourceId, quickInitial, quickSource } from "../lib/quickSources";
import { Field, Select, TextInput } from "./ui";

const ICONS: Record<string, SimpleIcon> = {
  siYoutube,
  siGithub,
  siReddit,
  siBluesky,
  siMastodon,
  siMedium,
  siSubstack,
  siStackoverflow,
  siYcombinator,
  siV2ex,
  siArxiv,
  siTelegram,
  siGitlab,
  siCodeberg,
  siDevdotto,
  siLobsters,
  siLemmy,
  siDiscourse,
  siTumblr,
  siVimeo,
  siPinterest,
  siPypi,
  siSteam,
  siLetterboxd,
};

/** 太深的品牌色在深色主题下看不见，换成正文颜色。 */
function iconColor(hex: string): string {
  const n = Number.parseInt(hex, 16);
  const lum = 0.299 * ((n >> 16) & 255) + 0.587 * ((n >> 8) & 255) + 0.114 * (n & 255);
  return lum < 60 ? "var(--fg)" : `#${hex}`;
}

function Brand({ source, size = 18 }: { source: QuickSource; size?: number }) {
  const icon = ICONS[source.icon];
  if (!icon) return null;
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden style={{ color: iconColor(icon.hex) }}>
      <path d={icon.path} fill="currentColor" />
    </svg>
  );
}

const NAME: Record<QuickSourceId, string> = {
  youtube: "YouTube",
  github: "GitHub",
  reddit: "Reddit",
  bluesky: "Bluesky",
  mastodon: "Mastodon",
  medium: "Medium",
  substack: "Substack",
  stackoverflow: "Stack Overflow",
  hackernews: "Hacker News",
  v2ex: "V2EX",
  arxiv: "arXiv",
  telegram: "Telegram",
  gitlab: "GitLab",
  codeberg: "Codeberg",
  devto: "DEV",
  lobsters: "Lobsters",
  lemmy: "Lemmy",
  discourse: "Discourse",
  tumblr: "Tumblr",
  vimeo: "Vimeo",
  pinterest: "Pinterest",
  pypi: "PyPI",
  steam: "Steam",
  letterboxd: "Letterboxd",
};

/** 图标网格。 */
export function QuickGrid({ onPick }: { onPick: (id: QuickSourceId) => void }) {
  const t = useT();
  return (
    <div>
      <p className="mb-1.5 text-[12px] font-medium text-fg-2">
        {t("quick.title")} <span className="font-normal text-fg-3">· {t("quick.hint")}</span>
      </p>
      <div className="grid grid-cols-4 gap-1 sm:grid-cols-6">
        {QUICK_SOURCES.map((s) => (
          <button
            key={s.id}
            type="button"
            onClick={() => onPick(s.id)}
            title={t(`quick.${s.id}.desc` as MessageKey)}
            className="flex flex-col items-center gap-1 rounded-md px-1 py-2 text-[11px] text-fg-2 transition-colors hover:bg-hover hover:text-fg"
          >
            <Brand source={s} size={20} />
            <span className="max-w-full truncate">{NAME[s.id]}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

/** 选中一个网站后的表单。拼出的地址交给 `onUrl`（没填好是 null）。 */
export function QuickForm({
  id,
  onBack,
  onUrl,
}: {
  id: QuickSourceId;
  onBack: () => void;
  onUrl: (url: string | null) => void;
}) {
  const t = useT();
  const source = quickSource(id);
  const [values, setValues] = useState(() => quickInitial(source));
  const url = useMemo(() => source.build(values), [source, values]);
  useEffect(() => onUrl(url), [url, onUrl]);
  const key = (suffix: string) => `quick.${id}.${suffix}` as MessageKey;

  return (
    <div className="flex flex-col gap-3 rounded-md border border-line p-3">
      <div className="flex items-start gap-2">
        <button
          type="button"
          aria-label={t("quick.back")}
          title={t("quick.back")}
          onClick={onBack}
          className="inline-flex size-6 shrink-0 items-center justify-center rounded text-fg-3 hover:bg-hover hover:text-fg"
        >
          <ArrowLeft size={14} />
        </button>
        <Brand source={source} />
        <div className="min-w-0 flex-1">
          <p className="font-medium text-fg">{NAME[id]}</p>
          <p className="text-[12px] leading-relaxed text-fg-3">{t(key("desc"))}</p>
        </div>
      </div>
      {source.fields.map((f, i) =>
        f.options ? (
          <Field key={f.name} label={t(key(f.name))}>
            <Select value={values[f.name] ?? ""} onChange={(e) => setValues((v) => ({ ...v, [f.name]: e.target.value }))}>
              {f.options.map((o) => (
                <option key={o} value={o}>
                  {t(key(`${f.name}.${o}`))}
                </option>
              ))}
            </Select>
          </Field>
        ) : (
          <Field key={f.name} label={f.optional ? `${t(key(f.name))} · ${t("rsshub.optional")}` : t(key(f.name))}>
            <TextInput
              autoFocus={i === 0}
              value={values[f.name] ?? ""}
              onChange={(e) => setValues((v) => ({ ...v, [f.name]: e.target.value }))}
              placeholder={t(key(`${f.name}.placeholder`))}
              spellCheck={false}
            />
          </Field>
        ),
      )}
    </div>
  );
}
