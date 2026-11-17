//! RSSHub 的路由目录：能订阅哪些网站、每条路由要填哪些参数、参数是什么意思。
//!
//! 目录来自 RSSHub 官方发布的 `routes.json`（https://rsshub.js.org/build/routes.json），
//! 由核心下载并缓存在本机。这里只负责读它、搜索它，以及按网址猜路由（RSSHub 叫 radar）。
//! 把参数填进路径、拼出订阅地址由界面做（src/lib/rsshub.ts）。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CATALOG_URL: &str = "https://rsshub.js.org/build/routes.json";

/// 描述太长的截断，免得一次搜索回传几百 KB。
const MAX_DESCRIPTION: usize = 1500;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ParamOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Param {
    pub name: String,
    pub description: String,
    /// 路径里写成 `:name?` 的可以不填。
    pub optional: bool,
    /// RSSHub 给出的可选值。有的话界面显示成下拉框。
    pub options: Vec<ParamOption>,
    pub default: Option<String>,
    /// 这个参数能带 `/`（路径里写成 `:name{.+}`），填的时候不转义斜杠。
    pub multi_segment: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Route {
    /// 命名空间，订阅地址的第一段，如 `github`。
    pub namespace: String,
    pub site: String,
    pub site_url: Option<String>,
    pub name: String,
    /// 命名空间之后的路径模板，如 `/activity/:user`。
    pub path: String,
    pub example: String,
    pub description: String,
    pub params: Vec<Param>,
    pub categories: Vec<String>,
    /// 需要在 RSSHub 实例上另外配置（比如 API 密钥）才能用。
    pub require_config: bool,
    /// 需要浏览器渲染，自建实例可能没开。
    pub require_puppeteer: bool,
    pub anti_crawler: bool,
    #[serde(skip)]
    radar: Vec<(String, String)>,
    #[serde(skip)]
    haystack: String,
}

/// 按网址猜到的一条路由，以及从网址里取出的参数值。
#[derive(Debug, Clone, Serialize)]
pub struct RadarMatch {
    pub route: Route,
    pub values: HashMap<String, String>,
}

#[derive(Debug, Default)]
pub struct Catalog {
    routes: Vec<Route>,
}

#[derive(Deserialize)]
struct RawNamespace {
    #[serde(default)]
    name: String,
    url: Option<String>,
    #[serde(default)]
    routes: HashMap<String, RawRoute>,
    zh: Option<RawLocalized>,
}

#[derive(Deserialize)]
struct RawLocalized {
    name: Option<String>,
}

#[derive(Deserialize)]
struct RawRoute {
    path: Value,
    #[serde(default)]
    name: String,
    #[serde(default)]
    example: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    parameters: HashMap<String, Value>,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    features: HashMap<String, Value>,
    #[serde(default)]
    radar: Vec<RawRadar>,
    zh: Option<RawLocalized>,
}

#[derive(Deserialize)]
struct RawRadar {
    #[serde(default)]
    source: Vec<String>,
    target: Option<Value>,
}

/// 路径模板里的一个参数：名字、能不能省、能不能带斜杠。
#[derive(Debug, PartialEq)]
struct Token {
    name: String,
    optional: bool,
    multi: bool,
}

/// 读出路径模板里的参数，如 `/:category{.+}?` → category（可省、可带斜杠）。
fn tokens(path: &str) -> Vec<Token> {
    path.split('/')
        .filter_map(|seg| seg.strip_prefix(':'))
        .map(|seg| {
            let optional = seg.ends_with('?');
            let seg = seg.trim_end_matches('?');
            let (name, regex) = match seg.find('{') {
                Some(i) => (&seg[..i], &seg[i..]),
                None => (seg, ""),
            };
            Token {
                name: name.to_string(),
                optional,
                multi: regex.contains(".+") || regex.contains(".*"),
            }
        })
        .collect()
}

fn text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

fn param(token: &Token, raw: Option<&Value>) -> Param {
    let (description, options, default) = match raw {
        Some(Value::Object(o)) => {
            let options = o
                .get("options")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|opt| {
                            let value = text(opt.get("value")?)?;
                            let label = opt
                                .get("label")
                                .and_then(text)
                                .unwrap_or_else(|| value.clone());
                            Some(ParamOption { value, label })
                        })
                        .collect()
                })
                .unwrap_or_default();
            (
                o.get("description").and_then(text).unwrap_or_default(),
                options,
                o.get("default").and_then(text).filter(|d| !d.is_empty()),
            )
        }
        Some(v) => (text(v).unwrap_or_default(), Vec::new(), None),
        None => (String::new(), Vec::new(), None),
    };
    Param {
        name: token.name.clone(),
        description: truncate(description.trim(), MAX_DESCRIPTION),
        optional: token.optional,
        options,
        default,
        multi_segment: token.multi,
    }
}

