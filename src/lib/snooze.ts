// 稍后提醒的几个预设时间。这里只有纯函数，方便测试：npm run test:ui

export type SnoozePreset = "later" | "tonight" | "tomorrow" | "weekend" | "nextWeek";

const at = (base: Date, days: number, hour: number): Date => {
  const d = new Date(base);
  d.setDate(d.getDate() + days);
  d.setHours(hour, 0, 0, 0);
  return d;
};

/**
 * 现在能选的预设时间，按先后排：
 * - 稍后：三小时后，取整到整点；
 * - 今晚 19 点（已经过了 17 点就不给，免得只隔一会儿）；
 * - 明早 8 点；
 * - 周末：下一个周六早上 9 点（今天、明天就是周六时不给，和「明早」重了）；
 * - 下周：下周一早上 8 点。
 */
export function snoozePresets(now: Date): { key: SnoozePreset; at: Date }[] {
  const later = new Date(now);
  later.setHours(later.getHours() + 3, 0, 0, 0);
  if (later.getTime() - now.getTime() < 2.5 * 3_600_000) later.setHours(later.getHours() + 1);
  const out: { key: SnoozePreset; at: Date }[] = [{ key: "later", at: later }];
  if (now.getHours() < 17) out.push({ key: "tonight", at: at(now, 0, 19) });
  out.push({ key: "tomorrow", at: at(now, 1, 8) });
  const day = now.getDay(); // 0 周日 … 6 周六
  const toSaturday = (6 - day + 7) % 7;
  if (toSaturday >= 2) out.push({ key: "weekend", at: at(now, toSaturday, 9) });
  const toMonday = ((1 - day + 7) % 7) || 7;
  out.push({ key: "nextWeek", at: at(now, toMonday, 8) });
  return out;
}

/** 自定义时间输入框（datetime-local）要的格式：本地时间的 YYYY-MM-DDTHH:mm。 */
export function toLocalInput(date: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}
