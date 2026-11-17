//! 对着本机的假 IMAP 服务器，把邮箱从添加到移除的整个流程走一遍。

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use postflow_core::{Core, MemorySecrets};
use postflow_testkit::{FakeImap, FakeSmtp, mail};
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

#[tokio::test(flavor = "multi_thread")]
async fn junk_and_trash_are_listed_on_their_own() {
    let server = FakeImap::start(USER, PASS).await;
    // 垃圾邮件标了用途；已删除只有名字，还是网易那样的中文名（「已删除」）
    server.add_mailbox("Junk", Some("\\Junk"));
    server.add_mailbox("&XfJSIJZk-", None);
    server.deliver("INBOX", &mail("in@x", "a@x", "Hello", "1"), &[]);
    server.deliver("Junk", &mail("spam@x", "b@x", "Win a prize", "2"), &[]);
    server.deliver("&XfJSIJZk-", &mail("old@x", "c@x", "Old receipt", "3"), &[]);
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
    wait_for("junk and trash synced", || async {
        mail_items(&core, "junk").await.len() == 1 && mail_items(&core, "trash").await.len() == 1
    })
    .await;
    let titles = |items: Vec<Value>| -> Vec<String> {
        items
            .iter()
            .map(|i| i["title"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(titles(mail_items(&core, "inbox").await), ["Hello"]);
    assert_eq!(titles(mail_items(&core, "junk").await), ["Win a prize"]);
    assert_eq!(titles(mail_items(&core, "trash").await), ["Old receipt"]);
    assert!(mail_items(&core, "unread").await.len() == 1);
    assert!(mail_items(&core, "archived").await.is_empty());
    let nav = call(&core, "nav.get", json!({})).await;
    assert_eq!(nav["accounts"][0]["unread"], 1, "junk is not counted");

    // 能打开、能标记已读，写回垃圾邮件所在的邮箱
    let spam = find(&core, "junk", "Win a prize").await.unwrap();
    let opened = call(&core, "items.get", json!({ "id": spam["id"] })).await;
    assert!(opened["mail"]["body"].as_str().unwrap().contains('2'));
    call(
        &core,
        "items.set",
        json!({ "ids": [spam["id"]], "read": true }),
    )
    .await;
    wait_for("seen in junk on the server", || async {
        server.flags("Junk", "spam@x").unwrap_or_default() == ["\\Seen"]
    })
    .await;

    // 归档对它不起作用；移回收件箱才行
    call(
        &core,
        "items.set",
        json!({ "ids": [spam["id"]], "archived": true }),
    )
    .await;
    assert!(find(&core, "junk", "Win a prize").await.is_some());
    call(
        &core,
        "items.set",
        json!({ "ids": [spam["id"]], "archived": false }),
    )
    .await;
    wait_for("moved to the inbox on the server", || async {
        server.contains("INBOX", "spam@x") && !server.contains("Junk", "spam@x")
    })
    .await;
    call(&core, "accounts.sync", json!({ "id": account["id"] })).await;
    wait_for("listed once, in the inbox", || async {
        let inbox = titles(mail_items(&core, "inbox").await);
        inbox.len() == 2
            && inbox.contains(&"Win a prize".to_string())
            && mail_items(&core, "junk").await.is_empty()
    })
    .await;

    // 在别的客户端里删掉的邮件，同步后出现在已删除里
    server.remove("INBOX", "in@x");
    server.deliver(
        "&XfJSIJZk-",
        &mail("in@x", "a@x", "Hello", "1"),
        &["\\Seen"],
    );
    call(&core, "accounts.sync", json!({ "id": account["id"] })).await;
    wait_for("deleted elsewhere shows in trash", || async {
        titles(mail_items(&core, "trash").await).contains(&"Hello".to_string())
            && find(&core, "inbox", "Hello").await.is_none()
    })
    .await;
    core.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn refresh_reconnects_after_a_refused_login() {
    // 有的邮箱会临时拒绝登录（比如 QQ 邮箱登录太频繁时）。之后点刷新要重新连，
    // 不能一直停在那里，否则新邮件再也收不到
    let server = FakeImap::start(USER, PASS).await;
    server.deliver("INBOX", &mail("one@x", "a@x", "First", "1"), &[]);
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
    call(&core, "accounts.add", account_params(server.port, PASS)).await;
    wait_for("initial sync", || async {
        find(&core, "inbox", "First").await.is_some()
    })
    .await;

    server.refuse_logins(Some("Login frequency limited, please try again later"));
    server.drop_connections();
    wait_for("the refusal is reported on the account", || async {
        call(&core, "nav.get", json!({})).await["accounts"][0]["error_code"] == "mail_auth"
    })
    .await;

    // 应用自己发起的收信（窗口回到前台）不去重试登录，免得密码真错了时一直去试
    let attempts = || {
        server
            .commands()
            .iter()
            .filter(|c| c.starts_with("LOGIN"))
            .count()
    };
    let before = attempts();
    call(&core, "accounts.sync", json!({ "auto": true })).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        attempts(),
        before,
        "an automatic check must not retry a refused login"
    );

    // 用户点刷新就重新连
    server.refuse_logins(None);
    server.deliver("INBOX", &mail("two@x", "a@x", "Second", "2"), &[]);
    call(&core, "feeds.refresh", json!({})).await;
    wait_for("new mail after refreshing", || async {
        find(&core, "inbox", "Second").await.is_some()
    })
    .await;
    let account = call(&core, "nav.get", json!({})).await["accounts"][0].clone();
    assert!(account["error_code"].is_null(), "{account}");
}

#[tokio::test(flavor = "multi_thread")]
async fn replies_are_sent_threaded_and_saved_to_sent() {
    let server = FakeImap::start(USER, PASS).await;
    let smtp = FakeSmtp::start(USER, PASS).await;
    server.deliver(
        "INBOX",
        "From: Ada <ada@example.com>\r\nTo: me@example.com, Bob <bob@example.com>\r\n\
         Cc: carol@example.com\r\nSubject: Lunch?\r\nDate: Mon, 16 Nov 2026 08:00:00 +0000\r\n\
         Message-ID: <lunch-2@example.com>\r\nReferences: <lunch-1@example.com>\r\n\r\n\
         Noon at the usual place?\r\n",
        &[],
    );
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
    let id = account["id"].clone();
    wait_for("initial sync", || async {
        mail_items(&core, "inbox").await.len() == 1
    })
    .await;

    // 本机的收信服务器：发信服务器猜同一台机器的 1025 端口
    let auto = call(&core, "accounts.smtp", json!({ "id": id })).await;
    assert_eq!(
        auto,
        json!({ "host": "127.0.0.1", "port": 1025, "security": "none", "custom": false })
    );

    // 填错的发信服务器保存不了
    let err = core
        .rpc(
            "accounts.update",
            json!({ "id": id, "smtp_host": "127.0.0.1", "smtp_port": 9, "smtp_security": "none" }),
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, "send_connect");
    call(
        &core,
        "accounts.update",
        json!({ "id": id, "smtp_host": "127.0.0.1", "smtp_port": smtp.port, "smtp_security": "none" }),
    )
    .await;
    let custom = call(&core, "accounts.smtp", json!({ "id": id })).await;
    assert_eq!(custom["port"], smtp.port);
    assert_eq!(custom["custom"], true);

    // 回复要用到的原信内容
    let original = find(&core, "inbox", "Lunch?").await.unwrap();
    let ctx = call(
        &core,
        "mail.compose_context",
        json!({ "id": original["id"] }),
    )
    .await;
    assert_eq!(ctx["account_id"], id);
    assert_eq!(ctx["message_id"], "lunch-2@example.com");
    assert_eq!(ctx["from"][0]["address"], "ada@example.com");
    assert_eq!(ctx["to"].as_array().unwrap().len(), 2);
    assert_eq!(ctx["text"], "Noon at the usual place?");

    // 收件人写错了：不发
    let err = core
        .rpc(
            "mail.send",
            json!({ "account_id": id, "to": ["not an address"], "subject": "x", "text": "y" }),
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, "send_bad_address");
    assert!(smtp.delivered().is_empty());

    call(
        &core,
        "accounts.update",
        json!({ "id": id, "sender_name": " Sam Doe " }),
    )
    .await;
    let sent = call(
        &core,
        "mail.send",
        json!({
            "account_id": id,
            "to": ["Ada <ada@example.com>"],
            "cc": ["bob@example.com", "carol@example.com"],
            "subject": "Re: Lunch?",
            "text": "Sounds good.\n\n> Noon at the usual place?",
            "in_reply_to": ctx["thread"]["in_reply_to"],
            "references": ctx["thread"]["references"],
        }),
    )
    .await;
    let message_id = sent["message_id"].as_str().unwrap().to_string();
    assert!(message_id.ends_with("@example.com"));

    let delivered = smtp.delivered();
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].from, USER);
    assert_eq!(
        delivered[0].to,
        ["ada@example.com", "bob@example.com", "carol@example.com"]
    );
    let raw = String::from_utf8_lossy(&delivered[0].raw).to_string();
    assert!(raw.contains("In-Reply-To: <lunch-2@example.com>"), "{raw}");
    assert!(
        raw.contains("References: <lunch-1@example.com> <lunch-2@example.com>"),
        "{raw}"
    );
    // 侧栏里的邮箱名 Work 不是人名，发信用专门填的发件人名字
    assert!(raw.contains("From: \"Sam Doe\" <me@example.com>"), "{raw}");

    // 发出去的信过一会儿出现在服务器的「已发送」里，只存一份
    wait_for("copy saved to Sent", || async {
        server.contains("Sent", &message_id)
    })
    .await;
    assert_eq!(server.count("Sent"), 1);
    core.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn drafts_attachments_and_forwards() {
    let server = FakeImap::start(USER, PASS).await;
    server.add_mailbox("Drafts", Some("\\Drafts"));
    let smtp = FakeSmtp::start(USER, PASS).await;
    server.deliver("INBOX", ATTACHMENT_MAIL, &[]);
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
    let id = account["id"].clone();
    call(
        &core,
        "accounts.update",
        json!({ "id": id, "smtp_host": "127.0.0.1", "smtp_port": smtp.port, "smtp_security": "none" }),
    )
    .await;
    wait_for("initial sync", || async {
        mail_items(&core, "inbox").await.len() == 1
    })
    .await;

    // 加一个附件，存成草稿
    let file = call(
        &core,
        "mail.attach",
        json!({ "name": "notes.txt", "mime": "text/plain", "data": "aGVsbG8gZmlsZQ==" }),
    )
    .await;
    assert_eq!(file["size"], 10);
    let letter = |text: &str, draft: Value| {
        json!({
            "account_id": id,
            "to": ["Ada <ada@example.com>", "half typed"],
            "bcc": ["hidden@example.com"],
            "subject": "Plans",
            "text": text,
            "attachments": [file["token"]],
            "draft": draft,
        })
    };
    let first = call(
        &core,
        "mail.save_draft",
        letter("First version", Value::Null),
    )
    .await;
    let first_id = first["message_id"].as_str().unwrap().to_string();
    wait_for("draft listed", || async {
        mail_items(&core, "drafts").await.len() == 1
    })
    .await;
    assert!(server.contains("Drafts", &first_id));
    assert_eq!(
        server.flags("Drafts", &first_id).unwrap(),
        ["\\Draft", "\\Seen"]
    );
    // 草稿不在收件箱里
    assert_eq!(mail_items(&core, "inbox").await.len(), 1);

    // 再存一次替换掉上一版
    let second = call(
        &core,
        "mail.save_draft",
        letter("Second version", json!(first_id)),
    )
    .await;
    let second_id = second["message_id"].as_str().unwrap().to_string();
    wait_for("old draft replaced", || async {
        server.count("Drafts") == 1 && server.contains("Drafts", &second_id)
    })
    .await;
    wait_for("draft list follows", || async {
        mail_items(&core, "drafts")
            .await
            .first()
            .is_some_and(|d| d["id"] == second["item_id"])
    })
    .await;
    let listed = &mail_items(&core, "drafts").await[0];
    assert_eq!(listed["draft"], true);
    assert_eq!(listed["author"], "Ada");

    // 接着写：内容、密送和附件都还在
    let opened = call(&core, "mail.draft", json!({ "id": listed["id"] })).await;
    assert_eq!(opened["account_id"], id);
    assert_eq!(opened["message_id"], second_id.as_str());
    assert_eq!(opened["to"], json!(["Ada <ada@example.com>"]));
    assert_eq!(opened["bcc"], json!(["hidden@example.com"]));
    assert_eq!(opened["text"], "Second version");
    assert_eq!(opened["attachments"][0]["name"], "notes.txt");
    let draft_file = opened["attachments"][0]["token"].clone();

    // 转发带着原信的附件
    let report = find(&core, "inbox", "The report").await.unwrap();
    let ctx = call(
        &core,
        "mail.compose_context",
        json!({ "id": report["id"], "forward": true }),
    )
    .await;
    assert_eq!(ctx["attachments"][0]["name"], "numbers.csv");
    let forwarded = ctx["attachments"][0]["token"].clone();

    // 编号不认识的附件：让用户重新添加
    let err = core
        .rpc(
            "mail.send",
            json!({ "account_id": id, "to": ["ada@example.com"], "attachments": ["nope"] }),
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, "attachment_gone");

    call(
        &core,
        "mail.send",
        json!({
            "account_id": id,
            "to": ["ada@example.com"],
            "subject": "Plans",
            "text": "Final",
            "attachments": [draft_file, forwarded],
            "draft": second_id,
        }),
    )
    .await;
    let raw = String::from_utf8_lossy(&smtp.delivered()[0].raw).to_string();
    assert!(
        raw.contains("notes.txt") && raw.contains("numbers.csv"),
        "{raw}"
    );
    // 发出去以后草稿删掉
    wait_for("draft deleted after sending", || async {
        server.count("Drafts") == 0
    })
    .await;
    wait_for("draft list empty", || async {
        mail_items(&core, "drafts").await.is_empty()
    })
    .await;

    // 丢弃草稿
    let third = call(&core, "mail.save_draft", letter("Third", Value::Null)).await;
    wait_for("third draft saved", || async {
        server.count("Drafts") == 1
    })
    .await;
    call(
        &core,
        "mail.delete_draft",
        json!({ "account_id": id, "message_id": third["message_id"] }),
    )
    .await;
    wait_for("discarded draft deleted", || async {
        server.count("Drafts") == 0
    })
    .await;
    // 列表里的草稿按 ID 删
    let fourth = call(&core, "mail.save_draft", letter("Fourth", Value::Null)).await;
    wait_for("fourth draft listed", || async {
        mail_items(&core, "drafts").await.len() == 1
    })
    .await;
    call(
        &core,
        "mail.delete_draft",
        json!({ "id": fourth["item_id"] }),
    )
    .await;
    wait_for("listed draft deleted", || async {
        server.count("Drafts") == 0
    })
    .await;
    core.shutdown();
}
