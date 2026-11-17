import { Unplug } from "lucide-react";
import { type PointerEvent, useEffect, useRef, useState } from "react";
import { AddAccountDialog, EditAccountDialog, RemoveAccountDialog } from "./components/AccountDialogs";
import { AddFeedDialog, EditFeedDialog } from "./components/FeedDialogs";
import { ItemList } from "./components/ItemList";
import { Onboarding } from "./components/Onboarding";
import { Reader } from "./components/Reader";
import { SettingsDialog } from "./components/SettingsDialog";
import { UpdateDialog } from "./components/UpdateDialog";
import { Sidebar } from "./components/Sidebar";
import { StatsPage } from "./components/StatsPage";
import { Button, Spinner } from "./components/ui";
import { api } from "./lib/api";
import { resolveLang } from "./lib/i18n";
import { useCoreEvents, useStatus } from "./lib/queries";
import { useShortcuts } from "./lib/shortcuts";
import { LIST_MIN, READER_MIN, useUi } from "./lib/store";
import { isTauri } from "./lib/transport";
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
      void api.syncAccount().catch(() => {});
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
  const lang = useUi((s) => s.lang);

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const apply = () => {
      const resolved = theme === "system" ? (media.matches ? "light" : "dark") : theme;
      document.documentElement.dataset.theme = resolved;
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme]);

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
      className="group relative z-10 -mx-[4px] w-[9px] shrink-0 cursor-col-resize"
    >
      <div className="absolute inset-y-0 left-[4px] w-px bg-transparent transition-colors group-hover:bg-accent group-active:bg-accent" />
    </div>
  );
}

function Shell() {
  useShortcuts();
  useMailCatchUp();
  useEffect(() => startUpdateChecks(), []);
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

  return (
    <div className="flex h-full">
      <Sidebar width={sidebarWidth} />
      <Resizer width={sidebarWidth} onResize={setSidebarWidth} />
      {mainView === "stats" ? (
        <StatsPage />
      ) : (
        <>
          <ItemList width={listShown} />
          <Resizer width={listShown} onResize={(w) => setListWidth(Math.min(w, listMax))} />
          <Reader />
        </>
      )}
      <AddFeedDialog />
      <EditFeedDialog />
      <AddAccountDialog />
      <EditAccountDialog />
      <RemoveAccountDialog />
      <UpdateDialog />
      <SettingsDialog />
    </div>
  );
}

export default function App() {
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
