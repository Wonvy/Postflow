import { ShortcutsDialog } from "./components/ShortcutsDialog";
import { OutboxDialog } from "./components/Outbox";
import { SnoozeDialog } from "./components/Snooze";
import { CommandPalette } from "./components/CommandPalette";
import { PlayerBridge } from "./components/Player";
import { DownloadsBridge } from "./lib/episodes";
import { accentVars } from "./lib/accent";
import { useCanHover, useLayout, useReaderOverlay } from "./lib/mobile";
import { Unplug } from "lucide-react";
import { type PointerEvent, type ReactNode, useEffect, useRef, useState } from "react";
import { AddAccountDialog, EditAccountDialog, RemoveAccountDialog } from "./components/AccountDialogs";
import { AddFeedDialog, EditFeedDialog } from "./components/FeedDialogs";
import { ItemList, MarkAllReadDialog } from "./components/ItemList";
import { Onboarding } from "./components/Onboarding";
import { Reader } from "./components/Reader";
import { ComposeDialog } from "./components/Composer";
import { SettingsDialog } from "./components/SettingsDialog";
import { Toaster } from "./components/Toast";
import { ToolbarsDialog } from "./components/Toolbars";
import { SourceDialog } from "./components/SourceDialog";
import { watchFullscreen } from "./lib/fullscreen";
import { useSeedExamplePlugins } from "./lib/pluginExamples";
import { usePluginLoader } from "./lib/plugins";
import { WindowControls } from "./components/WindowControls";
import { useFirstRunWelcome, WelcomeGuide } from "./components/WelcomeGuide";
import { UpdateDialog, UpdateNotice } from "./components/UpdateDialog";
import { Sidebar } from "./components/Sidebar";
import { StatsPage } from "./components/StatsPage";
import { Button, cx, Logo, Spinner, TextInput } from "./components/ui";
import { api } from "./lib/api";
import { resolveLang, useT } from "./lib/i18n";
import { useTray, useUnreadBadge } from "./lib/notify";
import { useTrayActions } from "./lib/trayActions";
import { usePasteToSave } from "./components/ReadLater";
import { useCoreEvents, useNav, useStatus } from "./lib/queries";
import { useShortcuts } from "./lib/shortcuts";
import { LIST_MIN, READER_MIN, useUi } from "./lib/store";
import { isTauri, RpcError } from "./lib/transport";
import { startUpdateChecks } from "./lib/updater";

/** 收信的间隔：窗口回到前台时，离上次超过这么久就马上收一次。 */
const MAIL_CATCH_UP_MS = 60_000;

/**
 * 窗口回到前台（电脑睡醒、切回应用）时马上收一次信。邮箱平时靠服务器推送，
 * 推送没到或者连接在睡眠里断了，也不用等下一轮。
 */
function useMailCatchUp() {
  useEffect(() => {
    let last = Date.now();
    const check = () => {
      if (document.visibilityState !== "visible" || Date.now() - last < MAIL_CATCH_UP_MS) return;
      last = Date.now();
      void api.syncAccount(undefined, true).catch(() => {});
    };
    document.addEventListener("visibilitychange", check);
    window.addEventListener("focus", check);
    return () => {
      document.removeEventListener("visibilitychange", check);
      window.removeEventListener("focus", check);
    };
  }, []);
}

/** 把主题和语言写到 <html> 上，跟随系统时监听系统设置的变化。 */
function useAppearance() {
  const theme = useUi((s) => s.theme);
  const accent = useUi((s) => s.accentColor);
  const lang = useUi((s) => s.lang);

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const apply = () => {
      const resolved = theme === "system" ? (media.matches ? "light" : "dark") : theme;
      const root = document.documentElement;
      root.dataset.theme = resolved;
      // 主题色：浅色、深色界面各用一个深浅，盖掉 styles.css 里的默认
      const vars = accentVars(accent, resolved);
      root.style.setProperty("--accent", vars.accent);
      root.style.setProperty("--accent-strong", vars.strong);
      root.style.setProperty("--accent-soft", vars.soft);
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme, accent]);

  useEffect(() => {
    document.documentElement.lang = resolveLang(lang) === "zh" ? "zh-CN" : "en";
  }, [lang]);
}

