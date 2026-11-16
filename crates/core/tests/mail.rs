//! 对着本机的假 IMAP 服务器，把邮箱从添加到移除的整个流程走一遍。

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use postflow_core::{Core, MemorySecrets};
use postflow_testkit::{FakeImap, mail};
use serde_json::{Value, json};

const USER: &str = "me@example.com";
const PASS: &str = "app-password";

const HTML_MAIL: &str = "From: \"Newsletter\" <news@shop.example>\r\n\
To: me@example.com\r\n\
Subject: Weekly deals\r\n\
Date: Sun, 15 Nov 2026 10:00:00 +0000\r\n\
Message-ID: <html-1@shop.example>\r\n\
List-Unsubscribe: <https://shop.example/unsub>\r\n\
Content-Type: text/html; charset=utf-8\r\n\
\r\n\
<p>Big <b>sale</b></p><img src=\"https://shop.example/track.gif\"><script>alert(1)</script>\r\n";

const ATTACHMENT_MAIL: &str = "From: Bob <bob@example.com>\r\n\
To: me@example.com\r\n\
Subject: The report\r\n\
Date: Mon, 16 Nov 2026 07:00:00 +0000\r\n\
Message-ID: <att-1@example.com>\r\n\
Content-Type: multipart/mixed; boundary=\"XX\"\r\n\
\r\n\
--XX\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
See attached.\r\n\
--XX\r\n\
Content-Type: text/csv; name=\"numbers.csv\"\r\n\
Content-Disposition: attachment; filename=\"numbers.csv\"\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
YSxiCjEsMgo=\r\n\
--XX--\r\n";

async fn call(core: &Arc<Core>, method: &str, params: Value) -> Value {
    core.rpc(method, params, None)
        .await
        .unwrap_or_else(|e| panic!("{method} failed: {e}"))
}

async fn wait_for<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    for _ in 0..100 {
        if check().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("timed out waiting for: {what}");
}

async fn mail_items(core: &Arc<Core>, view: &str) -> Vec<Value> {
    call(
        core,
        "items.list",
        json!({ "filter": { "view": view, "kind": "mail" } }),
    )
    .await["items"]
        .as_array()
        .unwrap()
        .clone()
}

async fn find(core: &Arc<Core>, view: &str, title: &str) -> Option<Value> {
    mail_items(core, view)
        .await
        .into_iter()
        .find(|i| i["title"] == title)
}

