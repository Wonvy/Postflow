// 全部快捷键一览：按 ? 打开，命令面板里也有。阅读区空着时只列了一部分。

import type { MessageKey } from "../lib/i18n";
import { useT } from "../lib/i18n";
import { isMac } from "../lib/platform";
import { useUi } from "../lib/store";
import { Kbd, Modal } from "./ui";

const MOD = isMac ? "⌘" : "Ctrl+";
const SHIFT_MOD = isMac ? "⇧⌘" : "Ctrl+Shift+";

const GROUPS: { title: MessageKey; keys: [string[], MessageKey][] }[] = [
  {
    title: "shortcuts.browse",
    keys: [
      [["J", "K"], "reader.hintMove"],
      [["→", "←"], "reader.hintGroup"],
      [["X"], "reader.hintSelect"],
      [["/", `${MOD}F`], "reader.hintSearch"],
      [[`${MOD}K`], "commands.title"],
      [["1", "…", "5"], "shortcuts.feedKind"],
      [["R"], "action.refresh"],
      [["Esc"], "shortcuts.escape"],
    ],
  },
  {
    title: "shortcuts.handle",
    keys: [
      [["E"], "reader.hintArchive"],
      [["S"], "reader.hintStar"],
      [["U"], "reader.hintRead"],
      [["H"], "snooze.title"],
      [["⇧A"], "action.markAllRead"],
      [["V"], "reader.hintOpen"],
    ],
  },
  {
    title: "shortcuts.read",
    keys: [
      [["F", "⇧F"], "reader.hintMaximize"],
      [["T"], "reader.hintTranslate"],
      [["W"], "reader.hintWeb"],
      [["A"], "reader.hintAi"],
      [["+", "−", "0"], "reader.hintSize"],
      [["P"], "shortcuts.playPause"],
      [["⇧←", "⇧→"], "shortcuts.skip"],
    ],
  },
  {
    title: "shortcuts.write",
    keys: [
      [["C"], "reader.hintCompose"],
      [[`${MOD}R`], "compose.reply"],
      [[`${SHIFT_MOD}R`], "compose.replyAll"],
      [[`${SHIFT_MOD}F`], "compose.forward"],
      [[`${MOD}↩`], "compose.send"],
    ],
  },
];

export function ShortcutsDialog() {
  const t = useT();
  const open = useUi((s) => s.dialog?.type === "shortcuts");
  const setDialog = useUi((s) => s.setDialog);
  return (
    <Modal open={open} onClose={() => setDialog(null)} title={t("shortcuts.title")} width={620}>
      <div className="grid gap-x-8 gap-y-5 p-5 sm:grid-cols-2">
        {GROUPS.map((group) => (
          <section key={group.title}>
            <h3 className="mb-2 text-[12px] font-medium text-fg-3">{t(group.title)}</h3>
            <dl className="flex flex-col gap-1.5">
              {group.keys.map(([keys, label]) => (
                <div key={label} className="flex items-center justify-between gap-3 text-[13px]">
                  <dt className="min-w-0 truncate text-fg-2">{t(label)}</dt>
                  <dd className="flex shrink-0 gap-1">
                    {keys.map((k) => (
                      <Kbd key={k}>{k}</Kbd>
                    ))}
                  </dd>
                </div>
              ))}
            </dl>
          </section>
        ))}
      </div>
    </Modal>
  );
}