impl Catalog {
    /// 读 `routes.json`。`zh` 为真时，有中文名字的用中文名字。
    pub fn parse(json: &[u8], zh: bool) -> serde_json::Result<Catalog> {
        let raw: HashMap<String, RawNamespace> = serde_json::from_slice(json)?;
        let mut routes = Vec::new();
        for (namespace, ns) in raw {
            let site = zh
                .then(|| ns.zh.as_ref().and_then(|z| z.name.clone()))
                .flatten()
                .filter(|n| !n.is_empty())
                .unwrap_or(ns.name);
            for (_, r) in ns.routes {
                // 有的路由一条对应好几个路径，取第一个
                let path = match &r.path {
                    Value::String(p) => p.clone(),
                    Value::Array(list) => match list.iter().find_map(Value::as_str) {
                        Some(p) => p.to_string(),
                        None => continue,
                    },
                    _ => continue,
                };
                let name = zh
                    .then(|| r.zh.as_ref().and_then(|z| z.name.clone()))
                    .flatten()
                    .filter(|n| !n.is_empty())
                    .unwrap_or(r.name);
                let params = tokens(&path)
                    .iter()
                    .map(|tok| param(tok, r.parameters.get(&tok.name)))
                    .collect();
                let flag = |key: &str| {
                    r.features
                        .get(key)
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                };
                let radar = r
                    .radar
                    .iter()
                    .filter_map(|rule| {
                        let target = rule.target.as_ref()?.as_str()?.to_string();
                        Some(rule.source.iter().map(move |s| (s.clone(), target.clone())))
                    })
                    .flatten()
                    .collect();
                let haystack = format!(
                    "{namespace} {site} {} {name} {path} {}",
                    ns.url.as_deref().unwrap_or(""),
                    r.categories.join(" ")
                )
                .to_lowercase();
                routes.push(Route {
                    namespace: namespace.clone(),
                    site: site.clone(),
                    site_url: ns.url.clone(),
                    name,
                    path,
                    example: r.example,
                    description: truncate(r.description.trim(), MAX_DESCRIPTION),
                    params,
                    categories: r.categories,
                    require_config: flag("requireConfig"),
                    require_puppeteer: flag("requirePuppeteer"),
                    anti_crawler: flag("antiCrawler"),
                    radar,
                    haystack,
                });
            }
        }
        // 固定顺序，同样的搜索每次结果一样
        routes.sort_by(|a, b| (&a.namespace, &a.path).cmp(&(&b.namespace, &b.path)));
        Ok(Catalog { routes })
    }

    pub fn len(&self) -> usize {
        self.routes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }

