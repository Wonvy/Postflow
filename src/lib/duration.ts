// 播放时长的写法。这里只有纯函数，方便测试：npm run test:ui

/** 秒数写成 1:02:03 或 12:03。 */
export function formatTime(seconds: number): string {
  const s = Number.isFinite(seconds) ? Math.max(0, Math.floor(seconds)) : 0;
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

/** 列表和阅读区里显示的总时长：1 小时 2 分、45 分钟。 */
export function formatLength(seconds: number, lang: "zh" | "en"): string {
  const minutes = Math.max(1, Math.round(seconds / 60));
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  if (lang === "zh") return h > 0 ? (m > 0 ? `${h} 小时 ${m} 分` : `${h} 小时`) : `${m} 分钟`;
  return h > 0 ? (m > 0 ? `${h} h ${m} min` : `${h} h`) : `${m} min`;
}
