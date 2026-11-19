// 界面与核心之间的通道。
// 桌面应用里走进程内调用，普通浏览器里走 HTTP，其余代码不需要知道是哪一种。

export type Scope = "feeds" | "items" | "settings" | "devices";

export type CoreEvent =
  | { type: "changed"; scope: Scope[]; source?: string }
  /** `active` 是正在刷新的订阅源和邮箱总数，`mail` 是其中的邮箱数（旧版核心没有这个字段）。 */
  | { type: "refresh"; active: number; mail?: number }
  | {
      type: "new_mail";
      account_id: string;
      mails: { id: string; from: string; subject: string; date: number }[];
      total: number;
    }
  | {
      type: "reminders";
      items: { id: string; kind: "mail" | "article"; title: string; source_title: string }[];
      total: number;
    }
  | { type: "status" }
  | {
      type: "model";
      pair: string;
      received: number;
      total: number;
      state: "downloading" | "installed" | "failed";
      error?: string;
    }
  | {
      /** 语音识别模型的下载进度。 */
      type: "speech_model";
      id: string;
      received: number;
      total: number;
      state: "downloading" | "installed" | "failed";
      error?: string;
    }
  | {
      /** 一期节目的字幕有了进展：新转出来的几句（落在 from–to 里的旧句子由它们替换），和现在的状态。 */
      type: "subtitles";
      id: string;
      state: "idle" | "downloading" | "loading" | "running" | "failed";
      cues: { start: number; end: number; text: string }[];
      from?: number;
      to?: number;
      covered: [number, number][];
      duration: number | null;
      complete: boolean;
      received: number;
      total: number | null;
      error?: string;
      message?: string;
    }
  | {
      type: "ai";
      request_id: string;
      /** 流式输出的一段文字。 */
      delta?: string;
      done: boolean;
      result?: Record<string, unknown>;
      error?: string;
      message?: string;
    };

export class RpcError extends Error {
  code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = "RpcError";
    this.code = code;
  }
}

/** 标识这个界面实例。核心把它带在变更事件里，自己引发的变更可以不必重复刷新。 */
export const clientId: string =
  typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : Math.random().toString(36).slice(2);

export const isTauri = "__TAURI_INTERNALS__" in window;

const ALL_CHANGED: CoreEvent = {
  type: "changed",
  scope: ["feeds", "items", "settings", "devices"],
};

function toRpcError(err: unknown): RpcError {
  if (err instanceof RpcError) return err;
  if (err && typeof err === "object" && "code" in err && "message" in err) {
    const e = err as { code: unknown; message: unknown };
    return new RpcError(String(e.code), String(e.message));
  }
  return new RpcError("network", err instanceof Error ? err.message : String(err));
}

async function tauriRpc<T>(method: string, params: unknown): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<T>("rpc", { method, params: params ?? null, client: clientId });
  } catch (err) {
    throw toRpcError(err);
  }
}

async function httpRpc<T>(method: string, params: unknown): Promise<T> {
  let response: Response;
  try {
    response = await fetch("/api/rpc", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ method, params: params ?? null, client: clientId }),
    });
  } catch (err) {
    throw toRpcError(err);
  }
  let body: { result?: T; error?: { code: string; message: string } } | null = null;
  try {
    body = await response.json();
  } catch {
    // 不是 JSON，多半是代理或服务没起来
  }
  if (body?.error) throw new RpcError(body.error.code, body.error.message);
  if (!response.ok || !body) {
    throw new RpcError("network", `HTTP ${response.status}`);
  }
  return body.result as T;
}

export function rpc<T>(method: string, params?: unknown): Promise<T> {
  return isTauri ? tauriRpc<T>(method, params) : httpRpc<T>(method, params);
}

/** 订阅核心事件，返回取消订阅的函数。 */
export function subscribe(handler: (event: CoreEvent) => void): () => void {
  if (isTauri) {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/event").then(({ listen }) =>
      listen<CoreEvent>("postflow:event", (e) => handler(e.payload)).then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      }),
    );
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }

  const source = new EventSource("/api/events");
  let dropped = false;
  source.onmessage = (e) => {
    try {
      handler(JSON.parse(e.data) as CoreEvent);
    } catch {
      // 忽略无法解析的事件
    }
  };
  source.onerror = () => {
    dropped = true;
  };
  source.onopen = () => {
    // 断线期间可能漏了事件，重连后把所有东西重新查一遍
    if (dropped) {
      dropped = false;
      handler(ALL_CHANGED);
      handler({ type: "status" });
    }
  };
  return () => source.close();
}
