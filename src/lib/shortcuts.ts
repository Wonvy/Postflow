import { useEffect, useRef } from "react";
import { useActions } from "./actions";
import { useUi } from "./store";

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

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey) return;
      if (isTyping(e.target) || useUi.getState().dialog !== null) return;
      // 菜单打开时按键属于菜单
      if (document.querySelector("[role=menu]")) return;

      const a = latest.current;
      const ui = useUi.getState();
      const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;

      if (e.shiftKey) {
        if (key === "a") {
          e.preventDefault();
          a.markAllRead();
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
        case "u":
          a.toggle("read");
          break;
        case "v":
          a.openOriginal();
          break;
        case "r":
          a.refreshAll();
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
        case "Escape":
          if (ui.selection.length > 0) ui.setSelection([]);
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
