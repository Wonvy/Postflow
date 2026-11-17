// 设置里的「存储空间」：数据目录和本机目录各占多少，哪些订阅源、邮箱占得最多，以及清理本机缓存。
//
// 数据目录是同步的事实来源，只显示不清理。能清的都是本机缓存，清掉后用到时会重新下载。
// 每一行右边是大小，下面一条细条按同一组里最大的那项画长短，悬停看精确字节数。

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { FolderOpen } from "lucide-react";
import { useState } from "react";
import { api, type StoragePart } from "../lib/api";
import { errorText, type MessageKey, useT } from "../lib/i18n";
import { formatSize, revealFolder } from "../lib/platform";
import { useNav } from "../lib/queries";
import { useUi } from "../lib/store";
import { isTauri } from "../lib/transport";
import { SettingRow } from "./SettingsParts";
import { SourceIcon } from "./SourceIcon";
import { toast } from "./Toast";
import { Button, IconButton, Spinner } from "./ui";

const PART_LABEL: Record<StoragePart["key"], MessageKey> = {
  articles: "storage.articles",
  oplog: "storage.oplog",
  index: "storage.index",
  translation: "storage.translation",
  logs: "storage.logs",
  icons: "storage.icons",
  rsshub: "storage.rsshub",
  other: "storage.other",
};

/** 占用最多的来源列多少个。 */
const TOP_SOURCES = 10;

export function StorageSettings({ open }: { open: boolean }) {
  const t = useT();
  const qc = useQueryClient();
  const nav = useNav();
  const setDialog = useUi((s) => s.setDialog);
  const usage = useQuery({
    queryKey: ["storage"],
    queryFn: api.storageUsage,
    enabled: open,
    staleTime: 0,
  });
  const [busy, setBusy] = useState<string | null>(null);

  const run = async (key: string, action: () => Promise<string>) => {
    setBusy(key);
    try {
      toast(await action());
      await qc.invalidateQueries({ queryKey: ["storage"] });
    } catch (err) {
      toast(errorText(t, err));
    } finally {
      setBusy(null);
    }
  };

  if (usage.isPending) {
    return (
      <div className="flex justify-center py-10 text-fg-3">
        <Spinner />
      </div>
    );
  }
  if (usage.isError) return <p className="py-6 text-[13px] text-danger">{errorText(t, usage.error)}</p>;

  const u = usage.data;
  const sourceName = (id: string, kind: string) =>
    kind === "mail"
      ? nav.data?.accounts.find((a) => a.id === id)?.name || nav.data?.accounts.find((a) => a.id === id)?.email
      : nav.data?.feeds.find((f) => f.id === id)?.title;
  const sources = u.index.sources.filter((s) => s.bytes > 0).slice(0, TOP_SOURCES);
  const sourceMax = sources[0]?.bytes ?? 0;
  const translation = u.local.parts.find((p) => p.key === "translation")?.bytes ?? 0;
  const indexFile = u.local.parts.find((p) => p.key === "index")?.bytes ?? 0;

  return (
    <>
      <Group
        title={t("storage.data")}
        hint={t("storage.dataHint")}
        path={u.data_dir}
        total={u.data.total}
        parts={u.data.parts.map((p) => ({ label: t(PART_LABEL[p.key]), bytes: p.bytes }))}
      />
      <Group
        title={t("storage.local")}
        hint={t("storage.localHint")}
        path={u.local_dir}
        total={u.local.total}
        parts={[
          { label: t("storage.mailCache"), bytes: u.index.mail },
          { label: t("storage.articleText"), bytes: u.index.articles },
          { label: t("storage.fulltextCache"), bytes: u.index.fulltext },
          // 索引文件里除了上面三样，剩下的是搜索索引、条目列表和数据库自己的结构
          { label: t("storage.indexOther"), bytes: Math.max(0, indexFile - u.index.mail - u.index.articles - u.index.fulltext) },
          ...u.local.parts
            .filter((p) => p.key !== "index")
            .map((p) => ({ label: t(PART_LABEL[p.key]), bytes: p.bytes })),
        ]}
      />

      {sources.length > 0 && (
        <section className="py-3">
          <h3 className="mb-2 text-[12px] font-medium text-fg-3">{t("storage.topSources")}</h3>
          <ul className="flex flex-col gap-2">
            {sources.map((s) => (
              <li key={`${s.kind}:${s.source_id}`} title={t("storage.bytes", { n: s.bytes.toLocaleString() })}>
                <div className="flex items-center gap-2 text-[13px]">
                  <SourceIcon
                    item={{ kind: s.kind, source_id: s.source_id, source_icon: iconOf(nav.data?.feeds, s.source_id), source_title: "" }}
                    size={14}
                  />
                  <span className="min-w-0 flex-1 truncate text-fg-2">
                    {sourceName(s.source_id, s.kind) ?? t("storage.removedSource")}
                  </span>
                  <span className="tabular shrink-0 text-[12px] text-fg-3">{t("storage.items", { n: s.items })}</span>
                  <span className="tabular w-16 shrink-0 text-right text-[12px] text-fg-2">{formatSize(s.bytes)}</span>
                </div>
                <Meter value={s.bytes} max={sourceMax} />
              </li>
            ))}
          </ul>
        </section>
      )}

      <section className="pt-3">
        <h3 className="mb-1 text-[12px] font-medium text-fg-3">{t("storage.cleanup")}</h3>
        <SettingRow
          label={t("storage.clearMail", { days: u.keep_mail_days })}
          hint={t("storage.clearMailHint", { size: formatSize(u.index.mail_clearable) })}
        >
          <Button
            busy={busy === "mail"}
            disabled={u.index.mail_clearable === 0}
            onClick={() =>
              void run("mail", async () => t("storage.clearedMail", { n: (await api.clearCache("mail")).count }))
            }
          >
            {t("storage.clear")}
          </Button>
        </SettingRow>
        <SettingRow
          label={t("storage.clearFulltext")}
          hint={t("storage.clearFulltextHint", { size: formatSize(u.index.fulltext) })}
        >
          <Button
            busy={busy === "fulltext"}
            disabled={u.index.fulltext === 0}
            onClick={() =>
              void run("fulltext", async () =>
                t("storage.clearedFulltext", { n: (await api.clearCache("fulltext")).count }),
              )
            }
          >
            {t("storage.clear")}
          </Button>
        </SettingRow>
        <SettingRow label={t("storage.clearIcons")} hint={t("storage.clearIconsHint")}>
          <Button
            busy={busy === "icons"}
            onClick={() =>
              void run("icons", async () => {
                await api.clearCache("icons");
                qc.removeQueries({ queryKey: ["senderIcon"] });
                return t("storage.clearedIcons");
              })
            }
          >
            {t("storage.clear")}
          </Button>
        </SettingRow>
        <SettingRow label={t("storage.compact")} hint={t("storage.compactHint")}>
          <Button
            busy={busy === "compact"}
            onClick={() =>
              void run("compact", async () => {
                await api.compactIndex();
                return t("storage.compacted");
              })
            }
          >
            {t("storage.compactAction")}
          </Button>
        </SettingRow>
        <SettingRow label={t("storage.translation")} hint={formatSize(translation)}>
          <Button onClick={() => setDialog({ type: "settings", page: "translation" })}>{t("storage.manage")}</Button>
        </SettingRow>
      </section>
    </>
  );
}

