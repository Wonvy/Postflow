//! 自动规则：新邮件和新文章进来时，按用户定的条件自动分类、标为已读、加星标或归档。
//! 比如「主题或正文包含『验证码』→ 分到『验证码』」。
//!
//! 规则存在同步的设置 `rules` 里，每台设备都按同一套规则处理自己收到的条目。
//! 每台设备在本机目录的 `rules-state.json` 里记着处理到了哪儿（条目的抓取时间），
//! 只处理之后新来的；已有的条目要在设置里点「应用到现有条目」。
//! 动作都是幂等的（再标一次已读、再分一次类没有影响），两台设备都处理了也不会出问题。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use postflow_store::{ItemPatch, RuleItem};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::refresh::now;
use crate::rpc::{RpcError, parse, reply};
use crate::{Core, Scope, Session};

pub(crate) const SETTING_KEY: &str = "rules";
const STATE_FILE: &str = "rules-state.json";
/// 一次处理多少条新条目。
const BATCH: usize = 500;
/// 「应用到现有条目」最多看最新的多少条。
const EXISTING_LIMIT: usize = 5000;
/// 测试规则时看最新的多少条、列出几条例子。
const TEST_LIMIT: usize = 1000;
const TEST_SAMPLES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppliesTo {
    #[default]
    All,
    Mail,
    Article,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    #[default]
    All,
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    /// 标题（邮件的主题）。
    Subject,
    /// 发件人名字和地址；文章是作者。
    From,
    /// 收件人和抄送（只有邮件有）。
    To,
    /// 正文：文章的摘要和正文，邮件开头的摘要。
    Body,
    /// 订阅源名或邮箱名。
    Source,
    /// 以上任意一处。
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    Contains,
    NotContains,
    Equals,
    StartsWith,
    EndsWith,
    /// 正则表达式，不区分大小写。
    Regex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    pub field: Field,
    pub op: Op,
    pub value: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Actions {
    /// 分到这个类别。
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub read: Option<bool>,
    #[serde(default)]
    pub star: Option<bool>,
    #[serde(default)]
    pub archive: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub applies_to: AppliesTo,
    #[serde(default, rename = "match")]
    pub mode: MatchMode,
    #[serde(default)]
    pub conditions: Vec<Condition>,
    #[serde(default)]
    pub actions: Actions,
    /// 命中后不再看后面的规则。
    #[serde(default)]
    pub stop: bool,
}

fn yes() -> bool {
    true
}

/// 一条规则编译好的样子：正则先编译，坏掉的正则让这条条件永远不成立。
struct Compiled<'a> {
    rule: &'a Rule,
    regexes: Vec<Option<regex::Regex>>,
}

fn compile(rule: &Rule) -> Compiled<'_> {
    let regexes = rule
        .conditions
        .iter()
        .map(|c| {
            (c.op == Op::Regex)
                .then(|| {
                    regex::RegexBuilder::new(&c.value)
                        .case_insensitive(true)
                        .size_limit(1 << 20)
                        .build()
                        .ok()
                })
                .flatten()
        })
        .collect();
    Compiled { rule, regexes }
}

/// 一个条目可以拿来比对的文字，按需算出来（正文要先把 HTML 转成文字）。
struct Texts<'a> {
    item: &'a RuleItem,
    body: std::cell::OnceCell<String>,
}

impl<'a> Texts<'a> {
    fn new(item: &'a RuleItem) -> Self {
        Texts {
            item,
            body: std::cell::OnceCell::new(),
        }
    }

    fn get(&self, field: Field) -> Vec<String> {
        let i = self.item;
        let body = || {
            self.body
                .get_or_init(|| postflow_feed::html_to_text(&i.body))
                .clone()
        };
        match field {
            Field::Subject => vec![i.title.clone()],
            Field::From => vec![format!("{} <{}>", i.author, i.from_address)],
            Field::To => vec![i.to.clone()],
            Field::Body => vec![body()],
            Field::Source => vec![i.source.clone()],
            Field::Any => vec![
                i.title.clone(),
                format!("{} <{}>", i.author, i.from_address),
                i.to.clone(),
                i.source.clone(),
                body(),
            ],
        }
    }
}

