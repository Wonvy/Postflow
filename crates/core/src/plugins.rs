//! 插件：放在本机目录 `plugins/<插件 ID>/` 下，每个插件一个 `manifest.json` 和一个 JS 入口文件。
//!
//! 核心只负责列出插件、读出插件的代码交给界面；插件在界面里运行，能用的接口见 docs/plugins.md。
//! 插件目录在本机，不同步；启用哪些插件也是每台设备自己选。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::Core;
use crate::rpc::{RpcError, parse, reply};

const DIR: &str = "plugins";
const MANIFEST: &str = "manifest.json";
/// 入口文件的大小上限。
const MAX_SOURCE: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub url: Option<String>,
    /// 入口文件，默认 main.js。
    #[serde(default = "main_js")]
    pub main: String,
    /// 需要的最低插件接口版本。
    #[serde(default = "one", rename = "minApiVersion")]
    pub min_api_version: u32,
}

fn main_js() -> String {
    "main.js".into()
}

fn one() -> u32 {
    1
}

#[derive(Debug, Serialize)]
struct Listed {
    #[serde(flatten)]
    manifest: Option<Manifest>,
    /// 插件文件夹的名字。
    folder: String,
    /// 读不了的原因（manifest 坏了、ID 和文件夹名对不上……）。
    error: Option<String>,
}

#[derive(Deserialize)]
struct IdParam {
    id: String,
}

/// 插件 ID：小写字母、数字和连字符，和文件夹名一致。
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !id.starts_with('-')
}

fn read_manifest(dir: &Path) -> Result<Manifest, String> {
    let text =
        std::fs::read_to_string(dir.join(MANIFEST)).map_err(|e| format!("{MANIFEST}: {e}"))?;
    let manifest: Manifest = serde_json::from_str(&text).map_err(|e| format!("{MANIFEST}: {e}"))?;
    let folder = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    check(&manifest, folder)?;
    Ok(manifest)
}

/// manifest 本身对不对：ID 合法、和文件夹名一致、有名字、入口文件在插件文件夹里。
fn check(manifest: &Manifest, folder: &str) -> Result<(), String> {
    if !valid_id(&manifest.id) {
        return Err(format!(
            "invalid id {:?}: use lowercase letters, digits and -",
            manifest.id
        ));
    }
    if manifest.id != folder {
        return Err(format!(
            "id {:?} must match the folder name {:?}",
            manifest.id, folder
        ));
    }
    if manifest.name.trim().is_empty() {
        return Err("name is empty".into());
    }
    let main = Path::new(&manifest.main);
    if main.is_absolute()
        || main
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(format!(
            "main {:?} must be a file inside the plugin folder",
            manifest.main
        ));
    }
    Ok(())
}

#[derive(Deserialize)]
struct InstallParams {
    /// manifest.json 的原文，原样写进插件文件夹。
    manifest: String,
    /// 入口文件的内容。
    source: String,
}

/// 把一个插件（manifest 和入口文件）写进插件目录。已经有同名插件时覆盖这两个文件，用来更新示例插件。
fn install(root: &Path, p: &InstallParams) -> Result<String, RpcError> {
    let manifest: Manifest = serde_json::from_str(&p.manifest)
        .map_err(|e| RpcError::new("plugin_invalid", format!("{MANIFEST}: {e}")))?;
    check(&manifest, &manifest.id).map_err(|e| RpcError::new("plugin_invalid", e))?;
    if p.source.len() as u64 > MAX_SOURCE {
        return Err(RpcError::new(
            "plugin_invalid",
            format!("{} is larger than 4 MB", manifest.main),
        ));
    }
    let dir = root.join(&manifest.id);
    let main = dir.join(&manifest.main);
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(main.parent().unwrap_or(&dir))?;
        std::fs::write(&main, &p.source)?;
        std::fs::write(dir.join(MANIFEST), &p.manifest)
    };
    write().map_err(|e| RpcError::new("io", format!("{}: {e}", dir.display())))?;
    Ok(manifest.id)
}

fn list(root: &Path) -> Vec<Listed> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out: Vec<Listed> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| {
            let folder = e.file_name().to_string_lossy().into_owned();
            match read_manifest(&e.path()) {
                Ok(manifest) => Listed {
                    manifest: Some(manifest),
                    folder,
                    error: None,
                },
                Err(error) => Listed {
                    manifest: None,
                    folder,
                    error: Some(error),
                },
            }
        })
        .collect();
    out.sort_by(|a, b| a.folder.cmp(&b.folder));
    out
}

