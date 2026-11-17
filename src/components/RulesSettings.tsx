// 设置里的「自动规则」：新邮件和新文章进来时按条件自动分类、标已读、加星标或归档。
// 规则的格式在 lib/rules.ts，匹配和执行在核心（crates/core/src/rules.rs）。

import { useQueryClient } from "@tanstack/react-query";
import { ArrowDown, ArrowUp, FlaskConical, Pencil, Plus, Trash2, Wand2, X } from "lucide-react";
import { useState } from "react";
import { api } from "../lib/api";
import { errorText, type MessageKey, useLang, useT } from "../lib/i18n";
import { useCategories } from "../lib/labels";
import { keys, useSettings } from "../lib/queries";
import {
  FIELDS,
  moveRule,
  newRule,
  OPS,
  parseRules,
  type Rule,
  type RuleCondition,
  ruleProblem,
  SETTING_KEY,
  verificationCodeRule,
} from "../lib/rules";
import { toast } from "./Toast";
import { Button, cx, Field, Segmented, Select, Switch, TextInput } from "./ui";

const FIELD_LABEL: Record<RuleCondition["field"], MessageKey> = {
  subject: "rules.field.subject",
  from: "rules.field.from",
  to: "rules.field.to",
  body: "rules.field.body",
  source: "rules.field.source",
  any: "rules.field.any",
};

const OP_LABEL: Record<RuleCondition["op"], MessageKey> = {
  contains: "rules.op.contains",
  not_contains: "rules.op.notContains",
  equals: "rules.op.equals",
  starts_with: "rules.op.startsWith",
  ends_with: "rules.op.endsWith",
  regex: "rules.op.regex",
};

export function RulesSettings() {
  const t = useT();
  const lang = useLang();
  const qc = useQueryClient();
  const settings = useSettings();
  const categories = useCategories();
  const rules = parseRules(settings.data?.[SETTING_KEY]);
  const [editing, setEditing] = useState<Rule | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const save = async (next: Rule[]) => {
    await api.setSetting(SETTING_KEY, next);
    await qc.invalidateQueries({ queryKey: keys.settings });
  };

  const applyExisting = async (rule?: Rule) => {
    setBusy(rule?.id ?? "all");
    try {
      const { matched } = await api.rulesApply(rule);
      toast(t("rules.applied", { n: matched }));
    } catch (err) {
      toast(errorText(t, err));
    } finally {
      setBusy(null);
    }
  };

  if (editing) {
    return (
      <RuleEditor
        rule={editing}
        categories={categories?.map((c) => c.name) ?? []}
        onCancel={() => setEditing(null)}
        onSave={async (rule) => {
          const exists = rules.some((r) => r.id === rule.id);
          await save(exists ? rules.map((r) => (r.id === rule.id ? rule : r)) : [...rules, rule]);
          setEditing(null);
        }}
      />
    );
  }

  const codeLabel = categories?.find((c) => /通知|Notification/i.test(c.name))?.name ?? (lang === "zh" ? "验证码" : "Codes");

  return (
    <div className="flex flex-col gap-3 py-2">
      <p className="text-[12px] leading-relaxed text-fg-3">{t("rules.hint")}</p>
      {rules.length === 0 && (
        <div className="rounded-md border border-dashed border-line-strong px-4 py-5 text-center text-[13px] text-fg-3">
          {t("rules.empty")}
        </div>
      )}
      <ul className="flex flex-col gap-1.5">
        {rules.map((rule, index) => (
          <li key={rule.id} className={cx("rounded-md border border-line px-3 py-2", !rule.enabled && "opacity-60")}>
            <div className="flex items-center gap-2">
              <Switch
                label={t("rules.enabled")}
                checked={rule.enabled}
                onChange={(enabled) => void save(rules.map((r) => (r.id === rule.id ? { ...r, enabled } : r)))}
              />
              <span className="min-w-0 flex-1 truncate font-medium text-fg">{rule.name || t("rules.untitled")}</span>
              <IconAction label={t("rules.up")} disabled={index === 0} onClick={() => void save(moveRule(rules, rule.id, -1))}>
                <ArrowUp size={13} />
              </IconAction>
              <IconAction
                label={t("rules.down")}
                disabled={index === rules.length - 1}
                onClick={() => void save(moveRule(rules, rule.id, 1))}
              >
                <ArrowDown size={13} />
              </IconAction>
              <IconAction label={t("rules.edit")} onClick={() => setEditing(rule)}>
                <Pencil size={13} />
              </IconAction>
              <IconAction label={t("rules.delete")} danger onClick={() => void save(rules.filter((r) => r.id !== rule.id))}>
                <Trash2 size={13} />
              </IconAction>
            </div>
            <p className="mt-1 pl-10 text-[12px] leading-relaxed text-fg-3">{describe(rule, t)}</p>
          </li>
        ))}
      </ul>
      <div className="flex flex-wrap items-center gap-2">
        <Button onClick={() => setEditing(newRule())}>
          <Plus size={14} />
          {t("rules.add")}
        </Button>
        <Button onClick={() => setEditing(verificationCodeRule(t("rules.codeTemplateName"), codeLabel))}>
          <Wand2 size={14} />
          {t("rules.codeTemplate")}
        </Button>
        <span className="flex-1" />
        {rules.some((r) => r.enabled) && (
          <Button variant="ghost" busy={busy === "all"} onClick={() => void applyExisting()}>
            {t("rules.applyAll")}
          </Button>
        )}
      </div>
    </div>
  );
}

