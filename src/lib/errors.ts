// 界面里没接住的错误也记进日志，排查问题时能看到。同样的错误一分钟内只记一次，最多记 20 条。

import { api } from "./api";

const seen = new Map<string, number>();
let budget = 20;
let budgetReset = Date.now();

function report(message: string) {
  const now = Date.now();
  if (now - budgetReset > 60_000) {
    budget = 20;
    budgetReset = now;
  }
  const last = seen.get(message);
  if (budget <= 0 || (last !== undefined && now - last < 60_000)) return;
  seen.set(message, now);
  budget--;
  api.log("error", message).catch(() => {
    // 连核心都连不上时没有别的地方可记
  });
}

export function captureErrors() {
  window.addEventListener("error", (e) => {
    const where = e.filename ? ` (${e.filename.replace(/^.*\//, "")}:${e.lineno}:${e.colno})` : "";
    report(`${e.message}${where}`);
  });
  window.addEventListener("unhandledrejection", (e) => {
    const reason = e.reason;
    const text = reason instanceof Error ? `${reason.name}: ${reason.message}` : String(reason);
    report(`unhandled promise rejection: ${text}`);
  });
}
