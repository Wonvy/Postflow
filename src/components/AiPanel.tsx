// 阅读面板右侧的 AI 栏：这一篇的总结和关键词，以及围绕它的问答。

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowUp, Copy, RotateCcw, Settings, Sparkles, Square, Trash2, X } from "lucide-react";
import { type KeyboardEvent, useEffect, useRef, useState } from "react";
import { useAiStream } from "../lib/ai";
import { type AiMessage, api, type ItemDetail } from "../lib/api";
import { errorDetail, errorText, useLang, useT } from "../lib/i18n";
import { copyText } from "../lib/platform";
import { useUi } from "../lib/store";
import { useAiProviders } from "./AiSettings";
import { Markdown } from "./Markdown";
import { Button, cx, IconButton, Spinner } from "./ui";

/** 核心返回的错误，或者流式请求里带回来的错误码。 */
type Failure = unknown;


export function AiPanel({ item, autoRun }: { item: ItemDetail; autoRun: boolean }) {
  const t = useT();
  const lang = useLang();
  const qc = useQueryClient();
  const setDialog = useUi((s) => s.setDialog);
  const toggle = useUi((s) => s.toggleAiPanel);
  const providers = useAiProviders();
  const list = providers.data?.providers ?? [];
  const main = list.find((p) => p.id === providers.data?.default) ?? list[0];
  const configured = list.length > 0;

  const resultsKey = ["ai", "results", item.id, lang];
  const results = useQuery({
    queryKey: resultsKey,
    queryFn: () => api.aiResults(item.id, lang),
    enabled: configured,
  });

  // ---- 总结和关键词 ----
  const [summaryReq, setSummaryReq] = useState<string | null>(null);
  const [keywordsReq, setKeywordsReq] = useState<string | null>(null);
  const [runError, setRunError] = useState<Failure | null>(null);
  const summaryStream = useAiStream(summaryReq);
  const keywordsStream = useAiStream(keywordsReq);
  const running = Boolean(
    (summaryReq && !summaryStream?.done) || (keywordsReq && !keywordsStream?.done),
  );

  const run = async (force: boolean) => {
    setRunError(null);
    const [s, k] = await Promise.allSettled([
      api.aiRun(item.id, "summary", lang, force),
      api.aiRun(item.id, "keywords", lang, force),
    ]);
    if (s.status === "fulfilled") {
      if (s.value.request_id) setSummaryReq(s.value.request_id);
    } else setRunError(s.reason);
    if (k.status === "fulfilled") {
      if (k.value.request_id) setKeywordsReq(k.value.request_id);
    } else if (s.status === "fulfilled") setRunError(k.reason);
    void qc.invalidateQueries({ queryKey: resultsKey });
  };

  // 用户在这一篇上点了 AI：还没有总结就直接开始。用 J/K 翻到别的条目时不自动花钱
  const started = useRef(false);
  useEffect(() => {
    if (started.current || !autoRun || !configured || !results.isSuccess) return;
    started.current = true;
    if (!results.data.summary) void run(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoRun, configured, results.isSuccess]);

  useEffect(() => {
    if (summaryStream?.done || keywordsStream?.done) void qc.invalidateQueries({ queryKey: resultsKey });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [summaryStream?.done, keywordsStream?.done]);

  const summary = summaryStream && !summaryStream.error ? summaryStream.text : results.data?.summary?.content;
  const streamedKeywords =
    keywordsStream?.done && !keywordsStream.error
      ? (keywordsStream.result?.result as { content?: string } | undefined)?.content
      : undefined;
  const keywordsRaw = streamedKeywords ?? results.data?.keywords?.content;
  let keywords: string[] = [];
  try {
    keywords = keywordsRaw ? (JSON.parse(keywordsRaw) as string[]) : [];
  } catch {
    keywords = [];
  }
  const streamError = summaryStream?.error
    ? { code: summaryStream.error, message: summaryStream.message ?? "" }
    : keywordsStream?.error && keywordsStream.error !== "ai_cancelled"
      ? { code: keywordsStream.error, message: keywordsStream.message ?? "" }
      : null;
  const error = runError ?? streamError;

  // ---- 问答 ----
  const chatKey = ["ai", "chat", item.id];
  const history = useQuery({
    queryKey: chatKey,
    queryFn: () => api.aiChatHistory(item.id).then((r) => r.messages),
    enabled: configured,
  });
  const [chatReq, setChatReq] = useState<string | null>(null);
  const chatStream = useAiStream(chatReq);
  const [chatError, setChatError] = useState<Failure | null>(null);
  const [draft, setDraft] = useState("");
  const scroller = useRef<HTMLDivElement>(null);
  const answering = Boolean(chatReq && !chatStream?.done);

  useEffect(() => {
    if (!chatReq || !chatStream?.done) return;
    if (chatStream.error) {
      if (chatStream.error !== "ai_cancelled") setChatError({ code: chatStream.error, message: chatStream.message ?? "" });
      setChatReq(null);
      void qc.invalidateQueries({ queryKey: chatKey });
      return;
    }
    // 等记录刷新以后再撤掉正在输出的那一条，免得闪一下
    void qc.invalidateQueries({ queryKey: chatKey }).then(() => setChatReq(null));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chatStream?.done]);

  // 新内容出来时滚到底
  useEffect(() => {
    const el = scroller.current;
    if (el && (chatReq || history.data)) el.scrollTop = el.scrollHeight;
  }, [chatStream?.text, history.data, chatReq]);

  const send = async (text: string) => {
    const question = text.trim();
    if (!question || answering) return;
    setChatError(null);
    setDraft("");
    try {
      const r = await api.aiChat(item.id, question, lang);
      qc.setQueryData<AiMessage[]>(chatKey, (old) => [...(old ?? []), r.message]);
      setChatReq(r.request_id);
    } catch (err) {
      setDraft(question);
      setChatError(err);
    }
  };

  const onKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      void send(draft);
    }
  };

  const quick = [
    t("ai.q.points"),
    t("ai.q.todo"),
    t("ai.q.explain"),
    ...(item.kind === "mail" ? [t("ai.q.reply")] : []),
  ];
  const messages = history.data ?? [];

  return (
    // 手机上盖住阅读区占满宽度，关掉回到正文
    <aside className="flex h-full w-[360px] shrink-0 flex-col border-l border-line bg-sidebar max-md:absolute max-md:inset-0 max-md:z-20 max-md:w-full max-md:border-l-0">
      <header className="flex h-11 shrink-0 items-center gap-2 border-b border-line pl-3.5 pr-1.5">
        <Sparkles size={15} strokeWidth={1.75} className="text-accent" />
        <span className="font-medium">{t("ai.panel")}</span>
        {main && (
          <span className="min-w-0 flex-1 truncate text-[12px] text-fg-3">
            {main.name} · {main.model}
          </span>
        )}
        {!main && <span className="flex-1" />}
        <IconButton label={t("action.close")} shortcut="A" onClick={toggle}>
          <X size={15} strokeWidth={1.75} />
        </IconButton>
      </header>

      {!configured ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 px-8 text-center">
          <Sparkles size={26} strokeWidth={1.25} className="text-fg-4" />
          <div>
            <p className="font-medium text-fg-2">{t("ai.setup")}</p>
            <p className="mt-1 text-[12px] leading-relaxed text-fg-3">{t("ai.setupHint")}</p>
          </div>
          <Button variant="primary" onClick={() => setDialog({ type: "settings", page: "ai" })}>
            <Settings size={14} />
            {t("ai.openSettings")}
          </Button>
        </div>
      ) : (
        <>
          <div ref={scroller} className="min-h-0 flex-1 overflow-y-auto px-4 pb-4 pt-3">
            <section>
              <div className="mb-2 flex h-6 items-center gap-1">
                <h2 className="flex-1 text-[12px] font-medium text-fg-3">{t("ai.summary")}</h2>
                {summary && !running && (
                  <>
                    <IconButton label={t("ai.copy")} onClick={() => void copyText(summary)}>
                      <Copy size={13} strokeWidth={1.75} />
                    </IconButton>
                    <IconButton label={t("ai.regenerate")} onClick={() => void run(true)}>
                      <RotateCcw size={13} strokeWidth={1.75} />
                    </IconButton>
                  </>
                )}
              </div>
              {summary ? (
                <div className="select-text text-[13px] leading-relaxed text-fg">
                  <Markdown text={summary} />
                  {summaryReq && !summaryStream?.done && <span className="ml-0.5 inline-block h-3.5 w-1.5 animate-pulse bg-accent align-middle" />}
                </div>
              ) : running ? (
                <p className="flex items-center gap-2 text-[12px] text-fg-3">
                  <Spinner size={13} />
                  {t("ai.thinking")}
                </p>
              ) : (
                <Button onClick={() => void run(false)}>
                  <Sparkles size={14} />
                  {t("ai.summarize")}
                </Button>
              )}
              {keywords.length > 0 && (
                <div className="mt-3 flex flex-wrap gap-1.5">
                  {keywords.map((k) => (
                    <span key={k} className="rounded-full border border-line bg-surface px-2 py-0.5 text-[12px] text-fg-2">
                      {k}
                    </span>
                  ))}
                </div>
              )}
              {error !== null && <AiError error={error} onSettings={() => setDialog({ type: "settings", page: "ai" })} />}
            </section>

            <section className="mt-5 border-t border-line pt-3">
              <div className="mb-2 flex h-6 items-center gap-1">
                <h2 className="flex-1 text-[12px] font-medium text-fg-3">{t("ai.chat")}</h2>
                {messages.length > 0 && !answering && (
                  <IconButton
                    label={t("ai.clear")}
                    onClick={() =>
                      void api.aiChatClear(item.id).then(() => qc.invalidateQueries({ queryKey: chatKey }))
                    }
                  >
                    <Trash2 size={13} strokeWidth={1.75} />
                  </IconButton>
                )}
              </div>
              {messages.length === 0 && !chatReq && (
                <div className="flex flex-wrap gap-1.5">
                  {quick.map((q) => (
                    <button
                      key={q}
                      type="button"
                      onClick={() => void send(q)}
                      className="rounded-full border border-line bg-surface px-2.5 py-1 text-[12px] text-fg-2 transition-colors hover:border-line-strong hover:text-fg"
                    >
                      {q}
                    </button>
                  ))}
                </div>
              )}
              <div className="flex flex-col gap-3">
                {messages.map((m) => (
                  <Bubble key={m.id} role={m.role} text={m.content} />
                ))}
                {chatReq &&
                  (chatStream?.text ? (
                    <Bubble role="assistant" text={chatStream.text} streaming={!chatStream.done} />
                  ) : (
                    <p className="flex items-center gap-2 text-[12px] text-fg-3">
                      <Spinner size={13} />
                      {t("ai.thinking")}
                    </p>
                  ))}
              </div>
              {chatError !== null && (
                <AiError error={chatError} onSettings={() => setDialog({ type: "settings", page: "ai" })} />
              )}
            </section>
          </div>

          <footer className="shrink-0 border-t border-line p-2">
            <div className="flex items-end gap-1.5 rounded-lg border border-line-strong bg-bg p-1.5 focus-within:border-accent">
              <textarea
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                onKeyDown={onKey}
                rows={Math.min(5, Math.max(1, draft.split("\n").length))}
                placeholder={t("ai.ask")}
                className="min-h-7 flex-1 resize-none bg-transparent px-1.5 py-1 text-[13px] text-fg outline-none placeholder:text-fg-3"
              />
              {answering ? (
                <IconButton label={t("ai.stop")} onClick={() => chatReq && void api.aiCancel(chatReq)}>
                  <Square size={13} strokeWidth={2} className="fill-current" />
                </IconButton>
              ) : (
                <button
                  type="button"
                  aria-label={t("ai.send")}
                  disabled={!draft.trim()}
                  onClick={() => void send(draft)}
                  className="flex size-7 shrink-0 items-center justify-center rounded-md bg-accent-strong text-white transition-opacity disabled:opacity-30"
                >
                  <ArrowUp size={15} strokeWidth={2} />
                </button>
              )}
            </div>
          </footer>
        </>
      )}
    </aside>
  );
}

