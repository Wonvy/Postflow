// 阅读区「原网页」模式里模拟的设备：视口大小和 User-Agent。
//
// 桌面应用用一个独立的网页视图显示原网页，两样都能改；浏览器里只能用 iframe，只能改宽高。
// 这里只有纯函数，方便测试：npm run test:ui

export type WebDevice = "desktop" | "ipad" | "iphone" | "android";

export interface DevicePreset {
  id: WebDevice;
  /** 竖着拿时的视口大小（CSS 像素）。桌面没有固定大小，铺满阅读区。 */
  size: { width: number; height: number } | null;
  /** 为空时用网页视图自己的。 */
  userAgent: string | null;
}

const SAFARI_MAC =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15";

export const DEVICES: DevicePreset[] = [
  { id: "desktop", size: null, userAgent: null },
  {
    id: "ipad",
    size: { width: 820, height: 1180 },
    userAgent:
      "Mozilla/5.0 (iPad; CPU OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Mobile/15E148 Safari/604.1",
  },
  {
    id: "iphone",
    size: { width: 393, height: 852 },
    userAgent:
      "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Mobile/15E148 Safari/604.1",
  },
  {
    id: "android",
    size: { width: 412, height: 915 },
    userAgent:
      "Mozilla/5.0 (Linux; Android 15; Pixel 9) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Mobile Safari/537.36",
  },
];

export function devicePreset(id: unknown): DevicePreset {
  return DEVICES.find((d) => d.id === id) ?? (DEVICES[0] as DevicePreset);
}

/**
 * 桌面预设的 User-Agent。macOS 上系统网页视图默认的 User-Agent 不带 Safari 字样，
 * 有些网站会因此给出旧版页面，所以换成 Safari 的；其他系统用网页视图自己的。
 */
export function desktopUserAgent(mac: boolean): string | null {
  return mac ? SAFARI_MAC : null;
}

export interface Frame {
  /** 网页看到的视口大小。 */
  width: number;
  height: number;
  /** 放进阅读区时缩小的比例，放得下时是 1。 */
  scale: number;
}

/**
 * 在 `area` 这么大的地方显示一个设备的页面。放不下就整体等比缩小，网页看到的视口不变。
 * 桌面预设直接铺满。`landscape` 把宽高对调。
 */
export function fitFrame(
  device: DevicePreset,
  landscape: boolean,
  area: { width: number; height: number },
): Frame {
  const available = { width: Math.max(1, Math.floor(area.width)), height: Math.max(1, Math.floor(area.height)) };
  if (!device.size) return { ...available, scale: 1 };
  const width = landscape ? device.size.height : device.size.width;
  const height = landscape ? device.size.width : device.size.height;
  const scale = Math.min(1, available.width / width, available.height / height);
  return { width, height, scale: Math.floor(scale * 1000) / 1000 };
}

/** 只认 http 和 https 的链接，别的（javascript:、file: 之类）一律不打开。 */
export function isWebUrl(url: string | null | undefined): url is string {
  if (!url) return false;
  try {
    const parsed = new URL(url);
    return parsed.protocol === "http:" || parsed.protocol === "https:";
  } catch {
    return false;
  }
}