    /// 搜索：每个词都要出现。网站名、命名空间对上的排前面，其次是路由名。
    pub fn search(&self, query: &str, limit: usize) -> Vec<Route> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        if words.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(i32, &Route)> = self
            .routes
            .iter()
            .filter(|r| words.iter().all(|w| r.haystack.contains(w.as_str())))
            .map(|r| {
                let site = r.site.to_lowercase();
                let name = r.name.to_lowercase();
                let host = r.site_url.as_deref().unwrap_or("").to_lowercase();
                let mut score = 0;
                for w in &words {
                    if r.namespace == *w || site == *w {
                        score += 100;
                    } else if site.starts_with(w.as_str()) || r.namespace.starts_with(w.as_str()) {
                        score += 60;
                    } else if host.contains(w.as_str()) || site.contains(w.as_str()) {
                        score += 40;
                    }
                    if name.contains(w.as_str()) {
                        score += 20;
                    }
                }
                // 不用另外配置的排前面
                if !r.require_config && !r.require_puppeteer {
                    score += 5;
                }
                (score, r)
            })
            .collect();
        scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
        scored
            .into_iter()
            .take(limit)
            .map(|(_, r)| r.clone())
            .collect()
    }

    /// 按网页地址猜能用的路由，比如 `https://github.com/DIYgod` → GitHub 用户动态，user = DIYgod。
    pub fn radar(&self, page_url: &str, limit: usize) -> Vec<RadarMatch> {
        let Ok(url) = url::Url::parse(page_url.trim()) else {
            return Vec::new();
        };
        let Some(host) = url
            .host_str()
            .map(|h| h.trim_start_matches("www.").to_lowercase())
        else {
            return Vec::new();
        };
        let segments: Vec<String> = url
            .path_segments()
            .map(|s| s.filter(|p| !p.is_empty()).map(percent_decode).collect())
            .unwrap_or_default();

        let mut found = Vec::new();
        for route in &self.routes {
            for (source, target) in &route.radar {
                let Some(values) = match_source(source, &host, &segments) else {
                    continue;
                };
                let Some(values) = target_values(target, &route.path, values) else {
                    continue;
                };
                found.push(RadarMatch {
                    route: route.clone(),
                    values,
                });
                break;
            }
        }
        // 匹配得越具体（取出的参数越多）越可能是想要的
        found.sort_by_key(|m| std::cmp::Reverse(m.values.len()));
        found.truncate(limit);
        found
    }
}

fn percent_decode(s: &str) -> String {
    url::form_urlencoded::parse(format!("x={}", s.replace('+', "%2B")).as_bytes())
        .next()
        .map(|(_, v)| v.into_owned())
        .unwrap_or_else(|| s.to_string())
}

/// 网址和 radar 的来源模板（如 `github.com/:user`）对不对得上，对上了返回取出的值。
fn match_source(source: &str, host: &str, segments: &[String]) -> Option<HashMap<String, String>> {
    let source = source
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let (src_host, src_path) = match source.find('/') {
        Some(i) => (&source[..i], &source[i..]),
        None => (source, "/"),
    };
    let src_host = src_host.trim_start_matches("www.").to_lowercase();
    let host_ok = if let Some(rest) = src_host.strip_prefix("*.") {
        host == rest || host.ends_with(&format!(".{rest}"))
    } else {
        host == src_host
    };
    if !host_ok {
        return None;
    }
    let path = src_path.split(['?', '#']).next().unwrap_or("");
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    let mut values = HashMap::new();
    let mut i = 0;
    for part in &parts {
        if *part == "*" {
            return Some(values);
        }
        if let Some(name) = part.strip_prefix(':') {
            let optional = name.ends_with('?');
            let name = name.trim_end_matches('?');
            match segments.get(i) {
                Some(v) => {
                    values.insert(name.to_string(), v.clone());
                    i += 1;
                }
                None if optional => {}
                None => return None,
            }
        } else if segments.get(i).map(String::as_str) == Some(*part) {
            i += 1;
        } else {
            return None;
        }
    }
    // 网址比模板长就不算对上（模板以 `*` 结尾的上面已经返回了），免得仓库页面也匹配到用户主页的路由
    if i < segments.len() {
        return None;
    }
    Some(values)
}

/// 目标路径（如 `/activity/:user`）要的参数都有了才算数；只保留路由用得上的值。
fn target_values(
    target: &str,
    route_path: &str,
    mut values: HashMap<String, String>,
) -> Option<HashMap<String, String>> {
    let wanted = tokens(target);
    for tok in &wanted {
        if !tok.optional && !values.contains_key(&tok.name) {
            return None;
        }
    }
    let known: Vec<String> = tokens(route_path).into_iter().map(|t| t.name).collect();
    values.retain(|k, _| known.contains(k));
    Some(values)
}

