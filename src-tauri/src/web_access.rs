//! 在浏览器中使用：桌面应用在本机端口上提供同一套界面，用的就是这个窗口里的核心和数据。
//!
//! 设置只存在这台设备上（本机目录的 web-access.json），不同步：端口、是否允许局域网访问、访问密钥。
//! 一定要访问密钥：链接和二维码里带一次，浏览器之后用 Cookie 记住（见 shibapost-web）。
//! 换密钥后旧链接和已经打开的页面都失效。关掉应用，网页版也就停了。

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use shibapost_core::Core;
use shibapost_web::{Access, Ui, UiFile};
use tauri::{AppHandle, Manager, Runtime};
use tokio::sync::watch;

const FILE: &str = "web-access.json";
const DEFAULT_PORT: u16 = 4319;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Config {
    enabled: bool,
    port: u16,
    lan: bool,
    key: String,
    /// 用户选的网卡（名字）。有好几块网卡、自动挑的不对时用；那块网卡不在了就回到自动挑的。
    #[serde(default)]
    lan_interface: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            port: DEFAULT_PORT,
            lan: false,
            key: shibapost_web::new_key(),
            lan_interface: None,
        }
    }
}

/// 界面看到的状态。
#[derive(Serialize)]
pub struct Status {
    enabled: bool,
    running: bool,
    port: u16,
    lan: bool,
    /// 本机打开用的地址，带访问密钥。
    local_url: Option<String>,
    /// 局域网里打开用的地址，带访问密钥。找不到本机的局域网地址时为空。
    lan_url: Option<String>,
    /// 这台电脑的各块网卡和地址，最可能是局域网的排在前面；`lan_url` 用的是 `lan_interface` 那块。
    lan_addresses: Vec<LanAddr>,
    lan_interface: Option<String>,
    /// 上次启动失败的原因，比如端口被占用。
    error: Option<String>,
}

#[derive(Default)]
pub struct WebAccess {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    config: Option<Config>,
    /// 正在运行的服务：往里发 true 就停。
    stop: Option<watch::Sender<bool>>,
    error: Option<String>,
}

fn path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_local_data_dir().ok().map(|d| d.join(FILE))
}

fn load<R: Runtime>(app: &AppHandle<R>) -> Config {
    let config: Option<Config> = path(app)
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    match config {
        Some(c) if c.key.len() >= 16 => c,
        // 第一次用，或者文件坏了：新的密钥
        Some(c) => Config {
            key: shibapost_web::new_key(),
            ..c
        },
        None => Config::default(),
    }
}

fn save<R: Runtime>(app: &AppHandle<R>, config: &Config) {
    if let (Some(p), Ok(bytes)) = (path(app), serde_json::to_vec_pretty(config))
        && let Err(err) = std::fs::write(&p, bytes)
    {
        tracing::warn!("web access: could not save {}: {err}", p.display());
    }
}

/// 一块网卡和它的 IPv4 地址。
#[derive(Clone, Serialize, PartialEq, Debug)]
pub struct LanAddr {
    name: String,
    ip: String,
}

/// 这块网卡有多像连着局域网的那块：真实的 Wi-Fi、有线网卡加分；VPN、虚拟机、Docker、WSL 之类的减分；
/// 家里、公司常用的地址段加分；以 .1 结尾的多半是网关或者虚拟网桥，减分。
fn lan_score(name: &str, ip: Ipv4Addr) -> i32 {
    let n = name.to_lowercase();
    let mut score = 0;
    const REAL: &[&str] = &[
        "wlan",
        "wi-fi",
        "wifi",
        "ethernet",
        "以太网",
        "无线",
        "eth",
        "wlp",
        "enp",
        "eno",
        "wl",
    ];
    const VIRTUAL: &[&str] = &[
        "vethernet",
        "docker",
        "br-",
        "veth",
        "utun",
        "tun",
        "tap",
        "wg",
        "vpn",
        "virtual",
        "vmnet",
        "vbox",
        "hyper-v",
        "zerotier",
        "zt",
        "tailscale",
        "loopback",
        "bridge",
        "awdl",
        "llw",
        "ipsec",
        "ppp",
        "nordlynx",
        "clash",
        "wintun",
        "singbox",
        "sing-box",
        "wsl",
        "anpi",
        "gif",
        "stf",
    ];
    // macOS 的 en0、en1……是真实网卡（Wi-Fi 或有线）
    let mac_en = n.starts_with("en") && n[2..].chars().all(|c| c.is_ascii_digit()) && n.len() > 2;
    if mac_en || REAL.iter().any(|k| n.contains(k)) {
        score += 100;
    }
    if VIRTUAL.iter().any(|k| n.contains(k)) {
        score -= 150;
    }
    let [a, b, _, last] = ip.octets();
    score += match (a, b) {
        (192, 168) => 30,
        (10, _) => 20,
        (172, 16..=31) => 10,
        // 100.64.0.0/10 是运营商级的地址，Tailscale 这类组网工具也用它
        (100, 64..=127) => -50,
        _ => 0,
    };
    if last == 1 {
        score -= 20;
    }
    score
}