/** 两栏之间的拖拽条。视觉上是一条线，可拖的范围比线宽。 */
function Resizer({ width, onResize }: { width: number; onResize: (width: number) => void }) {
  const start = useRef<{ x: number; width: number } | null>(null);

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    start.current = { x: e.clientX, width };
    e.currentTarget.setPointerCapture(e.pointerId);
  };
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    if (start.current) onResize(start.current.width + e.clientX - start.current.x);
  };
  const onPointerUp = () => {
    start.current = null;
  };

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      className="group relative z-10 -mx-[4px] w-[9px] shrink-0 cursor-col-resize touch:-mx-[10px] touch:w-[21px]"
    >
      <div className="absolute inset-y-0 left-[4px] w-px touch:left-[10px] bg-transparent transition-colors group-hover:bg-accent group-active:bg-accent" />
    </div>
  );
}

type PaneId = "sidebar" | "list" | "reader";

/**
 * 一栏的外壳：不占布局（display: contents），只记下鼠标进了哪一栏。`dim` 时里面那一栏变淡，
 * 样式见 styles.css 的 [data-pane]。
 */
function Pane({ id, dim, onEnter, children }: { id: PaneId; dim: boolean; onEnter: (id: PaneId) => void; children: ReactNode }) {
  return (
    <div className="contents" data-pane={id} data-dim={dim ? "" : undefined} onPointerEnter={() => onEnter(id)}>
      {children}
    </div>
  );
}

/**
 * 「聚焦鼠标所在的一栏」：鼠标停在哪一栏，另外几栏就变淡。只在有鼠标或触控板时起作用；
 * 鼠标经过分栏的拖拽条时保持原样，移出窗口、移到弹出的菜单和对话框上（它们不在栏里）时都恢复。
 */
function usePaneFocus() {
  const enabled = useUi((s) => s.focusPane);
  const canHover = useCanHover();
  const [hovered, setHovered] = useState<PaneId | null>(null);
  const active = enabled && canHover && hovered !== null;
  return {
    /** `shown`：现在显示着的几栏。只剩一栏，或者鼠标记着的那一栏已经不在（比如刚用快捷键最大化）时都不变淡。 */
    dim: (id: PaneId, shown: readonly PaneId[]) =>
      active && shown.length > 1 && shown.includes(hovered) && hovered !== id,
    enter: setHovered,
    leave: () => setHovered(null),
  };
}

/**
 * 「阅读区弹出」时打开的条目：盖在侧栏和列表上面的大面板，四周是变暗、模糊的遮罩。
 * 点遮罩、右上角的关闭按钮或按 Esc 关掉；最大化（F）时铺满窗口。快捷键照常用，J/K 直接换下一条。
 * 不用 role=dialog：它不是一个要先关掉才能做别的事的对话框，桌面应用的原网页视图也靠 role=dialog 判断有没有被盖住。
 */
function ReaderPopup() {
  const t = useT();
  const openId = useUi((s) => s.openId);
  const open = useUi((s) => s.open);
  const maximized = useUi((s) => s.readerMaximized);
  if (!openId) return null;
  return (
    <div className="fixed inset-0 z-30 flex items-center justify-center pb-[env(safe-area-inset-bottom)] pt-[env(safe-area-inset-top)]">
      <div aria-hidden className="anim-fade absolute inset-0 bg-overlay-reader backdrop-blur-[6px]" onClick={() => open(null)} />
      <section
        aria-label={t("reader.popup")}
        className={cx(
          "anim-pop relative flex overflow-hidden bg-bg shadow-pop [&>main]:border-l-0",
          maximized ? "h-full w-full" : "h-[calc(100%-48px)] w-[min(1040px,calc(100%-48px))] rounded-xl ring-1 ring-line",
        )}
      >
        <Reader />
      </section>
    </div>
  );
}

