// 插件加到条目右键菜单里的项（ui.addItemMenuItem），放在菜单的最后。

import { Puzzle } from "lucide-react";
import type { Item } from "../lib/api";
import { usePlugins } from "../lib/plugins";
import { toast } from "./Toast";
import { MenuItem, MenuSeparator } from "./ui";

export function PluginMenuItems({ items }: { items: Item[] }) {
  const menu = usePlugins((s) => s.menu);
  const shown = menu.filter((m) => {
    try {
      return !m.when || m.when(items);
    } catch {
      return false;
    }
  });
  if (shown.length === 0 || items.length === 0) return null;
  return (
    <>
      <MenuSeparator />
      {shown.map((m) => (
        <MenuItem
          key={m.key}
          icon={<Puzzle size={14} />}
          onSelect={() => {
            Promise.resolve()
              .then(() => m.onClick(items))
              .catch((err) => toast(err instanceof Error ? err.message : String(err)));
          }}
        >
          {m.label}
        </MenuItem>
      ))}
    </>
  );
}
