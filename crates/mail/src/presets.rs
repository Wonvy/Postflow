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
    /// 发信服务器。
    pub smtp_host: &'static str,
    pub smtp_port: u16,
    pub smtp_security: Security,
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
        smtp_host: "",
        smtp_port: 465,
        smtp_security: Security::Tls,
    }
}

impl Preset {
    /// 发信服务器，默认 465 端口直接走 TLS。
    const fn smtp(self, host: &'static str) -> Preset {
        Preset {
            smtp_host: host,
            ..self
        }
    }

    /// 发信服务器要先明文连上再 STARTTLS 的（587 端口）。
    const fn smtp_starttls(self, host: &'static str) -> Preset {
        Preset {
            smtp_host: host,
            smtp_port: 587,
            smtp_security: Security::Starttls,
            ..self
        }
    }
}

const PRESETS: &[(&[&str], Preset)] = &[
    (
        &["gmail.com", "googlemail.com"],
        p("gmail", "Gmail", "imap.gmail.com", "app_password").smtp("smtp.gmail.com"),
    ),
    (
        &[
            "outlook.com",
            "hotmail.com",
            "live.com",
            "msn.com",
            "outlook.cn",
        ],
        p("outlook", "Outlook", "outlook.office365.com", "oauth")
            .smtp_starttls("smtp-mail.outlook.com"),
    ),
    (
        &["icloud.com", "me.com", "mac.com"],
        p("icloud", "iCloud", "imap.mail.me.com", "app_password").smtp_starttls("smtp.mail.me.com"),
    ),
    (
        &["yahoo.com", "ymail.com", "rocketmail.com", "yahoo.co.jp"],
        p("yahoo", "Yahoo", "imap.mail.yahoo.com", "app_password").smtp("smtp.mail.yahoo.com"),
    ),
    (
        &["qq.com", "vip.qq.com", "foxmail.com"],
        p("qq", "QQ 邮箱", "imap.qq.com", "auth_code").smtp("smtp.qq.com"),
    ),
    (
        &["163.com", "vip.163.com"],
        p("163", "网易 163", "imap.163.com", "auth_code").smtp("smtp.163.com"),
    ),
    (
        &["126.com", "vip.126.com"],
        p("126", "网易 126", "imap.126.com", "auth_code").smtp("smtp.126.com"),
    ),
    (
        &["yeah.net"],
        p("yeah", "网易 yeah.net", "imap.yeah.net", "auth_code").smtp("smtp.yeah.net"),
    ),
    (
        &["188.com"],
        p("188", "网易 188", "imap.188.com", "auth_code").smtp("smtp.188.com"),
    ),
    (
        &["sina.com", "sina.cn", "vip.sina.com", "vip.sina.cn"],
        p("sina", "新浪邮箱", "imap.sina.com", "auth_code").smtp("smtp.sina.com"),
    ),
    (
        &["sohu.com"],
        p("sohu", "搜狐邮箱", "imap.sohu.com", "auth_code").smtp("smtp.sohu.com"),
    ),
    (
        &["139.com"],
        p("139", "139 邮箱", "imap.139.com", "auth_code").smtp("smtp.139.com"),
    ),
    (
        &["aliyun.com"],
        p("aliyun", "阿里邮箱", "imap.aliyun.com", "password").smtp("smtp.aliyun.com"),
    ),
    (
        &["fastmail.com", "fastmail.fm"],
        p("fastmail", "Fastmail", "imap.fastmail.com", "app_password").smtp("smtp.fastmail.com"),
    ),
    (
        &["zoho.com", "zohomail.com"],
        p("zoho", "Zoho Mail", "imap.zoho.com", "app_password").smtp("smtp.zoho.com"),
    ),
    (
        &["yandex.com", "yandex.ru", "ya.ru"],
        p("yandex", "Yandex", "imap.yandex.com", "app_password").smtp("smtp.yandex.com"),
    ),
    (
        &["gmx.com", "gmx.net", "gmx.de"],
        p("gmx", "GMX", "imap.gmx.com", "password").smtp("mail.gmx.com"),
    ),
    (
        &["mail.ru"],
        p("mailru", "Mail.ru", "imap.mail.ru", "app_password").smtp("smtp.mail.ru"),
    ),
    (
        &["aol.com"],
        p("aol", "AOL", "imap.aol.com", "app_password").smtp("smtp.aol.com"),
    ),
];

