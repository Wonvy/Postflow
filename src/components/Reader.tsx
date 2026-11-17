import { useQueryClient } from "@tanstack/react-query";
import {
  Archive,
  ArchiveRestore,
  BookOpenText,
  Download,
  ExternalLink,
  ImageOff,
  Languages,
  Mail,
  MailOpen,
  Newspaper,
  Paperclip,
  Sparkles,
  Star,
  TriangleAlert,
  X,
} from "lucide-react";
import { type MouseEvent, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { useActions } from "../lib/actions";
import { api, type ItemDetail, type MailDetail, type Mailbox } from "../lib/api";
import { errorText, useLang, useT } from "../lib/i18n";
import { formatSize, openExternal, saveAttachment } from "../lib/platform";
import { keys, useItem } from "../lib/queries";
import { useUi } from "../lib/store";
import { fullTime } from "../lib/time";
import {
  languageName,
  type TranslateSource,
  useModelDownloads,
  useTranslation,
} from "../lib/translate";
import { isTauri } from "../lib/transport";
import { AiPanel } from "./AiPanel";
import { Avatar, Button, cx, Favicon, ICON, IconButton, Kbd, Spinner } from "./ui";

export function Reader() {
  const openId = useUi((s) => s.openId);

  return (
    <main className="flex h-full min-w-0 flex-1 flex-col border-l border-line bg-bg">
      {openId ? <Article key={openId} id={openId} /> : <Empty />}
    </main>
  );
}

function Toolbar({ children }: { children?: ReactNode }) {
  return (
    <header
      data-tauri-drag-region
      className="flex h-11 shrink-0 items-center justify-end gap-0.5 border-b border-line px-2"
    >
      {children}
    </header>
  );
}

/** 桌面应用里链接交给系统浏览器打开，不在应用窗口里跳转。 */
function interceptLinks(e: MouseEvent) {
  const link = (e.target as HTMLElement).closest("a[href]");
  if (!link || !isTauri) return;
  e.preventDefault();
  void openExternal((link as HTMLAnchorElement).href);
}

function Article({ id }: { id: string }) {
  const t = useT();
  const qc = useQueryClient();
  const [allowRemote, setAllowRemote] = useState(false);
  const { data: item, isLoading, error, refetch, isFetching } = useItem(id, allowRemote);
  const { items, apply, toggle, openOriginal, openTargets } = useActions();
  const autoRead = useRef<string | null>(null);
  const [mode, setMode] = useState<"feed" | "full">("feed");
  const [loadingFull, setLoadingFull] = useState(false);
  const [fullError, setFullError] = useState<string | null>(null);

  // 翻译的是此刻显示的正文：邮件正文，或者文章的订阅内容 / 全文
  const source = useMemo<TranslateSource | null>(() => {
    if (!item) return null;
    if (item.mail) {
      return { title: item.title, html: item.mail.body, document: item.mail.body_kind === "html" };
    }
    const html = mode === "full" && item.full_text ? item.full_text : (item.content ?? "");
    return { title: item.title, html, document: false };
  }, [item, mode]);
  const translation = useTranslation(source);
  const aiPanel = useUi((s) => s.aiPanel);
  const aiPanelFor = useUi((s) => s.aiPanelFor);
  const toggleAiPanel = useUi((s) => s.toggleAiPanel);
  const phase = translation.phase;
  const shown = phase.kind === "on" ? { title: phase.title, html: phase.html } : null;

  // T 键
  const translateRequest = useUi((s) => s.translateRequest);
  const seenRequest = useRef(translateRequest);
  useEffect(() => {
    if (translateRequest === seenRequest.current) return;
    seenRequest.current = translateRequest;
    translation.toggle();
  }, [translateRequest, translation]);

  // 打开即视为已读。每条只自动标一次，之后手动改回未读不会被再次覆盖。
  // 用列表里已有的数据判断，不等正文加载完：用键盘快速翻过的条目也要算读过。
  useEffect(() => {
    if (autoRead.current === id) return;
    const known = items.find((i) => i.id === id) ?? (item?.id === id ? item : undefined);
    if (!known) return;
    autoRead.current = id;
    if (!known.read) apply([known], { read: true });
  }, [apply, id, item, items]);

  const summary = item ?? items.find((i) => i.id === id);
  const isMail = summary?.kind === "mail";
  // 打开的是折叠着的一组时，按钮反映整组的状态，点下去也作用到整组
  const group = openTargets();
  const all = (flag: "read" | "starred" | "archived") =>
    group.length > 0 ? group.every((i) => i[flag]) : Boolean(summary?.[flag]);

  const toggleFullText = async () => {
    if (!item) return;
    if (mode === "full") {
      setMode("feed");
      return;
    }
    if (item.full_text) {
      setMode("full");
      return;
    }
    setLoadingFull(true);
    setFullError(null);
    try {
      const { full_text } = await api.fullText(item.id);
      qc.setQueryData<ItemDetail>(keys.item(item.id), (old) => (old ? { ...old, full_text } : old));
      if (useUi.getState().openId === item.id) setMode("full");
    } catch (err) {
      if (useUi.getState().openId === item.id) setFullError(errorText(t, err));
    } finally {
      setLoadingFull(false);
    }
  };

  return (
    <>
      <Toolbar>
        {summary && (
          <>
            {!isMail && summary.url && (
              <IconButton
                label={mode === "full" ? t("action.feedContent") : t("action.fullText")}
                active={mode === "full"}
                disabled={loadingFull || !item}
                onClick={() => void toggleFullText()}
              >
                {loadingFull ? (
                  <Spinner size={15} />
                ) : mode === "full" ? (
                  <Newspaper {...ICON} />
                ) : (
                  <BookOpenText {...ICON} />
                )}
              </IconButton>
            )}
            <IconButton label={t("action.ai")} shortcut="A" active={aiPanel} onClick={toggleAiPanel}>
              <Sparkles {...ICON} />
            </IconButton>
            <IconButton
              label={phase.kind === "on" ? t("action.showOriginal") : t("action.translate")}
              shortcut="T"
              active={phase.kind === "on"}
              disabled={!item}
              onClick={translation.toggle}
            >
              {phase.kind === "checking" || phase.kind === "translating" || phase.kind === "downloading" ? (
                <Spinner size={15} />
              ) : (
                <Languages {...ICON} />
              )}
            </IconButton>
            <IconButton
              label={all("read") ? t("action.markUnread") : t("action.markRead")}
              shortcut="U"
              onClick={() => toggle("read", group.length > 0 ? group : [summary])}
            >
              {all("read") ? <Mail {...ICON} /> : <MailOpen {...ICON} />}
            </IconButton>
            <IconButton
              label={all("starred") ? t("action.unstar") : t("action.star")}
              shortcut="S"
              active={all("starred")}
              onClick={() => toggle("starred", group.length > 0 ? group : [summary])}
            >
              <Star {...ICON} className={cx(all("starred") && "fill-warning text-warning")} />
            </IconButton>
            <IconButton
              label={all("archived") ? t("action.unarchive") : t("action.archive")}
              shortcut="E"
              onClick={() => toggle("archived", group.length > 0 ? group : [summary])}
            >
              {all("archived") ? <ArchiveRestore {...ICON} /> : <Archive {...ICON} />}
            </IconButton>
            {!isMail && summary.url && (
              <IconButton label={t("action.openOriginal")} shortcut="V" onClick={openOriginal}>
                <ExternalLink {...ICON} />
              </IconButton>
            )}
          </>
        )}
      </Toolbar>

      <div className="flex min-h-0 flex-1">
      <div className="min-h-0 min-w-0 flex-1 overflow-y-auto">
        {item ? (
          item.mail ? (
            <MailView
              item={item}
              mail={item.mail}
              shown={shown}
              bar={<TranslateBar translation={translation} />}
              onShowImages={() => setAllowRemote(true)}
              onTrusted={() => {
                void qc.invalidateQueries({ queryKey: ["item", item.id] });
              }}
            />
          ) : (
            <ArticleView
              item={item}
              mode={mode}
              loadingFull={loadingFull}
              fullError={fullError}
              shown={shown}
              bar={<TranslateBar translation={translation} />}
            />
          )
        ) : error ? (
          <div className="flex h-full flex-col items-center justify-center gap-3 px-8 text-center">
            <TriangleAlert size={24} strokeWidth={1.5} className="text-fg-4" />
            <p className="max-w-[320px] text-fg-3">{errorText(t, error)}</p>
            <Button busy={isFetching} onClick={() => void refetch()}>
              {t("action.retry")}
            </Button>
          </div>
        ) : (
          <div className="flex h-full flex-col items-center justify-center gap-3 text-fg-3">
            {isLoading && <Spinner size={16} />}
            {isLoading && isMail && <p className="text-[12px]">{t("mail.loading")}</p>}
          </div>
        )}
      </div>
      {aiPanel && item && <AiPanel item={item} autoRun={aiPanelFor === item.id} />}
      </div>
    </>
  );
}

/** 译文开着时显示的标题和正文。 */
type Shown = { title: string; html: string } | null;

function ArticleView({
  item,
  mode,
  loadingFull,
  fullError,
  shown,
  bar,
}: {
  item: ItemDetail;
  mode: "feed" | "full";
  loadingFull: boolean;
  fullError: string | null;
  shown: Shown;
  bar: ReactNode;
}) {
  const t = useT();
  const lang = useLang();
  const original = mode === "full" && item.full_text ? item.full_text : (item.content ?? "");
  const html = shown?.html ?? original;
  return (
    <article className="mx-auto max-w-[720px] px-10 pb-24 pt-10">
      <h1 className="cursor-auto select-text text-[26px] font-semibold leading-[1.25] tracking-[-0.02em]">
        {shown?.title || item.title || t("reader.untitled")}
      </h1>
      <div className="mb-8 mt-4 flex flex-wrap items-center gap-x-2 gap-y-1 text-[13px] text-fg-3">
        <Favicon src={item.source_icon} />
        <span className="font-medium text-fg-2">{item.source_title}</span>
        {item.author && (
          <>
            <span aria-hidden>·</span>
            <span>{item.author}</span>
          </>
        )}
        <span aria-hidden>·</span>
        <time dateTime={new Date(item.published_at).toISOString()}>{fullTime(item.published_at, lang)}</time>
      </div>
      {bar}
      {fullError && (
        <p className="mb-6 rounded-md border border-line bg-hover px-3 py-2 text-[13px] text-fg-2">{fullError}</p>
      )}
      {loadingFull && (
        <p className="mb-6 flex items-center gap-2 text-[13px] text-fg-3">
          <Spinner size={13} />
          {t("reader.loadingFullText")}
        </p>
      )}
      {/* 内容在核心里已经净化过，见 postflow-feed 的 sanitize_html */}
      <div className="article" onClick={interceptLinks} dangerouslySetInnerHTML={{ __html: html }} />
    </article>
  );
}

function formatPeople(list: Mailbox[]): string {
  return list.map((m) => m.name || m.address || "").filter(Boolean).join(", ");
}

function MailView({
  item,
  mail,
  shown,
  bar,
  onShowImages,
  onTrusted,
}: {
  item: ItemDetail;
  mail: MailDetail;
  shown: Shown;
  bar: ReactNode;
  onShowImages: () => void;
  onTrusted: () => void;
}) {
  const t = useT();
  const lang = useLang();
  const sender = mail.from.name || mail.from.address || item.author || "";
  const to = formatPeople(mail.to);
  const cc = formatPeople(mail.cc);

  const trust = async () => {
    if (!mail.from.address) return;
    await api.trustSender(mail.from.address);
    onTrusted();
  };

  return (
    <article className="mx-auto max-w-[760px] px-8 pb-24 pt-8">
      <h1 className="cursor-auto select-text text-[22px] font-semibold leading-[1.3] tracking-[-0.015em]">
        {shown?.title || item.title || t("mail.noSubject")}
      </h1>

      <div className="mb-6 mt-5 flex items-start gap-3">
        <Avatar name={sender || "?"} seed={mail.from.address} size={32} />
        <div className="min-w-0 flex-1 select-text">
          <div className="flex min-w-0 items-baseline gap-2">
            <span className="truncate font-medium text-fg">{sender}</span>
            {mail.from.name && mail.from.address && (
              <span className="truncate text-[12px] text-fg-3">{mail.from.address}</span>
            )}
          </div>
          {(to || cc) && (
            <div className="mt-0.5 truncate text-[12px] text-fg-3" title={[to, cc].filter(Boolean).join(" · ")}>
              {to && (
                <>
                  {t("mail.to")} {to}
                </>
              )}
              {cc && (
                <>
                  {to && " · "}
                  {t("mail.cc")} {cc}
                </>
              )}
            </div>
          )}
        </div>
        <div className="shrink-0 text-right text-[12px] leading-relaxed text-fg-3">
          <time dateTime={new Date(item.published_at).toISOString()}>{fullTime(item.published_at, lang)}</time>
          <div className="truncate">{item.source_title}</div>
        </div>
      </div>

      {mail.has_remote_content && !mail.remote_allowed && (
        <div className="mb-5 flex flex-wrap items-center gap-x-3 gap-y-2 rounded-md border border-line bg-hover px-3 py-2 text-[12px] text-fg-2">
          <ImageOff size={14} strokeWidth={1.75} className="shrink-0 text-fg-3" />
          <span className="min-w-0 flex-1">{t("mail.remoteBlocked")}</span>
          <div className="flex gap-1">
            <Button className="h-6 px-2 text-[12px]" onClick={onShowImages}>
              {t("action.showImages")}
            </Button>
            {mail.from.address && (
              <Button variant="ghost" className="h-6 px-2 text-[12px]" onClick={() => void trust()}>
                {t("action.alwaysShowImages")}
              </Button>
            )}
          </div>
        </div>
      )}

      {bar}

      {mail.body_kind === "html" ? (
        <MailFrame html={shown?.html ?? mail.body} />
      ) : (
        // 纯文本邮件在核心里已转义成安全的 HTML，见 postflow-mail 的 text_to_html
        <div
          className="article"
          onClick={interceptLinks}
          dangerouslySetInnerHTML={{ __html: shown?.html ?? mail.body }}
        />
      )}

      {mail.attachments.length > 0 && <Attachments itemId={item.id} mail={mail} />}
    </article>
  );
}

/**
 * HTML 邮件放进沙箱 iframe：不允许脚本，只允许同源，好让外层量出内容高度、接管链接点击。
 * 文档自带内容安全策略，远程图片是否加载由核心决定。
 */
function MailFrame({ html }: { html: string }) {
  const ref = useRef<HTMLIFrameElement>(null);
  const [height, setHeight] = useState(240);

  useEffect(() => {
    const iframe = ref.current;
    if (!iframe) return;
    let observer: ResizeObserver | undefined;

    const onLoad = () => {
      const doc = iframe.contentDocument;
      if (!doc) return;
      const measure = () => {
        const h = Math.max(doc.documentElement.scrollHeight, doc.body?.scrollHeight ?? 0);
        setHeight(Math.max(80, Math.ceil(h)));
      };
      measure();
      observer?.disconnect();
      observer = new ResizeObserver(measure);
      observer.observe(doc.documentElement);
      for (const img of Array.from(doc.images)) img.addEventListener("load", measure);
      doc.addEventListener("click", (e) => {
        // iframe 里的节点来自另一个全局环境，不能用 instanceof 判断
        const target = e.target as { closest?: (s: string) => HTMLAnchorElement | null } | null;
        const link = target?.closest?.("a[href]");
        if (!link) return;
        e.preventDefault();
        void openExternal(link.getAttribute("href") ?? "");
      });
    };

    iframe.addEventListener("load", onLoad);
    return () => {
      iframe.removeEventListener("load", onLoad);
      observer?.disconnect();
    };
  }, [html]);

  return (
    <iframe
      ref={ref}
      title="mail"
      sandbox="allow-same-origin"
      referrerPolicy="no-referrer"
      srcDoc={html}
      style={{ height }}
      className="block w-full rounded-lg border border-line bg-white"
    />
  );
}

/** 正文上方的一条翻译状态：要不要下载模型、下载进度、译自哪种语言。 */
function TranslateBar({ translation }: { translation: ReturnType<typeof useTranslation> }) {
  const t = useT();
  const lang = useLang();
  const downloads = useModelDownloads((s) => s.byPair);
  const { phase, to, download, off, retry } = translation;
  const name = (code: string) => languageName(code, lang);

  const bar = (icon: ReactNode, text: ReactNode, actions?: ReactNode) => (
    <div className="mb-5 flex flex-wrap items-center gap-x-3 gap-y-2 rounded-md border border-line bg-hover px-3 py-2 text-[12px] text-fg-2">
      <span className="flex shrink-0 text-fg-3">{icon}</span>
      <div className="min-w-0 flex-1">{text}</div>
      {actions && <div className="flex gap-1">{actions}</div>}
    </div>
  );
  const icon = <Languages size={14} strokeWidth={1.75} />;
  const small = "h-6 px-2 text-[12px]";

  switch (phase.kind) {
    case "off":
      return null;
    case "checking":
    case "translating":
      return bar(<Spinner size={13} />, t(phase.kind === "checking" ? "translate.checking" : "translate.translating"));
    case "need": {
      const size = formatSize(phase.plan.missing.reduce((n, m) => n + m.size, 0));
      const text =
        phase.plan.missing.length > 1 && phase.plan.from
          ? t("translate.needPivot", { from: name(phase.plan.from), to: name(to), size })
          : t("translate.need", { to: name(to), size });
      return bar(
        icon,
        text,
        <>
          <Button variant="primary" className={small} onClick={() => download(phase.plan)}>
            {t("translate.download")}
          </Button>
          <Button variant="ghost" className={small} onClick={off}>
            {t("translate.cancel")}
          </Button>
        </>,
      );
    }
    case "downloading": {
      const states = phase.plan.missing.map((m) => downloads[m.pair]);
      const received = states.reduce((n, s) => n + (s?.received ?? 0), 0);
      const total = states.reduce((n, s) => n + (s?.total ?? 0), 0);
      const percent = total > 0 ? Math.min(100, Math.floor((received / total) * 100)) : 0;
      return bar(
        <Spinner size={13} />,
        <div className="flex items-center gap-3">
          <span className="shrink-0">{t("translate.downloading")}</span>
          <div className="h-1 min-w-[80px] flex-1 overflow-hidden rounded-full bg-active">
            <div className="h-full rounded-full bg-accent transition-[width]" style={{ width: `${percent}%` }} />
          </div>
          <span className="tabular w-16 shrink-0 text-right text-fg-3">
            {total > 0 ? `${percent}% · ${formatSize(total)}` : ""}
          </span>
        </div>,
      );
    }
    case "on":
      return bar(
        icon,
        t("translate.done", { from: name(phase.from), to: name(to) }),
        <Button variant="ghost" className={small} onClick={off}>
          {t("action.showOriginal")}
        </Button>,
      );
    case "same":
      return bar(
        icon,
        t("translate.same", { lang: name(phase.from) }),
        <IconButton label={t("translate.cancel")} className="h-6 w-6" onClick={off}>
          <X size={13} strokeWidth={1.75} />
        </IconButton>,
      );
    case "error":
      return bar(
        <TriangleAlert size={14} strokeWidth={1.75} />,
        errorText(t, phase.error),
        <>
          <Button className={small} onClick={() => void retry()}>
            {t("action.retry")}
          </Button>
          <Button variant="ghost" className={small} onClick={off}>
            {t("translate.cancel")}
          </Button>
        </>,
      );
  }
}

function Attachments({ itemId, mail }: { itemId: string; mail: MailDetail }) {
  const t = useT();
  const [busy, setBusy] = useState<number | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const save = async (index: number, name: string) => {
    setBusy(index);
    setNotice(null);
    try {
      if (await saveAttachment(itemId, index, name)) setNotice(t("mail.saved", { name }));
    } catch (err) {
      setNotice(errorText(t, err));
    } finally {
      setBusy(null);
    }
  };

  return (
    <section className="mt-6 border-t border-line pt-4">
      <h2 className="mb-2 flex items-center gap-1.5 text-[12px] font-medium text-fg-3">
        <Paperclip size={13} strokeWidth={1.75} />
        {t("mail.attachments", { n: mail.attachments.length })}
      </h2>
      <ul className="flex flex-wrap gap-2">
        {mail.attachments.map((a) => (
          <li key={a.index}>
            <button
              type="button"
              disabled={busy !== null}
              onClick={() => void save(a.index, a.name)}
              title={t("action.download")}
              className="group flex h-9 max-w-[280px] items-center gap-2 rounded-md border border-line bg-hover pl-2.5 pr-2 text-left transition-colors hover:bg-active disabled:opacity-60"
            >
              <span className="min-w-0 flex-1 truncate text-[13px] text-fg">{a.name}</span>
              <span className="tabular shrink-0 text-[12px] text-fg-3">{formatSize(a.size)}</span>
              {busy === a.index ? (
                <Spinner size={13} />
              ) : (
                <Download size={14} strokeWidth={1.75} className="shrink-0 text-fg-3 group-hover:text-fg" />
              )}
            </button>
          </li>
        ))}
      </ul>
      {notice && <p className="mt-2 text-[12px] text-fg-3">{notice}</p>}
    </section>
  );
}

function Empty() {
  const t = useT();
  const tabs = useUi((s) => s.sidebarLayout === "tabs");
  const hints: [string[], string][] = [
    ...(tabs ? ([[["1", "2", "3"], t("reader.hintTabs")]] as [string[], string][]) : []),
    [["J", "K"], t("reader.hintMove")],
    [["E"], t("reader.hintArchive")],
    [["S"], t("reader.hintStar")],
    [["U"], t("reader.hintRead")],
    [["V"], t("reader.hintOpen")],
    [["X"], t("reader.hintSelect")],
    [["T"], t("reader.hintTranslate")],
    [["A"], t("reader.hintAi")],
    [["→", "←"], t("reader.hintGroup")],
  ];
  return (
    <>
      <Toolbar />
      <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-6">
        <p className="text-fg-3">{t("reader.empty")}</p>
        <dl className="grid grid-cols-[auto_auto] items-center gap-x-4 gap-y-2 text-[12px] text-fg-3">
          {hints.map(([keysList, label]) => (
            <div key={label} className="contents">
              <dt className="flex justify-end gap-1">
                {keysList.map((k) => (
                  <Kbd key={k}>{k}</Kbd>
                ))}
              </dt>
              <dd>{label}</dd>
            </div>
          ))}
        </dl>
      </div>
    </>
  );
}