function IconAction({
  label,
  danger,
  disabled,
  onClick,
  children,
}: {
  label: string;
  danger?: boolean;
  disabled?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
      className={cx(
        "inline-flex size-6 items-center justify-center rounded text-fg-3 hover:bg-hover disabled:pointer-events-none disabled:opacity-30",
        danger ? "hover:text-danger" : "hover:text-fg",
      )}
    >
      {children}
    </button>
  );
}

/** 一句话说明规则：「主题包含『验证码』时 → 分到『通知』、标为已读」。 */
function describe(rule: Rule, t: ReturnType<typeof useT>): string {
  const conds = rule.conditions
    .filter((c) => c.value.trim())
    .map((c) => `${t(FIELD_LABEL[c.field])}${t(OP_LABEL[c.op])}${t("rules.quoted", { value: c.value })}`)
    .join(rule.match === "any" ? t("rules.or") : t("rules.and"));
  const acts = [
    rule.actions.label && t("rules.doLabel", { label: rule.actions.label }),
    rule.actions.read != null && (rule.actions.read ? t("rules.doRead") : t("rules.doUnread")),
    rule.actions.star != null && (rule.actions.star ? t("rules.doStar") : t("rules.doUnstar")),
    rule.actions.archive != null && (rule.actions.archive ? t("rules.doArchive") : t("rules.doUnarchive")),
    rule.stop && t("rules.doStop"),
  ].filter(Boolean);
  const scope = rule.applies_to === "mail" ? t("rules.scopeMail") : rule.applies_to === "article" ? t("rules.scopeArticle") : "";
  return `${scope}${conds || "—"} → ${acts.join(t("rules.listSep")) || "—"}`;
}