fn source(root: &Path, id: &str) -> Result<String, RpcError> {
    if !valid_id(id) {
        return Err(RpcError::new("bad_params", "invalid plugin id"));
    }
    let dir = root.join(id);
    let manifest = read_manifest(&dir).map_err(|e| RpcError::new("plugin_invalid", e))?;
    let path = dir.join(&manifest.main);
    let size = std::fs::metadata(&path)
        .map_err(|e| RpcError::new("plugin_invalid", format!("{}: {e}", manifest.main)))?
        .len();
    if size > MAX_SOURCE {
        return Err(RpcError::new(
            "plugin_invalid",
            format!("{} is larger than 4 MB", manifest.main),
        ));
    }
    std::fs::read_to_string(&path)
        .map_err(|e| RpcError::new("plugin_invalid", format!("{}: {e}", manifest.main)))
}

impl Core {
    fn plugins_dir(&self) -> PathBuf {
        self.local_dir.join(DIR)
    }

    pub(crate) async fn plugins_rpc(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let root = self.plugins_dir();
        match method {
            "plugins.list" => {
                let _ = std::fs::create_dir_all(&root);
                let dir = std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone());
                let plugins = tokio::task::spawn_blocking(move || list(&root))
                    .await
                    .map_err(|e| RpcError::new("internal", e.to_string()))?;
                reply(json!({ "dir": dir, "plugins": plugins }))
            }
            "plugins.source" => {
                let p: IdParam = parse(params)?;
                let code = tokio::task::spawn_blocking(move || source(&root, &p.id))
                    .await
                    .map_err(|e| RpcError::new("internal", e.to_string()))??;
                reply(json!({ "source": code }))
            }
            "plugins.install" => {
                let p: InstallParams = parse(params)?;
                let id = tokio::task::spawn_blocking(move || install(&root, &p))
                    .await
                    .map_err(|e| RpcError::new("internal", e.to_string()))??;
                reply(json!({ "id": id }))
            }
            _ => Err(RpcError::new("unknown_method", method.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, folder: &str, manifest: &str, main: Option<&str>) {
        let d = dir.join(folder);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(MANIFEST), manifest).unwrap();
        if let Some(code) = main {
            std::fs::write(d.join("main.js"), code).unwrap();
        }
    }

    #[test]
    fn lists_good_plugins_and_explains_broken_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            root,
            "hello",
            r#"{"id":"hello","name":"Hello","version":"1.0.0"}"#,
            Some("export default {}"),
        );
        write(root, "wrong-folder", r#"{"id":"other","name":"X"}"#, None);
        write(root, "broken", "{nope", None);
        write(
            root,
            "escape",
            r#"{"id":"escape","name":"E","main":"../../secret.js"}"#,
            None,
        );
        let listed = list(root);
        assert_eq!(listed.len(), 4);
        let ok: Vec<_> = listed.iter().filter(|l| l.error.is_none()).collect();
        assert_eq!(ok.len(), 1);
        assert_eq!(ok[0].manifest.as_ref().unwrap().main, "main.js");
        assert!(listed.iter().any(|l| {
            l.error
                .as_deref()
                .is_some_and(|e| e.contains("folder name"))
        }));
        assert!(listed.iter().any(|l| {
            l.error
                .as_deref()
                .is_some_and(|e| e.contains("inside the plugin folder"))
        }));

        assert_eq!(source(root, "hello").unwrap(), "export default {}");
        assert!(source(root, "../hello").is_err());
        assert!(source(root, "escape").is_err());
    }

    #[test]
    fn installs_and_updates_a_plugin() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let params = |manifest: &str, source: &str| InstallParams {
            manifest: manifest.into(),
            source: source.into(),
        };
        let id = install(
            root,
            &params(r#"{"id":"hello","name":"Hello"}"#, "export default {}"),
        )
        .unwrap();
        assert_eq!(id, "hello");
        assert_eq!(source(root, "hello").unwrap(), "export default {}");
        // 再装一次就是更新
        install(
            root,
            &params(
                r#"{"id":"hello","name":"Hello"}"#,
                "export default { a: 1 }",
            ),
        )
        .unwrap();
        assert_eq!(source(root, "hello").unwrap(), "export default { a: 1 }");

        assert!(install(root, &params(r#"{"id":"../x","name":"X"}"#, "")).is_err());
        assert!(
            install(
                root,
                &params(r#"{"id":"x","name":"X","main":"../x.js"}"#, "")
            )
            .is_err()
        );
        assert!(install(root, &params("{nope", "")).is_err());
        assert!(!root.join("x").exists());
    }

    #[test]
    fn plugin_ids() {
        assert!(valid_id("word-count-2"));
        assert!(!valid_id("Hello"));
        assert!(!valid_id("a/b"));
        assert!(!valid_id("-x"));
        assert!(!valid_id(""));
    }
}
