//! 对着本机的假 IMAP 服务器把连接上的每个操作走一遍。

use std::time::Duration;

use postflow_mail::{Connection, Error, FlagChange, IdleOutcome, Security, ServerConfig};
use postflow_testkit::{FakeImap, mail};

fn config(port: u16, user: &str) -> ServerConfig {
    ServerConfig {
        host: "127.0.0.1".into(),
        port,
        security: Security::None,
        username: user.into(),
    }
}

#[tokio::test]
async fn wrong_password_is_an_auth_error() {
    let server = FakeImap::start("me@example.com", "secret").await;
    let err = Connection::connect(&config(server.port, "me@example.com"), "nope")
        .await
        .err()
        .unwrap();
    assert!(matches!(err, Error::Auth(_)), "{err:?}");
    assert!(err.needs_user());
    assert_eq!(err.code(), "mail_auth");
}

#[tokio::test]
async fn nothing_listening_is_a_connect_error() {
    let err = Connection::connect(&config(9, "me"), "x")
        .await
        .err()
        .unwrap();
    assert!(matches!(err, Error::Connect(_)), "{err:?}");
    assert!(!err.needs_user());
}

#[tokio::test]
async fn list_fetch_flag_move_and_search() {
    let server = FakeImap::start("me@example.com", "secret").await;
    for n in 1..=5 {
        server.deliver(
            "INBOX",
            &mail(
                &format!("m{n}@x"),
                "Ada <ada@x>",
                &format!("Hello {n}"),
                "body",
            ),
            &[],
        );
    }
    server.deliver(
        "INBOX",
        &mail("seen@x", "Bob <bob@x>", "Already read", "hi"),
        &["\\Seen"],
    );

    let mut conn = Connection::connect(&config(server.port, "me@example.com"), "secret")
        .await
        .unwrap();
    assert!(conn.supports_idle());
    let info = conn.select("INBOX").await.unwrap();
    assert_eq!(info.exists, 6);
    assert_eq!(info.uidvalidity, 1000);
    // 网易邮箱要求先发 ID 才能打开收件箱
    assert!(server.commands().iter().any(|c| c.starts_with("ID ")));

    // 只取最新的 3 封
    let newest = conn.list_newest(info.exists, 3).await.unwrap();
    assert_eq!(
        newest.iter().map(|l| l.uid).collect::<Vec<_>>(),
        vec![4, 5, 6]
    );
    assert!(newest[2].seen && !newest[0].seen);
    assert!(newest[0].size > 0);
    assert!(newest[0].internal_date.is_some());

    // UID 从 7 起没有邮件，但「7:*」按协议也会带回最后一封，要被滤掉
    assert!(conn.list_from(7).await.unwrap().is_empty());
    assert_eq!(conn.list_from(5).await.unwrap().len(), 2);

    let raw = conn.fetch_raw(&[4, 5], false).await.unwrap();
    assert_eq!(raw.len(), 2);
    assert!(String::from_utf8_lossy(&raw[0].1).contains("Hello 4"));
    let headers = conn.fetch_raw(&[4], true).await.unwrap();
    let header = String::from_utf8_lossy(&headers[0].1).to_string();
    assert!(header.contains("Subject: Hello 4") && !header.contains("body"));

    conn.set_flag(&[4, 5], FlagChange::Seen(true))
        .await
        .unwrap();
    conn.set_flag(&[5], FlagChange::Flagged(true))
        .await
        .unwrap();
    assert_eq!(server.flags("INBOX", "m4@x").unwrap(), vec!["\\Seen"]);
    assert_eq!(
        server.flags("INBOX", "m5@x").unwrap(),
        vec!["\\Flagged", "\\Seen"]
    );
    conn.set_flag(&[5], FlagChange::Seen(false)).await.unwrap();
    assert_eq!(server.flags("INBOX", "m5@x").unwrap(), vec!["\\Flagged"]);

    // 归档：找到标了 \Archive 的邮箱，移过去
    let archive = conn.archive_mailbox().await.unwrap();
    assert_eq!(archive, "Archive");
    conn.move_to(&[4], &archive).await.unwrap();
    assert!(server.contains("Archive", "m4@x"));
    assert!(!server.contains("INBOX", "m4@x"));
    assert_eq!(server.count("INBOX"), 5);

    // 再按 Message-ID 找回来、移回收件箱
    conn.select(&archive).await.unwrap();
    let found = conn.find_by_message_id("m4@x").await.unwrap();
    assert_eq!(found.len(), 1);
    assert!(
        conn.find_by_message_id("missing@x")
            .await
            .unwrap()
            .is_empty()
    );
    conn.move_to(&found, "INBOX").await.unwrap();
    assert!(server.contains("INBOX", "m4@x"));
    assert_eq!(server.count("Archive"), 0);

    conn.logout().await;
}

#[tokio::test]
async fn idle_ends_on_time_even_when_the_server_keeps_it_alive() {
    // 有的服务器 IDLE 期间只发「还在」，从不通知新邮件。每一句「还在」都不能把结束时间往后推，
    // 否则连接会一直停在 IDLE 里，新邮件永远收不到
    let server = FakeImap::start("me", "pw").await;
    server.quiet_idle(Duration::from_millis(100));
    let mut conn = Connection::connect(&config(server.port, "me"), "pw")
        .await
        .unwrap();
    conn.select("INBOX").await.unwrap();
    let deliver = server.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        deliver.deliver("INBOX", &mail("quiet@x", "a@x", "Quiet", "b"), &[]);
    });
    let (mut conn, outcome) = tokio::time::timeout(
        Duration::from_secs(5),
        conn.idle(Duration::from_millis(600), std::future::pending()),
    )
    .await
    .expect("idle should end on its own deadline despite keepalives")
    .unwrap();
    assert_eq!(outcome, IdleOutcome::Timeout);
    // 结束 IDLE 以后再同步，就能看到那封没被通知的新邮件
    conn.select("INBOX").await.unwrap();
    assert_eq!(conn.list_from(1).await.unwrap().len(), 1);
}

#[tokio::test]
async fn idle_wakes_up_on_new_mail_and_can_be_interrupted() {
    let server = FakeImap::start("me", "pw").await;
    let mut conn = Connection::connect(&config(server.port, "me"), "pw")
        .await
        .unwrap();
    conn.select("INBOX").await.unwrap();

    // 新邮件到达
    let deliver = server.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        deliver.deliver("INBOX", &mail("new@x", "a@x", "New", "b"), &[]);
    });
    let (conn, outcome) = tokio::time::timeout(
        Duration::from_secs(5),
        conn.idle(Duration::from_secs(30), std::future::pending()),
    )
    .await
    .expect("idle should return when mail arrives")
    .unwrap();
    assert_eq!(outcome, IdleOutcome::NewData);

    // 被调用方打断，连接还能继续用
    let (mut conn, outcome) = tokio::time::timeout(
        Duration::from_secs(5),
        conn.idle(
            Duration::from_secs(30),
            tokio::time::sleep(Duration::from_millis(200)),
        ),
    )
    .await
    .expect("idle should return when interrupted")
    .unwrap();
    assert_eq!(outcome, IdleOutcome::Interrupted);
    assert_eq!(conn.list_from(1).await.unwrap().len(), 1);

    // 什么都没发生就超时
    let (conn, outcome) = conn
        .idle(Duration::from_millis(300), std::future::pending())
        .await
        .unwrap();
    assert_eq!(outcome, IdleOutcome::Timeout);
    conn.logout().await;
}
