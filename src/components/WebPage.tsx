// 阅读区的「原网页」模式：在右边直接显示条目的网页，上面一条选模拟的设备。
//
// 桌面应用里用一个独立的系统网页视图叠在这块地方上（见 src-tauri/src/web_page.rs），
// 不受网站「禁止被嵌入」的限制，还能换 User-Agent。浏览器里（以及手机应用里）只能用 iframe：
// 先让核心看一眼网站许不许嵌入，不许的直接说明原因，给出在浏览器中打开的办法。

import { useQuery } from "@tanstack/react-query";
import {
  ExternalLink,
  type LucideIcon,
  Monitor,
  RefreshCw,
  RotateCw,
  ShieldOff,
  Smartphone,
  Tablet,
  TabletSmartphone,
} from "lucide-react";
import { type ReactNode, useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { useT } from "../lib/i18n";
import { isMac, openExternal } from "../lib/platform";
import { useUi } from "../lib/store";
import { isTauri } from "../lib/transport";
import { DEVICES, desktopUserAgent, devicePreset, type Frame, fitFrame, type WebDevice } from "../lib/webDevice";
import { Button, cx, IconButton, Spinner, Tip } from "./ui";

/** 手机上的应用没有独立网页视图可用，和浏览器一样走 iframe。 */
const nativeView = isTauri && !/iPhone|iPad|Android/i.test(navigator.userAgent);

const DEVICE_ICON: Record<WebDevice, LucideIcon> = {
  desktop: Monitor,
  ipad: Tablet,
  iphone: Smartphone,
  android: TabletSmartphone,
};

/** 模拟设备时四周留的空。 */
const DEVICE_MARGIN = 16;

function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

export function WebPage({ url, onShowContent }: { url: string; onShowContent: () => void }) {
  const t = useT();
  const deviceId = useUi((s) => s.webDevice);
  const landscape = useUi((s) => s.webLandscape);
  const setDevice = useUi((s) => s.setWebDevice);
  const setLandscape = useUi((s) => s.setWebLandscape);
  const device = devicePreset(deviceId);
  const userAgent = device.id === "desktop" ? desktopUserAgent(isMac) : device.userAgent;

  // 放网页的这块地方有多大
  const area = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  useLayoutEffect(() => {
    const el = area.current;
    if (!el) return;
    const update = () => setSize({ width: el.clientWidth, height: el.clientHeight });
    update();
    const observer = new ResizeObserver(update);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  const margin = device.size ? DEVICE_MARGIN : 0;
  const frame = fitFrame(device, landscape, {
    width: size.width - margin * 2,
    height: size.height - margin * 2,
  });

  const [reloads, setReloads] = useState(0);
  const [loading, setLoading] = useState(true);
  const stopLoading = useCallback(() => setLoading(false), []);
  const [shownUrl, setShownUrl] = useState(url);
  useEffect(() => {
    setShownUrl(url);
    setLoading(true);
  }, [url]);

  return (
    <div className="@container/web flex min-h-0 min-w-0 flex-1 flex-col">
      <div className="flex h-9 shrink-0 items-center gap-1 border-b border-line px-2">
        <div role="radiogroup" aria-label={t("web.devices")} className="inline-flex rounded-md border border-line p-px">
          {DEVICES.map((d) => {
            const Icon = DEVICE_ICON[d.id];
            const label = t(`web.${d.id}`);
            return (
              // 提示往上弹：往下会落在桌面应用的网页视图后面，看不见
              <Tip
                key={d.id}
                side="top"
                label={
                  d.size
                    ? `${label} · ${d.size.width}×${d.size.height}${nativeView ? "" : ` — ${t("web.uaNote")}`}`
                    : label
                }
              >
                <button
                  type="button"
                  role="radio"
                  aria-checked={device.id === d.id}
                  aria-label={label}
                  onClick={() => setDevice(d.id)}
                  className={cx(
                    "inline-flex h-6 items-center gap-1 rounded-[5px] px-1.5 text-[12px] transition-colors",
                    device.id === d.id ? "bg-active text-fg" : "text-fg-3 hover:text-fg-2",
                  )}
                >
                  <Icon size={14} strokeWidth={1.75} />
                  <span className="hidden @[560px]/web:inline">{label}</span>
                </button>
              </Tip>
            );
          })}
        </div>
        {device.size && (
          <IconButton
            label={t("web.rotate")}
            tipSide="top"
            active={landscape}
            className="size-6"
            onClick={() => setLandscape(!landscape)}
          >
            <RotateCw size={14} strokeWidth={1.75} />
          </IconButton>
        )}
        <Tip side="top" label={shownUrl}>
          <span className="tabular ml-1 min-w-0 flex-1 truncate text-center text-[12px] text-fg-3">
            {hostOf(shownUrl)}
            {device.size && (
              <span className="ml-2 text-fg-4">
                {frame.width}×{frame.height}
                {frame.scale < 1 && ` · ${Math.round(frame.scale * 100)}%`}
              </span>
            )}
          </span>
        </Tip>
        {loading && <Spinner size={13} />}
        <IconButton
          label={t("web.reload")}
          tipSide="top"
          className="size-6"
          onClick={() => {
            setLoading(true);
            setReloads((n) => n + 1);
          }}
        >
          <RefreshCw size={14} strokeWidth={1.75} />
        </IconButton>
        <IconButton
          label={t("action.openOriginal")}
          tipSide="top"
          className="size-6"
          onClick={() => void openExternal(shownUrl)}
        >
          <ExternalLink size={14} strokeWidth={1.75} />
        </IconButton>
      </div>

      <div
        ref={area}
        className={cx("relative flex min-h-0 flex-1 items-center justify-center overflow-hidden", device.size && "bg-hover")}
      >
        {size.width > 0 &&
          (nativeView ? (
            <NativeFrame
              url={url}
              userAgent={userAgent}
              frame={frame}
              device={Boolean(device.size)}
              reloads={reloads}
              onLoading={(busy, at) => {
                setLoading(busy);
                if (at) setShownUrl(at);
              }}
            />
          ) : (
            <IframeFrame
              url={url}
              frame={frame}
              device={Boolean(device.size)}
              reloads={reloads}
              onLoaded={stopLoading}
              onShowContent={onShowContent}
            />
          ))}
      </div>
    </div>
  );
}

/** 模拟设备时外面的那圈圆角边框；桌面直接铺满。 */
function Screen({ frame, device, children }: { frame: Frame; device: boolean; children?: ReactNode }) {
  return (
    <div
      className={cx("relative shrink-0 overflow-hidden bg-white", device && "rounded-[14px] shadow-pop ring-1 ring-line-strong")}
      style={{ width: Math.round(frame.width * frame.scale), height: Math.round(frame.height * frame.scale) }}
    >
      {children}
    </div>
  );
}

function IframeFrame({
  url,
  frame,
  device,
  reloads,
  onLoaded,
  onShowContent,
}: {
  url: string;
  frame: Frame;
  device: boolean;
  reloads: number;
  onLoaded: () => void;
  onShowContent: () => void;
}) {
  const t = useT();
  const check = useQuery({
    queryKey: ["web-check", url],
    queryFn: () => api.webCheck(url, window.location.href),
    staleTime: 10 * 60_000,
    retry: false,
  });
  const blocked = check.data?.embeddable === false;
  // 不让嵌入的网页不会加载，上面的转圈要停
  useEffect(() => {
    if (blocked) onLoaded();
  }, [blocked, onLoaded]);

  if (check.isPending) {
    return (
      <div className="flex flex-col items-center gap-2 text-[12px] text-fg-3">
        <Spinner size={16} />
        {t("web.checking")}
      </div>
    );
  }
  // 查不了（网络出错之类）就直接试着嵌入，让 iframe 自己显示错误
  if (check.data && blocked) {
    return (
      <Blocked
        host={hostOf(check.data.final_url || url)}
        reason={check.data.reason ?? ""}
        onOpen={() => void openExternal(url)}
        onShowContent={onShowContent}
      />
    );
  }
  return (
    <Screen frame={frame} device={device}>
      <iframe
        key={reloads}
        title={hostOf(url)}
        src={url}
        // 网页在自己的源里运行，不能跳转顶层窗口，弹出的新窗口不受限制
        sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-popups-to-escape-sandbox allow-downloads"
        referrerPolicy="no-referrer"
        onLoad={onLoaded}
        className="absolute left-0 top-0 block border-0 bg-white"
        style={{
          width: frame.width,
          height: frame.height,
          transform: frame.scale < 1 ? `scale(${frame.scale})` : undefined,
          transformOrigin: "0 0",
        }}
      />
    </Screen>
  );
}

function Blocked({
  host,
  reason,
  onOpen,
  onShowContent,
}: {
  host: string;
  reason: string;
  onOpen: () => void;
  onShowContent: () => void;
}) {
  const t = useT();
  return (
    <div className="flex max-w-[420px] flex-col items-center gap-3 px-8 text-center">
      <ShieldOff size={26} strokeWidth={1.5} className="text-fg-4" />
      <div>
        <p className="font-medium text-fg-2">{t("web.blockedTitle")}</p>
        <p className="mt-1.5 text-[12px] leading-relaxed text-fg-3">{t("web.blockedBody", { host, reason })}</p>
        <p className="mt-1.5 text-[12px] leading-relaxed text-fg-4">{t("web.blockedDesktop")}</p>
      </div>
      <div className="flex gap-2">
        <Button variant="primary" onClick={onOpen}>
          <ExternalLink size={14} />
          {t("action.openOriginal")}
        </Button>
        <Button onClick={onShowContent}>{t("web.showContent")}</Button>
      </div>
    </div>
  );
}

/** 页面上弹出的菜单、对话框。它们画在网页里，盖不住叠在上面的系统网页视图，弹出时要先把它收起来。 */
const OVERLAYS = "[role=menu],[role=dialog]";

function overlapsOverlay(rect: DOMRect): boolean {
  return Array.from(document.querySelectorAll(OVERLAYS)).some((el) => {
    const r = el.getBoundingClientRect();
    return r.right > rect.left && r.left < rect.right && r.bottom > rect.top && r.top < rect.bottom;
  });
}

/**
 * 对系统网页视图的调用一个接一个地发：换条目时旧的阅读区先收起、新的再显示，
 * 两个调用要是顺序反了，网页就被收起来了。
 */
let queue: Promise<unknown> = Promise.resolve();
function call(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  queue = queue
    .then(async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      return invoke(cmd, args);
    })
    .catch(() => undefined);
  return queue;
}

type Bounds = { x: number; y: number; width: number; height: number; zoom: number };
const sameBounds = (a: Bounds | null, b: Bounds) =>
  a !== null && a.x === b.x && a.y === b.y && a.width === b.width && a.height === b.height && a.zoom === b.zoom;

/**
 * 桌面应用：在这块地方占个位置，把它在窗口里的位置和大小交给系统网页视图。
 * 阅读区大小变了、弹出菜单盖到这里了、离开原网页模式了，都同步过去。
 */
function NativeFrame({
  url,
  userAgent,
  frame,
  device,
  reloads,
  onLoading,
}: {
  url: string;
  userAgent: string | null;
  frame: Frame;
  device: boolean;
  reloads: number;
  onLoading: (loading: boolean, url?: string) => void;
}) {
  const slot = useRef<HTMLDivElement>(null);
  /** 系统网页视图上次打开的地址，和上次告诉它的位置。 */
  const shown = useRef<{ url: string; userAgent: string | null } | null>(null);
  const placed = useRef<Bounds | null>(null);
  const [covered, setCovered] = useState(false);
  const coveredRef = useRef(covered);
  coveredRef.current = covered;
  const onLoadingRef = useRef(onLoading);
  onLoadingRef.current = onLoading;
  const scale = useRef(frame.scale);
  scale.current = frame.scale;

  const bounds = (): Bounds | null => {
    const r = slot.current?.getBoundingClientRect();
    if (!r || r.width === 0) return null;
    return { x: r.left, y: r.top, width: r.width, height: r.height, zoom: scale.current };
  };

  /** 位置有变化时告诉系统网页视图。`show` 时顺便把收起的它显示出来。 */
  const place = (show = false) => {
    const next = bounds();
    if (!next || coveredRef.current || (!show && sameBounds(placed.current, next))) return;
    placed.current = next;
    void call("web_frame", { frame: next, show });
  };

  // 加载状态和当前地址
  useEffect(() => {
    let off: (() => void) | undefined;
    let cancelled = false;
    void import("@tauri-apps/api/event").then(({ listen }) =>
      listen<{ url: string; loading: boolean }>("postflow:web", (e) =>
        onLoadingRef.current(e.payload.loading, e.payload.url),
      ).then((fn) => {
        if (cancelled) fn();
        else off = fn;
      }),
    );
    return () => {
      cancelled = true;
      off?.();
    };
  }, []);

  // 打开、换地址、换设备；被菜单盖住时收起，菜单关了再显示（不重新加载）
  useEffect(() => {
    if (covered) {
      void call("web_hide");
      return;
    }
    const next = bounds();
    if (!next) return;
    if (shown.current?.url === url && shown.current.userAgent === userAgent) {
      place(true);
      return;
    }
    shown.current = { url, userAgent };
    placed.current = next;
    onLoadingRef.current(true);
    void call("web_show", { url, userAgent, frame: next });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [url, userAgent, covered]);

  // 布局变了（拖动分栏、开关 AI 栏、缩放窗口、换设备）就跟着挪
  useLayoutEffect(() => place());
  useEffect(() => {
    const el = slot.current;
    if (!el) return;
    let raf = 0;
    const sync = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => place());
    };
    const observer = new ResizeObserver(sync);
    observer.observe(el);
    window.addEventListener("resize", sync);
    return () => {
      cancelAnimationFrame(raf);
      observer.disconnect();
      window.removeEventListener("resize", sync);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 菜单、对话框盖到这里时收起
  useEffect(() => {
    const check = () => {
      const r = slot.current?.getBoundingClientRect();
      setCovered(r ? overlapsOverlay(r) : false);
    };
    const observer = new MutationObserver(check);
    observer.observe(document.body, { childList: true });
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (reloads > 0) void call("web_reload");
  }, [reloads]);

  // 离开原网页模式、换到别的条目
  useEffect(
    () => () => {
      void call("web_hide");
    },
    [],
  );

  return (
    <Screen frame={frame} device={device}>
      <div ref={slot} className="absolute inset-0" />
    </Screen>
  );
}