function RuleEditor({
  rule: initial,
  categories,
  onCancel,
  onSave,
}: {
  rule: Rule;
  categories: string[];
  onCancel: () => void;
  onSave: (rule: Rule) => Promise<void>;
}) {
  const t = useT();
  const [rule, setRule] = useState<Rule>(() => structuredClone(initial));
  const [testing, setTesting] = useState(false);
  const [result, setResult] = useState<{ matched: number; checked: number; samples: { id: string; title: string; source: string }[] } | null>(null);
  const [saving, setSaving] = useState(false);
  const problem = ruleProblem(rule);

  const update = (patch: Partial<Rule>) => {
    setRule((r) => ({ ...r, ...patch }));
    setResult(null);
  };
  const setCondition = (index: number, patch: Partial<RuleCondition>) =>
    update({ conditions: rule.conditions.map((c, i) => (i === index ? { ...c, ...patch } : c)) });
  const setAction = (patch: Partial<Rule["actions"]>) => update({ actions: { ...rule.actions, ...patch } });

  /** 三态的动作：不管、设为是、设为否。 */
  const tri = (value: boolean | null | undefined, onChange: (v: boolean | null) => void, on: MessageKey, off: MessageKey) => (
    <Select
      className="w-36"
      value={value == null ? "" : value ? "on" : "off"}
      onChange={(e) => onChange(e.target.value === "" ? null : e.target.value === "on")}
    >
      <option value="">{t("rules.keep")}</option>
      <option value="on">{t(on)}</option>
      <option value="off">{t(off)}</option>
    </Select>
  );

  const test = async () => {
    setTesting(true);
    try {
      setResult(await api.rulesTest(rule));
    } catch (err) {
      toast(errorText(t, err));
    } finally {
      setTesting(false);
    }
  };

  const labelOptions = [...new Set([...categories, ...(rule.actions.label ? [rule.actions.label] : [])])];

  return (
    <div className="flex flex-col gap-4 py-2">
      <Field label={t("rules.name")}>
        <TextInput value={rule.name} onChange={(e) => update({ name: e.target.value })} placeholder={t("rules.namePlaceholder")} />
      </Field>
      <div className="flex flex-wrap items-center gap-3">
        <Segmented<Rule["applies_to"]>
          value={rule.applies_to}
          onChange={(applies_to) => update({ applies_to })}
          options={[
            { value: "all", label: t("rules.appliesAll") },
            { value: "mail", label: t("rules.appliesMail") },
            { value: "article", label: t("rules.appliesArticle") },
          ]}
        />
        <Segmented<Rule["match"]>
          value={rule.match}
          onChange={(match) => update({ match })}
          options={[
            { value: "all", label: t("rules.matchAll") },
            { value: "any", label: t("rules.matchAny") },
          ]}
        />
      </div>

      <div className="flex flex-col gap-2">
        <span className="text-[12px] font-medium text-fg-2">{t("rules.conditions")}</span>
        {rule.conditions.map((c, i) => (
          <div key={i} className="flex items-center gap-2">
            <Select className="w-28 shrink-0" value={c.field} onChange={(e) => setCondition(i, { field: e.target.value as RuleCondition["field"] })}>
              {FIELDS.map((f) => (
                <option key={f} value={f}>
                  {t(FIELD_LABEL[f])}
                </option>
              ))}
            </Select>
            <Select className="w-28 shrink-0" value={c.op} onChange={(e) => setCondition(i, { op: e.target.value as RuleCondition["op"] })}>
              {OPS.map((o) => (
                <option key={o} value={o}>
                  {t(OP_LABEL[o])}
                </option>
              ))}
            </Select>
            <TextInput
              value={c.value}
              onChange={(e) => setCondition(i, { value: e.target.value })}
              placeholder={c.op === "regex" ? "验证码|verification code" : t("rules.valuePlaceholder")}
              className={cx("min-w-0 flex-1", c.op === "regex" && "font-mono text-[12px]")}
              spellCheck={false}
            />
            <IconAction
              label={t("rules.removeCondition")}
              disabled={rule.conditions.length <= 1}
              onClick={() => update({ conditions: rule.conditions.filter((_, j) => j !== i) })}
            >
              <X size={13} />
            </IconAction>
          </div>
        ))}
        <button
          type="button"
          onClick={() => update({ conditions: [...rule.conditions, { field: "subject", op: "contains", value: "" }] })}
          className="flex w-fit items-center gap-1 text-[12px] text-accent hover:underline"
        >
          <Plus size={12} />
          {t("rules.addCondition")}
        </button>
        <p className="text-[11px] leading-relaxed text-fg-3">{t("rules.bodyHint")}</p>
      </div>

      <div className="flex flex-col gap-2">
        <span className="text-[12px] font-medium text-fg-2">{t("rules.actions")}</span>
        <div className="grid grid-cols-[auto_1fr] items-center gap-x-3 gap-y-2 text-[13px] text-fg-2">
          <span>{t("rules.actLabel")}</span>
          <div className="flex items-center gap-2">
            <Select
              className="w-36"
              value={labelOptions.includes(rule.actions.label ?? "") ? (rule.actions.label ?? "") : ""}
              onChange={(e) => setAction({ label: e.target.value || null })}
            >
              <option value="">{t("rules.keep")}</option>
              {labelOptions.map((l) => (
                <option key={l} value={l}>
                  {l}
                </option>
              ))}
            </Select>
            <TextInput
              className="w-40"
              value={rule.actions.label ?? ""}
              onChange={(e) => setAction({ label: e.target.value || null })}
              placeholder={t("rules.newLabel")}
            />
          </div>
          <span>{t("rules.actRead")}</span>
          {tri(rule.actions.read, (read) => setAction({ read }), "rules.setRead", "rules.setUnread")}
          <span>{t("rules.actStar")}</span>
          {tri(rule.actions.star, (star) => setAction({ star }), "rules.setStar", "rules.setUnstar")}
          <span>{t("rules.actArchive")}</span>
          {tri(rule.actions.archive, (archive) => setAction({ archive }), "rules.setArchive", "rules.setUnarchive")}
        </div>
        <label className="flex items-center gap-2 text-[13px] text-fg-2">
          <Switch label={t("rules.stop")} checked={rule.stop} onChange={(stop) => update({ stop })} />
          {t("rules.stop")}
        </label>
      </div>

      {result && (
        <div className="rounded-md bg-hover/60 px-3 py-2 text-[12px] text-fg-2">
          <p>{t("rules.testResult", { matched: result.matched, checked: result.checked })}</p>
          {result.samples.length > 0 && (
            <ul className="mt-1 list-disc pl-4 text-fg-3">
              {result.samples.map((s) => (
                <li key={s.id} className="truncate">
                  {s.title || "—"} <span className="text-fg-4">· {s.source}</span>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
      {problem && <p className="text-[12px] text-fg-3">{t(`rules.problem.${problem}` as MessageKey)}</p>}

      <div className="flex items-center gap-2">
        <Button busy={testing} disabled={problem === "noCondition" || problem === "badRegex"} onClick={() => void test()}>
          <FlaskConical size={14} />
          {t("rules.test")}
        </Button>
        <span className="flex-1" />
        <Button variant="ghost" onClick={onCancel}>
          {t("action.cancel")}
        </Button>
        <Button
          variant="primary"
          busy={saving}
          disabled={problem !== null}
          onClick={async () => {
            setSaving(true);
            try {
              await onSave({ ...rule, name: rule.name.trim() });
            } finally {
              setSaving(false);
            }
          }}
        >
          {t("rules.save")}
        </Button>
      </div>
    </div>
  );
}