function iconOf(feeds: { id: string; icon_url: string | null }[] | undefined, id: string): string | null {
  return feeds?.find((f) => f.id === id)?.icon_url ?? null;
}

/** 一组：标题、总大小、目录（桌面应用里能在文件管理器里打开），下面每一项一行。 */
function Group({
  title,
  hint,
  path,
  total,
  parts,
}: {
  title: string;
  hint: string;
  path: string;
  total: number;
  parts: { label: string; bytes: number }[];
}) {
  const t = useT();
  const shown = parts.filter((p) => p.bytes > 0).sort((a, b) => b.bytes - a.bytes);
  const max = shown[0]?.bytes ?? 0;
  return (
    <section className="border-b border-line py-3">
      <div className="flex items-baseline gap-2">
        <h3 className="text-[13px] font-medium text-fg">{title}</h3>
        <span className="tabular text-[13px] text-fg-2">{formatSize(total)}</span>
        <span className="min-w-0 flex-1" />
        {isTauri && (
          <IconButton label={t("storage.reveal")} onClick={() => void revealFolder(path)} className="-my-1.5">
            <FolderOpen size={14} strokeWidth={1.75} />
          </IconButton>
        )}
      </div>
      <p className="mt-0.5 select-text truncate text-[11px] text-fg-3" title={path}>
        {path}
      </p>
      <p className="mt-1 text-[12px] leading-relaxed text-fg-3">{hint}</p>
      <ul className="mt-2.5 flex flex-col gap-2">
        {shown.map((p) => (
          <li key={p.label} title={t("storage.bytes", { n: p.bytes.toLocaleString() })}>
            <div className="flex items-baseline justify-between gap-3 text-[13px]">
              <span className="text-fg-2">{p.label}</span>
              <span className="tabular text-[12px] text-fg-2">{formatSize(p.bytes)}</span>
            </div>
            <Meter value={p.bytes} max={max} />
          </li>
        ))}
      </ul>
    </section>
  );
}

/** 细条：按组里最大的那项画长短。最小也露出一点，免得很小的项看起来像没有。 */
function Meter({ value, max }: { value: number; max: number }) {
  const pct = max > 0 ? Math.max(1.5, (value / max) * 100) : 0;
  return (
    <div className="mt-1 h-1 overflow-hidden rounded-full bg-hover" aria-hidden>
      <div className="h-full rounded-full bg-accent" style={{ width: `${pct}%` }} />
    </div>
  );
}
