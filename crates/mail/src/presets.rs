//! 常见邮箱服务商的服务器设置。用户只填邮箱地址和密码，其余由这里推断。

use std::time::Duration;

use serde::Serialize;

use crate::imap::{Security, ServerConfig};

#[derive(Debug, Clone, Serialize)]
pub struct Preset {
    /// 稳定的标识，界面据此显示本地化的名称和提示。
    pub id: &'static str,
    pub name: &'static str,
    pub imap_host: &'static str,
    pub imap_port: u16,
    pub imap_security: Security,
    /// 这个服务商要求的密码类型，界面据此提示用户去哪里生成。
    /// password：普通密码；app_password：应用专用密码；auth_code：授权码；oauth：只支持 OAuth 登录。
    pub password_kind: &'static str,
    /// 用户名是否只取 @ 前面的部分。
    pub username_local_part: bool,
    /// 这个服务商的邮箱域名，界面用来给出示例地址。企业邮箱用自己的域名，这里为空。
    pub domains: &'static [&'static str],
}

const fn p(
    id: &'static str,
    name: &'static str,
    imap_host: &'static str,
    password_kind: &'static str,
) -> Preset {
    Preset {
        id,
        name,
        imap_host,
        imap_port: 993,
        imap_security: Security::Tls,
        password_kind,
        username_local_part: false,
        domains: &[],
    }
}

const PRESETS: &[(&[&str], Preset)] = &[
    (
        &["gmail.com", "googlemail.com"],
        p("gmail", "Gmail", "imap.gmail.com", "app_password"),
    ),
    (
        &[
            "outlook.com",
            "hotmail.com",
            "live.com",
            "msn.com",
            "outlook.cn",
        ],
        p("outlook", "Outlook", "outlook.office365.com", "oauth"),
    ),
    (
        &["icloud.com", "me.com", "mac.com"],
        p("icloud", "iCloud", "imap.mail.me.com", "app_password"),
    ),
    (
        &["yahoo.com", "ymail.com", "rocketmail.com", "yahoo.co.jp"],
        p("yahoo", "Yahoo", "imap.mail.yahoo.com", "app_password"),
    ),
    (
        &["qq.com", "vip.qq.com", "foxmail.com"],
        p("qq", "QQ 邮箱", "imap.qq.com", "auth_code"),
    ),
    (
        &["163.com", "vip.163.com"],
        p("163", "网易 163", "imap.163.com", "auth_code"),
    ),
    (
        &["126.com", "vip.126.com"],
        p("126", "网易 126", "imap.126.com", "auth_code"),
    ),
    (
        &["yeah.net"],
        p("yeah", "网易 yeah.net", "imap.yeah.net", "auth_code"),
    ),
    (
        &["188.com"],
        p("188", "网易 188", "imap.188.com", "auth_code"),
    ),
    (
        &["sina.com", "sina.cn", "vip.sina.com"],
        p("sina", "新浪邮箱", "imap.sina.com", "auth_code"),
    ),
    (
        &["sohu.com"],
        p("sohu", "搜狐邮箱", "imap.sohu.com", "auth_code"),
    ),
    (
        &["139.com"],
        p("139", "139 邮箱", "imap.139.com", "auth_code"),
    ),
    (
        &["aliyun.com"],
        p("aliyun", "阿里邮箱", "imap.aliyun.com", "password"),
    ),
    (
        &["fastmail.com", "fastmail.fm"],
        p("fastmail", "Fastmail", "imap.fastmail.com", "app_password"),
    ),
    (
        &["zoho.com", "zohomail.com"],
        p("zoho", "Zoho Mail", "imap.zoho.com", "app_password"),
    ),
    (
        &["yandex.com", "yandex.ru", "ya.ru"],
        p("yandex", "Yandex", "imap.yandex.com", "app_password"),
    ),
    (
        &["gmx.com", "gmx.net", "gmx.de"],
        p("gmx", "GMX", "imap.gmx.com", "password"),
    ),
    (
        &["mail.ru"],
        p("mailru", "Mail.ru", "imap.mail.ru", "app_password"),
    ),
    (
        &["aol.com"],
        p("aol", "AOL", "imap.aol.com", "app_password"),
    ),
];

