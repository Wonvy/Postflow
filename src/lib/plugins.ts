// 插件运行时：加载、卸载插件，以及交给每个插件的接口对象。开发说明见 docs/plugins.md。
//
// 插件是本机插件目录里的 ES 模块，由核心读出代码，这里用 blob: 地址动态 import。
// 插件和界面在同一个页面里运行，能做界面能做的一切，所以只加载用户在设置里手动启用的插件。
// 插件通过接口登记的东西（按钮、菜单项、样式、事件监听）在卸载时自动撤掉。

import { useEffect } from "react";
import { create } from "zustand";
import { toast } from "../components/Toast";
import { api, type Item, type ItemFilter, type ItemPatch } from "./api";
import { resolveLang } from "./i18n";
import { useUi } from "./store";
import { type CoreEvent, rpc, subscribe } from "./transport";

/** 插件接口的版本。改了不兼容的地方才加一。 */
export const API_VERSION = 1;

export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  description: string;
  author: string;
  url?: string | null;
  main: string;
  minApiVersion: number;
}

export interface ListedPlugin extends Partial<PluginManifest> {
  folder: string;
  error: string | null;
}

/** 按钮被点时交给插件的上下文。 */
export interface ToolContext {
  /** 阅读区打开的条目。 */
  item: Item | null;
  /** 列表里选中的条目 ID。 */
  selection: string[];
}

export interface ToolbarButton {
  /** 插件里自己起的 ID。 */
  id: string;
  toolbar: "list" | "reader";
  label: string;
  /** 24×24 的 SVG 标记（用 currentColor 描边）。不给就是拼图图标。 */
  icon?: string;
  onClick: (ctx: ToolContext) => void | Promise<void>;
}

export interface ItemMenuItem {
  id: string;
  label: string;
  /** 只对这些条目显示；不给就都显示。 */
  when?: (items: Item[]) => boolean;
  onClick: (items: Item[]) => void | Promise<void>;
}

type Registered<T> = T & { pluginId: string; key: string };

export type PluginStatus = { state: "loaded" } | { state: "error"; error: string };

interface Registry {
  buttons: Registered<ToolbarButton>[];
  menu: Registered<ItemMenuItem>[];
  status: Record<string, PluginStatus>;
}

/** 插件登记的按钮和菜单项，以及每个插件的加载状态。 */
export const usePlugins = create<Registry>()(() => ({ buttons: [], menu: [], status: {} }));

/** 工具栏上插件按钮的 ID，和布局里用的一样。 */
export function buttonKey(pluginId: string, id: string): string {
  return `plugin:${pluginId}:${id}`;
}

// ------------------------------------------------------------------ 事件

type EventName = "new-mail" | "items-changed" | "item-open" | "core";
const listeners = new Map<EventName, Set<(payload: unknown) => void>>();

function emit(name: EventName, payload: unknown) {
  for (const fn of listeners.get(name) ?? []) {
    try {
      fn(payload);
    } catch (err) {
      console.error(`[plugin] ${name} handler failed`, err);
    }
  }
}

let eventsStarted = false;
function startEvents() {
  if (eventsStarted) return;
  eventsStarted = true;
  subscribe((event: CoreEvent) => {
    emit("core", event);
    if (event.type === "new_mail") emit("new-mail", event);
    if (event.type === "changed") emit("items-changed", event);
  });
  let last = useUi.getState().openId;
  useUi.subscribe((s) => {
    if (s.openId !== last) {
      last = s.openId;
      emit("item-open", { id: last });
    }
  });
}

// ------------------------------------------------------------------ 接口对象

interface Loaded {
  dispose: (() => void)[];
  onunload?: () => void;
}
const loaded = new Map<string, Loaded>();

function storageKey(pluginId: string, key: string) {
  return `postflow.plugin.${pluginId}.${key}`;
}

