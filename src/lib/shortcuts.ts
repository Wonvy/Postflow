import { useEffect, useRef } from "react";
import { useActions } from "./actions";
import { IS_TABLET } from "./device";
import { normalizeReaderStyle, readerDefault } from "./readerStyle";
import { episodeOf, play, skip, toggleIfLoaded, usePlayer, BACK_SECONDS, FORWARD_SECONDS } from "./player";
import type { ContentKind } from "./api";
import { kindsIn } from "./feedKinds";
import { useNav } from "./queries";
import { articleActions, useUi } from "./store";
import { setFullscreen, toggleFullscreen, useFullscreen } from "./fullscreen";

function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target.isContentEditable ||
    target.tagName === "INPUT" ||
    target.tagName === "TEXTAREA" ||
    target.tagName === "SELECT"
  );
}

/** 全局快捷键。输入框里打字、对话框打开时不响应。 */
export function useShortcuts() {
  const actions = useActions();
  // 监听器只装一次，通过 ref 拿到最新的操作
  const latest = useRef(actions);
  latest.current = actions;
  // 订阅里有哪几类源：数字键切换类型时只认有的
  const nav = useNav().data;
  const kinds = useRef<ContentKind[]>([]);
  kinds.current = kindsIn(nav?.feeds ?? [], nav?.newsletters ?? []);

  useEffect(() => {
    // 对话框或新手引导开着时，按键属于它们
    const dialogOpen = () => {
      const ui = useUi.getState();
      return ui.dialog !== null || ui.welcomeStep !== null;
    };
    const onKeyDown = (e: KeyboardEvent) => {
      // ⌘K / Ctrl+K：命令面板，再按一次关上（输入框里打字时也行）
      if ((e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "k") {
        const ui = useUi.getState();
        if (ui.dialog === null || ui.dialog.type === "commands") {
          e.preventDefault();
          ui.setDialog(ui.dialog ? null : { type: "commands" });
          return;
        }
      }
      // ⌘F / Ctrl+F：搜索（输入框里打字时也行，对话框开着时不抢）
      if (
        (e.metaKey || e.ctrlKey) &&
        !e.altKey &&
        !e.shiftKey &&
        e.key.toLowerCase() === "f" &&
        !dialogOpen()
      ) {
        e.preventDefault();
        useUi.getState().openSearch();
        return;
      }
      // ⌘R 回复，⇧⌘R 全部回复，⇧⌘F 转发（Windows、Linux 上是 Ctrl）。和 Apple 邮件一样
      if (
        (e.metaKey || e.ctrlKey) &&
        !e.altKey &&
        !isTyping(e.target) &&
        !dialogOpen() &&
        useUi.getState().openId !== null
      ) {
        const k = e.key.toLowerCase();
        const mode = k === "r" ? (e.shiftKey ? "reply-all" : "reply") : k === "f" && e.shiftKey ? "forward" : null;
        if (mode) {
          e.preventDefault();
          latest.current.compose(mode);
          return;
        }
      }
      if (e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey) return;
      if (isTyping(e.target) || dialogOpen()) return;
      // 菜单打开时按键属于菜单
      if (document.querySelector("[role=menu]")) return;

      const a = latest.current;
      const ui = useUi.getState();
      const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;

      // 阅读区字号：+ 放大，- 缩小，0 恢复。+ 在多数键盘上要按 Shift，所以放在 Shift 判断前面
      if (ui.openId && (key === "+" || key === "=" || key === "-" || key === "0")) {
        e.preventDefault();
        const size = normalizeReaderStyle(ui.readerStyle).size;
        ui.setReaderStyle({ size: key === "0" ? readerDefault(IS_TABLET).size : size + (key === "-" ? -1 : 1) });
        return;
      }

      if (e.shiftKey) {
        // Shift+← / Shift+→：播客后退 15 秒、前进 30 秒
        if ((key === "ArrowLeft" || key === "ArrowRight") && usePlayer.getState().current) {
          e.preventDefault();
          skip(key === "ArrowLeft" ? -BACK_SECONDS : FORWARD_SECONDS);
          return;
        }
        // ?（Shift+/）：全部快捷键。有的键盘布局和浏览器报的是 Shift 加「/」
        if (key === "?" || key === "/") {
          e.preventDefault();
          ui.setDialog({ type: "shortcuts" });
          return;
        }
        if (key === "a") {
          e.preventDefault();
          a.confirmMarkAllRead();
        }
        if (key === "f") {
          // 全屏（阅读区同时最大化）
          e.preventDefault();
          void toggleFullscreen();
        }
        return;
      }

      switch (key) {
        case "j":
        case "ArrowDown":
          a.move(1);
          break;
        case "k":
        case "ArrowUp":
          a.move(-1);
          break;
        case "e":
          a.toggle("archived");
          break;
        case "s":
          a.toggle("starred");
          break;
        case "h": {
          // 稍后提醒：打开选时间的对话框，作用于勾选的、没勾选时是打开的这一条
          const ids = ui.selection.length > 0 ? ui.selection : ui.openId ? [ui.openId] : [];
          if (ids.length === 0) return;
          // 订阅那边不显示「稍后提醒」时文章不提醒
          if (ui.space === "feeds" && !articleActions(ui.hiddenViews.feeds ?? []).snooze) return;
          ui.setDialog({ type: "snooze", ids });
          break;
        }
        case "u":
          a.toggle("read");
          break;
        case "1":
        case "2":
        case "3":
        case "4":
        case "5": {
          // 订阅：按侧栏顶上那排图标的顺序，1 是全部，后面依次是有的那几类（只有一类时没有这排图标）
          if (ui.space !== "feeds" || kinds.current.length < 2) return;
          const kind = (["all", ...kinds.current] as const)[Number(key) - 1];
          if (!kind) return;
          ui.setFeedKind(kind);
          break;
        }
        case "p": {
          // 播客：播放 / 暂停正在放的那一期；还没放过时，放打开的这一期
          if (toggleIfLoaded()) break;
          const open = ui.openId ? a.itemsOf([ui.openId])[0] : undefined;
          const episode = open ? episodeOf(open) : null;
          if (!episode) return;
          play(episode);
          break;
        }
        case "v":
          a.openOriginal();
          break;
        case "r":
          a.refreshAll();
          break;
        case "t":
          if (!ui.openId) return;
          ui.requestTranslate();
          break;
        case "a":
          if (!ui.openId) return;
          ui.toggleAiPanel();
          break;
        case "w":
          // 原网页开关
          if (!ui.openId) return;
          ui.setReaderWeb(!ui.readerWeb);
          break;
        case "/":
          ui.openSearch();
          break;
        case "c":
          a.compose("new");
          break;
        case "x":
          // 勾选或取消勾选当前打开的那一行，配合 J / K 可以只用键盘多选
          if (!ui.openId) return;
          a.toggleSelected(ui.openId);
          break;
        case "ArrowRight":
          a.expandOpen();
          break;
        case "ArrowLeft":
          a.collapseOpen();
          break;
        case "f":
          // 阅读区最大化：藏起侧栏和列表
          if (useFullscreen.getState().ours) return;
          ui.setReaderMaximized(!ui.readerMaximized);
          break;
        case " ": {
          // 空格：打开鼠标停着的那一行（设置里可以关）。鼠标不在列表的行上时不管，空格照常翻页
          if (!ui.spaceOpensHovered) return;
          const row = document.querySelector<HTMLElement>("[data-row-id]:hover");
          const id = row?.dataset.rowId;
          if (!id) return;
          ui.setSelection([]);
          ui.open(id);
          break;
        }
        case "Escape":
          // 先退出全屏，再退出最大化，最后才是取消选择、关掉条目
          if (useFullscreen.getState().on) void setFullscreen(false);
          else if (ui.readerMaximized) ui.setReaderMaximized(false);
          else if (ui.selection.length > 0) ui.setSelection([]);
          else if (ui.openId) ui.open(null);
          else return;
          break;
        default:
          return;
      }
      e.preventDefault();
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
