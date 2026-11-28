// 插件运行时：加载、卸载插件，以及交给每个插件的接口对象。开发说明见 docs/plugins.md。
//
// 插件是本机插件目录里的 ES 模块，由核心读出代码，这里用 blob: 地址动态 import。
// 插件和界面在同一个页面里运行，能做界面能做的一切，所以只加载用户在设置里手动启用的插件。
// 插件通过接口登记的东西（按钮、菜单项、样式、事件监听）在卸载时自动撤掉。

import { useEffect } from "react";
import { create } from "zustand";
import { toast } from "../components/Toast";
import { api, type Item, type ItemDetail, type ItemFilter, type ItemPatch } from "./api";
import { resolveLang } from "./i18n";
import { episodeOf, pause, play, playbackTime, seek, skip, stop, toggle, usePlayer } from "./player";
import { useUi } from "./store";
import { type Cue, cueAt, speakerOrder } from "./subtitleCues";
import { useSubtitles } from "./subtitles";
import { type SubtitleTranslation, translationOf, useSubtitleTranslationState } from "./subtitleTranslate";
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

/** 阅读区里正文上方的一块，插件自己往里画内容。 */
export interface ReaderSection {
  id: string;
  /** 只对这些条目显示；不给就都显示。 */
  when?: (item: ItemDetail) => boolean;
  /**
   * 往 `el` 里画内容。打开别的条目、切换全文、插件停用时调用返回的清理函数（如果有），再清空 `el`。
   * 可以是 async；画到一半换了条目时，`el` 已经不在页面上了，画了也看不到，不会出错。
   */
  render: (item: ItemDetail, el: HTMLElement) => void | (() => void) | Promise<void | (() => void)>;
}

type Registered<T> = T & { pluginId: string; key: string };

export type PluginStatus = { state: "loaded" } | { state: "error"; error: string };

interface Registry {
  buttons: Registered<ToolbarButton>[];
  menu: Registered<ItemMenuItem>[];
  sections: Registered<ReaderSection>[];
  status: Record<string, PluginStatus>;
}

/** 插件登记的按钮和菜单项，以及每个插件的加载状态。 */
export const usePlugins = create<Registry>()(() => ({ buttons: [], menu: [], sections: [], status: {} }));

/** 工具栏上插件按钮的 ID，和布局里用的一样。 */
export function buttonKey(pluginId: string, id: string): string {
  return `plugin:${pluginId}:${id}`;
}

// ------------------------------------------------------------------ 播放器和字幕（给插件看的样子）

/** 正在放的一期，给插件看的样子。没在放是 null。 */
export interface PluginPlayerState {
  id: string;
  title: string;
  source: string;
  url: string;
  playing: boolean;
  /** 放到第几秒（确切的）。 */
  position: number;
  /** 多长（秒）；还不知道是 null。 */
  duration: number | null;
  rate: number;
}

/** 一句字幕，给插件看的样子。 */
export interface PluginCue {
  start: number;
  end: number;
  text: string;
  /** 第几个说话的人（从 0 数，按开口先后）。没分出来是 null。 */
  speaker: number | null;
  /** 节目文字稿里写的说话人名字。 */
  speakerName: string | null;
  /** 自动翻译的译文。 */
  translation: string | null;
}

function playerState(): PluginPlayerState | null {
  const s = usePlayer.getState();
  if (!s.current) return null;
  return {
    id: s.current.id,
    title: s.current.title,
    source: s.current.source,
    url: s.current.media.url,
    playing: s.playing,
    position: playbackTime() ?? s.position,
    duration: s.duration > 0 ? s.duration : (s.current.media.duration ?? null),
    rate: s.rate,
  };
}

function toPluginCue(
  c: Cue,
  order: Map<number, number> | null,
  names: readonly string[] | undefined,
  translation: SubtitleTranslation | null,
): PluginCue {
  const speaker = c.speaker === undefined ? null : order ? (order.get(c.speaker) ?? null) : c.speaker;
  return {
    start: c.start,
    end: c.end,
    text: c.text,
    speaker,
    speakerName: c.speaker !== undefined ? (names?.[c.speaker] ?? null) : null,
    translation: translation ? (translationOf(translation, c) ?? null) : null,
  };
}

/** 正在放的这一期的字幕，和它的译文情况。 */
function currentSubtitles() {
  const entry = useSubtitles.getState().entry;
  if (!entry) return null;
  const tr = useSubtitleTranslationState.getState();
  const translation = tr.id === entry.id ? tr.value : null;
  const order = entry.source === "local" ? speakerOrder(entry.cues) : null;
  return { entry, translation, order };
}

function subtitlesState() {
  const now = currentSubtitles();
  if (!now) return null;
  const { entry, translation, order } = now;
  return {
    id: entry.id,
    source: entry.source,
    complete: entry.complete,
    translatedFrom: translation?.status === "on" ? translation.from : null,
    translatedTo: translation?.status === "on" ? translation.to : null,
    cues: entry.cues.map((c) => toPluginCue(c, order, entry.speakerNames, translation)),
  };
}

