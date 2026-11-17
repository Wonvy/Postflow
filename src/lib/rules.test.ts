// 自动规则的格式检查。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { moveRule, newRule, parseRules, ruleProblem, verificationCodeRule } from "./rules.ts";

test("saved rules are read back with defaults filled in", () => {
  const rules = parseRules([
    { id: "a", conditions: [{ field: "subject", op: "contains", value: "x" }, { field: "nope", op: "contains", value: "y" }] },
    { name: "no id" },
    "junk",
  ]);
  assert.equal(rules.length, 1);
  assert.equal(rules[0]?.enabled, true);
  assert.equal(rules[0]?.match, "all");
  assert.equal(rules[0]?.conditions.length, 1);
  assert.deepEqual(parseRules(null), []);
});

test("a rule needs a condition, an action and a valid regex", () => {
  const rule = newRule("r");
  assert.equal(ruleProblem(rule), "noCondition");
  rule.conditions[0]!.value = "验证码";
  assert.equal(ruleProblem(rule), "noAction");
  rule.actions.read = true;
  assert.equal(ruleProblem(rule), null);
  rule.conditions[0] = { field: "body", op: "regex", value: "(" };
  assert.equal(ruleProblem(rule), "badRegex");
  assert.equal(ruleProblem(verificationCodeRule("验证码", "通知")), null);
});

test("rules move up and down within bounds", () => {
  const [a, b, c] = [newRule("a"), newRule("b"), newRule("c")];
  const list = [a, b, c];
  assert.deepEqual(moveRule(list, c.id, -1).map((r) => r.name), ["a", "c", "b"]);
  assert.equal(moveRule(list, a.id, -1), list);
  assert.equal(moveRule(list, "missing", 1), list);
});
