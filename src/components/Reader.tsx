import { useQueryClient } from "@tanstack/react-query";
import {
  AlarmClock,
  Archive,
  ArchiveRestore,
  Bookmark,
  BookOpenText,
  Check,
  ChevronLeft,
  Download,
  Copy,
  ExternalLink,
  FilePen,
  Forward,
  Globe,
  ImageOff,
  Link,
  Languages,
  Mail,
  MailOpen,
  Newspaper,
  Paperclip,
  Reply,
  ReplyAll,
  Sparkles,
  Star,
  Tag,
  Trash2,
  TriangleAlert,
  X,
  ChevronDown,
  Ellipsis,
  Expand,
  FileCode,
  Maximize2,
  Minimize2,
  Shrink,
} from "lucide-react";
import type { Item } from "../lib/api";
import {
  type CSSProperties,
  type MouseEvent,
  type ReactNode,
  type RefObject,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useActions } from "../lib/actions";
import { api, type ItemDetail, type MailDetail, type Mailbox } from "../lib/api";
import { errorText, useLang, useT } from "../lib/i18n";
import { copyText, formatSize, hasInsetTitlebar, isMac, openExternal, saveAttachment } from "../lib/platform";
import { useReadingTracker } from "../lib/reading";
import { keys, useItem, useNav } from "../lib/queries";
import { mailZoom, readerVars, readerWidth, titleSize } from "../lib/readerStyle";
import { useDrawerSidebar, useIsMobile, useReaderOverlay } from "../lib/mobile";
import { SnoozeButton, snoozeTime } from "./Snooze";
import { canArchive, canSnooze, useArticleActions, useReaderStyle, useToolbarLayout, useUi } from "../lib/store";
import { toolsAt } from "../lib/toolbars";
import { fullTime } from "../lib/time";
import { readingMinutes, totalWords, wordCount } from "../lib/wordCount";
import {
  languageName,
  type TranslateSource,
  useModelDownloads,
  useTranslation,
} from "../lib/translate";
import { isTauri } from "../lib/transport";
import { isWebUrl } from "../lib/webDevice";
import { AiPanel } from "./AiPanel";
import { setFullscreen, useFullscreen } from "../lib/fullscreen";
import { PluginSections } from "./PluginSections";
import { StatusBar, ToolbarMenu, Tools } from "./Toolbars";
import { useWindowControlsGap } from "./WindowControls";
import { ItemMenuItems, useSetLabel } from "./ItemMenu";
import { MediaAttachment } from "./Player";
import { ReaderStyleMenu } from "./ReaderStyleMenu";
import { AccountIcon, ArticleIcon, SenderAvatar, SourceIcon } from "./SourceIcon";
import { useCategories, useLabelColor } from "../lib/labels";
import { useSaveLater } from "./ReadLater";
import { HighlightLayer, HighlightList } from "./Highlights";
import { InviteCard } from "./InviteCard";
import { ReadAloud } from "./ReadAloud";
import { SenderCard } from "./SenderCard";
import { WebPage } from "./WebPage";
import {
  Button,
  ContextMenu,
  cx,
  ICON,
  IconButton,
  Kbd,
  Menu,
  MenuItem,
  MenuSeparator,
  Modal,
  Spinner,
  Tip,
} from "./ui";

/** 快捷键提示里的修饰键。 */
const MOD = isMac ? "⌘" : "Ctrl+";

export function Reader() {
  const openId = useUi((s) => s.openId);

  return (
    <main className="flex h-full min-w-0 flex-1 flex-col border-l border-line bg-bg">
      {openId ? <Article key={openId} id={openId} /> : <Empty />}
    </main>
  );
}

function Toolbar({ leading, children }: { leading?: ReactNode; children?: ReactNode }) {
  const t = useT();
  // 阅读区的顶栏总在窗口最右上角（AI 栏在它下面），窗口按钮的位置由它留
  // 阅读区弹出来、又没铺满窗口时，右上角是列表的顶栏，不用给窗口按钮留位置
  const overlay = useReaderOverlay();
  const popupFull = useUi((s) => s.readerMaximized);
  const gap = useWindowControlsGap(!overlay || popupFull);
  // 手机上阅读区占满屏幕，左上角返回列表；按钮放不下时左右滑动
  const mobile = useIsMobile();
  // 平板竖屏时阅读区也不宽，按钮放不下时同样左右滑动
  const narrow = useDrawerSidebar();
  const close = () => useUi.getState().open(null);
  // 最大化时侧栏没了，macOS 的红绿灯落在阅读区顶栏上，左边要给它们留位置（全屏时没有红绿灯）
  const maximized = useUi((s) => s.readerMaximized);
  const fullscreen = useFullscreen((s) => s.on);
  const inset = maximized && !fullscreen && hasInsetTitlebar ? 78 : 0;
  return (
    <ToolbarMenu toolbar="reader">
    <header
      data-tauri-drag-region
      className="flex h-11 shrink-0 touch:h-13 items-center gap-0.5 border-b border-line px-2"
      style={{ ...(gap ? { paddingRight: gap + 8 } : {}), ...(inset ? { paddingLeft: inset } : {}) }}
    >
      {mobile && (
        <IconButton label={t("mobile.back")} onClick={close}>
          <ChevronLeft {...ICON} />
        </IconButton>
      )}
      <div data-tauri-drag-region className={cx("flex min-w-0 flex-1 items-center", mobile ? "pr-1" : "pr-2")}>
        {leading}
      </div>
      {narrow ? (
        <div className="no-scrollbar flex min-w-0 max-w-[calc(100%-44px)] items-center gap-0.5 overflow-x-auto">{children}</div>
      ) : (
        children
      )}
      {/* 弹出的阅读区：最右边一个关闭按钮（手机上左上角已经有返回） */}
      {overlay && !mobile && (
        <>
          <span aria-hidden className="mx-1 h-4 w-px shrink-0 bg-line-strong" />
          <IconButton label={t("action.close")} shortcut="Esc" onClick={close}>
            <X {...ICON} />
          </IconButton>
        </>
      )}
    </header>
    </ToolbarMenu>
  );
}

