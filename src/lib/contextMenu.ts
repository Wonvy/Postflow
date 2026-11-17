// 关掉系统自带的右键菜单（「重新载入」「检查元素」「返回」这些对应用没有意义）。
// 需要右键的地方都有应用自己的菜单（ui.tsx 里的 ContextMenu）。
//
// 例外：输入框和可编辑区域里保留系统菜单（拼写建议、粘贴）；在选中的文字上保留（拷贝）。
// 开发版里按住 Shift 右键还是系统菜单，方便打开开发者工具。

const EDITABLE = "input, textarea, select, [contenteditable]:not([contenteditable='false'])";

export function disableDefaultContextMenu(): void {
  document.addEventListener("contextmenu", (e) => {
    if (e.defaultPrevented) return;
    if (import.meta.env.DEV && e.shiftKey) return;
    const target = e.target instanceof Element ? e.target : null;
    if (target?.closest(EDITABLE)) return;
    const selection = document.getSelection();
    if (target && selection && !selection.isCollapsed && selection.containsNode(target, true)) return;
    e.preventDefault();
  });
}