/** 交给插件的接口。说明见 docs/plugins.md，那里的内容要和这里一致。 */
function makeApi(manifest: PluginManifest, track: (fn: () => void) => void) {
  const pid = manifest.id;
  const register = <T>(list: "buttons" | "menu", spec: T & { id: string }) => {
    const key = list === "buttons" ? buttonKey(pid, spec.id) : `${pid}:${spec.id}`;
    usePlugins.setState((s) => ({
      [list]: [...s[list].filter((x) => x.key !== key), { ...spec, pluginId: pid, key }],
    }));
    const remove = () => usePlugins.setState((s) => ({ [list]: s[list].filter((x) => x.key !== key) }));
    track(remove);
    return remove;
  };

  return Object.freeze({
    apiVersion: API_VERSION,
    manifest: Object.freeze({ ...manifest }),
    /** 界面语言：zh 或 en（导入的语言按它的基础语言）。 */
    lang: () => resolveLang(useUi.getState().lang),

    /** 直接调用核心的方法（docs/plugins.md 列出了常用的）。 */
    rpc: <T = unknown>(method: string, params?: unknown) => rpc<T>(method, params),

    items: Object.freeze({
      list: (filter: ItemFilter, limit = 50) => api.listItems(filter, undefined, limit),
      get: (id: string) => api.getItem(id),
      set: (ids: string[], patch: ItemPatch) => api.setItems(ids, patch),
      label: (ids: string[], label: string | null) => rpc("items.label", { ids, label }),
      open: (id: string) => useUi.getState().open(id),
      openedId: () => useUi.getState().openId,
      selection: () => [...useUi.getState().selection],
    }),

    ui: Object.freeze({
      toast: (message: string) => toast(String(message)),
      addToolbarButton: (spec: ToolbarButton) => register("buttons", spec),
      addItemMenuItem: (spec: ItemMenuItem) => register("menu", spec),
      /** 加一段 CSS，卸载时去掉。 */
      addStyle: (css: string) => {
        const el = document.createElement("style");
        el.dataset.plugin = pid;
        el.textContent = css;
        document.head.append(el);
        const remove = () => el.remove();
        track(remove);
        return remove;
      },
    }),

    events: Object.freeze({
      on: (name: EventName, handler: (payload: unknown) => void) => {
        startEvents();
        const set = listeners.get(name) ?? new Set();
        listeners.set(name, set);
        set.add(handler);
        const remove = () => set.delete(handler);
        track(remove);
        return remove;
      },
    }),

    /** 这个插件自己的存储，只在这台设备上。值要能转成 JSON。 */
    storage: Object.freeze({
      get: <T = unknown>(key: string): T | null => {
        try {
          const raw = localStorage.getItem(storageKey(pid, key));
          return raw === null ? null : (JSON.parse(raw) as T);
        } catch {
          return null;
        }
      },
      set: (key: string, value: unknown) => {
        try {
          localStorage.setItem(storageKey(pid, key), JSON.stringify(value));
        } catch {
          // 存不下就算了
        }
      },
      remove: (key: string) => {
        try {
          localStorage.removeItem(storageKey(pid, key));
        } catch {
          // ignore
        }
      },
    }),

    /** 登记一个卸载时要做的清理。 */
    onUnload: (fn: () => void) => track(fn),
  });
}

export type PluginApi = ReturnType<typeof makeApi>;

function setStatus(id: string, status: PluginStatus | null) {
  usePlugins.setState((s) => {
    const next = { ...s.status };
    if (status) next[id] = status;
    else delete next[id];
    return { status: next };
  });
}

export async function loadPlugin(manifest: PluginManifest): Promise<void> {
  if (loaded.has(manifest.id)) return;
  if (manifest.minApiVersion > API_VERSION) {
    setStatus(manifest.id, { state: "error", error: `needs plugin API ${manifest.minApiVersion}, this Postflow has ${API_VERSION}` });
    return;
  }
  const entry: Loaded = { dispose: [] };
  loaded.set(manifest.id, entry);
  try {
    const { source } = await rpc<{ source: string }>("plugins.source", { id: manifest.id });
    const url = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
    let mod: { default?: unknown };
    try {
      mod = await import(/* @vite-ignore */ url);
    } finally {
      URL.revokeObjectURL(url);
    }
    const plugin = mod.default;
    const pluginApi = makeApi(manifest, (fn) => entry.dispose.push(fn));
    if (typeof plugin === "function") {
      const cleanup = await plugin(pluginApi);
      if (typeof cleanup === "function") entry.dispose.push(cleanup);
    } else if (plugin && typeof plugin === "object" && typeof (plugin as { onload?: unknown }).onload === "function") {
      const p = plugin as { onload: (a: PluginApi) => unknown; onunload?: () => void };
      await p.onload(pluginApi);
      entry.onunload = p.onunload?.bind(p);
    } else {
      throw new Error("main.js must `export default` a function or an object with onload(api)");
    }
    if (loaded.get(manifest.id) === entry) setStatus(manifest.id, { state: "loaded" });
  } catch (err) {
    unloadPlugin(manifest.id);
    setStatus(manifest.id, { state: "error", error: err instanceof Error ? err.message : String(err) });
    console.error(`[plugin ${manifest.id}] failed to load`, err);
  }
}

export function unloadPlugin(id: string): void {
  const entry = loaded.get(id);
  if (!entry) return;
  loaded.delete(id);
  try {
    entry.onunload?.();
  } catch (err) {
    console.error(`[plugin ${id}] onunload failed`, err);
  }
  for (const fn of entry.dispose.reverse()) {
    try {
      fn();
    } catch (err) {
      console.error(`[plugin ${id}] cleanup failed`, err);
    }
  }
  setStatus(id, null);
}

export async function listPlugins(): Promise<{ dir: string; plugins: ListedPlugin[] }> {
  return rpc("plugins.list");
}

/** 应用启动时调用：把启用了的插件加载起来，启用列表变了就跟着加载或卸载。 */
export function usePluginLoader(): void {
  const enabled = useUi((s) => s.enabledPlugins);
  const reloadKey = useUi((s) => s.pluginReload);
  useEffect(() => {
    let cancelled = false;
    void listPlugins()
      .then(({ plugins }) => {
        if (cancelled) return;
        const wanted = new Set(enabled);
        for (const id of [...loaded.keys()]) if (!wanted.has(id)) unloadPlugin(id);
        for (const p of plugins) {
          if (p.error || !p.id || !wanted.has(p.id)) continue;
          void loadPlugin(p as PluginManifest);
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [enabled, reloadKey]);
}

/** 重新加载一个插件（改了代码以后）。 */
export async function reloadPlugin(manifest: PluginManifest): Promise<void> {
  unloadPlugin(manifest.id);
  await loadPlugin(manifest);
}
