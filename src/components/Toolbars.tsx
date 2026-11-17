// 可以自定义的工具栏：列表和阅读区的按钮按用户的布局排在顶部工具栏或底部状态栏，或者藏起来。
// 布局的规则在 lib/toolbars.ts；这里是摆放按钮、状态栏，以及「自定义工具栏」对话框。
// 在任一工具栏或状态栏上点右键就能打开对话框。

import {
  Archive,
  BookOpenText,
  CheckCheck,
  FilePen,
  Filter,
  Forward,
  Globe,
  GripVertical,
  Languages,
  ExternalLink,
  ListCollapse,
  type LucideIcon,
  MailOpen,
  PanelBottom,
  RefreshCw,
  Reply,
  ReplyAll,
  Search,
  Settings2,
  Sparkles,
  Star,
  Tag,
  Trash2,
  Type,
  LayoutList,
} from "lucide-react";
import { Fragment, type ReactElement, type ReactNode, useState } from "react";
import { type MessageKey, useT } from "../lib/i18n";
import { useToolbarLayout, useUi } from "../lib/store";
import {
  isDefaultLayout,
  moveTo,
  setPlace,
  type ToolbarId,
  type ToolPlace,
  toolsAt,
} from "../lib/toolbars";
import { ContextMenu, cx, MenuItem, Modal, Segmented } from "./ui";

/** 对话框里每个按钮的名字和图标。 */
const TOOL_META: Record<ToolbarId, Record<string, { label: MessageKey; icon: LucideIcon }>> = {
  list: {
    kindFilter: { label: "tool.kindFilter", icon: Filter },
    search: { label: "tool.search", icon: Search },
    markAllRead: { label: "tool.markAllRead", icon: CheckCheck },
    refresh: { label: "tool.refresh", icon: RefreshCw },
    view: { label: "tool.listStyle", icon: LayoutList },
    group: { label: "tool.group", icon: ListCollapse },
    translate: { label: "tool.translateList", icon: Languages },
  },
  reader: {
    editDraft: { label: "tool.editDraft", icon: FilePen },
    deleteDraft: { label: "tool.deleteDraft", icon: Trash2 },
    reply: { label: "tool.reply", icon: Reply },
    replyAll: { label: "tool.replyAll", icon: ReplyAll },
    forward: { label: "tool.forward", icon: Forward },
    label: { label: "tool.label", icon: Tag },
    ai: { label: "tool.ai", icon: Sparkles },
    read: { label: "tool.read", icon: MailOpen },
    star: { label: "tool.star", icon: Star },
    archive: { label: "tool.archive", icon: Archive },
    original: { label: "tool.original", icon: ExternalLink },
    style: { label: "tool.readerStyle", icon: Type },
    fullText: { label: "tool.fullText", icon: BookOpenText },
    web: { label: "tool.web", icon: Globe },
    translate: { label: "tool.translate", icon: Languages },
  },
};

/** 某个位置上的按钮，按布局的顺序。`tools` 里没有（或为 null）的按钮这时用不上，跳过。 */
export function Tools({
  toolbar,
  place,
  tools,
}: {
  toolbar: ToolbarId;
  place: ToolPlace;
  tools: Partial<Record<string, ReactNode>>;
}) {
  const layout = useToolbarLayout(toolbar);
  return (
    <>
      {toolsAt(layout, place).map((id) => (tools[id] ? <Fragment key={id}>{tools[id]}</Fragment> : null))}
    </>
  );
}

/** 工具栏和状态栏的右键菜单：自定义工具栏。`children` 必须是单个元素。 */
export function ToolbarMenu({ toolbar, children }: { toolbar: ToolbarId; children: ReactElement }) {
  const t = useT();
  const setDialog = useUi((s) => s.setDialog);
  return (
    <ContextMenu
      bare
      items={
        <MenuItem icon={<Settings2 size={14} />} onSelect={() => setDialog({ type: "toolbars", toolbar })}>
          {t("tool.customize")}
        </MenuItem>
      }
    >
      {children}
    </ContextMenu>
  );
}

