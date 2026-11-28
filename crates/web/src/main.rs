//! 不带窗口的 ShibaPost：只起核心和 HTTP 服务。
//!
//! 开发界面时用它配合 Vite，在普通浏览器里调试；也可以在没有桌面环境的机器上跑。

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use shibapost_core::Core;

const USAGE: &str = "\
用法: shibapost-web [选项]

选项:
  --local-dir <目录>   本机私有目录，放索引和本机配置（默认 ~/.shibapost）
  --port <端口>        监听端口（默认 4319）
  --static <目录>      界面静态文件目录（默认使用当前目录下的 dist，如果存在）
  --memory-secrets     邮箱密码只放在内存里，不写系统钥匙串。仅供开发测试，重启后要重新输入
  -h, --help           显示这段说明
";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut local_dir: Option<PathBuf> = None;
    let mut static_dir: Option<PathBuf> = None;
    let mut port: u16 = 4319;
    let mut memory_secrets = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or(format!("{name} 需要一个值"));
        match arg.as_str() {
            "--local-dir" => local_dir = Some(value("--local-dir")?.into()),
            "--static" => static_dir = Some(value("--static")?.into()),
            "--port" => port = value("--port")?.parse().map_err(|_| "端口必须是数字")?,
            "--memory-secrets" => memory_secrets = true,
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            other => return Err(format!("不认识的参数: {other}\n\n{USAGE}").into()),
        }
    }

    let local_dir: PathBuf = match local_dir {
        Some(dir) => dir,
        None => {
            let home = std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .ok_or("找不到用户主目录，请用 --local-dir 指定")?;
            PathBuf::from(home).join(".shibapost")
        }
    };
    shibapost_core::logging::init(&local_dir, true);
    let static_dir = static_dir.or_else(|| {
        let dist = PathBuf::from("dist");
        dist.join("assets").is_dir().then_some(dist)
    });

    let core = if memory_secrets {
        tracing::warn!("mail passwords are kept in memory only (--memory-secrets)");
        let secrets = std::sync::Arc::new(shibapost_core::MemorySecrets::default());
        Core::open_with_secrets(&local_dir, secrets).await?
    } else {
        Core::open(&local_dir).await?
    };
    // 只绑定回环地址，见 lib.rs 顶部的说明
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("local dir: {}", local_dir.display());
    tracing::info!("listening on http://{addr}");

    // 开发用：只绑定回环地址、不要访问密钥。桌面应用里的网页访问一定带密钥
    let ui = static_dir.map_or(shibapost_web::Ui::None, shibapost_web::Ui::Dir);
    let (stop, stopped) = tokio::sync::watch::channel(false);
    let server = tokio::spawn(shibapost_web::serve(
        core.clone(),
        listener,
        ui,
        shibapost_web::Access::default(),
        stopped,
    ));
    tokio::select! {
        result = server => result??,
        _ = tokio::signal::ctrl_c() => {
            let _ = stop.send(true);
        }
    }
    core.shutdown();
    Ok(())
}