/** 工具栏左边：这一条来自哪个订阅源或邮箱，点一下列表换成那里的全部内容。 */
function SourceChip({ item }: { item: Item }) {
  const t = useT();
  const { goToSource } = useActions();
  if (!item.source_title) return null;
  return (
    <Tip label={t("source.open", { name: item.source_title })}>
      <button
        type="button"
        onClick={() => goToSource(item)}
        className="flex h-7 min-w-0 max-w-full touch:h-10 items-center gap-2 rounded-md px-1.5 text-fg-2 transition-colors hover:bg-hover hover:text-fg"
      >
        <SourceIcon item={item} size={18} />
        <span className="min-w-0 truncate text-[13px] font-medium">{item.source_title}</span>
      </button>
    </Tip>
  );
}

/** 在阅读区里点右键时，鼠标下面是什么：选中的文字、链接、图片。 */
interface ContextInfo {
  selection: string;
  link: string | null;
  image: string | null;
}

const EMPTY_CONTEXT: ContextInfo = { selection: "", link: null, image: null };

/** 触屏上长按是选文字，不弹菜单。 */
const coarsePointer = typeof matchMedia === "function" && matchMedia("(pointer: coarse)").matches;

/** 阅读区的右键菜单：先是针对鼠标下内容的操作，然后是这一条的操作，最后是阅读区的开关。 */
function ReaderMenu({
  context,
  fallback,
  toggles,
}: {
  context: { current: ContextInfo };
  fallback: Item;
  toggles: ReactNode;
}) {
  const t = useT();
  const { openTargets } = useActions();
  const saveLater = useSaveLater();
  const [info] = useState(() => context.current);
  const group = openTargets();
  const targets = group.length > 0 ? group : [fallback];
  const link = info.link && /^(https?:|mailto:)/i.test(info.link) ? info.link : null;
  const image = info.image && /^https?:/i.test(info.image) ? info.image : null;
  const specific = Boolean(info.selection || link || image);
  return (
    <>
      {info.selection && (
        <MenuItem icon={<Copy size={14} />} onSelect={() => void copyText(info.selection)}>
          {t("menu.copySelection")}
        </MenuItem>
      )}
      {link && (
        <>
          <MenuItem icon={<ExternalLink size={14} />} onSelect={() => void openExternal(link)}>
            {t("menu.openLink")}
          </MenuItem>
          <MenuItem icon={<Link size={14} />} onSelect={() => void copyText(link.replace(/^mailto:/i, ""))}>
            {t("menu.copyLink")}
          </MenuItem>
          {/^https?:/i.test(link) && (
            <MenuItem icon={<Bookmark size={14} />} onSelect={() => void saveLater(link)}>
              {t("menu.saveLater")}
            </MenuItem>
          )}
        </>
      )}
      {image && (
        <MenuItem icon={<ImageOff size={14} />} onSelect={() => void copyText(image)}>
          {t("menu.copyImage")}
        </MenuItem>
      )}
      {specific && <MenuSeparator />}
      <ItemMenuItems items={targets} />
      <MenuSeparator />
      {toggles}
    </>
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
  const { items, apply, toggle, openOriginal, openTargets, compose, deleteDrafts } = useActions();
  const fullscreenOn = useFullscreen((s) => s.on);
  // 「更多」里只放工具栏上没有的：用户把哪个按钮放回了工具栏，菜单里就不再重复
  const readerLayout = useToolbarLayout("reader");
  const onBar = new Set([...toolsAt(readerLayout, "top"), ...toolsAt(readerLayout, "status")]);
  const autoRead = useRef<string | null>(null);
  const [mode, setMode] = useState<"feed" | "full">("feed");
  const [loadingFull, setLoadingFull] = useState(false);
  const [fullError, setFullError] = useState<string | null>(null);

  // 翻译的是此刻显示的正文：邮件正文，或者文章的订阅内容 / 全文
  const source = useMemo<TranslateSource | null>(() => {
    if (!item) return null;
    if (item.mail) {
      return { id: item.id, title: item.title, html: item.mail.body, document: item.mail.body_kind === "html" };
    }
    const html = mode === "full" && item.full_text ? item.full_text : (item.content ?? "");
    return { id: item.id, title: item.title, html, document: false };
  }, [item, mode]);
  const translation = useTranslation(source);
  // 翻译开关开着：打开的每一条都自动翻译；原文已经是目标语言时什么都不显示
  const readerTranslate = useUi((s) => s.readerTranslate);
  const setReaderTranslate = useUi((s) => s.setReaderTranslate);
  const autoTranslated = useRef(false);
  useEffect(() => {
    if (!readerTranslate || !source || autoTranslated.current || translation.phase.kind !== "off") return;
    autoTranslated.current = true;
    translation.auto();
  }, [readerTranslate, source, translation]);

  /** 翻译按钮和 T 键：切换开关。打开时这一条马上翻译，关上时回到原文。 */
  const toggleTranslate = () => {
    if (readerTranslate || translation.phase.kind !== "off") {
      setReaderTranslate(false);
      translation.off();
    } else {
      setReaderTranslate(true);
      autoTranslated.current = true;
      translation.toggle();
    }
  };
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
    toggleTranslate();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [translateRequest]);

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

  const readerFullText = useUi((s) => s.readerFullText);
  const setReaderFullText = useUi((s) => s.setReaderFullText);
  // 这个订阅源设了「打开时自动取全文」
  const nav = useNav().data;
  const feedFullText = nav?.feeds.find((f) => f.id === summary?.source_id)?.full_text === true;
  // 从 Pocket 之类导入的稍后读只有地址，打开时去抓正文
  const laterWithoutText = !!summary && summary.source_id === nav?.later_id && !item?.content?.trim();

  // 原网页模式：开着时有网页链接的条目直接显示原网页，没有链接的（多数邮件）照常显示正文
  const readerWeb = useUi((s) => s.readerWeb);
  const setReaderWeb = useUi((s) => s.setReaderWeb);
  const webUrl = summary && isWebUrl(summary.url) ? summary.url : null;
  const webShown = readerWeb && webUrl !== null;

  // 右键点在了什么上面。HTML 邮件在 iframe 里，由 MailFrame 记下来
  const context = useRef<ContextInfo>(EMPTY_CONTEXT);
  const captureContext = (e: MouseEvent) => {
    const target = e.target as HTMLElement;
    if (target.tagName === "IFRAME") return;
    context.current = {
      selection: window.getSelection()?.toString() ?? "",
      link: target.closest<HTMLAnchorElement>("a[href]")?.href ?? null,
      image: target instanceof HTMLImageElement ? target.currentSrc || target.src : null,
    };
  };

  // 订阅统计：这篇文章看了多久、看到了哪里。邮件不记
  const scrollRef = useRef<HTMLDivElement>(null);
  useReadingTracker(id, scrollRef, Boolean(item), summary?.kind === "article" && !webShown);

  /** 显示全文：抓过的直接用，没抓过的去原网页抓。 */
  const showFullText = async () => {
    if (!item) return;
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

  /** 全文按钮：切换开关。开着时之后打开的每篇文章都直接显示全文。 */
  const toggleFullText = () => {
    if (readerFullText || mode === "full") {
      setReaderFullText(false);
      setMode("feed");
    } else {
      setReaderFullText(true);
      void showFullText();
    }
  };

  // 全文开关开着、或者这个源设了自动取全文：文章一打开就显示全文
  const autoFull = useRef(false);
  useEffect(() => {
    if (!(readerFullText || feedFullText || laterWithoutText) || autoFull.current || !item || item.mail || !item.url) return;
    autoFull.current = true;
    void showFullText();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [item, readerFullText, feedFullText, laterWithoutText]);


  /** 工具栏上的按钮。放在顶部还是状态栏、按什么顺序由用户的布局决定（Toolbars.tsx）。用不上的是 null。 */
  const targets = summary ? (group.length > 0 ? group : [summary]) : [];
  // 文章的归档、稍后提醒跟着订阅那边显示不显示这两个视图走
  const allowed = useArticleActions();
  const tools = !summary
    ? {}
    : {
        editDraft: summary.draft ? (
          <IconButton label={t("compose.editDraft")} onClick={() => compose("draft", summary)}>
            <FilePen {...ICON} />
          </IconButton>
        ) : null,
        deleteDraft: summary.draft ? (
          <IconButton label={t("compose.deleteDraft")} onClick={() => deleteDrafts([summary])}>
            <Trash2 {...ICON} />
          </IconButton>
        ) : null,
        // 回复旁边一个小箭头：全部回复、转发
        reply:
          isMail && !summary.draft ? (
            <span className="inline-flex items-center">
              <IconButton label={t("compose.reply")} shortcut={MOD + "R"} onClick={() => compose("reply", summary)}>
                <Reply {...ICON} />
              </IconButton>
              <Menu
                align="end"
                trigger={
                  <button
                    type="button"
                    aria-label={t("reader.replyMore")}
                    title={t("reader.replyMore")}
                    className="-ml-1 inline-flex h-7 w-4 shrink-0 touch:h-10 touch:w-6 items-center justify-center rounded-md text-fg-3 transition-colors hover:bg-hover hover:text-fg data-[state=open]:bg-hover"
                  >
                    <ChevronDown size={12} strokeWidth={2} />
                  </button>
                }
              >
                <MenuItem icon={<ReplyAll size={14} />} onSelect={() => compose("reply-all", summary)}>
                  {t("compose.replyAll")}
                </MenuItem>
                <MenuItem icon={<Forward size={14} />} onSelect={() => compose("forward", summary)}>
                  {t("compose.forward")}
                </MenuItem>
              </Menu>
            </span>
          ) : null,
        replyAll:
          isMail && !summary.draft ? (
            <IconButton
              label={t("compose.replyAll")}
              shortcut={(isMac ? "⇧⌘" : "Ctrl+Shift+") + "R"}
              onClick={() => compose("reply-all", summary)}
            >
              <ReplyAll {...ICON} />
            </IconButton>
          ) : null,
        forward:
          isMail && !summary.draft ? (
            <IconButton
              label={t("compose.forward")}
              shortcut={(isMac ? "⇧⌘" : "Ctrl+Shift+") + "F"}
              onClick={() => compose("forward", summary)}
            >
              <Forward {...ICON} />
            </IconButton>
          ) : null,
        style: !webShown ? <ReaderStyleMenu /> : null,
        fullText:
          !isMail && summary.url && !webShown ? (
            <IconButton
              label={readerFullText ? t("action.fullTextOff") : t("action.fullTextOn")}
              active={readerFullText || mode === "full"}
              disabled={loadingFull || !item}
              onClick={toggleFullText}
            >
              {loadingFull ? <Spinner size={15} /> : mode === "full" ? <Newspaper {...ICON} /> : <BookOpenText {...ICON} />}
            </IconButton>
          ) : null,
        web: (
          <IconButton
            label={readerWeb ? (webUrl ? t("web.toggleOff") : t("web.noLink")) : t("web.toggleOn")}
            shortcut="W"
            active={readerWeb}
            onClick={() => setReaderWeb(!readerWeb)}
          >
            <Globe {...ICON} />
          </IconButton>
        ),
        label: <LabelMenu ids={targets.map((i) => i.id)} current={summary.label} />,
        snooze:
          summary.draft || summary.sent || !canSnooze(targets, allowed) ? null : <SnoozeButton item={summary} />,
        ai: (
          <IconButton label={t("action.ai")} shortcut="A" active={aiPanel} onClick={toggleAiPanel}>
            <Sparkles {...ICON} />
          </IconButton>
        ),
        translate: !webShown ? (
          <IconButton
            label={readerTranslate ? t("action.translateOff") : t("action.translateOn")}
            shortcut="T"
            active={readerTranslate || phase.kind === "on"}
            disabled={!item}
            onClick={toggleTranslate}
          >
            {phase.kind === "checking" || phase.kind === "translating" || phase.kind === "downloading" ? (
              <Spinner size={15} />
            ) : (
              <Languages {...ICON} />
            )}
          </IconButton>
        ) : null,
        read: (
          <IconButton
            label={all("read") ? t("action.markUnread") : t("action.markRead")}
            shortcut="U"
            onClick={() => toggle("read", targets)}
          >
            {all("read") ? <Mail {...ICON} /> : <MailOpen {...ICON} />}
          </IconButton>
        ),
        star: (
          <IconButton
            label={all("starred") ? t("action.unstar") : t("action.star")}
            shortcut="S"
            // 星标自己有黄色的实心样式，不再加强调色底
            aria-pressed={all("starred")}
            onClick={() => toggle("starred", targets)}
          >
            <Star {...ICON} className={cx(all("starred") && "fill-warning text-warning")} />
          </IconButton>
        ),
        archive:
          !summary.draft && !summary.sent && canArchive(targets, allowed) ? (
            <IconButton
              label={all("archived") ? t("action.unarchive") : t("action.archive")}
              shortcut="E"
              onClick={() => toggle("archived", targets)}
            >
              {all("archived") ? <ArchiveRestore {...ICON} /> : <Archive {...ICON} />}
            </IconButton>
          ) : null,
        original:
          !isMail && summary.url ? (
            <IconButton label={t("action.openOriginal")} shortcut="V" onClick={openOriginal}>
              <ExternalLink {...ICON} />
            </IconButton>
          ) : null,
        maximize: <MaximizeButton />,
        fullscreen: <FullscreenButton />,
        // 不常用的放这里：标为未读、在浏览器中打开、全文、查看源代码、全屏
        more: (
          <Menu
            align="end"
            trigger={
              <button
                type="button"
                aria-label={t("tool.more")}
                title={t("tool.more")}
                className="inline-flex size-7 shrink-0 touch:size-10 items-center justify-center rounded-md text-fg-2 transition-colors hover:bg-hover hover:text-fg data-[state=open]:bg-hover"
              >
                <Ellipsis {...ICON} />
              </button>
            }
          >
            {!onBar.has("read") && (
              <MenuItem
                icon={all("read") ? <Mail size={14} /> : <MailOpen size={14} />}
                onSelect={() => toggle("read", targets)}
              >
                {all("read") ? t("action.markUnread") : t("action.markRead")}
              </MenuItem>
            )}
            {summary.url && !onBar.has("original") && (
              <MenuItem icon={<ExternalLink size={14} />} onSelect={openOriginal}>
                {t("action.openOriginal")}
              </MenuItem>
            )}
            {!isMail && summary.url && !webShown && !onBar.has("fullText") && (
              <MenuItem icon={<BookOpenText size={14} />} onSelect={toggleFullText}>
                {readerFullText ? t("action.fullTextOff") : t("action.fullTextOn")}
              </MenuItem>
            )}
            <MenuItem
              icon={<FileCode size={14} />}
              onSelect={() => useUi.getState().setDialog({ type: "source", itemId: summary.id, title: summary.title })}
            >
              {isMail ? t("source.mail") : t("source.xml")}
            </MenuItem>
            {!onBar.has("fullscreen") && (
              <>
                <MenuSeparator />
                <MenuItem icon={<Expand size={14} />} onSelect={() => void setFullscreen(!fullscreenOn)}>
                  {fullscreenOn ? t("reader.exitFullscreen") : t("reader.fullscreen")}
                </MenuItem>
              </>
            )}
          </Menu>
        ),
      };

  return (
    <>
      <Toolbar leading={summary && <SourceChip item={summary} />}>
        <Tools toolbar="reader" place="top" tools={tools} />
      </Toolbar>

      {summary && (summary.snoozed_until ?? 0) > Date.now() && <SnoozedBar item={summary} />}
      <div className="relative flex min-h-0 flex-1">
      {webShown && webUrl ? (
        <WebPage url={webUrl} onShowContent={() => setReaderWeb(false)} />
      ) : (
      <ContextMenu
        bare
        disabled={coarsePointer || !summary}
        onContextMenu={captureContext}
        items={
          summary && (
            <ReaderMenu
              context={context}
              fallback={summary}
              toggles={
                <>
                  {item && (
                    <MenuItem icon={<Languages size={14} />} onSelect={toggleTranslate}>
                      {readerTranslate || phase.kind !== "off" ? t("menu.translateOff") : t("menu.translate")}
                    </MenuItem>
                  )}
                  <MenuItem icon={<Sparkles size={14} />} onSelect={toggleAiPanel}>
                    {t("action.ai")}
                  </MenuItem>
                  {webUrl && (
                    <MenuItem icon={<Globe size={14} />} onSelect={() => setReaderWeb(true)}>
                      {t("web.toggleOn")}
                    </MenuItem>
                  )}
                </>
              }
            />
          )
        }
      >
      <div ref={scrollRef} className="min-h-0 min-w-0 flex-1 overflow-y-auto">
        {item ? (
          item.mail ? (
            <MailView
              item={item}
              mail={item.mail}
              onEditDraft={() => compose("draft", item)}
              shown={shown}
              bar={<TranslateBar translation={translation} />}
              onShowImages={() => setAllowRemote(true)}
              onTrusted={() => {
                void qc.invalidateQueries({ queryKey: ["item", item.id] });
              }}
              onContext={(info) => {
                context.current = info;
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
      </ContextMenu>
      )}
      {aiPanel && item && <AiPanel item={item} autoRun={aiPanelFor === item.id} />}
      </div>
      <StatusBar toolbar="reader" tools={tools} info={summary && <ReaderInfo summary={summary} item={item} mode={mode} scrollRef={scrollRef} />} />
    </>
  );
}

/** 阅读区状态栏左边：发布时间、字数、预计阅读时间、读到了哪里。 */
function ReaderInfo({
  summary,
  item,
  mode,
  scrollRef,
}: {
  summary: Item;
  item: ItemDetail | undefined;
  mode: "feed" | "full";
  scrollRef: RefObject<HTMLDivElement | null>;
}) {
  const t = useT();
  const lang = useLang();
  const count = useMemo(() => {
    if (!item) return null;
    // 邮件的正文不论哪种都是 HTML（纯文本邮件也转成了安全的 HTML 片段）
    const html = item.mail ? item.mail.body : mode === "full" && item.full_text ? item.full_text : item.content;
    const text = html ? (new DOMParser().parseFromString(html, "text/html").body.textContent ?? "") : "";
    return wordCount(text);
  }, [item, mode]);
  const [progress, setProgress] = useState(0);
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const update = () => {
      const max = el.scrollHeight - el.clientHeight;
      setProgress(max <= 0 ? 100 : Math.min(100, Math.round((el.scrollTop / max) * 100)));
    };
    update();
    el.addEventListener("scroll", update, { passive: true });
    const observer = new ResizeObserver(update);
    observer.observe(el);
    return () => {
      el.removeEventListener("scroll", update);
      observer.disconnect();
    };
  }, [scrollRef, item]);

  const words = count ? totalWords(count) : 0;
  return (
    <>
      <span className="truncate">{fullTime(summary.published_at, lang)}</span>
      {words > 0 && count && (
        <span className="truncate">
          {t("status.words", { n: words.toLocaleString() })} · {t("status.minutes", { n: readingMinutes(count) })}
        </span>
      )}
      {item && <span>{t("status.progress", { n: progress })}</span>}
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
  const style = useReaderStyle();
  // 划线：看译文时不显示（位置是按原文记的）
  const body = useRef<HTMLDivElement>(null);
  const highlightable = !shown?.html;
  const [noteFocus, setNoteFocus] = useState<{ id: string; n: number } | null>(null);
  const focusNote = (id: string) => setNoteFocus((f) => ({ id, n: (f?.n ?? 0) + 1 }));
  return (
    <article
      className="mx-auto px-10 pb-24 pt-10"
      style={{ maxWidth: readerWidth(style, "article"), ...readerVars(style) } as CSSProperties}
    >
      <h1
        className="cursor-auto select-text font-semibold leading-[1.25] tracking-[-0.02em]"
        style={{ fontSize: titleSize(style, "article") }}
      >
        {shown?.title || item.title || t("reader.untitled")}
      </h1>
      <div className="mb-8 mt-4 flex flex-wrap items-center gap-x-2 gap-y-1 text-[13px] text-fg-3">
        <ArticleIcon item={item} />
        <span className="font-medium text-fg-2">{item.source_title}</span>
        {item.author && (
          <>
            <span aria-hidden>·</span>
            <span>{item.author}</span>
          </>
        )}
        <span aria-hidden>·</span>
        <time dateTime={new Date(item.published_at).toISOString()}>{fullTime(item.published_at, lang)}</time>
        {!item.media && <ReadAloud itemId={item.id} title={shown?.title || item.title} body={body} />}
      </div>
      {/* 播客的播放条、视频附件 */}
      <MediaAttachment item={item} />
      {bar}
      <PluginSections item={item} mode={mode} />
      {fullError && (
        <p className="mb-6 rounded-md border border-line bg-hover px-3 py-2 text-[13px] leading-relaxed text-fg-2">
          {t("reader.fullTextFallback", { reason: fullError })}
          {item.url && (
            <button
              type="button"
              onClick={() => void openExternal(item.url ?? "")}
              className="ml-1 text-accent hover:underline"
            >
              {t("action.openOriginal")}
            </button>
          )}
        </p>
      )}
      {loadingFull && (
        <p className="mb-6 flex items-center gap-2 text-[13px] text-fg-3">
          <Spinner size={13} />
          {t("reader.loadingFullText")}
        </p>
      )}
      {/* 内容在核心里已经净化过，见 postflow-feed 的 sanitize_html */}
      <div
        ref={body}
        className="article"
        onClick={(e) => {
          // 点正文里的划线：跳到下面它的笔记
          const mark = (e.target as HTMLElement).closest<HTMLElement>("mark[data-hl]");
          if (mark?.dataset.hl && !window.getSelection()?.toString()) focusNote(mark.dataset.hl);
          interceptLinks(e);
        }}
        dangerouslySetInnerHTML={{ __html: html }}
      />
      <HighlightLayer item={item} body={body} html={html} enabled={highlightable} onFocusNote={focusNote} />
      <HighlightList item={item} focus={noteFocus} enabled={highlightable} />
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
  onContext,
  onEditDraft,
}: {
  item: ItemDetail;
  mail: MailDetail;
  onEditDraft: () => void;
  shown: Shown;
  bar: ReactNode;
  onShowImages: () => void;
  onTrusted: () => void;
  onContext: (info: ContextInfo) => void;
}) {
  const t = useT();
  const lang = useLang();
  const sender = mail.from.name || mail.from.address || item.author || "";
  const to = formatPeople(mail.to);
  const cc = formatPeople(mail.cc);
  const style = useReaderStyle();

  const trust = async () => {
    if (!mail.from.address) return;
    await api.trustSender(mail.from.address);
    onTrusted();
  };

  return (
    <article
      className="mx-auto px-8 pb-24 pt-8"
      style={{ maxWidth: readerWidth(style, "mail"), ...readerVars(style) } as CSSProperties}
    >
      {item.draft && (
        <div className="mb-5 flex items-center gap-3 rounded-lg border border-line bg-hover px-3 py-2 text-[13px] text-fg-2">
          <FilePen size={15} className="shrink-0 text-fg-3" />
          <span className="flex-1">{t("compose.draftNote")}</span>
          <Button variant="primary" onClick={onEditDraft}>
            {t("compose.editDraft")}
          </Button>
        </div>
      )}
      <h1
        className="cursor-auto select-text font-semibold leading-[1.3] tracking-[-0.015em]"
        style={{ fontSize: titleSize(style, "mail") }}
      >
        {shown?.title || item.title || t("mail.noSubject")}
      </h1>

      <div className="mb-6 mt-5 flex items-start gap-3">
        {mail.from.address ? (
          <SenderCard name={sender} address={mail.from.address} accountId={item.source_id}>
            <button type="button" aria-label={t("sender.open")} className="shrink-0 rounded-full">
              <SenderAvatar name={sender || "?"} address={mail.from.address} size={32} />
            </button>
          </SenderCard>
        ) : (
          <SenderAvatar name={sender || "?"} address={mail.from.address} size={32} />
        )}
        <div className="min-w-0 flex-1 select-text">
          <div className="flex min-w-0 items-baseline gap-2">
            {mail.from.address ? (
              <SenderCard name={sender} address={mail.from.address} accountId={item.source_id}>
                <button type="button" className="truncate font-medium text-fg hover:underline">
                  {sender}
                </button>
              </SenderCard>
            ) : (
              <span className="truncate font-medium text-fg">{sender}</span>
            )}
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
          <div className="flex min-w-0 items-center justify-end gap-1.5">
            <AccountIcon accountId={item.source_id} fallback={item.source_title} size={13} />
            <span className="truncate">{item.source_title}</span>
          </div>
          <Unsubscribe id={item.id} info={mail.unsubscribe ?? null} sender={sender} />
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

      <InviteCard item={item} />
      {bar}
      <PluginSections item={item} />

      {mail.body_kind === "html" ? (
        <MailFrame html={shown?.html ?? mail.body} zoom={mailZoom(style)} onContext={onContext} />
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
 * 邮件列表的退订：能一键退订的由 Postflow 替用户发退订请求；否则打开退订网页，或者写一封退订邮件。
 * 都先确认一下。
 */
function Unsubscribe({
  id,
  info,
  sender,
}: {
  id: string;
  info: MailDetail["unsubscribe"] | null;
  sender: string;
}) {
  const t = useT();
  const [asking, setAsking] = useState(false);
  const [state, setState] = useState<"idle" | "busy" | "done">("idle");
  const [error, setError] = useState<string | null>(null);
  if (!info) return null;
  const how = info.one_click ? "one_click" : info.url ? "page" : "mail";

  const run = async () => {
    if (how !== "one_click") {
      void openExternal((how === "page" ? info.url : info.mailto) ?? "");
      setAsking(false);
      return;
    }
    setState("busy");
    setError(null);
    try {
      await api.unsubscribe(id);
      setState("done");
      setAsking(false);
    } catch (err) {
      setError(errorText(t, err));
      setState("idle");
    }
  };

  return (
    <>
      {state === "done" ? (
        <span className="mt-0.5 inline-flex items-center gap-1 text-fg-3">
          <Check size={12} strokeWidth={2} />
          {t("unsub.done")}
        </span>
      ) : (
        <button
          type="button"
          onClick={() => setAsking(true)}
          className="mt-0.5 text-fg-3 underline-offset-2 hover:text-accent hover:underline"
        >
          {t("unsub.button")}
        </button>
      )}
      <Modal open={asking} onClose={() => setAsking(false)} title={t("unsub.title")} width={400}>
        <div className="flex flex-col gap-4 p-4 text-left">
          <p className="leading-relaxed text-fg-2">{t(`unsub.body_${how}`, { sender: sender || "?" })}</p>
          {error && (
            <p role="alert" className="text-[13px] text-danger">
              {error}
            </p>
          )}
          <div className="flex justify-end gap-2">
            {how === "one_click" && error && info.url && (
              <Button variant="ghost" className="mr-auto" onClick={() => void openExternal(info.url ?? "")}>
                {t("unsub.openPage")}
              </Button>
            )}
            <Button variant="ghost" onClick={() => setAsking(false)}>
              {t("action.cancel")}
            </Button>
            <Button variant="primary" busy={state === "busy"} data-autofocus="" onClick={() => void run()}>
              {how === "one_click" ? t("unsub.confirm") : how === "page" ? t("unsub.openPage") : t("unsub.writeMail")}
            </Button>
          </div>
        </div>
      </Modal>
    </>
  );
}

/**
 * HTML 邮件放进沙箱 iframe：不允许脚本，只允许同源，好让外层量出内容高度、接管链接点击。
 * 文档自带内容安全策略，远程图片是否加载由核心决定。
 */
function MailFrame({
  html,
  zoom,
  onContext,
}: {
  html: string;
  zoom: number;
  onContext: (info: ContextInfo) => void;
}) {
  const ref = useRef<HTMLIFrameElement>(null);
  const onContextRef = useRef(onContext);
  onContextRef.current = onContext;
  const [height, setHeight] = useState(240);
  // HTML 邮件有自己的样式，阅读设置里的字号只能整体缩放它
  const zoomRef = useRef(zoom);
  zoomRef.current = zoom;
  useEffect(() => {
    const root = ref.current?.contentDocument?.documentElement;
    if (root) root.style.zoom = String(zoom);
  }, [zoom]);

  useEffect(() => {
    const iframe = ref.current;
    if (!iframe) return;
    let observer: ResizeObserver | undefined;

    const onLoad = () => {
      const doc = iframe.contentDocument;
      if (!doc) return;
      doc.documentElement.style.zoom = String(zoomRef.current);
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
      // 右键：记下点在了什么上面，再把事件转给外面，弹出和正文其他地方一样的菜单
      doc.addEventListener("contextmenu", (e) => {
        e.preventDefault();
        const target = e.target as {
          closest?: (s: string) => HTMLAnchorElement | null;
          tagName?: string;
          src?: string;
        } | null;
        onContextRef.current({
          selection: doc.getSelection()?.toString() ?? "",
          link: target?.closest?.("a[href]")?.getAttribute("href") ?? null,
          image: target?.tagName === "IMG" ? (target.src ?? null) : null,
        });
        const rect = iframe.getBoundingClientRect();
        const scale = zoomRef.current;
        iframe.dispatchEvent(
          new window.MouseEvent("contextmenu", {
            bubbles: true,
            cancelable: true,
            button: 2,
            clientX: rect.left + e.clientX * scale,
            clientY: rect.top + e.clientY * scale,
          }),
        );
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

/** 工具栏里设置分类。只有在设置里定义了类别时才出现。 */
function LabelMenu({ ids, current }: { ids: string[]; current: string | null | undefined }) {
  const t = useT();
  const categories = useCategories();
  const labelColor = useLabelColor();
  const setLabel = useSetLabel();
  if (!categories || categories.length === 0) return null;
  const set = (label: string | null) => setLabel(ids, label);
  return (
    <Menu
      align="end"
      trigger={
        <button
          type="button"
          aria-label={t("label.set")}
          title={current ? `${t("label.set")}：${current}` : t("label.set")}
          className="inline-flex size-7 shrink-0 touch:size-10 items-center justify-center rounded-md text-fg-2 transition-colors hover:bg-hover hover:text-fg data-[state=open]:bg-hover"
        >
          {current ? (
            <span aria-hidden className="size-2.5 rounded-full" style={{ background: labelColor(current) }} />
          ) : (
            <Tag {...ICON} />
          )}
        </button>
      }
    >
      <div className="px-2 pb-1 pt-1.5 text-[12px] font-medium text-fg-3">{t("label.set")}</div>
      {categories.map((c) => (
        <MenuItem
          key={c.name}
          icon={<span aria-hidden className="size-2 rounded-full" style={{ background: labelColor(c.name) }} />}
          onSelect={() => set(c.name)}
        >
          <span className="flex-1">{c.name}</span>
          {current === c.name && <span className="ml-3 text-fg-3">✓</span>}
        </MenuItem>
      ))}
      <MenuSeparator />
      <MenuItem onSelect={() => set(null)}>{t("label.none")}</MenuItem>
    </Menu>
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
          <Button
            variant="ghost"
            className={small}
            title={t("translate.skipHint")}
            onClick={() => {
              useUi.getState().skipModels(phase.plan.missing.map((m) => m.pair));
              off();
            }}
          >
            {t("translate.skip")}
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
        t(phase.engine === "ai" ? "translate.doneAi" : "translate.done", {
          from: phase.from ? name(phase.from) : t("translate.original"),
          to: name(to),
        }),
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

/** 阅读区最大化：藏起侧栏和列表。快捷键 F，Esc 退出。 */
function MaximizeButton() {
  const t = useT();
  const maximized = useUi((s) => s.readerMaximized);
  const setMaximized = useUi((s) => s.setReaderMaximized);
  // Postflow 自己开的全屏里阅读区一直是最大化的，这个按钮没用
  const ownFullscreen = useFullscreen((s) => s.on && s.ours);
  // 手机上阅读区本来就占满屏幕，最大化没有意义，也给其他按钮腾地方
  const mobile = useIsMobile();
  if (mobile) return null;
  // 全屏时这个按钮变成「退出全屏」
  if (ownFullscreen) {
    return (
      <IconButton label={t("reader.exitFullscreen")} shortcut="Esc" active onClick={() => void setFullscreen(false)}>
        <Shrink {...ICON} />
      </IconButton>
    );
  }
  return (
    <IconButton
      label={maximized ? t("reader.restore") : t("reader.maximizeOrFullscreen")}
      shortcut="F"
      active={maximized}
      // 按住 Shift 点：全屏（快捷键 ⇧F）
      onClick={(e) => (e.shiftKey ? void setFullscreen(true) : setMaximized(!maximized))}
    >
      {maximized ? <Minimize2 {...ICON} /> : <Maximize2 {...ICON} />}
    </IconButton>
  );
}

/** 全屏：整个窗口（浏览器里是整个页面）全屏，阅读区同时最大化。快捷键 Shift+F，Esc 退出。 */
function FullscreenButton() {
  const t = useT();
  const on = useFullscreen((s) => s.on);
  return (
    <IconButton
      label={on ? t("reader.exitFullscreen") : t("reader.fullscreen")}
      shortcut="⇧F"
      active={on}
      onClick={() => void setFullscreen(!on)}
    >
      {on ? <Shrink {...ICON} /> : <Expand {...ICON} />}
    </IconButton>
  );
}

/** 最大化时关掉了正在看的条目，阅读区是空的：给一个回到原来布局的按钮。 */
function RestoreButton() {
  const t = useT();
  const maximized = useUi((s) => s.readerMaximized);
  const fullscreen = useFullscreen((s) => s.on);
  const setMaximized = useUi((s) => s.setReaderMaximized);
  if (!maximized && !fullscreen) return null;
  return (
    <IconButton
      label={t("reader.restore")}
      onClick={() => (fullscreen ? void setFullscreen(false) : setMaximized(false))}
    >
      <Minimize2 {...ICON} />
    </IconButton>
  );
}

function Empty() {
  const t = useT();
  // 订阅那边不用归档、稍后提醒时，E 是标为已读，H 不提示
  const feeds = useUi((s) => s.space === "feeds");
  const allowed = useArticleActions();
  const archives = !feeds || allowed.archive;
  const hints: [string[], string][] = [
    [["J", "K"], t("reader.hintMove")],
    [["F", "⇧F"], t("reader.hintMaximize")],
    [["E"], archives ? t("reader.hintArchive") : t("action.markRead")],
    [["S"], t("reader.hintStar")],
    ...(!feeds || allowed.snooze ? ([[["H"], t("snooze.title")]] as [string[], string][]) : []),
    [["U"], t("reader.hintRead")],
    [["V"], t("reader.hintOpen")],
    [["X"], t("reader.hintSelect")],
    [["T"], t("reader.hintTranslate")],
    [["W"], t("reader.hintWeb")],
    [[MOD + "K"], t("commands.title")],
    [["?"], t("shortcuts.title")],
    [["/"], t("reader.hintSearch")],
    [["C"], t("reader.hintCompose")],
    [[MOD + "R"], t("reader.hintReply")],
    [["A"], t("reader.hintAi")],
    [["+", "−"], t("reader.hintSize")],
    [["→", "←"], t("reader.hintGroup")],
  ];
  return (
    <>
      <Toolbar>
        <RestoreButton />
      </Toolbar>
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

/** 这一条正稍后提醒着：顶上一行说到什么时候，可以取消。 */
function SnoozedBar({ item }: { item: Item }) {
  const t = useT();
  const lang = useLang();
  const { snooze } = useActions();
  return (
    <div className="flex h-8 shrink-0 items-center gap-2 border-b border-line bg-accent-soft/40 px-4 text-[12px] text-fg-2">
      <AlarmClock size={13} strokeWidth={2} className="shrink-0 text-accent" />
      <span className="min-w-0 flex-1 truncate">
        {t("snooze.until", { time: snoozeTime(item.snoozed_until ?? 0, lang) })}
      </span>
      <button type="button" onClick={() => snooze(null, [item])} className="shrink-0 text-accent hover:underline">
        {t("snooze.cancel")}
      </button>
    </div>
  );
}
