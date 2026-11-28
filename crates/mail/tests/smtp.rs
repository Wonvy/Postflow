//! 对着本机的假发信服务器发信，以及把发出的信存进假 IMAP 服务器的「已发送」。

use shibapost_mail::{Auth, Connection, Error, Mailbox, Outgoing, Security, ServerConfig, smtp};
use shibapost_testkit::{FakeImap, FakeSmtp};

fn config(port: u16, user: &str) -> ServerConfig {
    ServerConfig {
        host: "127.0.0.1".into(),
        port,
        security: Security::None,
        username: user.into(),
    }
}

fn mb(address: &str) -> Mailbox {
    Mailbox {
        name: None,
        address: Some(address.into()),
    }
}

fn letter() -> Outgoing {
    Outgoing {
        from: Mailbox {
            name: Some("Me".into()),
            address: Some("me@example.com".into()),
        },
        to: vec![mb("ada@example.com")],
        cc: vec![mb("bob@example.com")],
        bcc: vec![mb("secret@example.com")],
        subject: "Lunch?".into(),
        text: "Hi Ada,\n.\n..two dots\n\nSee you".into(),
        ..Outgoing::default()
    }
}

#[tokio::test]
async fn sends_with_a_password_and_keeps_bcc_off_the_headers() {
    let server = FakeSmtp::start("me@example.com", "secret").await;
    let built = shibapost_mail::build(&letter(), 1_700_000_000).unwrap();
    smtp::send(
        &config(server.port, "me@example.com"),
        &Auth::Password("secret".into()),
        &built.sender,
        &built.recipients,
        &built.raw,
    )
    .await
    .unwrap();

    let delivered = server.delivered();
    assert_eq!(delivered.len(), 1);
    let mail = &delivered[0];
    assert_eq!(mail.user, "me@example.com");
    assert_eq!(mail.from, "me@example.com");
    assert_eq!(
        mail.to,
        ["ada@example.com", "bob@example.com", "secret@example.com"]
    );
    // 以「.」开头的行在路上多写了一个点，到了服务器那边要原样还原
    assert_eq!(mail.raw, built.raw);
    let text = String::from_utf8_lossy(&mail.raw);
    assert!(!text.contains("secret@example.com"));
}

#[tokio::test]
async fn oauth_tokens_sign_in_too() {
    let server = FakeSmtp::start("me@example.com", "token-1").await;
    let built = shibapost_mail::build(&letter(), 0).unwrap();
    let cfg = config(server.port, "me@example.com");
    smtp::send(
        &cfg,
        &Auth::OAuth2("token-1".into()),
        &built.sender,
        &built.recipients,
        &built.raw,
    )
    .await
    .unwrap();
    assert_eq!(server.delivered().len(), 1);

    let err = smtp::check(&cfg, &Auth::OAuth2("stale".into()))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Auth(_)), "{err:?}");
}

#[tokio::test]
async fn wrong_password_and_rejected_recipients_are_reported() {
    let server = FakeSmtp::start("me@example.com", "secret").await;
    let cfg = config(server.port, "me@example.com");
    let err = smtp::check(&cfg, &Auth::Password("nope".into()))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Auth(_)), "{err:?}");
    assert_eq!(err.detail(), "Username and Password not accepted.");

    smtp::check(&cfg, &Auth::Password("secret".into()))
        .await
        .unwrap();

    let mut out = letter();
    out.cc = vec![mb("reject@example.com")];
    let built = shibapost_mail::build(&out, 0).unwrap();
    let err = smtp::send(
        &cfg,
        &Auth::Password("secret".into()),
        &built.sender,
        &built.recipients,
        &built.raw,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, Error::Server(_)), "{err:?}");
    assert!(err.detail().starts_with("reject@example.com: "), "{err:?}");
    assert!(server.delivered().is_empty());
}

#[tokio::test]
async fn nothing_listening_is_a_connect_error() {
    let err = smtp::check(&config(9, "me"), &Auth::Password("x".into()))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Connect(_)), "{err:?}");
}

#[tokio::test]
async fn sent_copies_go_to_the_marked_mailbox_once() {
    let server = FakeImap::start("me@example.com", "secret").await;
    server.add_mailbox("Sent Items", Some("\\Sent"));
    let mut conn = Connection::connect(
        &config(server.port, "me@example.com"),
        &Auth::Password("secret".into()),
    )
    .await
    .unwrap();
    conn.select("INBOX").await.unwrap();

    let built = shibapost_mail::build(&letter(), 0).unwrap();
    let sent = conn.sent_mailbox().await.unwrap();
    assert_eq!(sent, "Sent Items");
    assert!(
        !conn
            .contains_message_id(&sent, &built.message_id)
            .await
            .unwrap()
    );
    conn.append(&sent, &built.raw).await.unwrap();
    assert!(
        conn.contains_message_id(&sent, &built.message_id)
            .await
            .unwrap()
    );
    // 找完回到原来的邮箱
    assert_eq!(conn.selected(), Some("INBOX"));
    assert_eq!(server.count("Sent Items"), 1);
    assert_eq!(
        server.flags("Sent Items", &built.message_id),
        Some(vec!["\\Seen".to_string()])
    );
}

#[tokio::test]
async fn a_sent_mailbox_is_created_when_there_is_none() {
    let server = FakeImap::start("me@example.com", "secret").await;
    let mut conn = Connection::connect(
        &config(server.port, "me@example.com"),
        &Auth::Password("secret".into()),
    )
    .await
    .unwrap();
    assert_eq!(conn.sent_mailbox().await.unwrap(), "Sent");
    conn.append("Sent", b"Subject: hi\r\n\r\nhello\r\n")
        .await
        .unwrap();
    assert_eq!(server.count("Sent"), 1);
}
