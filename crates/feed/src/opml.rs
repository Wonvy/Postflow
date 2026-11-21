//! OPML 导入导出，用来和其他阅读器互相迁移订阅列表。

use std::collections::BTreeMap;

use ::opml::{Head, OPML, Outline};
use serde::Serialize;

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpmlFeed {
    pub url: String,
    pub title: String,
    pub site_url: Option<String>,
    /// 所在文件夹，空字符串表示不在任何文件夹里。多层嵌套用「 / 」连接。
    pub folder: String,
}

pub fn parse(xml: &str) -> Result<Vec<OpmlFeed>> {
    let xml = xml.trim_start_matches('\u{feff}').trim_start();
    let doc = OPML::from_str(xml).map_err(|e| Error::Opml(e.to_string()))?;
    let mut out: Vec<OpmlFeed> = Vec::new();
    for outline in &doc.body.outlines {
        walk(outline, "", &mut out);
    }
    Ok(out)
}

fn walk(outline: &Outline, folder: &str, out: &mut Vec<OpmlFeed>) {
    let name = outline
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .unwrap_or(outline.text.trim())
        .to_string();

    let url = outline
        .xml_url
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty());
    if let Some(url) = url
        && !out.iter().any(|f| f.url == url)
    {
        out.push(OpmlFeed {
            url: url.to_string(),
            title: name.clone(),
            site_url: outline.html_url.clone().filter(|u| !u.trim().is_empty()),
            folder: folder.to_string(),
        });
    }

    if !outline.outlines.is_empty() {
        // 带 xmlUrl 的节点自己是订阅源，不当作文件夹
        let child_folder = if url.is_some() || name.is_empty() {
            folder.to_string()
        } else if folder.is_empty() {
            name
        } else {
            format!("{folder} / {name}")
        };
        for child in &outline.outlines {
            walk(child, &child_folder, out);
        }
    }
}

pub fn write(title: &str, feeds: &[OpmlFeed]) -> Result<String> {
    let as_outline = |f: &OpmlFeed| Outline {
        text: f.title.clone(),
        title: Some(f.title.clone()),
        r#type: Some("rss".to_string()),
        xml_url: Some(f.url.clone()),
        html_url: f.site_url.clone(),
        ..Outline::default()
    };

    let mut top_level = Vec::new();
    #[derive(Default)]
    struct FolderTree {
        feeds: Vec<Outline>,
        children: BTreeMap<String, FolderTree>,
    }
    fn folder_outlines(tree: FolderTree) -> Vec<Outline> {
        tree.children
            .into_iter()
            .map(|(name, child)| {
                let mut outlines = folder_outlines(FolderTree {
                    feeds: Vec::new(),
                    children: child.children,
                });
                outlines.extend(child.feeds);
                Outline {
                    text: name.clone(),
                    title: Some(name),
                    outlines,
                    ..Outline::default()
                }
            })
            .collect()
    }
    let mut folders = FolderTree::default();
    for feed in feeds {
        if feed.folder.is_empty() {
            top_level.push(as_outline(feed));
        } else {
            let mut node = &mut folders;
            for name in feed.folder.split(" / ") {
                node = node.children.entry(name.to_string()).or_default();
            }
            node.feeds.push(as_outline(feed));
        }
    }

    let mut doc = OPML {
        version: "2.0".to_string(),
        head: Some(Head {
            title: Some(title.to_string()),
            ..Head::default()
        }),
        ..OPML::default()
    };
    doc.body.outlines.extend(folder_outlines(folders));
    doc.body.outlines.extend(top_level);

    let body = doc.to_string().map_err(|e| Error::Opml(e.to_string()))?;
    if body.trim_start().starts_with("<?xml") {
        Ok(body)
    } else {
        Ok(format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{body}\n"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<opml version=\"1.0\">
  <head><title>Subscriptions</title><dateCreated>Mon, 16 Nov 2026 08:00:00 GMT</dateCreated></head>
  <body>
    <outline text=\"Tech\" title=\"Tech\">
      <outline type=\"rss\" text=\"Example\" title=\"Example Blog\" xmlUrl=\"https://example.com/feed.xml\" htmlUrl=\"https://example.com/\"/>
      <outline text=\"Rust\">
        <outline type=\"rss\" text=\"Nested &amp; deep\" xmlUrl=\"https://rust.example/atom.xml\"/>
      </outline>
    </outline>
    <outline type=\"rss\" text=\"Loose\" xmlUrl=\"https://loose.example/rss\"/>
    <outline type=\"rss\" text=\"Duplicate\" xmlUrl=\"https://loose.example/rss\"/>
    <outline text=\"Empty folder\"/>
  </body>
</opml>";

    #[test]
    fn parses_folders_and_nested_folders() {
        let feeds = parse(SAMPLE).unwrap();
        assert_eq!(feeds.len(), 3);
        assert_eq!(feeds[0].title, "Example Blog");
        assert_eq!(feeds[0].folder, "Tech");
        assert_eq!(feeds[0].site_url.as_deref(), Some("https://example.com/"));
        assert_eq!(feeds[1].title, "Nested & deep");
        assert_eq!(feeds[1].folder, "Tech / Rust");
        assert_eq!(feeds[2].folder, "");
    }

    #[test]
    fn roundtrip() {
        let feeds = parse(SAMPLE).unwrap();
        let xml = write("Postflow", &feeds).unwrap();
        assert!(xml.starts_with("<?xml"));
        let mut again = parse(&xml).unwrap();
        let mut expected = feeds.clone();
        again.sort_by(|a, b| a.url.cmp(&b.url));
        expected.sort_by(|a, b| a.url.cmp(&b.url));
        assert_eq!(again, expected);
    }

    #[test]
    fn rejects_garbage() {
        assert!(matches!(parse("not xml at all"), Err(Error::Opml(_))));
        assert!(matches!(
            parse("<html><body>hi</body></html>"),
            Err(Error::Opml(_))
        ));
    }
}