/// 企业邮箱用自己的域名，没法从地址推断，只能让用户手动选。
const BUSINESS: &[Preset] = &[
    p("exmail", "腾讯企业邮箱", "imap.exmail.qq.com", "auth_code"),
    p("qiye163", "网易企业邮箱", "imap.qiye.163.com", "auth_code"),
    p(
        "qiye_aliyun",
        "阿里企业邮箱",
        "imap.qiye.aliyun.com",
        "password",
    ),
    p(
        "workspace",
        "Google Workspace",
        "imap.gmail.com",
        "app_password",
    ),
    p("m365", "Microsoft 365", "outlook.office365.com", "oauth"),
    p("feishu", "飞书邮箱", "imap.feishu.cn", "app_password"),
    p("lark", "Lark Mail", "imap.larksuite.com", "app_password"),
    p("263", "263 企业邮箱", "imap.263.net", "password"),
    p(
        "icloud_custom",
        "iCloud 自定域名",
        "imap.mail.me.com",
        "app_password",
    ),
    p(
        "fastmail_custom",
        "Fastmail 自定域名",
        "imap.fastmail.com",
        "app_password",
    ),
    p(
        "zoho_custom",
        "Zoho Mail 自定域名",
        "imap.zoho.com",
        "app_password",
    ),
];

/// 企业邮箱和自定义域名的邮件交给谁收，看域名的 MX 记录就知道。按 MX 主机名后缀对应到服务商。
const MX_PROVIDERS: &[(&str, &str)] = &[
    ("qq.com", "exmail"),
    ("mxhichina.com", "qiye_aliyun"),
    ("qiye.aliyun.com", "qiye_aliyun"),
    ("qiye.163.com", "qiye163"),
    ("netease.com", "qiye163"),
    ("ym.163.com", "qiye163"),
    ("google.com", "workspace"),
    ("googlemail.com", "workspace"),
    ("outlook.com", "m365"),
    ("feishu.cn", "feishu"),
    ("larksuite.com", "lark"),
    ("263.net", "263"),
    ("263xmail.com", "263"),
    ("icloud.com", "icloud_custom"),
    ("messagingengine.com", "fastmail_custom"),
    ("zoho.com", "zoho_custom"),
];

/// 查 MX 记录的超时。查不到就退回到按常见规则猜。
const MX_TIMEOUT: Duration = Duration::from_secs(4);

/// 所有已知的服务商，供手动选择。
pub fn presets() -> Vec<Preset> {
    PRESETS
        .iter()
        .map(|(domains, p)| Preset {
            domains,
            ..p.clone()
        })
        .chain(BUSINESS.iter().cloned())
        .collect()
}

/// 根据 MX 主机名找服务商。
pub fn provider_for_mx(mx_hosts: &[String]) -> Option<Preset> {
    mx_hosts.iter().find_map(|host| {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        let id = MX_PROVIDERS
            .iter()
            .find(|(suffix, _)| host == *suffix || host.ends_with(&format!(".{suffix}")))
            .map(|(_, id)| *id)?;
        BUSINESS.iter().find(|p| p.id == id).cloned()
    })
}

async fn mx_hosts(domain: &str) -> Vec<String> {
    use hickory_resolver::TokioResolver;
    use hickory_resolver::proto::rr::RData;

    let Ok(resolver) = TokioResolver::builder_tokio().and_then(|b| b.build()) else {
        return Vec::new();
    };
    let lookup = tokio::time::timeout(MX_TIMEOUT, resolver.mx_lookup(format!("{domain}."))).await;
    let Ok(Ok(lookup)) = lookup else {
        return Vec::new();
    };
    let mut hosts: Vec<(u16, String)> = lookup
        .answers()
        .iter()
        .filter_map(|r| match &r.data {
            RData::MX(mx) => Some((mx.preference, mx.exchange.to_ascii())),
            _ => None,
        })
        .collect();
    hosts.sort();
    hosts.into_iter().map(|(_, h)| h).collect()
}

