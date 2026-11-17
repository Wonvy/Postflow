//! 「查看源代码」：订阅源里某一篇文章原本的 XML。存档里只存解析后的内容，所以重新抓一次源，
//! 再按 guid 或链接找出那一篇的 `<item>`（RSS）或 `<entry>`（Atom）。

use crate::client::Client;
use crate::{Error, Result};

/// 整个源太大时只给这么多。
const MAX_FEED: usize = 512 * 1024;

impl Client {
    /// 抓取地址的原文（不解析）。
    pub async fn fetch_text(&self, url: &str) -> Result<String> {
        let page = self.get(url, None, None).await?.ok_or(Error::Status(304))?;
        Ok(String::from_utf8_lossy(&page.body).into_owned())
    }
}

/// `xml` 里所有 `<tag …>…</tag>`，按出现顺序。
fn blocks<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = xml[from..].find(&open) {
        let start = from + i;
        let after = xml[start + open.len()..].chars().next();
        // `<item>`、`<item attr…>`，不是 `<items>`
        if !matches!(after, Some(c) if c == '>' || c == '/' || c.is_whitespace()) {
            from = start + open.len();
            continue;
        }
        let Some(j) = xml[start..].find(&close) else {
            break;
        };
        let end = start + j + close.len();
        out.push(&xml[start..end]);
        from = end;
    }
    out
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// 在源的原文里找出 guid 是 `guid`、或者链接是 `link` 的那一篇。找不到返回 `None`。
pub fn item_xml(xml: &str, guid: &str, link: Option<&str>) -> Option<String> {
    let needles: Vec<String> = [Some(guid), link]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .flat_map(|s| [s.to_string(), escape(s)])
        .collect();
    if needles.is_empty() {
        return None;
    }
    let mut all = blocks(xml, "item");
    all.extend(blocks(xml, "entry"));
    // guid 优先：同一个链接可能出现在别的条目的正文里
    for needle in &needles {
        if let Some(block) = all.iter().find(|b| b.contains(needle.as_str())) {
            return Some((*block).to_string());
        }
    }
    None
}

/// 找不到那一篇时给整个源，太长就截断。
pub fn whole_feed(xml: String) -> String {
    if xml.len() <= MAX_FEED {
        return xml;
    }
    let mut cut = MAX_FEED;
    while !xml.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}\n…", &xml[..cut])
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSS: &str = r#"<rss><channel><title>t</title>
<item><title>One</title><link>https://a.test/1?x=1&amp;y=2</link><guid>g-1</guid></item>
<item><title>Two</title><link>https://a.test/2</link><guid isPermaLink="false">g-2</guid></item>
</channel></rss>"#;

    #[test]
    fn finds_rss_items_by_guid_or_link() {
        let two = item_xml(RSS, "g-2", None).unwrap();
        assert!(two.starts_with("<item>") && two.contains("Two") && two.ends_with("</item>"));
        let one = item_xml(RSS, "", Some("https://a.test/1?x=1&y=2")).unwrap();
        assert!(one.contains("One"), "{one}");
        assert_eq!(item_xml(RSS, "nope", Some("https://a.test/9")), None);
        assert_eq!(item_xml(RSS, "", None), None);
    }

    #[test]
    fn finds_atom_entries() {
        let atom = r#"<feed><entry xml:lang="en"><id>tag:x,1</id><title>A</title></entry><entry><id>tag:x,2</id></entry></feed>"#;
        assert_eq!(
            item_xml(atom, "tag:x,2", None).as_deref(),
            Some("<entry><id>tag:x,2</id></entry>")
        );
        assert!(
            item_xml(atom, "tag:x,1", None)
                .unwrap()
                .contains("<title>A</title>")
        );
    }

    #[test]
    fn ignores_lookalike_tags() {
        let xml = "<items><item><guid>g</guid></item></items>";
        assert_eq!(
            item_xml(xml, "g", None).as_deref(),
            Some("<item><guid>g</guid></item>")
        );
    }
}
