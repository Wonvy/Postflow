// 在线更新：发现新版本、下载、安装、重启。只在桌面应用里有。
//
// 更新包由发布流程用私钥签名，应用里内置了对应的公钥，签名对不上的包不会被安装。

import type { Update } from "@tauri-apps/plugin-updater";
import { create } from "zustand";
import { isTauri } from "./transport";

/** 启动后等一会儿再查，不和首屏加载抢网络。 */
const FIRST_CHECK_DELAY = 8_000;
/** 应用一直开着时，隔多久再查一次。 */
const CHECK_INTERVAL = 6 * 60 * 60 * 1000;

export const RELEASES_URL = "https://github.com/Wonvy/Postflow/releases";

export type UpdatePhase =
  | "idle"
  | "checking"
  | "latest"
  | "available"
  | "downloading"
  | "installing"
  | "error";

interface UpdaterState {
  /** 这个安装方式能不能在应用里直接更新。不能的话只提示去下载页。 */
  supported: boolean;
  phase: UpdatePhase;
  version: string | null;
  /** 发布流程放进 latest.json 的更新说明（CHANGELOG.md 里最近几个版本），更新窗口里直接显示。 */
  notes: string | null;
  downloaded: number;
  total: number | null;
  error: string | null;
  dialogOpen: boolean;
  setDialogOpen: (open: boolean) => void;
}

export const useUpdater = create<UpdaterState>()((set) => ({
  supported: false,
  phase: "idle",
  version: null,
  notes: null,
  downloaded: 0,
  total: null,
  error: null,
  dialogOpen: false,
  setDialogOpen: (dialogOpen) => set({ dialogOpen }),
}));

let pending: Update | null = null;
let started = false;

function message(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/**
 * 查一次有没有新版本。`manual` 为 true 时（用户点了「检查更新」），有新版本就直接打开更新窗口、
 * 显示更新说明，失败也会显示出来；自动检查失败就安静地算了。
 */
export async function checkForUpdate(manual = false): Promise<void> {
  if (!isTauri) return;
  const { phase } = useUpdater.getState();
  if (phase === "checking" || phase === "downloading" || phase === "installing") return;
  useUpdater.setState({ phase: "checking", error: null });
  try {
    const { check } = await import("@tauri-apps/plugin-updater");
    const update = await check({ timeout: 30_000 });
    if (update) {
      pending = update;
      useUpdater.setState({
        phase: "available",
        version: update.version,
        notes: update.body?.trim() || null,
        ...(manual ? { dialogOpen: true } : {}),
      });
    } else {
      useUpdater.setState({ phase: "latest" });
    }
  } catch (err) {
    useUpdater.setState(
      manual ? { phase: "error", error: message(err) } : { phase: pending ? "available" : "idle" },
    );
  }
}

/** 下载并安装，完成后重启应用。Windows 上安装程序会自己关掉应用再装。 */
export async function installUpdate(): Promise<void> {
  if (!pending) return;
  useUpdater.setState({ phase: "downloading", downloaded: 0, total: null, error: null });
  try {
    await pending.downloadAndInstall((event) => {
      switch (event.event) {
        case "Started":
          useUpdater.setState({ total: event.data.contentLength ?? null });
          break;
        case "Progress":
          useUpdater.setState((s) => ({ downloaded: s.downloaded + event.data.chunkLength }));
          break;
        case "Finished":
          useUpdater.setState({ phase: "installing" });
          break;
      }
    });
    const { relaunch } = await import("@tauri-apps/plugin-process");
    await relaunch();
  } catch (err) {
    useUpdater.setState({ phase: "error", error: message(err) });
  }
}

/** 应用启动时调用一次：判断能否在线更新，然后定时检查。 */
export function startUpdateChecks(): void {
  if (!isTauri || started) return;
  started = true;
  void import("@tauri-apps/api/core")
    .then(({ invoke }) => invoke<boolean>("updater_supported"))
    .catch(() => false)
    .then((supported) => {
      useUpdater.setState({ supported });
      setTimeout(() => void checkForUpdate(), FIRST_CHECK_DELAY);
      setInterval(() => void checkForUpdate(), CHECK_INTERVAL);
    });
}
