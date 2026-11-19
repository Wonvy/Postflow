// 重要联系人名单的纯函数（不依赖界面），见 vip.ts。用 Node 自带的测试工具测：npm run test:ui

export const VIP_SETTING = "vip_senders";

/** 设置里存的地址表（小写、去重）。 */
export function vipList(settings: Record<string, unknown> | undefined): string[] {
  const raw = settings?.[VIP_SETTING];
  return Array.isArray(raw) ? [...new Set(raw.filter((a): a is string => typeof a === "string").map((a) => a.trim().toLowerCase()))] : [];
}

/** 新邮件里哪些是重要联系人发来的。 */
export function vipOnly<T extends { address?: string | null }>(mails: readonly T[], vip: readonly string[]): T[] {
  const set = new Set(vip);
  return mails.filter((m) => !!m.address && set.has(m.address.trim().toLowerCase()));
}
