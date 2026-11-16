//! 由内容算出的稳定 ID。
//!
//! 两台设备各自抓到同一个订阅源、同一篇文章时必须得到相同的 ID，
//! 否则一台设备上的「已读」对不上另一台设备上的条目。

use sha2::{Digest, Sha256};

fn short_hash(parts: &[&str], hex_len: usize) -> String {
    let mut hasher = Sha256::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            hasher.update([0u8]);
        }
        hasher.update(part.as_bytes());
    }
    let digest = hasher.finalize();
    let mut out = String::with_capacity(hex_len);
    for byte in digest.iter() {
        if out.len() >= hex_len {
            break;
        }
        out.push_str(&format!("{byte:02x}"));
    }
    out.truncate(hex_len);
    out
}

/// 订阅源 ID：订阅地址的哈希。调用方应先把地址规范化。
pub fn feed_id(url: &str) -> String {
    short_hash(&[url.trim()], 16)
}

/// 条目 ID：订阅源 ID 与条目 guid 的哈希。
pub fn item_id(feed_id: &str, guid: &str) -> String {
    short_hash(&[feed_id, guid.trim()], 24)
}

/// 邮箱账户 ID：邮箱地址的哈希。两台设备各自添加同一个邮箱，会得到同一个账户。
pub fn account_id(email: &str) -> String {
    short_hash(&["account", &email.trim().to_lowercase()], 16)
}

/// 邮件条目 ID：账户与 Message-ID 的哈希。
pub fn mail_id(account_id: &str, message_id: &str) -> String {
    short_hash(&["mail", account_id, message_id.trim()], 24)
}

/// 随机生成的设备 ID，只在首次配置时生成一次。
pub fn new_device_id() -> String {
    format!("{:012x}", rand::random::<u64>() & 0xffff_ffff_ffff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_stable_and_distinct() {
        let a = feed_id("https://example.com/feed.xml");
        assert_eq!(a, feed_id("  https://example.com/feed.xml "));
        assert_eq!(a.len(), 16);
        assert_ne!(a, feed_id("https://example.com/atom.xml"));

        let i = item_id(&a, "post-1");
        assert_eq!(i.len(), 24);
        assert_eq!(i, item_id(&a, "post-1"));
        assert_ne!(i, item_id(&a, "post-2"));
        // 分隔符保证 ("ab","c") 与 ("a","bc") 不会撞
        assert_ne!(item_id("ab", "c"), item_id("a", "bc"));
    }

    #[test]
    fn device_id_shape() {
        let id = new_device_id();
        assert_eq!(id.len(), 12);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
