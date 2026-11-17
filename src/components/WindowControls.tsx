// Windows 桌面应用右上角的最小化、最大化（还原）、关闭。系统标题栏在 src-tauri/src/lib.rs 里去掉了。
//
// 样式照着 Windows 11：每个按钮 46px 宽，关闭按钮悬停时变红。
// 按钮下面那一栏的顶栏用 useWindowControlsGap 给它们留位置。

import { useEffect, useState } from "react";
import { useFullscreen } from "../lib/fullscreen";
import { useT } from "../lib/i18n";
import { hasWindowControls, WINDOW_CONTROLS_WIDTH } from "../lib/platform";
import { isTauri } from "../lib/transport";
import { cx } from "./ui";

async function currentWindow() {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}

export function WindowControls() {
  const t = useT();
  const [maximized, setMaximized] = useState(false);
  const fullscreen = useFullscreen((s) => s.on);

  useEffect(() => {
    if (!hasWindowControls || !isTauri) return;
    let unlisten: (() => void) | undefined;
    let alive = true;
    void currentWindow().then(async (win) => {
      const update = () => void win.isMaximized().then((m) => alive && setMaximized(m));
      update();
      const stop = await win.onResized(update);
      if (alive) unlisten = stop;
      else stop();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  // 全屏时整个屏幕都给内容，窗口按钮不显示（Esc 或全屏按钮退出）
  if (!hasWindowControls || fullscreen) return null;

  const run = (action: "minimize" | "toggleMaximize" | "close") => {
    if (!isTauri) return;
    void currentWindow().then((win) => win[action]());
  };

  return (
    <div
      className="fixed right-0 top-0 z-[45] flex h-11"
      style={{ width: WINDOW_CONTROLS_WIDTH }}
      // 双击按钮不要触发窗口最大化
      onDoubleClick={(e) => e.stopPropagation()}
    >
      <Caption label={t("window.minimize")} onClick={() => run("minimize")}>
        <path d="M0 5.5h10" />
      </Caption>
      <Caption
        label={maximized ? t("window.restore") : t("window.maximize")}
        onClick={() => run("toggleMaximize")}
      >
        {maximized ? (
          <>
            <rect x="0.5" y="2.5" width="7" height="7" rx="1" />
            <path d="M2.5 2.5V1.5a1 1 0 0 1 1-1h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-1" />
          </>
        ) : (
          <rect x="0.5" y="0.5" width="9" height="9" rx="1" />
        )}
      </Caption>
      <Caption label={t("window.close")} danger onClick={() => run("close")}>
        <path d="M0.5 0.5l9 9M9.5 0.5l-9 9" />
      </Caption>
    </div>
  );
}

function Caption({
  label,
  danger = false,
  onClick,
  children,
}: {
  label: string;
  danger?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className={cx(
        "flex h-full w-[46px] items-center justify-center text-fg-2 transition-colors",
        danger ? "hover:bg-[#c42b1c] hover:text-white active:bg-[#c42b1c]/85" : "hover:bg-hover active:bg-active",
      )}
    >
      <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1" aria-hidden>
        {children}
      </svg>
    </button>
  );
}

/** 这一栏的顶栏在窗口最右边时，右边要空出窗口按钮的宽度。返回要加的右内边距（px），不需要时为 0。 */
export function useWindowControlsGap(rightmost: boolean): number {
  return hasWindowControls && rightmost ? WINDOW_CONTROLS_WIDTH : 0;
}