fn test_one(cond: &Condition, regex: Option<&regex::Regex>, texts: &Texts) -> bool {
    let value = cond.value.trim().to_lowercase();
    let values = texts.get(cond.field);
    let check = |text: &str| -> bool {
        let text = text.to_lowercase();
        match cond.op {
            Op::Contains | Op::NotContains => text.contains(&value),
            Op::Equals => text.trim() == value,
            Op::StartsWith => text.trim_start().starts_with(&value),
            Op::EndsWith => text.trim_end().ends_with(&value),
            Op::Regex => regex.is_some_and(|r| r.is_match(&text)),
        }
    };
    if cond.op == Op::NotContains {
        // 「不包含」：哪一处都不包含才算
        !values.iter().any(|v| check(v))
    } else {
        values.iter().any(|v| check(v))
    }
}

fn matches(compiled: &Compiled, item: &RuleItem, texts: &Texts) -> bool {
    let rule = compiled.rule;
    if !rule.enabled || rule.conditions.is_empty() {
        return false;
    }
    let kind_ok = match rule.applies_to {
        AppliesTo::All => true,
        AppliesTo::Mail => item.kind == "mail",
        AppliesTo::Article => item.kind == "article",
    };
    if !kind_ok {
        return false;
    }
    let mut results = rule
        .conditions
        .iter()
        .zip(&compiled.regexes)
        .filter(|(c, _)| !c.value.trim().is_empty())
        .map(|(c, r)| test_one(c, r.as_ref(), texts))
        .peekable();
    if results.peek().is_none() {
        return false;
    }
    match rule.mode {
        MatchMode::All => results.all(|r| r),
        MatchMode::Any => results.any(|r| r),
    }
}

/// 对一批条目跑一遍规则，得出每个条目要做的事。先命中的规则说了算（同一项动作后面的不覆盖）。
#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    pub label: Option<String>,
    pub patch: ItemPatch,
}

pub fn evaluate(rules: &[Rule], items: &[RuleItem]) -> Vec<(String, Outcome)> {
    let compiled: Vec<Compiled> = rules.iter().map(compile).collect();
    let mut out = Vec::new();
    for item in items {
        let texts = Texts::new(item);
        let mut outcome = Outcome::default();
        let mut hit = false;
        for c in &compiled {
            if !matches(c, item, &texts) {
                continue;
            }
            hit = true;
            let a = &c.rule.actions;
            if outcome.label.is_none() {
                outcome.label = a
                    .label
                    .as_ref()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty());
            }
            outcome.patch.read = outcome.patch.read.or(a.read);
            outcome.patch.starred = outcome.patch.starred.or(a.star);
            outcome.patch.archived = outcome.patch.archived.or(a.archive);
            if c.rule.stop {
                break;
            }
        }
        if hit {
            out.push((item.id.clone(), outcome));
        }
    }
    out
}

/// 设置里存的规则。读不懂的条目跳过。
pub fn parse_rules(value: Option<&Value>) -> Vec<Rule> {
    match value {
        Some(Value::Array(list)) => list
            .iter()
            .filter_map(|v| serde_json::from_value::<Rule>(v.clone()).ok())
            .collect(),
        _ => Vec::new(),
    }
}

/// 已读、星标、归档三项改动，用来把同样的改动归到一起。
type PatchKey = (Option<bool>, Option<bool>, Option<bool>);

#[derive(Serialize, Deserialize, Default)]
struct State {
    /// 处理到了哪个抓取时间。
    since: i64,
}

#[derive(Deserialize)]
struct ApplyParams {
    /// 只应用这一条（还没保存的也行）；不给就是全部启用的规则。
    rule: Option<Rule>,
}

#[derive(Deserialize)]
struct TestParams {
    rule: Rule,
}

pub(crate) struct RulesState {
    path: PathBuf,
    /// 同一时间只跑一遍，邮件和订阅同时来的时候排队。
    running: tokio::sync::Mutex<()>,
}

impl RulesState {
    pub(crate) fn new(local_dir: &std::path::Path) -> Self {
        RulesState {
            path: local_dir.join(STATE_FILE),
            running: tokio::sync::Mutex::new(()),
        }
    }

    fn read(&self) -> Option<State> {
        serde_json::from_slice(&std::fs::read(&self.path).ok()?).ok()
    }

    fn write(&self, state: &State) {
        if let Ok(bytes) = serde_json::to_vec(state) {
            let _ = std::fs::write(&self.path, bytes);
        }
    }
}

impl Core {
    async fn load_rules(&self, session: &Arc<Session>) -> Result<Vec<Rule>, RpcError> {
        let settings = session.db(|s| s.settings()).await?;
        Ok(parse_rules(settings.get(SETTING_KEY)))
    }

