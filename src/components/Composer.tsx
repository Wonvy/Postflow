// 写信窗口：写新邮件、回复、全部回复和转发。
//
// 信由本机直接交给邮箱的发信服务器，不经过别处。只写纯文本；回复时原信用「>」引在下面。

import * as Dialog from "@radix-ui/react-dialog";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Paperclip, Send, TriangleAlert, X } from "lucide-react";
import {
  type ClipboardEvent,
  type KeyboardEvent,
  type ReactNode,
  type RefObject,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { api, type Contact } from "../lib/api";
import { addressOf, type ComposeMode, isValidAddress, prefill, splitAddresses } from "../lib/compose";
import { errorDetail, errorText, useLang, useT } from "../lib/i18n";
import { isMac } from "../lib/platform";
import { keys, useItem, useNav } from "../lib/queries";
import { useUi } from "../lib/store";
import { toast } from "./Toast";
import { Button, cx, ICON, Select, Spinner, Tip } from "./ui";

/** 一栏收件人：已经确认的几个，加上正在输入还没确认的那段文字。 */
interface Recipients {
  list: string[];
  draft: string;
}

const EMPTY: Recipients = { list: [], draft: "" };

/** 确认的加上正在输入的。发信和判断「写了没有」时用。 */
function everyone(r: Recipients): string[] {
  return [...r.list, ...splitAddresses(r.draft)];
}

export const sendShortcut = isMac ? "⌘↩" : "Ctrl+Enter";

export function ComposeDialog() {
  const dialog = useUi((s) => s.dialog);
  // 关上就卸掉，下次打开是一个新的窗口，不留上一封的内容
  if (dialog?.type !== "compose") return null;
  return <Composer mode={dialog.mode} itemId={dialog.itemId ?? null} accountId={dialog.accountId ?? null} />;
}

function Composer({ mode, itemId, accountId }: { mode: ComposeMode; itemId: string | null; accountId: string | null }) {
  const t = useT();
  const lang = useLang();
  const qc = useQueryClient();
  const setDialog = useUi((s) => s.setDialog);
  const scope = useUi((s) => s.scope);
  const accounts = useNav().data?.accounts ?? [];

  const context = useQuery({
    queryKey: ["compose", itemId],
    queryFn: () => api.composeContext(itemId as string),
    enabled: itemId !== null && mode !== "new",
    staleTime: 0,
    gcTime: 0,
    retry: false,
  });
  const contacts = useQuery({
    queryKey: ["contacts"],
    queryFn: () => api.contacts().then((r) => r.contacts),
    staleTime: 5 * 60_000,
  });
  // 转发时原信的附件现在还带不过去，提前说一声
  const original = useItem(mode === "forward" ? itemId : null);
  const attachments = original.data?.mail?.attachments.length ?? 0;

  const defaultAccount =
    accountId ??
    context.data?.account_id ??
    (scope.kind === "account" ? scope.accountId : null) ??
    accounts[0]?.id ??
    "";
  const [from, setFrom] = useState<string | null>(null);
  const fromId = from ?? defaultAccount;
  const fromAccount = accounts.find((a) => a.id === fromId);
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
  const [initialText, setInitialText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<{ text: string; detail: string | null } | null>(null);
  const [confirmClose, setConfirmClose] = useState(false);
  const body = useRef<HTMLTextAreaElement>(null);
  const toField = useRef<HTMLInputElement>(null);

  // 原信取回来以后填好收件人、主题和引文，光标放在最前面
  const filled = useRef(false);
  const ready = mode === "new" || itemId === null || context.isSuccess;
  useEffect(() => {
    if (filled.current || !ready) return;
    filled.current = true;
    // 「自己」是收到这封信的那个邮箱。自己几个邮箱之间来往的信，回复时对方也是自己的另一个邮箱
    const receiver = accounts.find((a) => a.id === context.data?.account_id);
    const self = receiver ? [receiver.email] : [];
    const p = prefill(mode, context.data ?? null, self, lang);
    setTo({ list: p.to, draft: "" });
    setCc({ list: p.cc, draft: "" });
    setShowCc(p.cc.length > 0);
    setSubject(p.subject);
    setText(p.text);
    setInitialText(p.text);
    requestAnimationFrame(() => {
      if ((mode === "reply" || mode === "reply-all") && body.current) {
        body.current.focus();
        body.current.setSelectionRange(0, 0);
        body.current.scrollTop = 0;
      } else {
        toField.current?.focus();
      }
    });
  }, [accounts, context.data, lang, mode, ready]);

  const dirty =
    text !== initialText || (mode === "new" && (everyone(to).length > 0 || subject.trim() !== ""));

  const close = () => setDialog(null);
  const requestClose = () => {
    if (busy) return;
    if (dirty) setConfirmClose(true);
    else close();
  };

  const send = async () => {
    if (busy) return;
    const lists = { to: everyone(to), cc: everyone(cc), bcc: everyone(bcc) };
    const all = [...lists.to, ...lists.cc, ...lists.bcc];
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
      if (fromAccount && !fromAccount.sender_name && senderName.trim()) {
        await api.updateAccount(fromId, { sender_name: senderName.trim() });
        void qc.invalidateQueries({ queryKey: keys.nav });
      }
      await api.send({
        account_id: fromId,
        ...lists,
        subject: subject.trim(),
        text,
        in_reply_to: mode === "reply" || mode === "reply-all" ? (itemId ?? undefined) : undefined,
      });
      close();
      toast(t("compose.sent"));
      void qc.invalidateQueries({ queryKey: ["contacts"] });
    } catch (err) {
      const code = err && typeof err === "object" && "code" in err ? String(err.code) : "";
      const message = err && typeof err === "object" && "message" in err ? String(err.message) : "";
      setError(
        code === "send_bad_address"
          ? { text: t("compose.badAddress", { address: message }), detail: null }
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

  const title =
    mode === "reply"
      ? t("compose.reply")
      : mode === "reply-all"
        ? t("compose.replyAll")
        : mode === "forward"
          ? t("compose.forward")
          : t("compose.new");

  const known = useMemo(() => {
    const own: Contact[] = accounts.map((a) => ({ name: a.name || null, address: a.email, count: 0 }));
    return [...(contacts.data ?? []), ...own];
  }, [accounts, contacts.data]);

  return (
    <Dialog.Root open onOpenChange={(next) => !next && requestClose()}>
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
          className="anim-pop fixed left-1/2 top-[7vh] z-40 flex h-[min(680px,86vh)] w-[680px] max-w-[calc(100vw-32px)] -translate-x-1/2 flex-col rounded-xl bg-surface shadow-pop outline-none"
        >
          <div className="flex h-11 shrink-0 items-center justify-between border-b border-line pl-4 pr-2">
            <Dialog.Title className="min-w-0 truncate text-[13px] font-semibold">
              {mode === "new" || !subject.trim() ? title : `${title} · ${subject.trim()}`}
            </Dialog.Title>
            <button
              type="button"
              aria-label={t("action.close")}
              onClick={requestClose}
              className="inline-flex size-7 items-center justify-center rounded-md text-fg-3 hover:bg-hover hover:text-fg"
            >
              <X {...ICON} />
            </button>
          </div>

          {!ready ? (
            <div className="flex flex-1 items-center justify-center gap-2 text-fg-3">
              {context.isError ? (
                <span className="flex items-center gap-2 text-danger">
                  <TriangleAlert size={14} strokeWidth={2} />
                  {errorText(t, context.error)}
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
                        onChange={(e) => setFrom(e.target.value)}
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
                    <RecipientInput value={bcc} onChange={setBcc} contacts={known} autoFocus />
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
              <textarea
                ref={body}
                value={text}
                onChange={(e) => setText(e.target.value)}
                aria-label={t("compose.body")}
                placeholder={mode === "new" ? t("compose.bodyPlaceholder") : undefined}
                className="min-h-0 flex-1 resize-none bg-transparent px-4 py-3 text-[14px] leading-relaxed text-fg outline-none placeholder:text-fg-3"
              />
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
            {confirmClose ? (
              <>
                <span className="mr-auto pl-1 text-[13px] text-fg-2">{t("compose.discardAsk")}</span>
                <Button variant="ghost" onClick={() => setConfirmClose(false)}>
                  {t("compose.keepWriting")}
                </Button>
                <Button variant="danger" onClick={close}>
                  {t("compose.discard")}
                </Button>
              </>
            ) : (
              <>
                {attachments > 0 ? (
                  <span className="mr-auto flex min-w-0 items-center gap-1.5 pl-1 text-[12px] text-fg-3">
                    <Paperclip size={13} className="shrink-0" />
                    <span className="truncate">{t("compose.attachmentsNotForwarded", { n: attachments })}</span>
                  </span>
                ) : (
                  <span className="mr-auto" />
                )}
                <Button variant="ghost" onClick={requestClose} disabled={busy}>
                  {t("compose.discard")}
                </Button>
                <Tip label={t("compose.send")} shortcut={sendShortcut} side="top">
                  <Button variant="primary" busy={busy} disabled={!ready || !fromId} onClick={() => void send()}>
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
