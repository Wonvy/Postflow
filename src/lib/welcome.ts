// 新手引导什么时候自动弹出。界面在 components/WelcomeGuide.tsx。

/**
 * 进入主界面、侧栏数据第一次读到时，新手引导该怎么办：
 * - `open`：第一次用，还没有任何邮箱和订阅，打开引导；
 * - `mark-done`：还没看过引导，但已经有邮箱或订阅（从旧版本升级，或者数据目录里带着别的设备的数据），
 *   不需要引导，直接记成看过；
 * - `nothing`：看过或跳过了。
 */
export function welcomeOnStart(done: boolean, accounts: number, feeds: number): "open" | "mark-done" | "nothing" {
  if (done) return "nothing";
  return accounts === 0 && feeds === 0 ? "open" : "mark-done";
}
