// 自动规则：条件和动作的格式，和核心里的 crates/core/src/rules.rs 对应。
// 规则存在同步的设置 `rules` 里。这里只有纯函数，方便测试：npm run test:ui

export type RuleField = "subject" | "from" | "to" | "body" | "source" | "any";
export type RuleOp = "contains" | "not_contains" | "equals" | "starts_with" | "ends_with" | "regex";

export interface RuleCondition {
  field: RuleField;
  op: RuleOp;
  value: string;
}

export interface Rule {
  id: string;
  name: string;
  enabled: boolean;
  applies_to: "all" | "mail" | "article";
  match: "all" | "any";
  conditions: RuleCondition[];
  actions: { label?: string | null; read?: boolean | null; star?: boolean | null; archive?: boolean | null };
  stop: boolean;
}

export const SETTING_KEY = "rules";
export const FIELDS: RuleField[] = ["subject", "from", "to", "body", "source", "any"];
export const OPS: RuleOp[] = ["contains", "not_contains", "equals", "starts_with", "ends_with", "regex"];

function newId(): string {
  return `r${Date.now().toString(36)}${Math.random().toString(36).slice(2, 7)}`;
}

export function newRule(name = ""): Rule {
  return {
    id: newId(),
    name,
    enabled: true,
    applies_to: "all",
    match: "all",
    conditions: [{ field: "subject", op: "contains", value: "" }],
    actions: {},
    stop: false,
  };
}

/** 现成的规则：验证码邮件分到一类并标为已读。`label` 是分到的类别名。 */
export function verificationCodeRule(name: string, label: string): Rule {
  return {
    ...newRule(name),
    applies_to: "mail",
    match: "any",
    conditions: [
      { field: "subject", op: "regex", value: "验证码|校验码|动态码|verification code|security code|one-time (pass)?code|OTP" },
      { field: "body", op: "regex", value: "验证码|校验码|verification code|security code" },
    ],
    actions: { label, read: true },
  };
}

/** 设置里存的规则，读不懂的跳过，缺的字段补上默认值。 */
export function parseRules(value: unknown): Rule[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((raw): Rule[] => {
    if (!raw || typeof raw !== "object") return [];
    const r = raw as Partial<Rule>;
    if (typeof r.id !== "string") return [];
    return [
      {
        id: r.id,
        name: typeof r.name === "string" ? r.name : "",
        enabled: r.enabled !== false,
        applies_to: r.applies_to === "mail" || r.applies_to === "article" ? r.applies_to : "all",
        match: r.match === "any" ? "any" : "all",
        conditions: Array.isArray(r.conditions)
          ? r.conditions.filter(
              (c): c is RuleCondition =>
                !!c && FIELDS.includes(c.field) && OPS.includes(c.op) && typeof c.value === "string",
            )
          : [],
        actions: r.actions && typeof r.actions === "object" ? r.actions : {},
        stop: r.stop === true,
      },
    ];
  });
}

/** 规则能不能保存：至少一个填了值的条件、至少一个动作，正则写得对。返回问题，没有问题返回 null。 */
export function ruleProblem(rule: Rule): "noCondition" | "noAction" | "badRegex" | null {
  const filled = rule.conditions.filter((c) => c.value.trim() !== "");
  if (filled.length === 0) return "noCondition";
  for (const c of filled) {
    if (c.op !== "regex") continue;
    try {
      new RegExp(c.value, "i");
    } catch {
      return "badRegex";
    }
  }
  const a = rule.actions;
  if (!a.label?.trim() && a.read == null && a.star == null && a.archive == null) return "noAction";
  return null;
}

export function moveRule(rules: Rule[], id: string, by: -1 | 1): Rule[] {
  const from = rules.findIndex((r) => r.id === id);
  const to = from + by;
  if (from < 0 || to < 0 || to >= rules.length) return rules;
  const next = [...rules];
  const [rule] = next.splice(from, 1);
  if (rule) next.splice(to, 0, rule);
  return next;
}