    /// 把规则的结果写下去：分类、已读、星标、归档。邮件的状态会写回服务器。返回动了几条。
    async fn apply_outcomes(
        &self,
        session: &Arc<Session>,
        outcomes: Vec<(String, Outcome)>,
    ) -> Result<usize, RpcError> {
        if outcomes.is_empty() {
            return Ok(0);
        }
        let count = outcomes.len();
        let labels: Vec<(String, Option<String>)> = outcomes
            .iter()
            .filter_map(|(id, o)| o.label.clone().map(|l| (id.clone(), Some(l))))
            .collect();
        if !labels.is_empty() {
            session.db(move |s| s.set_labels(&labels, true)).await?;
        }
        // 同样的改动放在一起改
        let mut groups: HashMap<PatchKey, Vec<String>> = HashMap::new();
        for (id, o) in outcomes {
            let p = &o.patch;
            if p.read.is_some() || p.starred.is_some() || p.archived.is_some() {
                groups
                    .entry((p.read, p.starred, p.archived))
                    .or_default()
                    .push(id);
            }
        }
        for ((read, starred, archived), ids) in groups {
            let patch = ItemPatch {
                read,
                starred,
                archived,
            };
            let for_db = patch.clone();
            let refs = session.db(move |s| s.set_item_state(&ids, &for_db)).await?;
            self.dispatch(session, refs, &patch);
        }
        self.emit_changed(&[Scope::Feeds, Scope::Items], None);
        Ok(count)
    }

    /// 处理上次之后新来的条目。邮件同步完、订阅刷新完都会调用。
    pub(crate) async fn run_rules_on_new(&self, session: &Arc<Session>) {
        let _guard = self.rules_state.running.lock().await;
        let Some(state) = self.rules_state.read() else {
            // 第一次：从现在开始，以前的条目不动
            self.rules_state.write(&State { since: now() });
            return;
        };
        let rules = match self.load_rules(session).await {
            Ok(rules) => rules,
            Err(e) => return tracing::warn!("could not read rules: {}", e.message),
        };
        let mut since = state.since;
        loop {
            let from = since;
            let items = match session.db(move |s| s.rule_items(Some(from), BATCH)).await {
                Ok(items) => items,
                Err(e) => return tracing::warn!("could not read items for rules: {}", e.message),
            };
            let Some(last) = items.last().map(|i| i.fetched_at) else {
                break;
            };
            if rules.iter().any(|r| r.enabled) {
                let outcomes = evaluate(&rules, &items);
                if !outcomes.is_empty() {
                    tracing::debug!(count = outcomes.len(), "rules matched new items");
                }
                if let Err(e) = self.apply_outcomes(session, outcomes).await {
                    tracing::warn!("could not apply rules: {}", e.message);
                }
            }
            since = last;
            if items.len() < BATCH {
                break;
            }
        }
        self.rules_state.write(&State { since });
    }