impl crate::Client {
    /// 下载 RSSHub 的路由目录（几 MB 的 JSON）。
    pub async fn rsshub_catalog(&self) -> crate::Result<Vec<u8>> {
        let page = self
            .get(CATALOG_URL, None, Some(std::time::Duration::from_secs(120)))
            .await?
            .ok_or(crate::Error::Status(304))?;
        Ok(page.body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "github": {
        "name": "GitHub", "url": "github.com",
        "routes": {
          "/activity/:user": {
            "path": "/activity/:user", "name": "User Activities", "example": "/github/activity/DIYgod",
            "parameters": { "user": "GitHub username" },
            "categories": ["programming"],
            "features": { "requireConfig": false },
            "radar": [{ "source": ["github.com/:user"], "target": "/activity/:user" }]
          },
          "/issue/:user/:repo/:state?": {
            "path": "/issue/:user/:repo/:state?", "name": "Repo Issues", "example": "/github/issue/DIYgod/RSSHub",
            "parameters": {
              "user": "GitHub username", "repo": "GitHub repo name",
              "state": { "description": "the state of the issues", "default": "open",
                         "options": [{ "value": "open", "label": "Open" }, { "value": "closed", "label": "Closed" }] }
            },
            "radar": [{ "source": ["github.com/:user/:repo/issues", "github.com/:user/:repo/issues/:id"], "target": "/issue/:user/:repo" }]
          }
        }
      },
      "81": {
        "name": "解放军", "url": "81rc.81.cn", "zh": { "name": "中国人民解放军" },
        "routes": {
          "/81rc/:category{.+}?": {
            "path": "/81rc/:category{.+}?", "name": "专业技术人才网", "example": "/81/81rc/sy/gzdt_210283",
            "parameters": { "category": "分类" },
            "features": { "requireConfig": true }
          }
        }
      }
    }"#;

    fn catalog() -> Catalog {
        Catalog::parse(SAMPLE.as_bytes(), false).unwrap()
    }

    #[test]
    fn reads_routes_and_parameters() {
        let c = catalog();
        assert_eq!(c.len(), 3);
        let issues = c.search("issues", 5).remove(0);
        assert_eq!(issues.namespace, "github");
        assert_eq!(issues.params.len(), 3);
        let state = &issues.params[2];
        assert!(state.optional);
        assert_eq!(state.default.as_deref(), Some("open"));
        assert_eq!(
            state.options[1],
            ParamOption {
                value: "closed".into(),
                label: "Closed".into()
            }
        );
        let category = &c.search("81rc", 5)[0].params[0];
        assert!(category.optional && category.multi_segment);
    }

    #[test]
    fn search_ranks_the_site_first_and_needs_every_word() {
        let c = catalog();
        let found = c.search("github", 10);
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|r| r.namespace == "github"));
        assert_eq!(c.search("github activity", 10).len(), 1);
        assert!(c.search("nothing-like-this", 10).is_empty());
        assert!(c.search("  ", 10).is_empty());
    }

    #[test]
    fn chinese_names_when_asked() {
        let zh = Catalog::parse(SAMPLE.as_bytes(), true).unwrap();
        assert_eq!(zh.search("81rc", 1)[0].site, "中国人民解放军");
        assert_eq!(catalog().search("81rc", 1)[0].site, "解放军");
    }

    #[test]
    fn radar_finds_routes_from_a_page_address() {
        let c = catalog();
        let user = c.radar("https://github.com/DIYgod", 5);
        assert_eq!(user.len(), 1);
        assert_eq!(user[0].route.path, "/activity/:user");
        assert_eq!(user[0].values["user"], "DIYgod");

        let issues = c.radar("https://www.github.com/DIYgod/RSSHub/issues/123", 5);
        assert_eq!(
            issues.len(),
            1,
            "the user page route must not match a repo page"
        );
        assert_eq!(issues[0].route.path, "/issue/:user/:repo/:state?");
        assert_eq!(
            issues[0].values.get("repo").map(String::as_str),
            Some("RSSHub")
        );
        // id 不是这条路由的参数，丢掉
        assert!(!issues[0].values.contains_key("id"));

        assert!(c.radar("https://gitlab.com/DIYgod", 5).is_empty());
        assert!(c.radar("not a url", 5).is_empty());
    }
}