function Bubble({ role, text, streaming = false }: { role: "user" | "assistant"; text: string; streaming?: boolean }) {
  if (role === "user") {
    return (
      <div className="ml-8 self-end whitespace-pre-wrap rounded-lg bg-accent-soft px-3 py-2 text-[13px] leading-relaxed text-fg">
        {text}
      </div>
    );
  }
  return (
    <div className={cx("select-text text-[13px] leading-relaxed text-fg")}>
      <Markdown text={text} />
      {streaming && <span className="ml-0.5 inline-block h-3.5 w-1.5 animate-pulse bg-accent align-middle" />}
    </div>
  );
}

/** AI 出错时的提示：说明用界面语言；AI 服务自己回的原话（有的话）另起一行，淡一些。 */
function AiError({ error, onSettings }: { error: Failure; onSettings: () => void }) {
  const t = useT();
  const detail = errorDetail(error);
  return (
    <div className="mt-3 rounded-md border border-line bg-hover px-3 py-2 text-[12px] leading-relaxed text-fg-2">
      {errorText(t, error)}
      <button type="button" onClick={onSettings} className="ml-1.5 text-accent hover:underline">
        {t("ai.openSettings")}
      </button>
      {detail && <p className="mt-1 select-text break-words text-[11px] text-fg-3">{t("ai.serviceSaid", { text: detail })}</p>}
    </div>
  );
}
