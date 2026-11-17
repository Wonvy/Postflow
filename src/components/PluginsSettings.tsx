// 设置里的「插件」：本机插件目录里有哪些插件，在这台设备上启用哪些。开发说明见 docs/plugins.md。

import { useQuery } from "@tanstack/react-query";
import { BookOpen, FolderOpen, Puzzle, RefreshCw, TriangleAlert } from "lucide-react";
import { errorText, useT } from "../lib/i18n";
import { API_VERSION, listPlugins, type PluginManifest, reloadPlugin, usePlugins } from "../lib/plugins";
import { openExternal, revealFolder } from "../lib/platform";
import { useUi } from "../lib/store";
import { isTauri } from "../lib/transport";
import { Button, Spinner, Switch } from "./ui";

const DOCS_URL = "https://github.com/Wonvy/Postflow/blob/main/docs/plugins.md";

export function PluginsSettings({ open }: { open: boolean }) {
  const t = useT();
  const enabled = useUi((s) => s.enabledPlugins);
  const setEnabled = useUi((s) => s.setPluginEnabled);
  const reloadAll = useUi((s) => s.reloadPlugins);
  const status = usePlugins((s) => s.status);
  const list = useQuery({ queryKey: ["plugins"], queryFn: listPlugins, enabled: open, staleTime: 0 });

  return (
    <div className="flex flex-col gap-3 py-2">
      <p className="text-[12px] leading-relaxed text-fg-3">{t("plugins.hint")}</p>
      <p className="flex gap-2 rounded-md bg-warning/10 px-3 py-2 text-[12px] leading-relaxed text-fg-2">
        <TriangleAlert size={14} className="mt-0.5 shrink-0 text-warning" />
        {t("plugins.trust")}
      </p>
      {list.isPending && (
        <div className="flex justify-center py-6 text-fg-3">
          <Spinner />
        </div>
      )}
      {list.isError && <p className="text-[12px] text-danger">{errorText(t, list.error)}</p>}
      {list.data && list.data.plugins.length === 0 && (
        <div className="rounded-md border border-dashed border-line-strong px-4 py-5 text-center text-[13px] text-fg-3">
          {t("plugins.empty")}
        </div>
      )}
      {list.data && list.data.plugins.length > 0 && (
        <ul className="flex flex-col gap-1.5">
          {list.data.plugins.map((p) => {
            const id = p.id ?? p.folder;
            const on = enabled.includes(id);
            const state = status[id];
            const problem = p.error ?? (state?.state === "error" ? state.error : null);
            return (
              <li key={p.folder} className="rounded-md border border-line px-3 py-2">
                <div className="flex items-center gap-2.5">
                  <Puzzle size={15} className="shrink-0 text-fg-3" />
                  <div className="min-w-0 flex-1">
                    <p className="truncate font-medium text-fg">
                      {p.name ?? p.folder}
                      {p.version && <span className="ml-1.5 text-[12px] font-normal text-fg-3">{p.version}</span>}
                    </p>
                    {(p.description || p.author) && (
                      <p className="truncate text-[12px] text-fg-3">
                        {p.description}
                        {p.author && ` · ${p.author}`}
                      </p>
                    )}
                  </div>
                  {on && !p.error && (
                    <button
                      type="button"
                      title={t("plugins.reloadOne")}
                      aria-label={t("plugins.reloadOne")}
                      onClick={() => void reloadPlugin(p as PluginManifest)}
                      className="inline-flex size-6 items-center justify-center rounded text-fg-3 hover:bg-hover hover:text-fg"
                    >
                      <RefreshCw size={13} />
                    </button>
                  )}
                  <Switch
                    label={t("plugins.enable")}
                    checked={on}
                    disabled={Boolean(p.error)}
                    onChange={(next) => setEnabled(id, next)}
                  />
                </div>
                {problem && <p className="mt-1.5 select-text break-words text-[12px] text-danger">{problem}</p>}
                {on && state?.state === "loaded" && <p className="mt-1 text-[11px] text-fg-3">{t("plugins.running")}</p>}
              </li>
            );
          })}
        </ul>
      )}
      <div className="flex flex-wrap items-center gap-2">
        {isTauri && list.data && (
          <Button onClick={() => void revealFolder(list.data.dir)}>
            <FolderOpen size={14} />
            {t("plugins.openFolder")}
          </Button>
        )}
        <Button
          onClick={() => {
            reloadAll();
            void list.refetch();
          }}
        >
          <RefreshCw size={14} />
          {t("plugins.rescan")}
        </Button>
        <Button variant="ghost" onClick={() => void openExternal(DOCS_URL)}>
          <BookOpen size={14} />
          {t("plugins.docs")}
        </Button>
      </div>
      {list.data && (
        <p className="select-text truncate text-[11px] text-fg-3" title={list.data.dir}>
          {t("plugins.folder", { dir: list.data.dir, api: API_VERSION })}
        </p>
      )}
    </div>
  );
}
