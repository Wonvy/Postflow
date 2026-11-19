// 快速回复模板：存在同步设置 mail_templates 里，所有设备共用。写信时点「模板」插进正文，
// 「设置 → 通用 → 邮件」里可以改名、改内容、删除。

import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";
import { api } from "./api";
import { keys, useSettings } from "./queries";
import { TEMPLATES_SETTING, type Template, templateList, templateName } from "./templatesList";

export type { Template } from "./templatesList";

export function useTemplates() {
  const qc = useQueryClient();
  const settings = useSettings();
  const list = useMemo(() => templateList(settings.data as Record<string, unknown> | undefined), [settings.data]);
  const save = useCallback(
    async (next: Template[]) => {
      await api.setSetting(TEMPLATES_SETTING, next);
      void qc.invalidateQueries({ queryKey: keys.settings });
    },
    [qc],
  );
  const add = useCallback(
    (body: string, name?: string) =>
      save([...list, { id: Math.random().toString(36).slice(2, 10), name: name?.trim() || templateName(body), body: body.trim() }]),
    [list, save],
  );
  const update = useCallback(
    (id: string, patch: Partial<Omit<Template, "id">>) => save(list.map((t) => (t.id === id ? { ...t, ...patch } : t))),
    [list, save],
  );
  const remove = useCallback((id: string) => save(list.filter((t) => t.id !== id)), [list, save]);
  return { list, add, update, remove };
}