/// 企业邮箱用自己的域名，没法从地址推断，只能让用户手动选。
const BUSINESS: &[Preset] = &[
    p("exmail", "腾讯企业邮箱", "imap.exmail.qq.com", "auth_code").smtp("smtp.exmail.qq.com"),
    p("qiye163", "网易企业邮箱", "imap.qiye.163.com", "auth_code").smtp("smtp.qiye.163.com"),
    p(
        "qiye_aliyun",
        "阿里企业邮箱",
        "imap.qiye.aliyun.com",
        "password",
    )
    .smtp("smtp.qiye.aliyun.com"),
    p(
        "workspace",
        "Google Workspace",
        "imap.gmail.com",
        "app_password",
    )
    .smtp("smtp.gmail.com"),
    p("m365", "Microsoft 365", "outlook.office365.com", "oauth")
        .smtp_starttls("smtp.office365.com"),
    p("feishu", "飞书邮箱", "imap.feishu.cn", "app_password").smtp("smtp.feishu.cn"),
    p("lark", "Lark Mail", "imap.larksuite.com", "app_password").smtp("smtp.larksuite.com"),
    p("263", "263 企业邮箱", "imap.263.net", "password").smtp("smtp.263.net"),
    p(
        "icloud_custom",
        "iCloud 自定域名",
        "imap.mail.me.com",
        "app_password",
    )
    .smtp_starttls("smtp.mail.me.com"),
    p(
        "fastmail_custom",
        "Fastmail 自定域名",
        "imap.fastmail.com",
        "app_password",
    )
    .smtp("smtp.fastmail.com"),
    p(
        "zoho_custom",
        "Zoho Mail 自定域名",
        "imap.zoho.com",
        "app_password",
    )
    .smtp("smtp.zoho.com"),
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
        .map(|(_, p)| {
            let mut p = p.clone();
            // 新浪的不同邮箱后缀有各自的收发服务器，界面仍只显示一个新浪入口。
            if p.id == "sina" {
                (p.imap_host, p.smtp_host) = match domain.as_str() {
                    "sina.cn" => ("imap.sina.cn", "smtp.sina.cn"),
                    "vip.sina.com" => ("imap.vip.sina.com", "smtp.vip.sina.com"),
                    "vip.sina.cn" => ("imap.vip.sina.cn", "smtp.vip.sina.cn"),
                    _ => ("imap.sina.com", "smtp.sina.com"),
                };
            }
            p
        });
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

/// 一个账户的发信服务器。服务商预设里有就用预设的；没有就照收信服务器猜：
/// `imap.example.com` 换成 `smtp.example.com`，用 465 端口和 TLS；本机的桥接程序猜 1025 端口。
pub fn smtp_for(email: &str, imap: &ServerConfig) -> ServerConfig {
    let imap_host = imap.host.trim().to_ascii_lowercase();
    let same_host = |p: &Preset| p.imap_host == imap_host && !p.smtp_host.is_empty();
    // 先看邮箱域名认得的服务商，再看收信服务器（企业邮箱优先，自定义域名多是企业邮箱）
    let preset = guess(email)
        .and_then(|(_, p)| p)
        .filter(same_host)
        .or_else(|| BUSINESS.iter().find(|p| same_host(p)).cloned())
        .or_else(|| {
            PRESETS
                .iter()
                .map(|(_, p)| p)
                .find(|p| same_host(p))
                .cloned()
        });
    let username = imap.username.clone();
    if let Some(p) = preset {
        return ServerConfig {
            host: p.smtp_host.to_string(),
            port: p.smtp_port,
            security: p.smtp_security,
            username,
        };
    }
    if crate::imap::is_loopback(&imap_host) {
        return ServerConfig {
            host: imap.host.trim().to_string(),
            port: 1025,
            security: imap.security,
            username,
        };
    }
    let host = match imap_host.strip_prefix("imap.") {
        Some(rest) => format!("smtp.{rest}"),
        None => imap_host,
    };
    ServerConfig {
        host,
        port: 465,
        security: Security::Tls,
        username,
    }
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
    fn sina_domains_use_their_own_servers() {
        for (domain, host, smtp) in [
            ("sina.com", "imap.sina.com", "smtp.sina.com"),
            ("sina.cn", "imap.sina.cn", "smtp.sina.cn"),
            ("vip.sina.com", "imap.vip.sina.com", "smtp.vip.sina.com"),
            ("vip.sina.cn", "imap.vip.sina.cn", "smtp.vip.sina.cn"),
        ] {
            let email = format!("user@{domain}");
            let (config, preset) = guess(&email).unwrap();
            assert_eq!(config.host, host);
            assert_eq!(config.username, email);
            assert_eq!(config.port, 993);
            assert_eq!(config.security, Security::Tls);
            assert_eq!(preset.unwrap().password_kind, "auth_code");
            let outgoing = smtp_for(&email, &config);
            assert_eq!(
                (outgoing.host.as_str(), outgoing.port, outgoing.security),
                (smtp, 465, Security::Tls)
            );
        }
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
    fn every_provider_has_a_send_server() {
        for p in presets() {
            assert!(!p.smtp_host.is_empty(), "{} has no SMTP server", p.id);
            assert!(matches!(
                (p.smtp_port, p.smtp_security),
                (465, Security::Tls) | (587, Security::Starttls)
            ));
        }
    }

    #[test]
    fn send_server_follows_the_account() {
        let imap = |host: &str, security| ServerConfig {
            host: host.into(),
            port: 993,
            security,
            username: "me".into(),
        };
        let smtp = |email: &str, host: &str| {
            let c = smtp_for(email, &imap(host, Security::Tls));
            (c.host, c.port, c.security)
        };
        assert_eq!(
            smtp("a@qq.com", "imap.qq.com"),
            ("smtp.qq.com".into(), 465, Security::Tls)
        );
        assert_eq!(
            smtp("a@hotmail.com", "outlook.office365.com"),
            ("smtp-mail.outlook.com".into(), 587, Security::Starttls)
        );
        // 自定义域名用 Microsoft 365
        assert_eq!(
            smtp("a@example.org", "outlook.office365.com"),
            ("smtp.office365.com".into(), 587, Security::Starttls)
        );
        assert_eq!(
            smtp("a@example.org", "imap.gmail.com"),
            ("smtp.gmail.com".into(), 465, Security::Tls)
        );
        assert_eq!(
            smtp("a@example.org", "IMAP.Example.org"),
            ("smtp.example.org".into(), 465, Security::Tls)
        );
        assert_eq!(
            smtp("a@example.org", "mail.example.org"),
            ("mail.example.org".into(), 465, Security::Tls)
        );
        let local = smtp_for("a@example.org", &imap("127.0.0.1", Security::None));
        assert_eq!(
            (local.host.as_str(), local.port, local.security),
            ("127.0.0.1", 1025, Security::None)
        );
        assert_eq!(local.username, "me");
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
