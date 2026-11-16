//! 诊断用：看某个邮箱地址会被识别成哪个服务商。
//!
//!     cargo run -p postflow-mail --example detect -- someone@example.com

#[tokio::main]
async fn main() {
    for email in std::env::args().skip(1) {
        match postflow_mail::detect(&email).await {
            Some((cfg, preset)) => println!(
                "{email}: {} -> {}:{} ({:?})",
                preset.map(|p| p.name).unwrap_or("未识别，按常见规则猜"),
                cfg.host,
                cfg.port,
                cfg.security
            ),
            None => println!("{email}: 地址格式不对"),
        }
    }
}
