//! 在浏览器中使用：桌面应用在本机端口上提供同一套界面，用的就是这个窗口里的核心和数据。
//!
//! 设置只存在这台设备上（本机目录的 web-access.json），不同步：端口、是否允许局域网访问、访问密钥。
//! 一定要访问密钥：链接和二维码里带一次，浏览器之后用 Cookie 记住（见 postflow-web）。
//! 换密钥后旧链接和已经打开的页面都失效。关掉应用，网页版也就停了。

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use postflow_core::Core;
use postflow_web::{Access, Ui, UiFile};
use serde::{Deserialize, Serialize};
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            port: DEFAULT_PORT,
            lan: false,
            key: postflow_web::new_key(),
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
            key: postflow_web::new_key(),
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

/// 这台电脑在局域网里的地址：向外「连」一下 UDP（不会真的发包），看系统选了哪个网卡。
fn lan_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 80)).ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
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
        let url = |host: String| format!("http://{host}:{}/?key={}", config.port, config.key);
        Status {
            enabled: config.enabled,
            running,
            port: config.port,
            lan: config.lan,
            local_url: running.then(|| url("127.0.0.1".into())),
            lan_url: (running && config.lan)
                .then(lan_ip)
                .flatten()
                .map(|ip| url(ip.to_string())),
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
            if let Err(err) = postflow_web::serve(core, listener, ui, access, stopped).await {
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
            config.key = postflow_web::new_key();
        }
        let config = config.clone();
        save(&app, &config);
    }
    state.apply(&app, core).await;
    Ok(state.status())
}
