//! 一个网页肯不肯被嵌进别的页面里显示。
//!
//! 阅读区的「原网页」模式在浏览器里只能用 iframe，网站用 `X-Frame-Options` 或 CSP 的
//! `frame-ancestors` 拒绝被嵌入时，iframe 里只有一块空白，页面也拿不到原因。所以先由核心
//! 看一眼响应头，拒绝的就直接告诉用户，并给出在浏览器中打开的办法。
//!
//! 规则按现在的浏览器来：有 `frame-ancestors` 时以它为准、忽略 `X-Frame-Options`；
//! `X-Frame-Options` 只认 `DENY` 和 `SAMEORIGIN`，`ALLOW-FROM` 之类浏览器早已不支持，当作没写。

use std::time::Duration;

use reqwest::header::{CONTENT_SECURITY_POLICY, HeaderMap, X_FRAME_OPTIONS};
use serde::Serialize;
use url::Url;

use crate::client::Client;
use crate::{Error, Result};

/// 检查的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FramePolicy {
    /// 能嵌进 `embedder` 这个页面。
    pub embeddable: bool,
    /// 不能嵌入时，是哪个响应头说的，原样给用户看，比如 `X-Frame-Options: DENY`。
    pub reason: Option<String>,
    /// 跳转之后实际的地址。
    pub final_url: String,
}

impl Client {
    /// 请求一次 `url`，只看响应头，判断它能不能嵌进 `embedder`（发起嵌入的页面的地址）里。
    pub async fn frame_policy(&self, url: &str, embedder: &str) -> Result<FramePolicy> {
        let embedder = Url::parse(embedder).map_err(|_| Error::InvalidUrl(embedder.to_string()))?;
        let resp = self
            .http
            .get(url)
            .timeout(Duration::from_secs(15))
            .send()
            .await
            .map_err(crate::client::describe)?;
        let final_url = resp.url().clone();
        // 只要响应头，正文不读，丢掉响应时连接就断了
        let verdict = evaluate(resp.headers(), &final_url, &embedder);
        Ok(FramePolicy {
            embeddable: verdict.is_none(),
            reason: verdict,
            final_url: final_url.to_string(),
        })
    }
}

/// 按响应头判断。能嵌入时返回 `None`，否则返回原因。
pub fn evaluate(headers: &HeaderMap, page: &Url, embedder: &Url) -> Option<String> {
    let policies: Vec<&str> = headers
        .get_all(CONTENT_SECURITY_POLICY)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .collect();
    let mut governed = false;
    for policy in policies {
        let Some(sources) = frame_ancestors(policy) else {
            continue;
        };
        governed = true;
        if !sources.iter().any(|s| source_matches(s, page, embedder)) {
            let list = if sources.is_empty() {
                "'none'".to_string()
            } else {
                sources.join(" ")
            };
            return Some(format!("Content-Security-Policy: frame-ancestors {list}"));
        }
    }
    if governed {
        return None;
    }

    for value in headers
        .get_all(X_FRAME_OPTIONS)
        .iter()
        .filter_map(|v| v.to_str().ok())
    {
        for token in value.split(',').map(|t| t.trim().to_ascii_uppercase()) {
            let blocked = match token.as_str() {
                "DENY" => true,
                "SAMEORIGIN" => page.origin() != embedder.origin(),
                _ => false,
            };
            if blocked {
                return Some(format!("X-Frame-Options: {token}"));
            }
        }
    }
    None
}

/// 一条 CSP 里 `frame-ancestors` 的来源列表。没有这条指令时返回 `None`；`'none'` 是空列表。
fn frame_ancestors(policy: &str) -> Option<Vec<String>> {
    policy.split(';').find_map(|directive| {
        let mut parts = directive.split_ascii_whitespace();
        let name = parts.next()?;
        if !name.eq_ignore_ascii_case("frame-ancestors") {
            return None;
        }
        Some(
            parts
                .filter(|s| !s.eq_ignore_ascii_case("'none'"))
                .map(str::to_string)
                .collect(),
        )
    })
}

fn default_port(scheme: &str) -> Option<u16> {
    match scheme {
        "http" | "ws" => Some(80),
        "https" | "wss" => Some(443),
        _ => None,
    }
}

/// 来源表达式的协议部分能不能匹配：`http` 也允许升级到 `https`。
fn scheme_matches(expected: &str, actual: &str) -> bool {
    expected.eq_ignore_ascii_case(actual)
        || (expected.eq_ignore_ascii_case("http") && actual == "https")
        || (expected.eq_ignore_ascii_case("ws") && actual == "wss")
}

