//! 语音识别模型的目录。
//!
//! 目录是仓库里的一个文件（`public/catalog/speech.json`），编译时打进程序，运行时再联网看有没有新的。
//! 以后出了新模型，只要运行库认识它的类型（`kind`），往目录里加一条就行，不用发新版本。
//! 模型文件放在别人的公开存储里（Hugging Face、GitHub），每个文件给几个下载地址和 SHA-256，
//! 哪个地址下的都一样校验。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 随程序带的那一份。联不上网、或者网上的读不懂时用它。
pub const BUILTIN: &str = include_str!("../../../public/catalog/speech.json");

/// 目录的在线版本：先走 jsDelivr（国内更容易打开），不行再直接读 GitHub。
pub const CATALOG_URLS: &[&str] = &[
    "https://cdn.jsdelivr.net/gh/Wonvy/Postflow@main/public/catalog/speech.json",
    "https://raw.githubusercontent.com/Wonvy/Postflow/main/public/catalog/speech.json",
];

/// 这个版本的程序读得懂的目录格式。
pub const SCHEMA: u32 = 1;

/// 这个版本的运行库能加载到第几代模型。目录里 `engine` 比它大的模型要等新版本。
pub const ENGINE: u32 = 1;

/// 这个版本认识的模型类型，和 engine.rs 里的一一对应。
pub const KINDS: &[&str] = &[
    "sense_voice",
    "whisper",
    "paraformer",
    "transducer",
    "nemo_ctc",
    "moonshine",
    "dolphin",
    "zipformer_ctc",
    "wenet_ctc",
    "fire_red_asr_ctc",
    // 说话人的声纹（区分是谁在说），不是识别模型
    "speaker_embedding",
];

/// 模型是做什么的。
pub const TASK_ASR: &str = "asr";
pub const TASK_SPEAKER: &str = "speaker";

/// 要下载的一个文件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteFile {
    /// 存到本机时的文件名。
    pub name: String,
    /// 它在模型里是什么：`model`、`tokens`、`encoder`、`decoder`……见 engine.rs。
    #[serde(default)]
    pub role: String,
    pub size: u64,
    /// 文件内容的 SHA-256（十六进制）。
    pub sha256: String,
    /// 下载地址，按顺序试。
    pub urls: Vec<String>,
    /// 内容不完全一样、但用起来一样的副本（比如同一个模型换了导出工具，文件里记的版本号不同），
    /// 各有各的大小、校验值和地址。主地址都下不了时用它们，比如国内只有魔搭上的那一份下得动。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternates: Vec<Alternate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alternate {
    pub size: u64,
    pub sha256: String,
    pub urls: Vec<String>,
}

impl RemoteFile {
    /// 这个文件所有的下载地址，连同每个地址下来的文件应有的大小和校验值。
    pub fn sources(&self) -> Vec<(&str, u64, &str)> {
        let mut out: Vec<(&str, u64, &str)> = self
            .urls
            .iter()
            .map(|u| (u.as_str(), self.size, self.sha256.as_str()))
            .collect();
        for alt in &self.alternates {
            out.extend(
                alt.urls
                    .iter()
                    .map(|u| (u.as_str(), alt.size, alt.sha256.as_str())),
            );
        }
        out
    }

    /// 一个文件的校验值对得上主文件或者某个副本。
    pub fn accepts(&self, sha256: &str) -> bool {
        self.sha256.eq_ignore_ascii_case(sha256)
            || self
                .alternates
                .iter()
                .any(|a| a.sha256.eq_ignore_ascii_case(sha256))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
    /// 模型类型，决定怎么加载。
    pub kind: String,
    /// 做什么用：识别（`asr`，默认）还是区分说话人（`speaker`）。
    #[serde(default = "asr")]
    pub task: String,
    /// 能识别的语言（ISO 639 代码）。`multi` 表示很多种。
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default = "one")]
    pub engine: u32,
    /// 推荐给这些界面语言的用户先装。
    #[serde(default)]
    pub recommended: Vec<String>,
    pub files: Vec<RemoteFile>,
    /// 加载时的选项，比如 `language`、`use_itn`。
    #[serde(default)]
    pub options: BTreeMap<String, String>,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub license_url: String,
    #[serde(default)]
    pub homepage: String,
    /// 一句话介绍，按界面语言。
    #[serde(default)]
    pub desc: BTreeMap<String, String>,
}

fn one() -> u32 {
    1
}

fn asr() -> String {
    TASK_ASR.to_string()
}

impl Model {
    /// 要下载多少字节。
    pub fn size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }

    /// 这个版本的程序能不能加载它。
    pub fn supported(&self) -> bool {
        self.engine <= ENGINE && KINDS.contains(&self.kind.as_str())
    }

    pub fn file(&self, role: &str) -> Option<&RemoteFile> {
        self.files.iter().find(|f| f.role == role)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Catalog {
    pub schema: u32,
    #[serde(default)]
    pub updated: String,
    pub models: Vec<Model>,
}

/// 模型 ID 会拿来当目录名，只能是这些字符。
pub fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && !id.starts_with('.')
        && id.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'.' | b'_')
        })
}