/// 和 `guess` 一样，但不认识的域名会再查一下 MX 记录，认出企业邮箱和自定义域名用的服务商。
pub async fn detect(email: &str) -> Option<(ServerConfig, Option<Preset>)> {
    let (config, preset) = guess(email)?;
    if preset.is_some() {
        return Some((config, preset));
    }
    let domain = email.trim().rsplit_once('@')?.1.to_ascii_lowercase();
    match provider_for_mx(&mx_hosts(&domain).await) {
        Some(p) => {
            let config = ServerConfig {
                host: p.imap_host.to_string(),
                port: p.imap_port,
                security: p.imap_security,
                username: email.trim().to_string(),
            };
            Some((config, Some(p)))
        }
        None => Some((config, None)),
    }
}

/// 根据邮箱地址推断服务器设置。不认识的域名按最常见的约定猜 `imap.<域名>`。
pub fn guess(email: &str) -> Option<(ServerConfig, Option<Preset>)> {
    let email = email.trim();
    let (local, domain) = email.rsplit_once('@')?;
    if local.is_empty() || domain.is_empty() || !domain.contains('.') {
        return None;
    }
    let domain = domain.to_ascii_lowercase();
    let preset = PRESETS
        .iter()
        .find(|(domains, _)| domains.contains(&domain.as_str()))
        .map(|(_, p)| p.clone());
    let config = match &preset {
        Some(p) => ServerConfig {
            host: p.imap_host.to_string(),
            port: p.imap_port,
            security: p.imap_security,
            username: if p.username_local_part {
                local.to_string()
            } else {
                email.to_string()
            },
        },
        None => ServerConfig {
            host: format!("imap.{domain}"),
            port: 993,
            security: Security::Tls,
            username: email.to_string(),
        },
    };
    Some((config, preset))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_and_unknown_domains() {
        assert!(
            presets()
                .iter()
                .any(|p| p.id == "qq" && p.domains.contains(&"qq.com"))
        );
        let (cfg, preset) = guess("someone@QQ.com").unwrap();
        assert_eq!(cfg.host, "imap.qq.com");
        assert_eq!(cfg.port, 993);
        assert_eq!(cfg.username, "someone@QQ.com");
        assert_eq!(preset.unwrap().password_kind, "auth_code");

        let (cfg, preset) = guess(" me@example.org ").unwrap();
        assert_eq!(cfg.host, "imap.example.org");
        assert_eq!(cfg.security, Security::Tls);
        assert!(preset.is_none());

        assert!(guess("not-an-address").is_none());
        assert!(guess("@example.org").is_none());
        assert!(guess("me@localhost").is_none());
    }

    #[test]
    fn business_providers_from_mx() {
        let id = |hosts: &[&str]| {
            provider_for_mx(&hosts.iter().map(|h| h.to_string()).collect::<Vec<_>>()).map(|p| p.id)
        };
        assert_eq!(id(&["mxbiz1.qq.com.", "mxbiz2.qq.com."]), Some("exmail"));
        assert_eq!(
            id(&["mxw.mxhichina.com.", "mxn.mxhichina.com."]),
            Some("qiye_aliyun")
        );
        assert_eq!(id(&["qiye163mx01.mxmail.netease.com."]), Some("qiye163"));
        assert_eq!(id(&["aspmx.l.google.com."]), Some("workspace"));
        assert_eq!(
            id(&["example-com.mail.protection.outlook.com."]),
            Some("m365")
        );
        assert_eq!(id(&["mx1.feishu.cn."]), Some("feishu"));
        assert_eq!(id(&["mx01.mail.icloud.com."]), Some("icloud_custom"));
        assert_eq!(
            id(&["in1-smtp.messagingengine.com."]),
            Some("fastmail_custom")
        );
        assert_eq!(id(&["mail.example.org."]), None);
        // 只看后缀的边界，不能把 notqq.com 认成 QQ
        assert_eq!(id(&["mx.notqq.com."]), None);
        assert_eq!(id(&[]), None);
    }

    #[test]
    fn preset_ids_are_unique() {
        let all = presets();
        let mut ids: Vec<_> = all.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), all.len());
    }
}