/// 从全部网卡里挑出能给手机用的 IPv4 地址，最像局域网的排前面。
fn rank_lan(list: Vec<(String, IpAddr)>) -> Vec<LanAddr> {
    let mut found: Vec<(i32, LanAddr)> = list
        .into_iter()
        .filter_map(|(name, ip)| match ip {
            IpAddr::V4(v4) if !v4.is_loopback() && !v4.is_link_local() && !v4.is_unspecified() => {
                Some((
                    lan_score(&name, v4),
                    LanAddr {
                        name,
                        ip: v4.to_string(),
                    },
                ))
            }
            _ => None,
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    let mut seen = std::collections::HashSet::new();
    found
        .into_iter()
        .map(|(_, a)| a)
        .filter(|a| seen.insert(a.ip.clone()))
        .collect()
}

/// 这台电脑的各块网卡。
fn lan_addresses() -> Vec<LanAddr> {
    rank_lan(local_ip_address::list_afinet_netifas().unwrap_or_default())
}

/// 桌面应用内置的界面文件。开发时（`tauri dev`）界面文件不打进程序里，改从构建好的 dist 目录读。
fn ui<R: Runtime>(app: &AppHandle<R>) -> Ui {
    let resolver = app.asset_resolver();
    if resolver.get("index.html".into()).is_none() {
        let dist = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../dist"));
        tracing::info!("web access: serving the interface from {}", dist.display());
        return Ui::Dir(dist);
    }
    Ui::Assets(Arc::new(move |path: &str| {
        let asset = resolver.get(path.to_string())?;
        Some(UiFile {
            bytes: asset.bytes,
            mime: asset.mime_type,
            csp: asset.csp_header,
        })
    }))
}

impl WebAccess {
    fn status(&self) -> Status {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let config = inner.config.clone().unwrap_or_default();
        let running = inner.stop.is_some();
        let url = |host: &str| format!("http://{host}:{}/?key={}", config.port, config.key);
        let addresses = if config.lan {
            lan_addresses()
        } else {
            Vec::new()
        };
        // 选过网卡、它还在就用它，否则用排在最前面的
        let chosen = config
            .lan_interface
            .as_ref()
            .and_then(|name| addresses.iter().find(|a| &a.name == name))
            .or_else(|| addresses.first());
        Status {
            enabled: config.enabled,
            running,
            port: config.port,
            lan: config.lan,
            local_url: running.then(|| url("127.0.0.1")),
            lan_url: if running && config.lan {
                chosen.map(|a| url(&a.ip))
            } else {
                None
            },
            lan_addresses: addresses.clone(),
            lan_interface: chosen.map(|a| a.name.clone()),
            error: inner.error.clone(),
        }
    }

    /// 按设置停掉旧的服务、需要时启动新的。
    async fn apply<R: Runtime>(&self, app: &AppHandle<R>, core: Arc<Core>) {
        let config = {
            let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(stop) = inner.stop.take() {
                let _ = stop.send(true);
            }
            inner.error = None;
            inner.config.get_or_insert_with(|| load(app)).clone()
        };
        if !config.enabled {
            return;
        }
        let ip = if config.lan {
            Ipv4Addr::UNSPECIFIED
        } else {
            Ipv4Addr::LOCALHOST
        };
        let addr = SocketAddr::from((ip, config.port));
        // 刚停掉的服务可能还没放开端口，稍等再试几次
        let mut listener = Err(std::io::Error::other("not tried"));
        for _ in 0..10 {
            listener = tokio::net::TcpListener::bind(addr).await;
            if listener.is_ok() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
        let listener = match listener {
            Ok(l) => l,
            Err(err) => {
                tracing::warn!("web access: could not listen on {addr}: {err}");
                self.inner.lock().unwrap_or_else(|e| e.into_inner()).error = Some(err.to_string());
                return;
            }
        };
        let (stop, stopped) = watch::channel(false);
        let access = Access {
            key: Some(config.key.clone()),
            lan: config.lan,
        };
        let ui = ui(app);
        tauri::async_runtime::spawn(async move {
            if let Err(err) = shibapost_web::serve(core, listener, ui, access, stopped).await {
                tracing::warn!("web access stopped: {err}");
            }
        });
        tracing::info!("web access: listening on http://{addr}");
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).stop = Some(stop);
    }
}

/// 应用启动时：上次开着就接着开。
pub fn start<R: Runtime>(app: &AppHandle<R>, core: Arc<Core>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<WebAccess>();
        let enabled = {
            let mut inner = state.inner.lock().unwrap_or_else(|e| e.into_inner());
            inner.config.get_or_insert_with(|| load(&app)).enabled
        };
        if enabled {
            state.apply(&app, core).await;
        }
    });
}

