//! Mozilla 公开的翻译模型目录。
//!
//! 模型来自 Firefox 的离线翻译项目（MPL-2.0），目录和文件都放在 Mozilla 的公开存储里。
//! 每种语言都只和英语互译，其他语言之间经英语中转。

use std::collections::BTreeMap;

use serde::Deserialize;

pub const CATALOG_URL: &str = "https://storage.googleapis.com/moz-fx-translations-data--303e-prod-translations-data/db/models.json";

/// 一个模型由这几种文件组成。中日韩这类词表不共用的语言对有单独的源词表和目标词表。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Model,
    Vocab,
    SrcVocab,
    TrgVocab,
    Lex,
}

impl FileKind {
    /// 存到本机时用的文件名。
    pub fn file_name(self) -> &'static str {
        match self {
            FileKind::Model => "model.bin",
            FileKind::Vocab => "vocab.spm",
            FileKind::SrcVocab => "srcvocab.spm",
            FileKind::TrgVocab => "trgvocab.spm",
            FileKind::Lex => "lex.bin",
        }
    }

    fn from_key(key: &str) -> Option<FileKind> {
        Some(match key {
            "model" => FileKind::Model,
            "vocab" => FileKind::Vocab,
            "srcVocab" => FileKind::SrcVocab,
            "trgVocab" => FileKind::TrgVocab,
            "lexicalShortlist" => FileKind::Lex,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RemoteFile {
    pub kind: FileKind,
    pub url: String,
    /// 解压后内容的 SHA-256。目录里只有模型文件给了。
    pub sha256: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RemoteModel {
    pub from: String,
    pub to: String,
    /// 模型文件解压后的大小，占了整个模型的绝大部分，界面上用它提示要下载多少。
    pub size: u64,
    pub files: Vec<RemoteFile>,
}

impl RemoteModel {
    pub fn pair(&self) -> String {
        pair_name(&self.from, &self.to)
    }
}

#[derive(Debug, Default)]
pub struct Catalog {
    models: BTreeMap<(String, String), RemoteModel>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Registry {
    base_url: String,
    models: BTreeMap<String, Vec<Entry>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    #[serde(default)]
    release_status: Option<String>,
    source_language: String,
    target_language: String,
    files: BTreeMap<String, EntryFile>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EntryFile {
    path: String,
    #[serde(default)]
    uncompressed_size: Option<u64>,
    #[serde(default)]
    uncompressed_hash: Option<String>,
}

/// 只用正式发布的模型。同一语言对有几个版本时，优先桌面版。
fn rank(status: Option<&str>) -> Option<u8> {
    match status? {
        "Release Desktop" => Some(0),
        "Release" => Some(1),
        "Release Android" => Some(2),
        _ => None,
    }
}

impl Catalog {
    pub fn parse(json: &[u8]) -> Result<Catalog, String> {
        let registry: Registry = serde_json::from_slice(json).map_err(|e| e.to_string())?;
        let base = registry.base_url.trim_end_matches('/');
        // 本机地址只在测试里出现
        let loopback =
            base.starts_with("http://127.0.0.1:") || base.starts_with("http://localhost:");
        if !base.starts_with("https://") && !loopback {
            return Err(format!("catalog base URL is not https: {base}"));
        }
        let mut best: BTreeMap<(String, String), (u8, RemoteModel)> = BTreeMap::new();
        for entry in registry.models.into_values().flatten() {
            let Some(rank) = rank(entry.release_status.as_deref()) else {
                continue;
            };
            let (from, to) = (entry.source_language, entry.target_language);
            if !valid_code(&from) || !valid_code(&to) {
                continue;
            }
            let mut files = Vec::new();
            let mut size = 0;
            for (key, file) in entry.files {
                let Some(kind) = FileKind::from_key(&key) else {
                    continue;
                };
                if file.path.split('/').any(|p| p == ".." || p.is_empty()) {
                    continue;
                }
                if kind == FileKind::Model {
                    size = file.uncompressed_size.unwrap_or(0);
                }
                files.push(RemoteFile {
                    kind,
                    url: format!("{base}/{}", file.path),
                    sha256: file.uncompressed_hash.map(|h| h.to_ascii_lowercase()),
                });
            }
            let has = |k: FileKind| files.iter().any(|f| f.kind == k);
            let vocab =
                has(FileKind::Vocab) || (has(FileKind::SrcVocab) && has(FileKind::TrgVocab));
            if !has(FileKind::Model) || !has(FileKind::Lex) || !vocab {
                continue;
            }
            let key = (from.clone(), to.clone());
            if best.get(&key).is_some_and(|(r, _)| *r <= rank) {
                continue;
            }
            best.insert(
                key,
                (
                    rank,
                    RemoteModel {
                        from,
                        to,
                        size,
                        files,
                    },
                ),
            );
        }
        Ok(Catalog {
            models: best.into_iter().map(|(k, (_, m))| (k, m)).collect(),
        })
    }

    pub fn get(&self, from: &str, to: &str) -> Option<&RemoteModel> {
        self.models.get(&(from.to_string(), to.to_string()))
    }

    pub fn models(&self) -> impl Iterator<Item = &RemoteModel> {
        self.models.values()
    }
}

/// 语言代码只允许小写字母和下划线（例如 `zh_hant`），它们会拿来拼目录名。
pub fn valid_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 12
        && code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
}

pub fn pair_name(from: &str, to: &str) -> String {
    format!("{from}-{to}")
}

/// 拆开 `zh-en` 这样的语言对名称，并检查两边都是合法的语言代码。
pub fn split_pair(pair: &str) -> Option<(&str, &str)> {
    let (from, to) = pair.split_once('-')?;
    (valid_code(from) && valid_code(to)).then_some((from, to))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "generated": "2026-11-16",
      "baseUrl": "https://example.org/bucket/",
      "models": {
        "zh-en": [
          { "architecture": "tiny", "releaseStatus": null, "sourceLanguage": "zh", "targetLanguage": "en",
            "files": { "model": { "path": "a/model.gz", "uncompressedSize": 1 }, "vocab": { "path": "a/vocab.gz" },
                       "lexicalShortlist": { "path": "a/lex.gz" } } },
          { "architecture": "base-memory", "releaseStatus": "Release Android", "sourceLanguage": "zh", "targetLanguage": "en",
            "files": { "model": { "path": "b/model.gz", "uncompressedSize": 2 }, "vocab": { "path": "b/vocab.gz" },
                       "lexicalShortlist": { "path": "b/lex.gz" } } },
          { "architecture": "base", "releaseStatus": "Release Desktop", "sourceLanguage": "zh", "targetLanguage": "en",
            "files": { "model": { "path": "c/model.gz", "uncompressedSize": 3, "uncompressedHash": "ABC" },
                       "vocab": { "path": "c/vocab.gz" }, "lexicalShortlist": { "path": "c/lex.gz" } } }
        ],
        "en-zh": [
          { "releaseStatus": "Release", "sourceLanguage": "en", "targetLanguage": "zh",
            "files": { "model": { "path": "d/model.gz", "uncompressedSize": 4 }, "srcVocab": { "path": "d/src.gz" },
                       "trgVocab": { "path": "d/trg.gz" }, "lexicalShortlist": { "path": "d/lex.gz" } } }
        ],
        "en-xx": [
          { "releaseStatus": "Release", "sourceLanguage": "en", "targetLanguage": "xx",
            "files": { "model": { "path": "e/model.gz" }, "lexicalShortlist": { "path": "e/lex.gz" } } }
        ],
        "en-yy": [
          { "releaseStatus": "Release", "sourceLanguage": "en", "targetLanguage": "../yy",
            "files": { "model": { "path": "f/model.gz" }, "vocab": { "path": "f/v.gz" }, "lexicalShortlist": { "path": "f/lex.gz" } } }
        ]
      }
    }"#;

    #[test]
    fn picks_the_desktop_release() {
        let catalog = Catalog::parse(SAMPLE.as_bytes()).unwrap();
        let zh = catalog.get("zh", "en").unwrap();
        assert_eq!(zh.size, 3);
        let model = zh.files.iter().find(|f| f.kind == FileKind::Model).unwrap();
        assert_eq!(model.url, "https://example.org/bucket/c/model.gz");
        assert_eq!(model.sha256.as_deref(), Some("abc"));
    }

    #[test]
    fn keeps_split_vocab_and_skips_broken_entries() {
        let catalog = Catalog::parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(catalog.get("en", "zh").unwrap().files.len(), 4);
        // 缺词表的、语言代码不合法的都不要
        assert!(catalog.get("en", "xx").is_none());
        assert_eq!(catalog.models().count(), 2);
    }

    #[test]
    fn pair_names() {
        assert_eq!(split_pair("zh_hant-en"), Some(("zh_hant", "en")));
        assert_eq!(split_pair("../x-en"), None);
        assert_eq!(split_pair("en"), None);
    }
}
