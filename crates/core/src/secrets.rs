//! 邮箱密码的存放处。密码只存在这台设备的系统钥匙串里，永远不写进数据目录。

use std::collections::HashMap;
use std::sync::Mutex;

pub trait Secrets: Send + Sync {
    fn get(&self, account_id: &str) -> Option<String>;
    fn set(&self, account_id: &str, secret: &str) -> Result<(), String>;
    fn delete(&self, account_id: &str);
}

/// 系统钥匙串：macOS 的钥匙串、Windows 凭据管理器、Linux 的 Secret Service。
pub struct Keychain;

const SERVICE: &str = "app.postflow.mail";

impl Secrets for Keychain {
    fn get(&self, account_id: &str) -> Option<String> {
        keyring::Entry::new(SERVICE, account_id)
            .ok()?
            .get_password()
            .ok()
    }

    fn set(&self, account_id: &str, secret: &str) -> Result<(), String> {
        keyring::Entry::new(SERVICE, account_id)
            .and_then(|e| e.set_password(secret))
            .map_err(|e| e.to_string())
    }

    fn delete(&self, account_id: &str) {
        if let Ok(entry) = keyring::Entry::new(SERVICE, account_id) {
            let _ = entry.delete_credential();
        }
    }
}

/// 只在内存里，测试用。
#[derive(Default)]
pub struct MemorySecrets(Mutex<HashMap<String, String>>);

impl Secrets for MemorySecrets {
    fn get(&self, account_id: &str) -> Option<String> {
        self.0.lock().ok()?.get(account_id).cloned()
    }

    fn set(&self, account_id: &str, secret: &str) -> Result<(), String> {
        self.0
            .lock()
            .map_err(|e| e.to_string())?
            .insert(account_id.to_string(), secret.to_string());
        Ok(())
    }

    fn delete(&self, account_id: &str) {
        if let Ok(mut map) = self.0.lock() {
            map.remove(account_id);
        }
    }
}