function Shell() {
  useShortcuts();
  useMailCatchUp();
  const unreadMail = useNav().data?.counts.unread_mail;
  useUnreadBadge(unreadMail);
  useTray(unreadMail);
  useTrayActions();
  // 在输入框外面粘贴一个网址：存进稍后读
  usePasteToSave();
  useEffect(() => startUpdateChecks(), []);
  useEffect(() => watchFullscreen(), []);
  usePluginLoader();
  useSeedExamplePlugins();
  useFirstRunWelcome();
  const sidebarWidth = useUi((s) => s.sidebarWidth);
  const listWidth = useUi((s) => s.listWidth);
  const setSidebarWidth = useUi((s) => s.setSidebarWidth);
  const setListWidth = useUi((s) => s.setListWidth);
  const mainView = useUi((s) => s.mainView);

  // 卡片视图里列表可以拉得很宽，但要给阅读面板留出位置
  const [viewport, setViewport] = useState(window.innerWidth);
  useEffect(() => {
    const onResize = () => setViewport(window.innerWidth);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  const listMax = Math.max(LIST_MIN, viewport - sidebarWidth - READER_MIN);
  const listShown = Math.min(listWidth, listMax);

  // 阅读区最大化（或全屏）时只留阅读区
  const layout = useLayout();
  // 阅读区弹出时窗口里只有侧栏和列表，最大化只是把弹出的阅读区铺满
  const overlay = useReaderOverlay();
  const maximized = useUi((s) => s.readerMaximized) && mainView !== "stats" && !overlay;
  const focus = usePaneFocus();
  const shownPanes: PaneId[] = overlay
    ? layout === "tablet"
      ? ["list"]
      : ["sidebar", "list"]
    : maximized
    ? ["reader"]
    : mainView === "stats"
      ? ["sidebar", "list"]
      : layout === "tablet"
        ? ["list", "reader"]
        : ["sidebar", "list", "reader"];
  const pane = (id: PaneId, node: ReactNode) => (
    <Pane id={id} dim={focus.dim(id, shownPanes)} onEnter={focus.enter}>
      {node}
    </Pane>
  );
  const openId = useUi((s) => s.openId);
  const drawerOpen = useUi((s) => s.drawerOpen);
  const setDrawerOpen = useUi((s) => s.setDrawerOpen);
  // 平板转到横屏变成三栏时，滑出来的侧栏收回去，转回竖屏也不会自己冒出来
  useEffect(() => {
    if (layout === "desktop" && drawerOpen) setDrawerOpen(false);
  }, [layout, drawerOpen, setDrawerOpen]);

  const dialogs = (
    <>
      <AddFeedDialog />
      <EditFeedDialog />
      <AddAccountDialog />
      <EditAccountDialog />
      <RemoveAccountDialog />
      <MarkAllReadDialog />
      <UpdateDialog />
      <UpdateNotice />
      <SettingsDialog />
      <ToolbarsDialog />
      <ComposeDialog />
      <SourceDialog />
      <WelcomeGuide />
      <CommandPalette />
      <ShortcutsDialog />
      <SnoozeDialog />
      <OutboxDialog />
      <PlayerBridge />
      <DownloadsBridge />
      <Toaster />
    </>
  );

  // 手机和平板布局里侧栏从左边滑出来，点旁边的空白收回去
  const drawer = drawerOpen && (
    <>
      <div aria-hidden className="anim-fade fixed inset-0 z-30 bg-overlay" onClick={() => setDrawerOpen(false)} />
      <div className="anim-drawer fixed inset-y-0 left-0 z-30 flex pb-[env(safe-area-inset-bottom)] pt-[env(safe-area-inset-top)] shadow-pop">
        <Sidebar width={Math.min(layout === "tablet" ? 320 : 300, viewport - 56)} />
      </div>
    </>
  );

  // 手机：一次一栏。打开了一条就只显示阅读区，否则是列表
  if (layout === "phone") {
    return (
      <div className="flex h-full pb-[env(safe-area-inset-bottom)] pt-[env(safe-area-inset-top)]">
        {mainView === "stats" ? <StatsPage /> : openId ? <Reader /> : <ItemList width={viewport} />}
        {drawer}
        {dialogs}
      </div>
    );
  }

  // 平板：列表和阅读区并排，侧栏收起来，阅读区宽一点
  if (layout === "tablet") {
    const tabletListMax = Math.max(LIST_MIN, viewport - READER_MIN);
    const tabletList = Math.min(listWidth, tabletListMax);
    return (
      <div
        className="flex h-full pb-[env(safe-area-inset-bottom)] pt-[env(safe-area-inset-top)]"
        onPointerLeave={focus.leave}
      >
        {mainView === "stats" ? (
          <StatsPage />
        ) : (
          <>
            {overlay ? (
              pane("list", <ItemList width={viewport} />)
            ) : (
              <>
                {!maximized && (
                  <>
                    {pane("list", <ItemList width={tabletList} />)}
                    <Resizer width={tabletList} onResize={(w) => setListWidth(Math.min(w, tabletListMax))} />
                  </>
                )}
                {pane("reader", <Reader />)}
              </>
            )}
          </>
        )}
        {overlay && mainView !== "stats" && <ReaderPopup />}
        {drawer}
        {dialogs}
      </div>
    );
  }

  return (
    <div className="flex h-full" onPointerLeave={focus.leave}>
      {!maximized && (
        <>
          {pane("sidebar", <Sidebar width={sidebarWidth} />)}
          <Resizer width={sidebarWidth} onResize={setSidebarWidth} />
        </>
      )}
      {mainView === "stats" ? (
        pane("list", <StatsPage />)
      ) : (
        <>
          {overlay ? (
            // 两栏：列表占满侧栏右边
            pane("list", <ItemList width={Math.max(LIST_MIN, viewport - sidebarWidth)} />)
          ) : (
            <>
              {!maximized && (
                <>
                  {pane("list", <ItemList width={listShown} />)}
                  <Resizer width={listShown} onResize={(w) => setListWidth(Math.min(w, listMax))} />
                </>
              )}
              {pane("reader", <Reader />)}
            </>
          )}
        </>
      )}
      {overlay && mainView !== "stats" && <ReaderPopup />}
      {dialogs}
    </div>
  );
}

export default function App() {
  // 窗口按钮在所有界面上都要有：加载中、连不上核心、首次设置
  return (
    <>
      <Body />
      <WindowControls />
    </>
  );
}

function Body() {
  useAppearance();
  useCoreEvents();
  const status = useStatus();

  if (status.isPending) {
    return (
      <div data-tauri-drag-region className="flex h-full items-center justify-center text-fg-3">
        <Spinner size={16} />
      </div>
    );
  }

  // 网页访问时还没有访问密钥（比如手机主屏幕上第一次打开）：请用户贴上桌面应用里的链接
  if (status.error instanceof RpcError && status.error.code === "unauthorized") {
    return <AccessKeyScreen />;
  }

  if (status.isError || !status.data) {
    return (
      <div data-tauri-drag-region className="flex h-full flex-col items-center justify-center gap-4 p-8 text-center">
        <Unplug size={28} strokeWidth={1.25} className="text-fg-4" />
        <div>
          <p className="font-medium text-fg-2">Cannot reach the Postflow core</p>
          {!isTauri && (
            <p className="mt-1 max-w-[360px] text-[12px] leading-relaxed text-fg-3">
              Start it with <code className="select-text font-mono text-fg-2">npm run server</code> and try again.
            </p>
          )}
        </div>
        <Button onClick={() => void status.refetch()} busy={status.isFetching}>
          Retry
        </Button>
      </div>
    );
  }

  return status.data.configured ? <Shell /> : <Onboarding status={status.data} />;
}

/** 从贴进来的访问链接里取出密钥；直接贴密钥也行。 */
function keyFrom(text: string): string | null {
  const value = text.trim();
  if (!value) return null;
  try {
    const key = new URL(value).searchParams.get("key");
    if (key) return key;
  } catch {
    // 不是链接：当作密钥本身
  }
  return /^[A-Za-z0-9]{16,}$/.test(value) ? value : null;
}

/** 网页访问需要访问密钥时的页面。 */
function AccessKeyScreen() {
  const t = useT();
  const [text, setText] = useState("");
  const key = keyFrom(text);
  return (
    <div className="flex h-full flex-col items-center justify-center gap-5 p-8 text-center">
      <Logo size={36} />
      <div className="max-w-[360px]">
        <p className="font-medium text-fg">{t("access.title")}</p>
        <p className="mt-1.5 text-[13px] leading-relaxed text-fg-3">{t("access.text")}</p>
      </div>
      <form
        className="flex w-full max-w-[360px] flex-col gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (key) window.location.href = `/?key=${encodeURIComponent(key)}`;
        }}
      >
        <TextInput
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder={t("access.placeholder")}
          aria-label={t("access.placeholder")}
          inputMode="url"
        />
        <Button variant="primary" type="submit" disabled={!key}>
          {t("access.open")}
        </Button>
      </form>
    </div>
  );
}
