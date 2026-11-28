//! 诊断用：对一个 IMAP 服务器走一遍连接和登录，打印每一步的结果。
//!
//!     cargo run -p shibapost-mail --example probe -- imap.qq.com 993 tls 用户名 密码

use std::time::Instant;

use shibapost_mail::{Auth, Connection, Security, ServerConfig};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [host, port, security, username, password] = args.as_slice() else {
        eprintln!("usage: probe <host> <port> <tls|starttls|none> <username> <password>");
        std::process::exit(2);
    };
    let security = match security.as_str() {
        "tls" => Security::Tls,
        "starttls" => Security::Starttls,
        _ => Security::None,
    };
    let cfg = ServerConfig {
        host: host.clone(),
        port: port.parse().expect("port"),
        security,
        username: username.clone(),
    };
    let start = Instant::now();
    match Connection::connect(&cfg, &Auth::Password(password.to_string())).await {
        Ok(mut conn) => {
            println!("{host}: logged in after {:?}", start.elapsed());
            match conn.select("INBOX").await {
                Ok(info) => println!("{host}: INBOX has {} messages", info.exists),
                Err(e) => println!("{host}: select failed: [{}] {e}", e.code()),
            }
            conn.logout().await;
        }
        Err(e) => println!("{host}: [{}] {e:?} after {:?}", e.code(), start.elapsed()),
    }
}