/// 文件名只能是一个普通的名字，不能带路径。
pub fn safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_'))
}

fn check_file(file: &RemoteFile) -> Result<(), String> {
    if !safe_file_name(&file.name) {
        return Err(format!("bad file name: {}", file.name));
    }
    // 只认 https；本机地址例外（自己架的镜像，也方便测试）
    let allowed = |u: &String| {
        u.starts_with("https://")
            || u.starts_with("http://127.0.0.1:")
            || u.starts_with("http://localhost:")
    };
    let groups = std::iter::once((&file.sha256, &file.urls))
        .chain(file.alternates.iter().map(|a| (&a.sha256, &a.urls)));
    for (sha256, urls) in groups {
        if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!("{}: missing or bad sha256", file.name));
        }
        if urls.is_empty() || !urls.iter().all(allowed) {
            return Err(format!("{}: download addresses must be https", file.name));
        }
    }
    Ok(())
}

impl Catalog {
    /// 读一份目录。格式比这个版本新的、或者有不安全内容的整份不要；个别条目有问题只丢那一条。
    pub fn parse(bytes: &[u8]) -> Result<Catalog, String> {
        let mut catalog: Catalog = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if catalog.schema != SCHEMA {
            return Err(format!("unknown catalog schema {}", catalog.schema));
        }
        catalog.models.retain(|m| {
            let mut names: Vec<&str> = m.files.iter().map(|f| f.name.as_str()).collect();
            names.sort_unstable();
            let unique = names.windows(2).all(|w| w[0] != w[1]);
            let ok = safe_id(&m.id)
                && !m.files.is_empty()
                && unique
                && m.files.iter().all(|f| check_file(f).is_ok());
            if !ok {
                tracing::warn!(model = %m.id, "ignoring a bad entry in the speech model catalog");
            }
            ok
        });
        let mut seen = std::collections::HashSet::new();
        catalog.models.retain(|m| seen.insert(m.id.clone()));
        Ok(catalog)
    }

    pub fn builtin() -> Catalog {
        Catalog::parse(BUILTIN.as_bytes()).expect("the built-in speech catalog is valid")
    }

    pub fn get(&self, id: &str) -> Option<&Model> {
        self.models.iter().find(|m| m.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_is_valid_and_loadable() {
        let raw: serde_json::Value = serde_json::from_str(BUILTIN).unwrap();
        let catalog = Catalog::builtin();
        // 内置目录里不能有被丢掉的条目
        assert_eq!(
            catalog.models.len(),
            raw["models"].as_array().unwrap().len()
        );
        assert!(!catalog.models.is_empty());
        for m in &catalog.models {
            assert!(m.supported(), "{} is not loadable by this build", m.id);
            assert!(!m.name.is_empty() && !m.languages.is_empty(), "{}", m.id);
            if m.task == TASK_ASR {
                assert!(m.file("tokens").is_some(), "{} has no tokens file", m.id);
            } else {
                assert_eq!(m.task, TASK_SPEAKER, "{}", m.id);
                assert!(m.file("model").is_some(), "{} has no model file", m.id);
            }
            assert!(!m.license.is_empty(), "{} has no license", m.id);
            for lang in ["zh", "en"] {
                assert!(
                    m.desc.contains_key(lang),
                    "{} has no {lang} description",
                    m.id
                );
            }
        }
        // 中文和英文界面各有一个推荐的
        for lang in ["zh", "en"] {
            assert!(
                catalog
                    .models
                    .iter()
                    .any(|m| m.recommended.iter().any(|l| l == lang)),
                "nothing recommended for {lang}"
            );
        }
    }

    #[test]
    fn unsafe_entries_are_dropped() {
        let file = |name: &str, url: &str| {
            serde_json::json!({
                "name": name, "role": "model", "size": 1,
                "sha256": "0".repeat(64), "urls": [url]
            })
        };
        let model = |id: &str, f: serde_json::Value| serde_json::json!({ "id": id, "name": id, "kind": "sense_voice", "files": [f] });
        let doc = serde_json::json!({
            "schema": 1,
            "models": [
                model("good", file("model.onnx", "https://example.com/m")),
                model("../evil", file("model.onnx", "https://example.com/m")),
                model("path", file("../model.onnx", "https://example.com/m")),
                model("plain-http", file("model.onnx", "http://example.com/m")),
                model("good", file("model.onnx", "https://example.com/dup")),
                { "id": "future", "name": "Future", "kind": "something_new", "engine": 9,
                  "files": [file("model.onnx", "https://example.com/m")] },
            ]
        });
        let catalog = Catalog::parse(doc.to_string().as_bytes()).unwrap();
        let ids: Vec<&str> = catalog.models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["good", "future"]);
        assert!(catalog.get("good").unwrap().supported());
        // 新类型的模型留在目录里，界面上说「需要新版本」
        assert!(!catalog.get("future").unwrap().supported());

        let mut newer = doc;
        newer["schema"] = 2.into();
        assert!(Catalog::parse(newer.to_string().as_bytes()).is_err());
    }
}