fn account_params(port: u16, password: &str) -> Value {
    json!({
        "email": USER,
        "password": password,
        "name": "Work",
        "imap_host": "127.0.0.1",
        "imap_port": port,
        "imap_security": "none",
        "username": USER,
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn mail_accounts_end_to_end() {
    let server = FakeImap::start(USER, PASS).await;
    server.deliver(
        "INBOX",
        &mail(
            "plain-1@example.com",
            "Ada <ada@example.com>",
            "Lunch?",
            "Noon at the usual place",
        ),
        &[],
    );
    server.deliver(
        "INBOX",
        &mail(
            "seen-1@example.com",
            "Carol <carol@example.com>",
            "Old news",
            "x",
        ),
        &["\\Seen"],
    );
    server.deliver("INBOX", HTML_MAIL, &[]);
    server.deliver("INBOX", ATTACHMENT_MAIL, &["\\Seen"]);
    let big = mail(
        "big-1@example.com",
        "Dan <dan@example.com>",
        "Huge",
        &"a".repeat(2_200_000),
    );
    server.deliver("INBOX", &big, &["\\Seen"]);

    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let core = Core::open_with_secrets(
        tmp.path().join("local-a"),
        Arc::new(MemorySecrets::default()),
    )
    .await
    .unwrap();
    call(
        &core,
        "app.setup",
        json!({ "data_dir": data_dir.to_str().unwrap(), "device_name": "A" }),
    )
    .await;

    // 常见服务商的设置可以从地址推断出来
    let guess = call(
        &core,
        "accounts.guess",
        json!({ "email": "someone@qq.com" }),
    )
    .await;
    assert_eq!(guess["imap_host"], "imap.qq.com");
    assert_eq!(guess["preset"]["password_kind"], "auth_code");
    let err = core
        .rpc("accounts.guess", json!({ "email": "nope" }), None)
        .await
        .unwrap_err();
    assert_eq!(err.code, "invalid_email");

    // 密码不对：报错，什么都不保存
    let err = core
        .rpc("accounts.add", account_params(server.port, "wrong"), None)
        .await
        .unwrap_err();
    assert_eq!(err.code, "mail_auth");
    // 服务器的原话原样带给界面，去掉了库加的包装和机器码
    assert_eq!(err.message, "Invalid credentials");
    assert!(
        call(&core, "nav.get", json!({})).await["accounts"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    // 明文连接只允许连本机
    let mut remote = account_params(server.port, PASS);
    remote["imap_host"] = json!("imap.example.com");
    assert_eq!(
        core.rpc("accounts.add", remote, None)
            .await
            .unwrap_err()
            .code,
        "bad_params"
    );

    let account = call(&core, "accounts.add", account_params(server.port, PASS)).await;
    let account_id = account["id"].as_str().unwrap().to_string();
    assert_eq!(account["name"], "Work");

    // 后台任务连上服务器，把收件箱同步下来
    wait_for("initial sync", || async {
        mail_items(&core, "inbox").await.len() == 5
    })
    .await;
    let nav = call(&core, "nav.get", json!({})).await;
    assert_eq!(nav["accounts"][0]["unread"], 2);
    assert!(nav["accounts"][0]["error"].is_null());
    assert!(nav["counts"]["unread"].as_i64().unwrap() >= 2);

    let lunch = find(&core, "inbox", "Lunch?").await.unwrap();
    assert_eq!(lunch["kind"], "mail");
    assert_eq!(lunch["author"], "Ada");
    assert_eq!(lunch["source_title"], "Work");
    assert_eq!(lunch["summary"], "Noon at the usual place");
    let lunch_id = lunch["id"].as_str().unwrap().to_string();

    // HTML 邮件：脚本去掉，远程图片默认不加载
    let html = find(&core, "inbox", "Weekly deals").await.unwrap();
    let html_id = html["id"].as_str().unwrap().to_string();
    let detail = call(&core, "items.get", json!({ "id": html_id })).await;
    let body = detail["mail"]["body"].as_str().unwrap();
    assert_eq!(detail["mail"]["body_kind"], "html");
    assert!(!body.contains("<script"));
    assert!(!body.contains("track.gif"));
    assert_eq!(detail["mail"]["has_remote_content"], true);
    assert_eq!(detail["mail"]["remote_allowed"], false);
    assert_eq!(detail["mail"]["from"]["address"], "news@shop.example");
    let once = call(
        &core,
        "items.get",
        json!({ "id": html_id, "allow_remote": true }),
    )
    .await;
    assert!(once["mail"]["body"].as_str().unwrap().contains("track.gif"));
    // 信任这个发件人以后，不用每次都点
    call(
        &core,
        "mail.trust_sender",
        json!({ "address": "News@Shop.example" }),
    )
    .await;
    let trusted = call(&core, "items.get", json!({ "id": html_id })).await;
    assert_eq!(trusted["mail"]["remote_allowed"], true);
    assert_eq!(trusted["mail"]["sender_trusted"], true);

    // 附件
    let report = find(&core, "inbox", "The report").await.unwrap();
    assert_eq!(report["has_attachments"], true);
    let detail = call(&core, "items.get", json!({ "id": report["id"] })).await;
    let att = &detail["mail"]["attachments"][0];
    assert_eq!(att["name"], "numbers.csv");
    let file = call(
        &core,
        "mail.attachment",
        json!({ "id": report["id"], "index": att["index"] }),
    )
    .await;
    assert_eq!(file["data"], "YSxiCjEsMgo=");
    let saved = tmp.path().join("numbers.csv");
    call(
        &core,
        "mail.attachment",
        json!({ "id": report["id"], "index": att["index"], "path": saved.to_str().unwrap() }),
    )
    .await;
    assert_eq!(std::fs::read(&saved).unwrap(), b"a,b\n1,2\n");

    // 超过大小上限的邮件同步时只取了信头，打开时才从服务器取正文
    let huge = find(&core, "inbox", "Huge").await.unwrap();
    assert_eq!(huge["summary"], "");
    let detail = call(&core, "items.get", json!({ "id": huge["id"] })).await;
    assert!(detail["mail"]["body"].as_str().unwrap().len() > 2_000_000);

    // 已读、星标写回服务器
    call(
        &core,
        "items.set",
        json!({ "ids": [lunch_id], "read": true, "starred": true }),
    )
    .await;
    wait_for("flags on the server", || async {
        server
            .flags("INBOX", "plain-1@example.com")
            .unwrap_or_default()
            == ["\\Flagged", "\\Seen"]
    })
    .await;
    assert_eq!(
        call(&core, "nav.get", json!({})).await["accounts"][0]["unread"],
        1
    );

    // 归档：移到服务器的归档邮箱；再移回来
    call(
        &core,
        "items.set",
        json!({ "ids": [lunch_id], "archived": true }),
    )
    .await;
    assert!(find(&core, "inbox", "Lunch?").await.is_none());
    assert!(find(&core, "archived", "Lunch?").await.is_some());
    wait_for("archived on the server", || async {
        server.contains("Archive", "plain-1@example.com")
    })
    .await;
    assert!(!server.contains("INBOX", "plain-1@example.com"));

    call(
        &core,
        "items.set",
        json!({ "ids": [lunch_id], "archived": false }),
    )
    .await;
    wait_for("back in the inbox on the server", || async {
        server.contains("INBOX", "plain-1@example.com")
    })
    .await;
    call(&core, "accounts.sync", json!({ "id": account_id })).await;
    wait_for("unarchived message keeps its identity", || async {
        let all = mail_items(&core, "inbox").await;
        all.iter().filter(|i| i["title"] == "Lunch?").count() == 1 && all.len() == 5
    })
    .await;
    let back = find(&core, "inbox", "Lunch?").await.unwrap();
    assert_eq!(back["id"], lunch_id);
    assert_eq!(back["starred"], true);

    // 新邮件经 IDLE 实时到达，不需要手动刷新
    server.deliver(
        "INBOX",
        &mail(
            "new-1@example.com",
            "Eve <eve@example.com>",
            "Just arrived",
            "hello",
        ),
        &[],
    );
    wait_for("new mail via IDLE", || async {
        find(&core, "inbox", "Just arrived").await.is_some()
    })
    .await;

    // 别的客户端改了标记、删了邮件，这里跟着变
    server.set_flag("INBOX", "new-1@example.com", "\\Seen", true);
    wait_for("flag change from another client", || async {
        find(&core, "inbox", "Just arrived")
            .await
            .is_some_and(|i| i["read"] == true)
    })
    .await;
    server.remove("INBOX", "seen-1@example.com");
    wait_for("deletion from another client", || async {
        find(&core, "inbox", "Old news").await.is_none()
    })
    .await;

    // 「全部标为已读」里的邮件也写回服务器
    call(
        &core,
        "items.mark_all_read",
        json!({ "filter": { "view": "inbox", "source_id": account_id } }),
    )
    .await;
    wait_for("mark all read on the server", || async {
        server
            .flags("INBOX", "html-1@shop.example")
            .unwrap_or_default()
            .contains(&"\\Seen".to_string())
    })
    .await;

    // 第二台设备：读到同一个账户，但没有密码，提示输入；输入后开始同步
    let other = Core::open_with_secrets(
        tmp.path().join("local-b"),
        Arc::new(MemorySecrets::default()),
    )
    .await
    .unwrap();
    call(
        &other,
        "app.setup",
        json!({ "data_dir": data_dir.to_str().unwrap(), "device_name": "B" }),
    )
    .await;
    wait_for("second device reports the missing password", || async {
        call(&other, "nav.get", json!({})).await["accounts"][0]["error_code"] == "mail_no_password"
    })
    .await;
    call(
        &other,
        "accounts.update",
        json!({ "id": account_id, "password": PASS }),
    )
    .await;
    wait_for("second device syncs", || async {
        !mail_items(&other, "inbox").await.is_empty()
    })
    .await;
    assert!(call(&other, "nav.get", json!({})).await["accounts"][0]["error_code"].is_null());
    other.shutdown();

    // 移除邮箱：邮件从本机消失，服务器上的不受影响
    call(&core, "accounts.remove", json!({ "id": account_id })).await;
    assert!(mail_items(&core, "inbox").await.is_empty());
    assert!(
        call(&core, "nav.get", json!({})).await["accounts"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(server.count("INBOX") >= 4);
    core.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn renumbered_mailbox_is_resynced() {
    let server = FakeImap::start(USER, PASS).await;
    server.deliver("INBOX", &mail("a@x", "a@x", "First", "1"), &[]);
    server.deliver("INBOX", &mail("b@x", "b@x", "Second", "2"), &[]);
    let tmp = tempfile::tempdir().unwrap();
    let core =
        Core::open_with_secrets(tmp.path().join("local"), Arc::new(MemorySecrets::default()))
            .await
            .unwrap();
    call(
        &core,
        "app.setup",
        json!({ "data_dir": tmp.path().join("data").to_str().unwrap() }),
    )
    .await;
    let account = call(&core, "accounts.add", account_params(server.port, PASS)).await;
    wait_for("initial sync", || async {
        mail_items(&core, "inbox").await.len() == 2
    })
    .await;

    // 服务器重建了收件箱，所有 UID 都变了。同步后还是这两封，不多不少
    server.renumber("INBOX");
    call(&core, "accounts.sync", json!({ "id": account["id"] })).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    wait_for("resync after uidvalidity change", || async {
        mail_items(&core, "inbox").await.len() == 2
    })
    .await;
    let first = find(&core, "inbox", "First").await.unwrap();
    call(
        &core,
        "items.set",
        json!({ "ids": [first["id"]], "read": true }),
    )
    .await;
    wait_for("flag written with the new uid", || async {
        server.flags("INBOX", "a@x").unwrap_or_default() == ["\\Seen"]
    })
    .await;
    core.shutdown();
}