    pub(crate) async fn rules_rpc(
        &self,
        method: &str,
        params: Value,
        session: &Arc<Session>,
    ) -> Result<Value, RpcError> {
        match method {
            // 试一条规则：在最新的条目里有多少条会命中，列几条例子。不改动任何东西。
            "rules.test" => {
                let p: TestParams = parse(params)?;
                let mut rule = p.rule;
                rule.enabled = true;
                let items = session.db(|s| s.rule_items(None, TEST_LIMIT)).await?;
                let hits = evaluate(std::slice::from_ref(&rule), &items);
                let by_id: HashMap<&str, &RuleItem> =
                    items.iter().map(|i| (i.id.as_str(), i)).collect();
                let samples: Vec<Value> = hits
                    .iter()
                    .take(TEST_SAMPLES)
                    .filter_map(|(id, _)| by_id.get(id.as_str()))
                    .map(|i| json!({ "id": i.id, "title": i.title, "source": i.source, "kind": i.kind }))
                    .collect();
                reply(json!({ "matched": hits.len(), "checked": items.len(), "samples": samples }))
            }
            // 把规则应用到已有的条目（最新的几千条）
            "rules.apply" => {
                let p: ApplyParams = parse(params)?;
                let rules = match p.rule {
                    Some(mut rule) => {
                        rule.enabled = true;
                        vec![rule]
                    }
                    None => self.load_rules(session).await?,
                };
                let items = session.db(|s| s.rule_items(None, EXISTING_LIMIT)).await?;
                let outcomes = evaluate(&rules, &items);
                let count = self.apply_outcomes(session, outcomes).await?;
                reply(json!({ "matched": count }))
            }
            _ => Err(RpcError::new("unknown_method", method.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: &str, title: &str, from: &str, body: &str) -> RuleItem {
        RuleItem {
            id: format!("{kind}:{title}"),
            kind: kind.into(),
            source_id: "s".into(),
            source: if kind == "mail" {
                "工作".into()
            } else {
                "Rust Blog".into()
            },
            title: title.into(),
            author: "Sender".into(),
            from_address: from.into(),
            to: r#"[{"address":"me@example.com"}]"#.into(),
            body: body.into(),
            fetched_at: 1,
        }
    }

    fn rule(json: Value) -> Rule {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn verification_codes_go_to_their_category() {
        let r = rule(json!({
            "id": "1", "name": "验证码", "applies_to": "mail", "match": "any",
            "conditions": [
                { "field": "subject", "op": "contains", "value": "验证码" },
                { "field": "body", "op": "regex", "value": "verification code|\\b\\d{6}\\b" }
            ],
            "actions": { "label": "验证码", "read": true }
        }));
        let items = vec![
            item(
                "mail",
                "您的验证码",
                "noreply@bank.example",
                "验证码 123456",
            ),
            item(
                "mail",
                "Sign in",
                "no-reply@x.example",
                "<p>Your verification code is 482913</p>",
            ),
            item("mail", "周会", "boss@example.com", "下午三点开会"),
            item("article", "验证码的历史", "", "文章"),
        ];
        let out = evaluate(&[r], &items);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].1.label.as_deref(), Some("验证码"));
        assert_eq!(out[0].1.patch.read, Some(true));
        assert_eq!(out[1].0, "mail:Sign in");
    }

    #[test]
    fn all_conditions_must_hold_and_not_contains_checks_every_field() {
        let r = rule(json!({
            "id": "2", "match": "all",
            "conditions": [
                { "field": "from", "op": "ends_with", "value": "@github.com>" },
                { "field": "any", "op": "not_contains", "value": "security" }
            ],
            "actions": { "archive": true }
        }));
        let items = vec![
            item("mail", "PR merged", "notifications@github.com", "ok"),
            item("mail", "Security alert", "notifications@github.com", "ok"),
            item("mail", "PR merged", "someone@gitlab.com", "ok"),
        ];
        let out = evaluate(&[r], &items);
        assert_eq!(
            out.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
            ["mail:PR merged"]
        );
        assert_eq!(out[0].1.patch.archived, Some(true));
    }

    #[test]
    fn earlier_rules_win_and_stop_ends_the_chain() {
        let a = rule(
            json!({ "id": "a", "conditions": [{ "field": "source", "op": "equals", "value": "rust blog" }],
                             "actions": { "label": "Rust", "star": true }, "stop": true }),
        );
        let b = rule(
            json!({ "id": "b", "conditions": [{ "field": "subject", "op": "contains", "value": "release" }],
                             "actions": { "label": "Releases", "read": true } }),
        );
        let out = evaluate(
            &[a.clone(), b.clone()],
            &[item("article", "Rust 1.99 release", "", "")],
        );
        assert_eq!(
            out[0].1,
            Outcome {
                label: Some("Rust".into()),
                patch: ItemPatch {
                    read: None,
                    starred: Some(true),
                    archived: None
                }
            }
        );

        let mut a2 = a;
        a2.stop = false;
        let out = evaluate(&[a2, b], &[item("article", "Rust 1.99 release", "", "")]);
        assert_eq!(out[0].1.label.as_deref(), Some("Rust"));
        assert_eq!(out[0].1.patch.read, Some(true));
    }

    #[test]
    fn disabled_empty_and_broken_rules_never_match() {
        let items = vec![item("mail", "hello", "a@b.c", "")];
        let disabled = rule(
            json!({ "id": "d", "enabled": false, "conditions": [{ "field": "any", "op": "contains", "value": "hello" }] }),
        );
        let empty = rule(
            json!({ "id": "e", "conditions": [{ "field": "any", "op": "contains", "value": "  " }] }),
        );
        let broken = rule(
            json!({ "id": "f", "conditions": [{ "field": "any", "op": "regex", "value": "(" }] }),
        );
        assert!(evaluate(&[disabled, empty, broken], &items).is_empty());
        assert!(parse_rules(Some(&json!([{ "nope": 1 }, { "id": "ok" }]))).len() == 1);
        assert!(parse_rules(None).is_empty());
    }
}