/** 底部状态栏：左边是信息，右边是放到状态栏的按钮。 */
export function StatusBar({
  toolbar,
  info,
  tools,
}: {
  toolbar: ToolbarId;
  info?: ReactNode;
  tools: Partial<Record<string, ReactNode>>;
}) {
  return (
    <ToolbarMenu toolbar={toolbar}>
      <footer className="flex h-9 shrink-0 items-center gap-0.5 border-t border-line pl-3 pr-1.5 text-[12px] text-fg-3">
        <div className="tabular flex min-w-0 flex-1 items-center gap-3 truncate">{info}</div>
        <Tools toolbar={toolbar} place="status" tools={tools} />
      </footer>
    </ToolbarMenu>
  );
}

/** 「自定义工具栏」对话框：拖动排序，每个按钮选放在顶部、状态栏还是隐藏。 */
export function ToolbarsDialog() {
  const t = useT();
  const dialog = useUi((s) => s.dialog);
  const setDialog = useUi((s) => s.setDialog);
  const setToolbar = useUi((s) => s.setToolbar);
  const open = dialog?.type === "toolbars";
  const [picked, setPicked] = useState<ToolbarId | null>(null);
  const toolbar: ToolbarId = picked ?? (open ? dialog.toolbar : undefined) ?? "list";
  const layout = useToolbarLayout(toolbar);
  const [dragging, setDragging] = useState<string | null>(null);
  const [over, setOver] = useState<number | null>(null);

  const close = () => {
    setDialog(null);
    setPicked(null);
  };

  const placeOptions = [
    { value: "top" as const, label: t("tool.placeTop") },
    { value: "status" as const, label: t("tool.placeStatus") },
    { value: "hidden" as const, label: t("tool.placeHidden") },
  ];

  return (
    <Modal open={open} onClose={close} title={t("tool.title")} width={460}>
      <div className="flex flex-col gap-3 p-4">
        <Segmented<ToolbarId>
          value={toolbar}
          onChange={setPicked}
          options={[
            { value: "list", label: t("tool.listBar") },
            { value: "reader", label: t("tool.readerBar") },
          ]}
        />
        <p className="text-[12px] leading-relaxed text-fg-3">{t("tool.hint")}</p>
        <ul className="flex flex-col gap-px rounded-md border border-line p-1">
          {layout.map((slot, index) => {
            const meta = TOOL_META[toolbar][slot.id];
            if (!meta) return null;
            const Icon = meta.icon;
            return (
              <li
                key={slot.id}
                draggable
                onDragStart={(e) => {
                  setDragging(slot.id);
                  e.dataTransfer.effectAllowed = "move";
                  e.dataTransfer.setData("text/plain", slot.id);
                }}
                onDragEnd={() => {
                  setDragging(null);
                  setOver(null);
                }}
                onDragOver={(e) => {
                  if (!dragging) return;
                  e.preventDefault();
                  setOver(index);
                }}
                onDrop={(e) => {
                  e.preventDefault();
                  if (dragging) setToolbar(toolbar, moveTo(layout, dragging, index));
                  setDragging(null);
                  setOver(null);
                }}
                className={cx(
                  "flex items-center gap-2 rounded px-1.5 py-1",
                  dragging === slot.id && "opacity-40",
                  over === index && dragging !== slot.id && "bg-accent-soft",
                  slot.place === "hidden" && "text-fg-3",
                )}
              >
                <GripVertical size={14} className="shrink-0 cursor-grab text-fg-4" aria-hidden />
                <Icon size={14} strokeWidth={1.75} className="shrink-0" />
                <span className="min-w-0 flex-1 truncate text-[13px]">{t(meta.label)}</span>
                <Segmented<ToolPlace>
                  value={slot.place}
                  onChange={(place) => setToolbar(toolbar, setPlace(layout, slot.id, place))}
                  options={placeOptions}
                />
              </li>
            );
          })}
        </ul>
        <div className="flex items-center justify-between">
          <span className="flex items-center gap-1.5 text-[12px] text-fg-3">
            <PanelBottom size={13} />
            {t("tool.statusHint")}
          </span>
          <button
            type="button"
            disabled={isDefaultLayout(toolbar, layout)}
            onClick={() => setToolbar(toolbar, null)}
            className="rounded px-1.5 py-0.5 text-[12px] text-accent hover:bg-hover disabled:pointer-events-none disabled:text-fg-4"
          >
            {t("tool.reset")}
          </button>
        </div>
      </div>
    </Modal>
  );
}
