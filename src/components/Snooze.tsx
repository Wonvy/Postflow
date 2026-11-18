// 稍后提醒：阅读区工具栏上的按钮和右键菜单里的子菜单，选一个预设时间或者自己挑时间。
// 预设时间在 lib/snooze.ts，保存和隐藏在 actions.ts 的 `snooze`。

import { AlarmClock, X } from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { useActions } from "../lib/actions";
import type { Item } from "../lib/api";
import { useLang, useT } from "../lib/i18n";
import { type SnoozePreset, snoozePresets, toLocalInput } from "../lib/snooze";
import { useUi } from "../lib/store";
import { Button, cx, Field, Menu, MenuItem, MenuSeparator, MenuSub, Modal, TextInput } from "./ui";

const LABEL: Record<SnoozePreset, "snooze.later" | "snooze.tonight" | "snooze.tomorrow" | "snooze.weekend" | "snooze.nextWeek"> = {
  later: "snooze.later",
  tonight: "snooze.tonight",
  tomorrow: "snooze.tomorrow",
  weekend: "snooze.weekend",
  nextWeek: "snooze.nextWeek",
};

/** 提醒时间的写法：今天的只写时间，一周内写星期，再远写日期。 */
export function snoozeTime(ms: number, lang: string, now = Date.now()): string {
  const date = new Date(ms);
  const sameDay = new Date(now).toDateString() === date.toDateString();
  const withinWeek = ms - now < 6 * 86_400_000;
  const locale = lang === "zh" ? "zh-CN" : "en";
  return new Intl.DateTimeFormat(locale, {
    ...(sameDay ? {} : withinWeek ? { weekday: "short" } : { month: "short", day: "numeric", weekday: "short" }),
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

/** 这几条里有还没到时间的稍后提醒。 */
const snoozedNow = (items: Item[]) => items.some((i) => (i.snoozed_until ?? 0) > Date.now());

/** 预设时间、自己挑时间、取消。工具栏的菜单和右键子菜单共用。 */
function SnoozeItems({ items, onPick }: { items: Item[]; onPick: (until: number | null) => void }) {
  const t = useT();
  const lang = useLang();
  const setDialog = useUi((s) => s.setDialog);
  const presets = snoozePresets(new Date());
  return (
    <>
      {presets.map((p) => (
        <MenuItem key={p.key} onSelect={() => onPick(p.at.getTime())}>
          <span className="flex w-full items-center justify-between gap-4">
            {t(LABEL[p.key])}
            <span className="text-[12px] text-fg-3">{snoozeTime(p.at.getTime(), lang)}</span>
          </span>
        </MenuItem>
      ))}
      <MenuItem onSelect={() => setDialog({ type: "snooze", ids: items.map((i) => i.id) })}>{t("snooze.custom")}</MenuItem>
      {snoozedNow(items) && (
        <>
          <MenuSeparator />
          <MenuItem icon={<X size={14} />} onSelect={() => onPick(null)}>
            {t("snooze.cancel")}
          </MenuItem>
        </>
      )}
    </>
  );
}

/** 阅读区工具栏上的按钮。 */
export function SnoozeButton({ item }: { item: Item }) {
  const t = useT();
  const { snooze } = useActions();
  const active = snoozedNow([item]);
  return (
    <Menu
      align="end"
      trigger={
        <button
          type="button"
          aria-label={t("snooze.title")}
          title={t("snooze.title")}
          className={cx(
            "inline-flex size-7 shrink-0 items-center justify-center rounded-md transition-colors data-[state=open]:bg-hover",
            active ? "bg-accent-soft text-accent" : "text-fg-2 hover:bg-hover hover:text-fg",
          )}
        >
          <AlarmClock size={16} strokeWidth={1.75} />
        </button>
      }
    >
      <SnoozeItems items={[item]} onPick={(until) => snooze(until, [item])} />
    </Menu>
  );
}

/** 右键菜单里的子菜单。 */
export function SnoozeSubMenu({ items }: { items: Item[] }): ReactNode {
  const t = useT();
  const { snooze } = useActions();
  return (
    <MenuSub icon={<AlarmClock size={14} />} label={t("snooze.title")}>
      <SnoozeItems items={items} onPick={(until) => snooze(until, items)} />
    </MenuSub>
  );
}

/** 自己挑提醒时间。 */
export function SnoozeDialog() {
  const t = useT();
  const dialog = useUi((s) => s.dialog);
  const setDialog = useUi((s) => s.setDialog);
  const { snooze, itemsOf } = useActions();
  const open = dialog?.type === "snooze";
  const ids = dialog?.type === "snooze" ? dialog.ids : [];
  const [value, setValue] = useState("");
  useEffect(() => {
    if (open) setValue(toLocalInput(new Date(Date.now() + 86_400_000)));
  }, [open]);
  const at = new Date(value).getTime();
  const valid = Number.isFinite(at) && at > Date.now();
  const save = () => {
    if (!valid) return;
    const items = itemsOf(ids);
    snooze(at, items.length > 0 ? items : undefined);
    setDialog(null);
  };
  return (
    <Modal open={open} onClose={() => setDialog(null)} title={t("snooze.title")} width={360}>
      <form
        className="flex flex-col gap-4 p-4"
        onSubmit={(e) => {
          e.preventDefault();
          save();
        }}
      >
        <Field label={t("snooze.pick")} hint={value && !valid ? t("snooze.past") : undefined}>
          <TextInput
            // biome-ignore lint/a11y/noAutofocus: 打开就能改时间
            autoFocus
            type="datetime-local"
            value={value}
            min={toLocalInput(new Date())}
            onChange={(e) => setValue(e.target.value)}
          />
        </Field>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={() => setDialog(null)}>
            {t("action.cancel")}
          </Button>
          <Button variant="primary" type="submit" disabled={!valid}>
            {t("snooze.set")}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