/// `frame-ancestors` 里的一个来源能不能匹配发起嵌入的页面。
fn source_matches(source: &str, page: &Url, embedder: &Url) -> bool {
    let scheme = embedder.scheme();
    if source == "*" {
        return matches!(scheme, "http" | "https" | "ws" | "wss") || scheme == page.scheme();
    }
    if source.eq_ignore_ascii_case("'self'") {
        return page.origin() == embedder.origin();
    }
    if source.starts_with('\'') {
        // 'unsafe-inline' 之类在 frame-ancestors 里没有意义
        return false;
    }
    // 只有协议，比如 `https:`
    if let Some(only) = source.strip_suffix(':')
        && !only.contains('/')
    {
        return scheme_matches(only, scheme);
    }

    let (source_scheme, rest) = match source.split_once("://") {
        Some((s, rest)) => (Some(s), rest),
        None => (None, source),
    };
    match source_scheme {
        Some(s) if !scheme_matches(s, scheme) => return false,
        // 没写协议时，跟受保护的页面同协议（http 页面也允许 https 的嵌入方）
        None if !scheme_matches(page.scheme(), scheme) => return false,
        _ => {}
    }
    let host_port = rest.split('/').next().unwrap_or_default();
    // IPv6 的地址本身带冒号，写在方括号里
    let (host, port) = match host_port.rsplit_once(':') {
        Some((h, p)) if !host_port.starts_with('[') || h.ends_with(']') => (h, Some(p)),
        _ => (host_port, None),
    };
    let Some(actual_host) = embedder.host_str() else {
        return false;
    };
    let host_ok = if let Some(suffix) = host.strip_prefix("*.") {
        let actual = actual_host.to_ascii_lowercase();
        let suffix = suffix.to_ascii_lowercase();
        actual.ends_with(&format!(".{suffix}"))
    } else {
        host.eq_ignore_ascii_case(actual_host)
    };
    if !host_ok {
        return false;
    }
    let actual_port = embedder.port_or_known_default();
    match port {
        Some("*") => true,
        Some(p) => p.parse::<u16>().ok() == actual_port,
        // 没写端口：嵌入方得用它那个协议的默认端口
        None => actual_port == default_port(scheme),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(*name, HeaderValue::from_static(value));
        }
        map
    }

    fn check(pairs: &[(&'static str, &'static str)], embedder: &str) -> Option<String> {
        let page = Url::parse("https://news.example.com/post/1").unwrap();
        evaluate(&headers(pairs), &page, &Url::parse(embedder).unwrap())
    }

    const LOCAL: &str = "http://127.0.0.1:4319/";

    #[test]
    fn pages_without_headers_can_be_framed() {
        assert_eq!(check(&[], LOCAL), None);
        assert_eq!(
            check(&[("content-security-policy", "default-src 'self'")], LOCAL),
            None
        );
    }

    #[test]
    fn x_frame_options() {
        assert_eq!(
            check(&[("x-frame-options", "DENY")], LOCAL).as_deref(),
            Some("X-Frame-Options: DENY")
        );
        assert_eq!(
            check(&[("x-frame-options", "sameorigin")], LOCAL).as_deref(),
            Some("X-Frame-Options: SAMEORIGIN")
        );
        assert_eq!(
            check(
                &[("x-frame-options", "SAMEORIGIN")],
                "https://news.example.com/"
            ),
            None
        );
        // 浏览器早已不认 ALLOW-FROM，当作没写
        assert_eq!(
            check(
                &[("x-frame-options", "ALLOW-FROM https://a.example.com")],
                LOCAL
            ),
            None
        );
    }

    #[test]
    fn frame_ancestors_wins_over_x_frame_options() {
        let allow_all = [
            ("x-frame-options", "DENY"),
            ("content-security-policy", "frame-ancestors *"),
        ];
        assert_eq!(check(&allow_all, LOCAL), None);

        let none = [(
            "content-security-policy",
            "script-src 'self'; frame-ancestors 'none'",
        )];
        assert_eq!(
            check(&none, LOCAL).as_deref(),
            Some("Content-Security-Policy: frame-ancestors 'none'")
        );

        let own = [(
            "content-security-policy",
            "frame-ancestors 'self' https://*.example.com",
        )];
        assert!(check(&own, LOCAL).is_some());
        assert_eq!(check(&own, "https://reader.example.com/"), None);
        assert!(check(&own, "https://example.org/").is_some());
    }

    #[test]
    fn host_and_scheme_sources() {
        let local = [(
            "content-security-policy",
            "frame-ancestors http://127.0.0.1:4319",
        )];
        assert_eq!(check(&local, LOCAL), None);
        assert!(check(&local, "http://127.0.0.1:4320/").is_some());

        let any_port = [("content-security-policy", "frame-ancestors 127.0.0.1:*")];
        // 没写协议时跟页面同协议，http 的嵌入方不匹配 https 的页面
        assert!(check(&any_port, LOCAL).is_some());
        assert_eq!(check(&any_port, "https://127.0.0.1:9000/"), None);

        let https_only = [("content-security-policy", "frame-ancestors https:")];
        assert!(check(&https_only, LOCAL).is_some());
        assert_eq!(check(&https_only, "https://reader.example.net/"), None);
    }

    #[test]
    fn every_policy_must_allow() {
        let two = [
            ("content-security-policy", "frame-ancestors *"),
            ("content-security-policy", "frame-ancestors 'self'"),
        ];
        assert!(check(&two, LOCAL).is_some());
    }
}
