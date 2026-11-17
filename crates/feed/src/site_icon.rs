//! 网站图标：发件人头像用发件人所在网站的图标。
//!
//! 先看首页上声明的图标（apple-touch-icon 这类大图标优先），没有就看根目录下的 favicon.ico。
//! 裸域名连不上时再试一次 `www.` 开头的。

use std::time::Duration;

use crate::Client;
use crate::preview::page_meta;

const PAGE_TIMEOUT: Duration = Duration::from_secs(8);
const ICON_TIMEOUT: Duration = Duration::from_secs(6);

/// 只接受普通的域名，防止拿这个接口去访问任意地址。
pub fn valid_domain(domain: &str) -> bool {
    domain.len() <= 253
        && domain.contains('.')
        && !domain.starts_with(['.', '-'])
        && !domain.ends_with(['.', '-'])
        && !domain.contains("..")
        && domain
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
        // 不是 IP 地址
        && !domain.bytes().all(|b| b.is_ascii_digit() || b == b'.')
}

impl Client {
    /// 一个网站的图标地址。找不到返回 `None`；网络出错也当作找不到。
    pub async fn site_icon(&self, domain: &str) -> Option<String> {
        if !valid_domain(domain) {
            return None;
        }
        for host in [domain.to_string(), format!("www.{domain}")] {
            if host.starts_with("www.www.") {
                continue;
            }
            match self.icon_of(&host).await {
                Ok(icon) => return icon,
                Err(()) => continue,
            }
        }
        None
    }

    /// `Err` 表示这个主机连不上，可以换一个试。
    async fn icon_of(&self, host: &str) -> Result<Option<String>, ()> {
        let home = format!("https://{host}/");
        let page = self.get(&home, None, Some(PAGE_TIMEOUT)).await;
        let reachable = page.is_ok();
        if let Ok(Some(page)) = page {
            let is_html = page
                .content_type
                .as_deref()
                .is_none_or(|t| t.contains("html"));
            if is_html
                && let Some(icon) =
                    page_meta(&String::from_utf8_lossy(&page.body), &page.final_url).icon
            {
                return Ok(Some(icon));
            }
        }
        let ico = format!("https://{host}/favicon.ico");
        match self.get(&ico, None, Some(ICON_TIMEOUT)).await {
            Ok(Some(page))
                if !page.body.is_empty()
                    && page
                        .content_type
                        .as_deref()
                        .is_none_or(|t| t.starts_with("image/") || t.contains("octet-stream")) =>
            {
                Ok(Some(page.final_url.to_string()))
            }
            _ if reachable => Ok(None),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::valid_domain;

    #[test]
    fn only_plain_domains() {
        assert!(valid_domain("github.com"));
        assert!(valid_domain("mail.example.co.uk"));
        assert!(!valid_domain("localhost"));
        assert!(!valid_domain("127.0.0.1"));
        assert!(!valid_domain("Example.com"));
        assert!(!valid_domain("a..b.com"));
        assert!(!valid_domain("evil.com/path"));
        assert!(!valid_domain("evil.com:8080"));
        assert!(!valid_domain(".example.com"));
    }
}
