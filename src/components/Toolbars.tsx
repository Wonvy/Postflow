// 可以自定义的工具栏：列表和阅读区的按钮按用户的布局排在顶部工具栏或底部状态栏，或者藏起来。
// 布局的规则在 lib/toolbars.ts；这里是摆放按钮、状态栏，以及「自定义工具栏」对话框。
// 在任一工具栏或状态栏上点右键就能打开对话框。

import {
  Archive,
  BookOpenText,
  CheckCheck,
  FilePen,
  Forward,
  Globe,
  GripVertical,
  Languages,
  ExternalLink,
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
  Puzzle,
  Maximize2,
  Expand,
} from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { Fragment, type ReactElement, type ReactNode, useState } from "react";
import type { ItemDetail } from "../lib/api";
import { type MessageKey, useT } from "../lib/i18n";
import { type ToolbarButton, usePlugins } from "../lib/plugins";
import { useToolbarLayout, useUi } from "../lib/store";
import {
  isDefaultLayout,
  moveTo,
  setPlace,
  type ToolbarId,
  type ToolPlace,
  toolsAt,
} from "../lib/toolbars";
import { toast } from "./Toast";
import { ContextMenu, cx, ICON, IconButton, MenuItem, Modal, Segmented } from "./ui";

/** 对话框里每个按钮的名字和图标。 */
const TOOL_META: Record<ToolbarId, Record<string, { label: MessageKey; icon: LucideIcon }>> = {
  list: {
    search: { label: "tool.search", icon: Search },
    markAllRead: { label: "tool.markAllRead", icon: CheckCheck },
    refresh: { label: "tool.refresh", icon: RefreshCw },
    view: { label: "tool.listStyle", icon: LayoutList },
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
    maximize: { label: "tool.maximize", icon: Maximize2 },
    fullscreen: { label: "tool.fullscreen", icon: Expand },
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
  const pluginButtons = usePlugins((s) => s.buttons).filter((b) => b.toolbar === toolbar);
  const layout = useToolbarLayout(
    toolbar,
    pluginButtons.map((b) => b.key),
  );
  const all: Partial<Record<string, ReactNode>> = { ...tools };
  for (const b of pluginButtons) all[b.key] = <PluginButton button={b} />;
  return (
    <>
      {toolsAt(layout, place).map((id) => (all[id] ? <Fragment key={id}>{all[id]}</Fragment> : null))}
    </>
  );
}

/** 插件加的按钮。点的时候把打开的条目和选中的条目交给插件。 */
function PluginButton({ button }: { button: ToolbarButton }) {
  const qc = useQueryClient();
  const [busy, setBusy] = useState(false);
  const run = async () => {
    const { openId, selection } = useUi.getState();
    // 打开的条目可能是按「显示远程图片」另取的，按 ID 前缀找
    const item = openId
      ? (qc.getQueriesData<ItemDetail>({ queryKey: ["item", openId] }).find(([, d]) => d)?.[1] ?? null)
      : null;
    setBusy(true);
    try {
      await button.onClick({ item, selection: [...selection] });
    } catch (err) {
      toast(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };
  return (
    <IconButton label={button.label} disabled={busy} onClick={() => void run()}>
      {button.icon ? (
        // 插件和界面一样受信任（只有用户手动启用的插件才会加载），图标直接用它给的 SVG
        <span aria-hidden className="inline-flex size-4 items-center justify-center [&>svg]:size-4" dangerouslySetInnerHTML={{ __html: button.icon }} />
      ) : (
        <Puzzle {...ICON} />
      )}
    </IconButton>
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
  quickTools,
}: {
  toolbar: ToolbarId;
  info?: ReactNode;
  tools: Partial<Record<string, ReactNode>>;
  quickTools?: ReactNode;
}) {
  return (
    <ToolbarMenu toolbar={toolbar}>
      <footer className="flex h-9 shrink-0 items-center gap-0.5 border-t border-line pl-3 pr-1.5 text-[12px] text-fg-3">
        <div className="tabular flex min-w-0 flex-1 items-center gap-3 truncate">{info}</div>
        {quickTools && <div className="flex shrink-0 items-center gap-0.5">{quickTools}</div>}
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
  const pluginButtons = usePlugins((s) => s.buttons).filter((b) => b.toolbar === toolbar);
  const layout = useToolbarLayout(
    toolbar,
    pluginButtons.map((b) => b.key),
  );
  const metaOf = (id: string): { label: string; icon: LucideIcon } | null => {
    const own = TOOL_META[toolbar][id];
    if (own) return { label: t(own.label), icon: own.icon };
    const plugin = pluginButtons.find((b) => b.key === id);
    return plugin ? { label: plugin.label, icon: Puzzle } : null;
  };
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
            const meta = metaOf(slot.id);
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
                <span className="min-w-0 flex-1 truncate text-[13px]">{meta.label}</span>
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
