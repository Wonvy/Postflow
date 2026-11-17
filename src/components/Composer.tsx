// 写信窗口：写新邮件、回复、全部回复、转发，以及接着写草稿。
//
// 信由本机直接交给邮箱的发信服务器，不经过别处。只写纯文本；回复时原信用「>」引在下面。
// 写的时候停一会儿就自动存进邮箱的「草稿」，手机和网页邮箱里也看得到；发出去或者丢弃时删掉。
// 附件先交给核心，这里只拿着编号。

import * as Dialog from "@radix-ui/react-dialog";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { FileText, Paperclip, Send, TriangleAlert, X } from "lucide-react";
import {
  type ClipboardEvent,
  type DragEvent,
  type KeyboardEvent,
  type ReactNode,
  type RefObject,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { api, type Contact, type Outgoing, type Thread } from "../lib/api";
import {
  addSignature,
  addressOf,
  type ComposeMode,
  isValidAddress,
  prefill,
  splitAddresses,
  swapSignature,
} from "../lib/compose";
import { errorDetail, errorText, useLang, useT } from "../lib/i18n";
import { formatSize, isMac } from "../lib/platform";
import { keys, useNav } from "../lib/queries";
import { useUi } from "../lib/store";
import { toast } from "./Toast";
import { Button, cx, IconButton, ICON, Select, Spinner, Tip } from "./ui";

/** 一栏收件人：已经确认的几个，加上正在输入还没确认的那段文字。 */
interface Recipients {
  list: string[];
  draft: string;
}

const EMPTY: Recipients = { list: [], draft: "" };
const NO_THREAD: Thread = { in_reply_to: null, references: [] };

/** 停下来这么久没再改，就存一次草稿。 */
const AUTOSAVE_MS = 3000;
/** 附件加起来超过这么大时提醒一句：很多邮箱最多收 25 MB。 */
const BIG_ATTACHMENTS = 25 * 1024 * 1024;
/** 单个文件再大就不收了，和核心的上限一致。 */
const MAX_FILE = 50 * 1024 * 1024;

/** 写信窗口里的一个附件。`token` 有了才算加好。 */
interface FileEntry {
  key: number;
  name: string;
  size: number;
  token: string | null;
  failed: boolean;
}

/** 确认的加上正在输入的。发信和判断「写了没有」时用。 */
function everyone(r: Recipients): string[] {
  return [...r.list, ...splitAddresses(r.draft)];
}

function readBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const url = String(reader.result);
      resolve(url.slice(url.indexOf(",") + 1));
    };
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
}

function errorCode(err: unknown): string {
  return err && typeof err === "object" && "code" in err ? String(err.code) : "";
}

function errorMessage(err: unknown): string {
  return err && typeof err === "object" && "message" in err ? String(err.message) : "";
}

export const sendShortcut = isMac ? "⌘↩" : "Ctrl+Enter";

export function ComposeDialog() {
  const dialog = useUi((s) => s.dialog);
  // 关上就卸掉，下次打开是一个新的窗口，不留上一封的内容
  if (dialog?.type !== "compose") return null;
  return <Composer mode={dialog.mode} itemId={dialog.itemId ?? null} accountId={dialog.accountId ?? null} />;
}

type Footer =
  /** 平常：附件按钮、草稿状态、放弃和发送。 */
  | "normal"
  /** 还没存过草稿的信，问要不要放弃。 */
  | "discard"
  /** 存过草稿的信，问要不要连草稿一起删掉。 */
  | "delete-draft"
  /** 草稿没存上，关掉会丢。 */
  | "unsaved";

type DraftStatus = { kind: "idle" } | { kind: "saving" } | { kind: "saved"; at: number } | { kind: "failed" };