#[derive(Deserialize)]
pub struct Update {
    enabled: Option<bool>,
    port: Option<u16>,
    lan: Option<bool>,
    /// 选用哪块网卡的地址（只影响显示的链接和二维码，服务本来就在所有网卡上）。
    lan_interface: Option<String>,
    /// 换一个新的访问密钥，旧链接失效。
    #[serde(default)]
    new_key: bool,
}

#[tauri::command]
pub async fn web_access_status<R: Runtime>(app: AppHandle<R>) -> Status {
    let state = app.state::<WebAccess>();
    {
        let mut inner = state.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.config.get_or_insert_with(|| load(&app));
    }
    state.status()
}

#[tauri::command]
pub async fn web_access_update<R: Runtime>(
    app: AppHandle<R>,
    update: Update,
) -> Result<Status, String> {
    if update.port.is_some_and(|p| p < 1024) {
        return Err("port must be 1024 or higher".into());
    }
    let state = app.state::<WebAccess>();
    let core = app.state::<crate::AppState>().core.clone();
    {
        let mut inner = state.inner.lock().unwrap_or_else(|e| e.into_inner());
        let config = inner.config.get_or_insert_with(|| load(&app));
        if let Some(v) = update.enabled {
            config.enabled = v;
        }
        if let Some(v) = update.port {
            config.port = v;
        }
        if let Some(v) = update.lan {
            config.lan = v;
        }
        if update.new_key {
            config.key = shibapost_web::new_key();
        }
        if let Some(name) = update.lan_interface.clone() {
            config.lan_interface = Some(name);
        }
        let config = config.clone();
        save(&app, &config);
    }
    state.apply(&app, core).await;
    Ok(state.status())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v4(s: &str) -> IpAddr {
        IpAddr::V4(s.parse().unwrap())
    }

    #[test]
    fn the_real_network_card_comes_before_vpn_and_virtual_ones() {
        // Windows：WSL / Hyper-V 的虚拟网卡、VPN、真正的 WLAN
        let ranked = rank_lan(vec![
            ("vEthernet (WSL)".into(), v4("172.18.0.1")),
            ("Clash".into(), v4("198.18.0.1")),
            ("WLAN".into(), v4("192.168.31.20")),
            ("Loopback Pseudo-Interface 1".into(), v4("127.0.0.1")),
        ]);
        assert_eq!(ranked[0].name, "WLAN");
        assert_eq!(ranked[0].ip, "192.168.31.20");
        assert!(ranked.iter().all(|a| a.ip != "127.0.0.1"));
        assert_eq!(ranked.len(), 3, "the others stay available to pick");

        // macOS：Wi-Fi 是 en0，VPN 是 utun，Docker 桌面版的网桥
        let ranked = rank_lan(vec![
            ("utun4".into(), v4("10.8.0.2")),
            ("bridge100".into(), v4("192.168.64.1")),
            ("en0".into(), v4("10.0.0.23")),
        ]);
        assert_eq!(ranked[0].name, "en0");

        // Linux：Docker 网桥和有线网卡
        let ranked = rank_lan(vec![
            ("docker0".into(), v4("172.17.0.1")),
            ("enp3s0".into(), v4("192.168.1.50")),
        ]);
        assert_eq!(ranked[0].name, "enp3s0");
    }

    #[test]
    fn link_local_and_ipv6_addresses_are_skipped() {
        let ranked = rank_lan(vec![
            ("en0".into(), v4("169.254.10.2")),
            ("en0".into(), "fe80::1".parse().unwrap()),
        ]);
        assert!(ranked.is_empty());
    }
}