/** 第 `seconds` 秒（默认现在）显示的那一句，带它是第几句。 */
function cueNow(seconds?: number): (PluginCue & { index: number }) | null {
  const now = currentSubtitles();
  const p = usePlayer.getState();
  if (!now || p.current?.id !== now.entry.id) return null;
  const at = typeof seconds === "number" ? seconds : (playbackTime() ?? p.position);
  const index = cueAt(now.entry.cues, at);
  const cue = now.entry.cues[index];
  if (!cue) return null;
  return { index, ...toPluginCue(cue, now.order, now.entry.speakerNames, now.translation) };
}

// ------------------------------------------------------------------ 事件

type EventName = "new-mail" | "items-changed" | "item-open" | "core" | "player" | "subtitle";
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

  // 播放器：换了一期、放 / 停、改了速度、跳了一下
  let was = usePlayer.getState();
  usePlayer.subscribe((s) => {
    const jumped = s.current?.id === was.current?.id && Math.abs(s.position - was.position) > 3;
    const changed = s.current?.id !== was.current?.id || s.playing !== was.playing || s.rate !== was.rate;
    was = s;
    if (changed || jumped) emit("player", playerState());
  });

  // 字幕：显示的那一句换了（有人听的时候才看，放着时每秒看几次）
  let shown: string | null = null;
  const check = () => {
    if (!listeners.get("subtitle")?.size) return;
    const cue = cueNow();
    const key = cue ? `${cue.index}|${cue.start}|${cue.text}|${cue.translation ?? ""}` : null;
    if (key === shown) return;
    shown = key;
    emit("subtitle", cue);
  };
  setInterval(() => {
    if (usePlayer.getState().playing) check();
  }, 250);
  usePlayer.subscribe(check);
  useSubtitles.subscribe(check);
  useSubtitleTranslationState.subscribe(check);
}

// ------------------------------------------------------------------ 接口对象

interface Loaded {
  dispose: (() => void)[];
  onunload?: () => void;
}
const loaded = new Map<string, Loaded>();

function storageKey(pluginId: string, key: string) {
  return `shibapost.plugin.${pluginId}.${key}`;
}

/** 交给插件的接口。说明见 docs/plugins.md，那里的内容要和这里一致。 */
function makeApi(manifest: PluginManifest, track: (fn: () => void) => void) {
  const pid = manifest.id;
  const register = <T>(list: "buttons" | "menu" | "sections", spec: T & { id: string }) => {
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

    /** 播客播放器。 */
    player: Object.freeze({
      /** 正在放的一期，没在放是 null。 */
      state: () => playerState(),
      /** 放某一期（条目 ID，可以从第 `at` 秒开始）；不给 ID 就是接着放现在这一期。 */
      play: async (id?: string, at?: number) => {
        const s = usePlayer.getState();
        if (!id || s.current?.id === id) {
          if (!s.current) return;
          if (typeof at === "number") seek(at);
          if (!s.playing) toggle();
          return;
        }
        const episode = episodeOf(await api.getItem(id));
        if (!episode) throw new Error(`item ${id} has no audio`);
        play(episode, [episode], typeof at === "number" ? at : undefined);
      },
      pause: () => pause(),
      seek: (seconds: number) => seek(Number(seconds)),
      skip: (seconds: number) => skip(Number(seconds)),
      setRate: (rate: number) => usePlayer.getState().setRate(Number(rate)),
      /** 关掉播放器。 */
      stop: () => stop(),
    }),

    /** 播客字幕。 */
    subtitles: Object.freeze({
      /** 字幕开着没有。 */
      enabled: () => useSubtitles.getState().on,
      setEnabled: (on: boolean) => useSubtitles.getState().setOn(Boolean(on)),
      /** 正在放的这一期的全部字幕（本机转写的会边转边多）。没开字幕或没在放是 null。 */
      current: () => subtitlesState(),
      /** 第 `seconds` 秒（默认现在）显示的那一句，没有是 null。 */
      cueAt: (seconds?: number) => cueNow(seconds),
      /** 某一期在这台设备上转写过的字幕（不管在不在放）。 */
      get: async (id: string) => {
        const snap = await api.subtitlesGet(id);
        const order = speakerOrder(snap.cues);
        return {
          id,
          complete: snap.complete,
          duration: snap.duration,
          cues: snap.cues.map((c) => toPluginCue(c, order, undefined, null)),
        };
      },
    }),

    ui: Object.freeze({
      toast: (message: string) => toast(String(message)),
      addToolbarButton: (spec: ToolbarButton) => register("buttons", spec),
      addItemMenuItem: (spec: ItemMenuItem) => register("menu", spec),
      /** 在阅读区正文上方加一块，插件自己画内容（比如关键词、摘要）。 */
      addReaderSection: (spec: ReaderSection) => register("sections", spec),
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
    setStatus(manifest.id, { state: "error", error: `needs plugin API ${manifest.minApiVersion}, this ShibaPost has ${API_VERSION}` });
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