function Composer({ mode, itemId, accountId }: { mode: ComposeMode; itemId: string | null; accountId: string | null }) {
  const t = useT();
  const lang = useLang();
  const qc = useQueryClient();
  const setDialog = useUi((s) => s.setDialog);
  const scope = useUi((s) => s.scope);
  const accounts = useNav().data?.accounts ?? [];

  const replying = mode === "reply" || mode === "reply-all" || mode === "forward";
  const context = useQuery({
    queryKey: ["compose", itemId, mode],
    queryFn: () => api.composeContext(itemId as string, mode === "forward"),
    enabled: itemId !== null && replying,
    staleTime: Infinity,
    gcTime: 0,
    retry: false,
  });
  const saved = useQuery({
    queryKey: ["draft", itemId],
    queryFn: () => api.draft(itemId as string),
    enabled: itemId !== null && mode === "draft",
    staleTime: Infinity,
    gcTime: 0,
    retry: false,
  });
  const loading = mode === "draft" ? saved : context;
  const contacts = useQuery({
    queryKey: ["contacts"],
    queryFn: () => api.contacts().then((r) => r.contacts),
    staleTime: 5 * 60_000,
  });

  const defaultAccount =
    accountId ??
    saved.data?.account_id ??
    context.data?.account_id ??
    (scope.kind === "account" ? scope.accountId : null) ??
    accounts[0]?.id ??
    "";
  const [from, setFrom] = useState<string | null>(null);
  const fromId = from ?? defaultAccount;
  const fromAccount = accounts.find((a) => a.id === fromId);
  /** 正文里现在放的是哪个签名，以及预先填好的引文（签名放在它上面）。换邮箱时换签名用。 */
  const signature = useRef("");
  const quoteTail = useRef("");
  // 这个邮箱还没填过发件人名字：在这里顺手填，发送时记进邮箱设置
  const [senderName, setSenderName] = useState("");
  const [to, setTo] = useState<Recipients>(EMPTY);
  const [cc, setCc] = useState<Recipients>(EMPTY);
  const [bcc, setBcc] = useState<Recipients>(EMPTY);
  const [showCc, setShowCc] = useState(false);
  // 点「抄送」打开的才把光标放进去；全部回复时预先填好的不抢正文的光标
  const [focusCc, setFocusCc] = useState(false);
  const [showBcc, setShowBcc] = useState(false);
  const [subject, setSubject] = useState("");
  const [text, setText] = useState("");
  const [thread, setThread] = useState<Thread>(NO_THREAD);
  const [files, setFiles] = useState<FileEntry[]>([]);
  const [dragging, setDragging] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<{ text: string; detail: string | null } | null>(null);
  const [footer, setFooter] = useState<Footer>("normal");
  const [status, setStatus] = useState<DraftStatus>({ kind: "idle" });
  const body = useRef<HTMLTextAreaElement>(null);
  const toField = useRef<HTMLInputElement>(null);
  const picker = useRef<HTMLInputElement>(null);
  const nextKey = useRef(0);

  /** 存在邮箱里的那份草稿：哪个邮箱、Message-ID。 */
  const draft = useRef<{ account: string; id: string } | null>(null);
  /** 上次存下的（或者刚打开时的）内容，和现在的比，不一样就要存。 */
  const savedKey = useRef<string | null>(null);
  const saving = useRef<Promise<boolean> | null>(null);
  /** 这次打开以后存过草稿。 */
  const resaved = useRef(false);

  // 原信或草稿取回来以后填好收件人、主题、正文和附件，光标放在该放的地方
  const filled = useRef(false);
  const ready = mode === "new" || itemId === null || loading.isSuccess;
  useEffect(() => {
    if (filled.current || !ready) return;
    filled.current = true;
    const d = saved.data;
    if (mode === "draft" && d) {
      setFrom(d.account_id);
      // 草稿里多半已经有这个邮箱的签名
      signature.current = accounts.find((a) => a.id === d.account_id)?.signature ?? "";
      setTo({ list: d.to, draft: "" });
      setCc({ list: d.cc, draft: "" });
      setBcc({ list: d.bcc, draft: "" });
      setShowCc(d.cc.length > 0);
      setShowBcc(d.bcc.length > 0);
      setSubject(d.subject);
      setText(d.text);
      setThread(d.thread);
      setFiles(d.attachments.map((a) => ({ ...a, key: nextKey.current++, failed: false })));
      if (d.message_id) draft.current = { account: d.account_id, id: d.message_id };
    } else {
      // 「自己」是收到这封信的那个邮箱。自己几个邮箱之间来往的信，回复时对方也是自己的另一个邮箱
      const ctx = context.data ?? null;
      const receiver = accounts.find((a) => a.id === ctx?.account_id);
      const p = prefill(mode, ctx, receiver ? [receiver.email] : [], lang);
      setTo({ list: p.to, draft: "" });
      setCc({ list: p.cc, draft: "" });
      setShowCc(p.cc.length > 0);
      setSubject(p.subject);
      const sig = accounts.find((a) => a.id === defaultAccount)?.signature ?? "";
      signature.current = sig;
      quoteTail.current = p.text;
      setText(addSignature(p.text, sig, p.text));
      if (ctx && mode !== "forward") setThread(ctx.thread);
      if (ctx && mode === "forward") {
        setFiles(ctx.attachments.map((a) => ({ ...a, key: nextKey.current++, failed: false })));
      }
    }
    requestAnimationFrame(() => {
      if ((mode === "reply" || mode === "reply-all" || mode === "draft") && body.current) {
        body.current.focus();
        body.current.setSelectionRange(0, 0);
        body.current.scrollTop = 0;
      } else {
        toField.current?.focus();
      }
    });
  }, [accounts, context.data, saved.data, lang, mode, ready, defaultAccount]);

  /** 换发信的邮箱：签名跟着换。 */
  const changeFrom = (id: string) => {
    const next = accounts.find((a) => a.id === id)?.signature ?? "";
    setText((current) => swapSignature(current, signature.current, next, quoteTail.current));
    signature.current = next;
    setFrom(id);
  };

  const tokens = files.flatMap((f) => (f.token ? [f.token] : []));
  const uploading = files.some((f) => !f.token && !f.failed);
  const letter: Outgoing = {
    account_id: fromId,
    to: everyone(to),
    cc: everyone(cc),
    bcc: everyone(bcc),
    subject: subject.trim(),
    text,
    in_reply_to: thread.in_reply_to,
    references: thread.references,
    attachments: tokens,
  };
  const key = JSON.stringify(letter);
  // 填好的那一刻就是「没改过」
  if (ready && filled.current && savedKey.current === null) savedKey.current = key;
  const dirty = savedKey.current !== null && key !== savedKey.current;
  const latest = useRef({ letter, key });
  latest.current = { letter, key };

  /** 存一次草稿。成功返回 true。正在存时等它存完再看要不要再存。 */
  const saveDraft = useCallback(async (): Promise<boolean> => {
    while (saving.current) await saving.current;
    const { letter: current, key: currentKey } = latest.current;
    if (currentKey === savedKey.current) return true;
    if (!current.account_id) return false;
    const run = (async () => {
      setStatus({ kind: "saving" });
      try {
        const old = draft.current;
        // 换了发件的邮箱：草稿存到新邮箱，旧邮箱里的那份删掉
        const sameAccount = old?.account === current.account_id;
        const result = await api.saveDraft({ ...current, draft: sameAccount ? old?.id : null });
        if (old && !sameAccount) void api.deleteDraft(old.account, old.id).catch(() => {});
        draft.current = { account: current.account_id, id: result.message_id };
        resaved.current = true;
        savedKey.current = currentKey;
        setStatus({ kind: "saved", at: Date.now() });
        return true;
      } catch {
        setStatus({ kind: "failed" });
        return false;
      }
    })();
    saving.current = run;
    try {
      return await run;
    } finally {
      saving.current = null;
    }
  }, []);

  // 停下来一会儿就自动存。正在发信或者附件还在加的时候不存
  useEffect(() => {
    if (!dirty || busy || uploading) return;
    const timer = setTimeout(() => void saveDraft(), AUTOSAVE_MS);
    return () => clearTimeout(timer);
  }, [key, dirty, busy, uploading, saveDraft]);

  // 窗口关掉：附件不用再留着
  const tokensRef = useRef<string[]>([]);
  tokensRef.current = tokens;
  useEffect(() => () => void api.attachDrop(tokensRef.current).catch(() => {}), []);

  /** 关窗口。打开的草稿存过新的一版（旧的那封在邮箱里已经删了），阅读区也关上。 */
  const close = () => {
    const ui = useUi.getState();
    if (mode === "draft" && itemId && ui.openId === itemId && resaved.current) ui.open(null);
    setDialog(null);
  };

  /** 关窗口：改过的先存成草稿；存不上就问一句。 */
  const requestClose = async () => {
    if (busy) return;
    if (footer !== "normal") {
      setFooter("normal");
      return;
    }
    if (!dirty) {
      close();
      return;
    }
    if (await saveDraft()) {
      close();
      toast(t("compose.draftSaved", { time: clock(Date.now(), lang) }));
    } else {
      setFooter("unsaved");
    }
  };

  /** 放弃这封信：存过草稿的连草稿一起删。 */
  const discard = async () => {
    while (saving.current) await saving.current;
    const old = draft.current;
    if (old) void api.deleteDraft(old.account, old.id).catch(() => {});
    resaved.current = true;
    void qc.invalidateQueries({ queryKey: ["items"] });
    close();
  };

  const addFiles = async (list: File[]) => {
    if (list.length === 0) return;
    setError(null);
    for (const file of list) {
      const k = nextKey.current++;
      setFiles((fs) => [...fs, { key: k, name: file.name || "attachment", size: file.size, token: null, failed: false }]);
      try {
        if (file.size > MAX_FILE) throw new Error("too large");
        const data = await readBase64(file);
        const added = await api.attach(file.name || "attachment", file.type, data);
        setFiles((fs) => fs.map((f) => (f.key === k ? { ...f, token: added.token } : f)));
      } catch {
        setFiles((fs) => fs.map((f) => (f.key === k ? { ...f, failed: true } : f)));
      }
    }
  };

  const removeFile = (entry: FileEntry) => {
    setFiles((fs) => fs.filter((f) => f.key !== entry.key));
    if (entry.token) void api.attachDrop([entry.token]).catch(() => {});
  };

  const send = async () => {
    if (busy || uploading) return;
    const { letter: current } = latest.current;
    const all = [...current.to, ...current.cc, ...current.bcc];
    if (all.length === 0) {
      setError({ text: t("compose.noRecipients"), detail: null });
      return;
    }
    const bad = all.find((a) => !isValidAddress(a));
    if (bad) {
      setError({ text: t("compose.badAddress", { address: bad }), detail: null });
      return;
    }
    if (!fromId) return;
    setBusy(true);
    setError(null);
    try {
      // 正在存的草稿存完再发，免得发完又冒出一封草稿
      while (saving.current) await saving.current;
      if (fromAccount && !fromAccount.sender_name && senderName.trim()) {
        await api.updateAccount(fromId, { sender_name: senderName.trim() });
        void qc.invalidateQueries({ queryKey: keys.nav });
      }
      const old = draft.current;
      await api.send({ ...current, draft: old?.account === current.account_id ? old.id : null });
      // 草稿在别的邮箱里：单独删掉
      if (old && old.account !== current.account_id) void api.deleteDraft(old.account, old.id).catch(() => {});
      draft.current = null;
      tokensRef.current = [];
      // 发的是阅读区里开着的那封草稿：它没了，阅读区也关上
      resaved.current = true;
      close();
      toast(t("compose.sent"));
      void qc.invalidateQueries({ queryKey: ["contacts"] });
    } catch (err) {
      const code = errorCode(err);
      const message = errorMessage(err);
      setError(
        code === "send_bad_address"
          ? { text: t("compose.badAddress", { address: message }), detail: null }
          : code === "send_too_large" && /^\d+$/.test(message)
            ? { text: t("compose.tooLarge", { n: message }), detail: null }
            : { text: errorText(t, err), detail: errorDetail(err) },
      );
      setBusy(false);
    }
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void send();
    }
  };

  // 把文件拖进窗口：添加为附件
  const hasFiles = (e: DragEvent) => Array.from(e.dataTransfer.types).includes("Files");
  const onDragOver = (e: DragEvent) => {
    if (!hasFiles(e)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
    setDragging(true);
  };
  const onDragLeave = (e: DragEvent) => {
    if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setDragging(false);
  };
  const onDrop = (e: DragEvent) => {
    if (!hasFiles(e)) return;
    e.preventDefault();
    setDragging(false);
    void addFiles(Array.from(e.dataTransfer.files));
  };
  // 在正文里粘贴图片之类的文件：添加为附件
  const onPaste = (e: ClipboardEvent<HTMLTextAreaElement>) => {
    const pasted = Array.from(e.clipboardData.files);
    if (pasted.length === 0) return;
    e.preventDefault();
    void addFiles(pasted);
  };

  const title =
    mode === "reply"
      ? t("compose.reply")
      : mode === "reply-all"
        ? t("compose.replyAll")
        : mode === "forward"
          ? t("compose.forward")
          : mode === "draft"
            ? t("compose.editDraft")
            : t("compose.new");

  const known = useMemo(() => {
    const own: Contact[] = accounts.map((a) => ({ name: a.name || null, address: a.email, count: 0 }));
    return [...(contacts.data ?? []), ...own];
  }, [accounts, contacts.data]);

  const total = files.reduce((n, f) => n + (f.failed ? 0 : f.size), 0);
  const hasDraft = draft.current !== null;

  return (
    <Dialog.Root open onOpenChange={(next) => !next && void requestClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="anim-fade fixed inset-0 z-40 bg-overlay" />
        <Dialog.Content
          aria-describedby={undefined}
          onOpenAutoFocus={(e) => e.preventDefault()}
          // 收件人的提示开着时，Esc 只收起提示，不关窗口
          onEscapeKeyDown={(e) => {
            if (document.activeElement?.hasAttribute("data-suggesting")) e.preventDefault();
          }}
          onKeyDown={onKeyDown}
          onDragOver={onDragOver}
          onDragLeave={onDragLeave}
          onDrop={onDrop}
          className="anim-pop fixed left-1/2 top-[7vh] z-40 flex h-[min(680px,86vh)] w-[680px] max-w-[calc(100vw-32px)] -translate-x-1/2 flex-col rounded-xl bg-surface shadow-pop outline-none"
        >
          <div className="flex h-11 shrink-0 items-center justify-between border-b border-line pl-4 pr-2">
            <Dialog.Title className="min-w-0 truncate text-[13px] font-semibold">
              {mode === "new" || !subject.trim() ? title : `${title} · ${subject.trim()}`}
            </Dialog.Title>
            <button
              type="button"
              aria-label={t("action.close")}
              onClick={() => void requestClose()}
              className="inline-flex size-7 items-center justify-center rounded-md text-fg-3 hover:bg-hover hover:text-fg"
            >
              <X {...ICON} />
            </button>
          </div>

          {!ready ? (
            <div className="flex flex-1 items-center justify-center gap-2 text-fg-3">
              {loading.isError ? (
                <span className="flex items-center gap-2 text-danger">
                  <TriangleAlert size={14} strokeWidth={2} />
                  {errorText(t, loading.error)}
                </span>
              ) : (
                <Spinner size={16} />
              )}
            </div>
          ) : (
            <>
              <div className="shrink-0 border-b border-line">
                <Row label={t("compose.from")}>
                  <div className="flex min-h-9 items-center gap-2 pr-3">
                    {accounts.length > 1 ? (
                      <Select
                        value={fromId}
                        onChange={(e) => changeFrom(e.target.value)}
                        // 只占内容那么宽，不拉满整行
                        style={{ width: "auto" }}
                        className="h-7 max-w-full border-transparent bg-transparent px-1 focus:border-accent"
                      >
                        {accounts.map((a) => (
                          <option key={a.id} value={a.id}>
                            {a.sender_name ? `${a.sender_name} <${a.email}>` : a.email}
                          </option>
                        ))}
                      </Select>
                    ) : (
                      <span className="truncate text-[13px] text-fg">
                        {fromAccount?.sender_name
                          ? `${fromAccount.sender_name} <${fromAccount.email}>`
                          : fromAccount?.email}
                      </span>
                    )}
                    {fromAccount && !fromAccount.sender_name && (
                      <input
                        value={senderName}
                        onChange={(e) => setSenderName(e.target.value)}
                        placeholder={t("compose.senderName")}
                        title={t("compose.senderNameHint")}
                        aria-label={t("compose.senderName")}
                        spellCheck={false}
                        autoComplete="name"
                        className="h-7 w-[150px] min-w-0 rounded-md border border-dashed border-line-strong bg-transparent px-2 text-[12px] text-fg outline-none placeholder:text-fg-3 focus:border-solid focus:border-accent"
                      />
                    )}
                  </div>
                </Row>
                <Row
                  label={t("compose.to")}
                  extra={
                    <span className="flex shrink-0 gap-1 pr-2">
                      {!showCc && (
                        <SmallToggle
                          onClick={() => {
                            setShowCc(true);
                            setFocusCc(true);
                          }}
                        >
                          {t("compose.cc")}
                        </SmallToggle>
                      )}
                      {!showBcc && (
                        <SmallToggle onClick={() => setShowBcc(true)}>{t("compose.bcc")}</SmallToggle>
                      )}
                    </span>
                  }
                >
                  <RecipientInput value={to} onChange={setTo} contacts={known} inputRef={toField} />
                </Row>
                {showCc && (
                  <Row label={t("compose.cc")}>
                    <RecipientInput value={cc} onChange={setCc} contacts={known} autoFocus={focusCc} />
                  </Row>
                )}
                {showBcc && (
                  <Row label={t("compose.bcc")}>
                    <RecipientInput
                      value={bcc}
                      onChange={setBcc}
                      contacts={known}
                      autoFocus={mode !== "draft" || bcc.list.length === 0}
                    />
                  </Row>
                )}
                <Row label={t("compose.subject")}>
                  <input
                    value={subject}
                    onChange={(e) => setSubject(e.target.value)}
                    className="h-9 w-full bg-transparent text-[13px] text-fg outline-none placeholder:text-fg-3"
                    spellCheck={false}
                    autoComplete="off"
                  />
                </Row>
              </div>
              <div className="relative flex min-h-0 flex-1 flex-col">
                <textarea
                  ref={body}
                  value={text}
                  onChange={(e) => setText(e.target.value)}
                  onPaste={onPaste}
                  aria-label={t("compose.body")}
                  placeholder={mode === "new" ? t("compose.bodyPlaceholder") : undefined}
                  className="min-h-0 flex-1 resize-none bg-transparent px-4 py-3 text-[14px] leading-relaxed text-fg outline-none placeholder:text-fg-3"
                />
                {dragging && (
                  <div className="pointer-events-none absolute inset-2 flex items-center justify-center rounded-lg border-2 border-dashed border-accent bg-accent-soft text-[13px] font-medium text-accent">
                    {t("compose.dropHere")}
                  </div>
                )}
              </div>
              {files.length > 0 && (
                <div className="flex shrink-0 flex-col gap-1.5 border-t border-line px-3 py-2">
                  <div className="flex flex-wrap gap-1.5">
                    {files.map((f) => (
                      <span
                        key={f.key}
                        title={f.failed ? t("compose.attachFailed", { name: f.name }) : f.name}
                        className={cx(
                          "flex h-7 max-w-[260px] items-center gap-1.5 rounded-md border pl-2 pr-1 text-[12px]",
                          f.failed ? "border-danger/40 bg-danger/10 text-danger" : "border-line bg-hover text-fg",
                        )}
                      >
                        {!f.token && !f.failed ? (
                          <Spinner size={12} />
                        ) : (
                          <FileText size={13} className="shrink-0 text-fg-3" />
                        )}
                        <span className="truncate">{f.name}</span>
                        <span className="shrink-0 text-fg-3">{formatSize(f.size)}</span>
                        <button
                          type="button"
                          aria-label={t("compose.removeAttachment", { name: f.name })}
                          onClick={() => removeFile(f)}
                          className="inline-flex size-5 shrink-0 items-center justify-center rounded text-fg-3 hover:bg-active hover:text-fg"
                        >
                          <X size={12} strokeWidth={2} />
                        </button>
                      </span>
                    ))}
                  </div>
                  {total > BIG_ATTACHMENTS && (
                    <p className="flex items-center gap-1.5 text-[12px] text-warning">
                      <TriangleAlert size={13} strokeWidth={2} />
                      {t("compose.attachBig", { size: formatSize(total) })}
                    </p>
                  )}
                </div>
              )}
            </>
          )}

          {error && (
            <div role="alert" className="flex shrink-0 items-start gap-2 border-t border-line px-4 py-2 text-[13px] text-danger">
              <TriangleAlert size={14} strokeWidth={2} className="mt-0.5 shrink-0" />
              <div className="min-w-0">
                <p>{error.text}</p>
                {error.detail && (
                  <p className="mt-0.5 select-text break-words text-[12px] text-fg-3">
                    {t("account.serverSaid", { text: error.detail })}
                  </p>
                )}
              </div>
            </div>
          )}

          <div className="flex h-14 shrink-0 items-center gap-2 border-t border-line px-3">
            {footer === "discard" || footer === "delete-draft" || footer === "unsaved" ? (
              <>
                <span className="mr-auto pl-1 text-[13px] text-fg-2">
                  {footer === "discard"
                    ? t("compose.discardAsk")
                    : footer === "delete-draft"
                      ? t("compose.deleteDraftAsk")
                      : t("compose.unsavedAsk")}
                </span>
                <Button variant="ghost" onClick={() => setFooter("normal")}>
                  {t("compose.keepWriting")}
                </Button>
                <Button
                  variant="danger"
                  // 草稿没存上时只是关掉：邮箱里之前存的那一版还留着
                  onClick={() => (footer === "unsaved" ? close() : void discard())}
                >
                  {footer === "delete-draft"
                    ? t("compose.deleteDraft")
                    : footer === "unsaved"
                      ? t("compose.closeAnyway")
                      : t("compose.discard")}
                </Button>
              </>
            ) : (
              <>
                <IconButton label={t("compose.attach")} disabled={!ready} onClick={() => picker.current?.click()}>
                  <Paperclip {...ICON} />
                </IconButton>
                <input
                  ref={picker}
                  type="file"
                  multiple
                  hidden
                  onChange={(e) => {
                    void addFiles(Array.from(e.target.files ?? []));
                    e.target.value = "";
                  }}
                />
                <span className="mr-auto min-w-0 truncate pl-1 text-[12px] text-fg-3" aria-live="polite">
                  {status.kind === "saving"
                    ? t("compose.draftSaving")
                    : status.kind === "saved"
                      ? t("compose.draftSaved", { time: clock(status.at, lang) })
                      : status.kind === "failed"
                        ? t("compose.draftFailed")
                        : ""}
                </span>
                <Button
                  variant="ghost"
                  disabled={busy}
                  onClick={() => {
                    if (hasDraft) setFooter("delete-draft");
                    else if (dirty) setFooter("discard");
                    else close();
                  }}
                >
                  {hasDraft ? t("compose.deleteDraft") : t("compose.discard")}
                </Button>
                <Tip label={t("compose.send")} shortcut={sendShortcut} side="top">
                  <Button
                    variant="primary"
                    busy={busy}
                    disabled={!ready || !fromId || uploading}
                    onClick={() => void send()}
                  >
                    <Send size={14} strokeWidth={2} />
                    {t("compose.send")}
                  </Button>
                </Tip>
              </>
            )}
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/** 几点几分。 */
function clock(at: number, lang: "zh" | "en"): string {
  return new Intl.DateTimeFormat(lang === "zh" ? "zh-CN" : "en-US", { hour: "2-digit", minute: "2-digit" }).format(
    new Date(at),
  );
}

function Row({ label, extra, children }: { label: string; extra?: ReactNode; children: ReactNode }) {
  return (
    <div className="flex min-h-9 items-start border-b border-line last:border-b-0">
      <span className="w-16 shrink-0 pl-4 pt-2.5 text-[12px] text-fg-3">{label}</span>
      <div className="min-w-0 flex-1">{children}</div>
      {extra && <div className="pt-1.5">{extra}</div>}
    </div>
  );
}

function SmallToggle({ onClick, children }: { onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="h-6 rounded px-1.5 text-[12px] text-fg-3 hover:bg-hover hover:text-fg"
    >
      {children}
    </button>
  );
}

/**
 * 收件人一栏：确认过的地址显示成小块，后面接着输入。逗号、分号、回车或离开这一栏时确认；
 * 粘贴一串地址会拆开；输入时从来过信的人里提示。
 */
function RecipientInput({
  value,
  onChange,
  contacts,
  inputRef,
  autoFocus,
}: {
  value: Recipients;
  onChange: (next: Recipients) => void;
  contacts: Contact[];
  inputRef?: RefObject<HTMLInputElement | null>;
  autoFocus?: boolean;
}) {
  const t = useT();
  const [active, setActive] = useState(0);
  const [focused, setFocused] = useState(false);
  // 按了 Esc 收起的提示，接着打字时再出来
  const [dismissed, setDismissed] = useState(false);
  const query = value.draft.trim().toLowerCase();
  const chosen = new Set(value.list.map(addressOf));
  const suggestions = query
    ? contacts
        .filter((c) => !chosen.has(c.address.toLowerCase()) && matches(c, query))
        // 地址或名字以输入开头的排前面
        .sort((a, b) => Number(startsWith(b, query)) - Number(startsWith(a, query)))
        .slice(0, 6)
    : [];
  const open = focused && !dismissed && suggestions.length > 0;

  const commit = (extra: string[] = []) => {
    const added = [...splitAddresses(value.draft), ...extra];
    const list = [...value.list];
    for (const a of added) if (!list.some((x) => addressOf(x) === addressOf(a))) list.push(a);
    onChange({ list, draft: "" });
    setActive(0);
  };

  const pick = (c: Contact) => {
    onChange({ list: [...value.list, c.name ? `${c.name} <${c.address}>` : c.address], draft: "" });
    setActive(0);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (open && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      e.preventDefault();
      setActive((i) => (i + (e.key === "ArrowDown" ? 1 : -1) + suggestions.length) % suggestions.length);
      return;
    }
    if ((e.key === "Enter" || e.key === "Tab") && !e.metaKey && !e.ctrlKey && value.draft.trim()) {
      e.preventDefault();
      const s = open ? suggestions[active] : undefined;
      if (s) pick(s);
      else commit();
      return;
    }
    if (e.key === "Escape" && open) {
      setDismissed(true);
      return;
    }
    if (e.key === "Backspace" && !value.draft && value.list.length > 0) {
      onChange({ ...value, list: value.list.slice(0, -1) });
    }
  };

  const onPaste = (e: ClipboardEvent<HTMLInputElement>) => {
    const pasted = e.clipboardData.getData("text");
    if (!/[,;\n，；]/.test(pasted)) return;
    e.preventDefault();
    commit(splitAddresses(pasted));
  };

  return (
    <div className="relative">
      <div className="flex min-h-9 flex-wrap items-center gap-1 py-1 pr-2">
        {value.list.map((entry, i) => {
          const ok = isValidAddress(entry);
          return (
            <span
              key={`${entry}-${i}`}
              title={ok ? entry : t("compose.badAddress", { address: entry })}
              className={cx(
                "flex h-6 max-w-[260px] items-center gap-1 rounded-md pl-2 pr-1 text-[12px]",
                ok ? "bg-hover text-fg" : "bg-danger/15 text-danger",
              )}
            >
              <span className="truncate">{entry}</span>
              <button
                type="button"
                aria-label={t("compose.removeRecipient", { address: entry })}
                onClick={() => onChange({ ...value, list: value.list.filter((_, j) => j !== i) })}
                className="inline-flex size-4 shrink-0 items-center justify-center rounded text-fg-3 hover:text-fg"
              >
                <X size={12} strokeWidth={2} />
              </button>
            </span>
          );
        })}
        <input
          ref={inputRef}
          autoFocus={autoFocus}
          data-suggesting={open ? "" : undefined}
          value={value.draft}
          onChange={(e) => {
            const next = e.target.value;
            // 打了逗号或分号就确认前面那个
            if (/[,;，；]\s*$/.test(next) && splitAddresses(next).length > 0) {
              const list = [...value.list];
              for (const a of splitAddresses(next)) if (!list.some((x) => addressOf(x) === addressOf(a))) list.push(a);
              onChange({ list, draft: "" });
            } else {
              onChange({ ...value, draft: next });
            }
            setActive(0);
            setDismissed(false);
          }}
          onKeyDown={onKeyDown}
          onPaste={onPaste}
          onFocus={() => setFocused(true)}
          onBlur={() => {
            setFocused(false);
            if (value.draft.trim()) commit();
          }}
          spellCheck={false}
          autoComplete="off"
          className="h-7 min-w-[120px] flex-1 bg-transparent text-[13px] text-fg outline-none"
        />
      </div>
      {open && (
        <ul
          role="listbox"
          className="anim-pop absolute left-0 top-full z-50 mt-0.5 w-[min(420px,100%)] rounded-lg bg-surface p-1 shadow-pop"
        >
          {suggestions.map((c, i) => (
            <li
              key={c.address}
              role="option"
              aria-selected={i === active}
              // 按下时就选，免得输入框先失焦把输入的文字当成地址确认掉
              onMouseDown={(e) => {
                e.preventDefault();
                pick(c);
              }}
              onMouseEnter={() => setActive(i)}
              className={cx(
                "flex cursor-default items-baseline gap-2 rounded-md px-2 py-1.5 text-[13px]",
                i === active && "bg-hover",
              )}
            >
              <span className="truncate text-fg">{c.name ?? c.address}</span>
              {c.name && <span className="truncate text-[12px] text-fg-3">{c.address}</span>}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/**
 * 输入的文字对得上这个人：名字里某个词、地址 @ 前面的部分，或者域名的开头。
 * 不在整个地址里随便找，免得输入「le」把所有 example.com 的人都列出来。
 */
function matches(c: Contact, query: string): boolean {
  const name = c.name?.toLowerCase() ?? "";
  const address = c.address.toLowerCase();
  if (query.includes("@")) return address.includes(query);
  const [local = "", domain = ""] = address.split("@");
  // 中文名字没有空格分词，直接找
  if (/[^\x00-\x7f]/.test(query)) return name.includes(query);
  return (
    name.split(/[\s.,]+/).some((w) => w.startsWith(query)) || local.includes(query) || domain.startsWith(query)
  );
}

function startsWith(c: Contact, query: string): boolean {
  return c.address.toLowerCase().startsWith(query) || (c.name?.toLowerCase().startsWith(query) ?? false);
}
