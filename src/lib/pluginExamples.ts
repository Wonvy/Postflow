// 随应用打包的示例插件（examples/plugins）。每台设备第一次启动时放进插件目录，不启用；
// 删掉的可以在「设置 → 插件」里重新安装。入口文件按需加载，textrank-zh 有一百多 KB。

import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import type { MessageKey } from "./i18n";
import { listPlugins } from "./plugins";
import { useUi } from "./store";
import { rpc } from "./transport";

const MANIFESTS = import.meta.glob<string>("../../examples/plugins/*/manifest.json", {
  eager: true,
  query: "?raw",
  import: "default",
});
const SOURCES = import.meta.glob<string>("../../examples/plugins/*/main.js", { query: "?raw", import: "default" });

/** 示例插件的说明，manifest 里是英文的，界面上用当前语言。 */
const DESCRIPTIONS: Record<string, MessageKey> = {
  "copy-code": "plugins.example.copyCode",
  "daily-digest": "plugins.example.dailyDigest",
  "important-senders": "plugins.example.importantSenders",
  "textrank-zh": "plugins.example.textrankZh",
};

export interface PluginExample {
  id: string;
  name: string;
  version: string;
  description: MessageKey | null;
  manifest: string;
  load: () => Promise<string>;
}

export const PLUGIN_EXAMPLES: PluginExample[] = Object.entries(MANIFESTS).flatMap(([path, manifest]) => {
  const load = SOURCES[path.replace(/manifest\.json$/, "main.js")];
  if (!load) return [];
  const parsed = JSON.parse(manifest) as { id: string; name: string; version?: string };
  return [
    {
      id: parsed.id,
      name: parsed.name,
      version: parsed.version ?? "",
      description: DESCRIPTIONS[parsed.id] ?? null,
      manifest,
      load,
    },
  ];
});

/** 把示例插件写进插件目录。已经装过的会被覆盖成应用里带的版本。 */
export async function installExample(example: PluginExample): Promise<void> {
  const source = await example.load();
  await rpc("plugins.install", { manifest: example.manifest, source });
}

let seeding = false;

/**
 * 应用启动时调用：这台设备还没放过示例插件的话，把插件目录里没有的示例放进去，不启用。
 * 只放一次，用户删掉的不会再出现；已经有的不覆盖（可能改过）。失败了下次启动再试。
 */
export function useSeedExamplePlugins(): void {
  const seeded = useUi((s) => s.examplePluginsSeeded);
  const qc = useQueryClient();
  useEffect(() => {
    if (seeded || seeding) return;
    seeding = true;
    void (async () => {
      try {
        const { plugins } = await listPlugins();
        const present = new Set(plugins.map((p) => p.folder));
        for (const example of PLUGIN_EXAMPLES) {
          if (!present.has(example.id)) await installExample(example);
        }
        useUi.getState().setExamplePluginsSeeded();
        await qc.invalidateQueries({ queryKey: ["plugins"] });
      } catch {
        // 下次启动再试
      } finally {
        seeding = false;
      }
    })();
  }, [seeded, qc]);
}
