//! The commands, against a real server listening on a real port.
//!
//! Nothing is mocked: the server is the same router the binary serves, on an
//! in-memory database and a temporary directory. What these prove is that what
//! the command line seals, the server stores without reading, and the command
//! line opens again.

#![expect(
    clippy::expect_used,
    reason = "test fixture; a panic is how it reports"
)]

use std::{net::SocketAddr, path::Path, sync::Arc};

use tempfile::TempDir;
use tessera_cli::{
    Error,
    client::Client,
    get, link, login,
    send::{self, Content, Options},
    shares,
};
use tessera_server::{
    api,
    config::{Config, DatabaseConfig},
    db,
    db::repository::storage_backend::StorageBackendRepository,
    security::secret::Sealer,
    state::{AppState, ServiceStatus, Storage},
    storage::{self, Pool},
};
use url::Url;

/// A server on a port, and its data directory: the key, and `blobs` where its
/// files land.
struct Server {
    url: Url,
    blobs: TempDir,
}

async fn server() -> Server {
    let blobs = tempfile::tempdir().expect("temp dir");
    let mut config = Config {
        database: DatabaseConfig {
            url: "sqlite::memory:".to_owned(),
            ..Default::default()
        },
        ..Config::default()
    };
    config.server.data_dir = blobs.path().display().to_string();
    config.features.encryption = true;

    let database = db::connect(&config.database).await.expect("database");
    db::migrate(&database).await.expect("migrations");
    let assets =
        tessera_server::assets::Source::resolve(&config.server.assets_dir).expect("assets");
    // The way the server starts: a key in the data directory, and the first
    // backend written into the empty table.
    let sealer = Sealer::for_config(&config).expect("a usable key");
    let backends = StorageBackendRepository::new(database.clone(), sealer.clone());
    storage::seed(
        &backends,
        &config.server.data_dir,
        time::OffsetDateTime::now_utc(),
    )
    .await
    .expect("seed");
    let configured = backends.all().await.expect("backends");
    let health = storage::health::Health::default();
    let state = AppState::new(
        Arc::new(config),
        database,
        Storage {
            pool: Pool {
                open: storage::build_all(&configured, &health),
                configured,
                health,
            },
            sealer,
        },
        assets,
    );
    state.set_status(ServiceStatus::Ready);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a port");
    let address = listener.local_addr().expect("an address");
    let app = api::build(state).into_make_service_with_connect_info::<SocketAddr>();
    tokio::spawn(async move { axum::serve(listener, app).await });
    Server {
        url: Url::parse(&format!("http://{address}")).expect("a url"),
        blobs,
    }
}

/// Deterministic bytes that no stretch of could be mistaken for another.
fn content(len: usize, seed: u8) -> Vec<u8> {
    let mut state = u32::from(seed) | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state.to_le_bytes()[0]
        })
        .collect()
}

fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("write a file to send");
    path
}

/// Every stored byte, concatenated, to search for plaintext in.
fn stored(server: &Server) -> Vec<u8> {
    fn walk(dir: &Path, into: &mut Vec<u8>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, into);
            } else {
                into.extend(std::fs::read(&path).unwrap_or_default());
            }
        }
    }
    let mut all = Vec::new();
    walk(&server.blobs.path().join("blobs"), &mut all);
    all
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

async fn fetch(
    link: &str,
    password: Option<&str>,
    output: &Path,
) -> tessera_cli::Result<get::Fetched> {
    let target = link::parse(link, None)?;
    let client = Client::new(target.server.clone(), None)?;
    get::get(
        &client,
        &target.code,
        target.key,
        password,
        output,
        &mut |_, _, _| {},
    )
    .await
}

/// Larger than one upload part, so it goes up in several requests, and not a
/// multiple of a chunk, so the last chunk is short.
const LARGE: usize = 9 * 1024 * 1024 + 12_345;

#[tokio::test]
async fn files_sealed_with_a_key_in_the_link_come_back_whole() {
    let server = server().await;
    let local = tempfile::tempdir().unwrap();
    let large = content(LARGE, 1);
    let small = b"a marker the server must never hold in the clear".to_vec();
    let paths = vec![
        write(local.path(), "large.bin", &large),
        write(local.path(), "notes 報告.txt", &small),
    ];

    let client = Client::new(server.url.clone(), None).unwrap();
    let sent = send::send(
        &client,
        Content::Files(paths),
        Options {
            encrypt: true,
            ..Options::default()
        },
        &mut |_, _, _| {},
    )
    .await
    .expect("send");
    assert!(
        sent.link.contains('#'),
        "the key rides in the link: {}",
        sent.link
    );

    let everything = stored(&server);
    assert!(
        !contains(&everything, &small),
        "plaintext reached the store"
    );
    assert!(
        !contains(&everything, "報告".as_bytes()),
        "a name reached the store"
    );

    let into = tempfile::tempdir().unwrap();
    let fetched = fetch(&sent.link, None, into.path()).await.expect("get");
    assert_eq!(fetched.files.len(), 2);
    assert_eq!(std::fs::read(into.path().join("large.bin")).unwrap(), large);
    assert_eq!(
        std::fs::read(into.path().join("notes 報告.txt")).unwrap(),
        small
    );
}

#[tokio::test]
async fn without_the_key_an_encrypted_share_is_refused_with_a_reason() {
    let server = server().await;
    let local = tempfile::tempdir().unwrap();
    let client = Client::new(server.url.clone(), None).unwrap();
    let sent = send::send(
        &client,
        Content::Files(vec![write(local.path(), "a.txt", b"sealed")]),
        Options {
            encrypt: true,
            ..Options::default()
        },
        &mut |_, _, _| {},
    )
    .await
    .expect("send");

    let bare = link::to_share(&server.url, &sent.code, None);
    let refusal = fetch(&bare, None, local.path()).await.unwrap_err();
    assert!(refusal.to_string().contains("after `#`"), "{refusal}");
}

#[tokio::test]
async fn a_key_from_a_password_opens_only_with_that_password() {
    let server = server().await;
    let local = tempfile::tempdir().unwrap();
    let bytes = content(200_000, 7);
    let client = Client::new(server.url.clone(), None).unwrap();
    let sent = send::send(
        &client,
        Content::Files(vec![write(local.path(), "secret.bin", &bytes)]),
        Options {
            encrypt: true,
            password: Some("correct horse battery staple".to_owned()),
            max_uses: Some(2),
            ..Options::default()
        },
        &mut |_, _, _| {},
    )
    .await
    .expect("send");
    assert!(
        !sent.link.contains('#'),
        "no key in the link: {}",
        sent.link
    );

    let into = tempfile::tempdir().unwrap();
    assert!(matches!(
        fetch(&sent.link, None, into.path()).await,
        Err(Error::PasswordRequired)
    ));
    assert!(matches!(
        fetch(&sent.link, Some("wrong horse battery staple"), into.path()).await,
        Err(Error::Server { status: 401, .. })
    ));
    fetch(
        &sent.link,
        Some("correct horse battery staple"),
        into.path(),
    )
    .await
    .expect("get with the password");
    assert_eq!(
        std::fs::read(into.path().join("secret.bin")).unwrap(),
        bytes
    );
}

#[tokio::test]
async fn a_short_password_is_refused_before_anything_is_sent() {
    let server = server().await;
    let local = tempfile::tempdir().unwrap();
    let client = Client::new(server.url.clone(), None).unwrap();
    let refusal = send::send(
        &client,
        Content::Files(vec![write(local.path(), "a.txt", b"x")]),
        Options {
            encrypt: true,
            password: Some("hunter2".to_owned()),
            ..Options::default()
        },
        &mut |_, _, _| {},
    )
    .await
    .unwrap_err();
    assert!(matches!(refusal, Error::Usage(_)), "{refusal}");
    assert!(
        stored(&server).is_empty(),
        "nothing should have been uploaded"
    );
}

#[tokio::test]
async fn a_sealed_note_comes_back_as_text() {
    let server = server().await;
    let client = Client::new(server.url.clone(), None).unwrap();
    let note = "meet at the usual place\n".as_bytes().to_vec();
    let sent = send::send(
        &client,
        Content::Note(note.clone()),
        Options {
            encrypt: true,
            ..Options::default()
        },
        &mut |_, _, _| {},
    )
    .await
    .expect("send");

    let into = tempfile::tempdir().unwrap();
    let fetched = fetch(&sent.link, None, into.path()).await.expect("get");
    assert_eq!(fetched.note, Some(note));
    assert!(fetched.files.is_empty());
}

/// The same commands without `--encrypt`: an ordinary share with a password.
#[tokio::test]
async fn a_plain_share_with_a_password_still_works() {
    let server = server().await;
    let local = tempfile::tempdir().unwrap();
    let client = Client::new(server.url.clone(), None).unwrap();
    let sent = send::send(
        &client,
        Content::Files(vec![write(local.path(), "plain.txt", b"in the clear")]),
        Options {
            password: Some("1234".to_owned()),
            ..Options::default()
        },
        &mut |_, _, _| {},
    )
    .await
    .expect("send");

    let into = tempfile::tempdir().unwrap();
    assert!(matches!(
        fetch(&sent.link, None, into.path()).await,
        Err(Error::PasswordRequired)
    ));
    fetch(&sent.link, Some("1234"), into.path())
        .await
        .expect("get");
    assert_eq!(
        std::fs::read(into.path().join("plain.txt")).unwrap(),
        b"in the clear"
    );
}

#[tokio::test]
async fn login_gets_a_token_once_the_browser_approves() {
    let server = server().await;
    let http = reqwest::Client::new();
    let registered = http
        .post(server.url.join("/api/v1/auth/register").expect("a url"))
        .json(&serde_json::json!({
            "username": "alice",
            "email": "alice@example.com",
            "password": "a perfectly adequate passphrase",
        }))
        .send()
        .await
        .expect("registered");
    let cookie = registered
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .expect("a session cookie")
        .to_owned();

    let client = Client::new(server.url.clone(), None).expect("a client");
    let started = client
        .start_device_sign_in(&login::client_name())
        .await
        .expect("started");
    let url = login::approval_url(&client, &started).expect("a link");
    assert_eq!(url.path(), "/device");

    let waiting = tokio::spawn({
        let client = client.clone();
        let started = started.clone();
        async move { login::wait_for_approval(&client, &started).await }
    });
    let approved = http
        .post(
            server
                .url
                .join(&format!(
                    "/api/v1/auth/device/{}/approve",
                    started.user_code
                ))
                .expect("a url"),
        )
        .header("cookie", cookie)
        .send()
        .await
        .expect("approved");
    assert_eq!(approved.status(), reqwest::StatusCode::NO_CONTENT);

    let token = waiting.await.expect("joined").expect("a token");
    let session: serde_json::Value = http
        .get(server.url.join("/api/v1/auth/session").expect("a url"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("a session")
        .json()
        .await
        .expect("json");
    assert_eq!(session["username"], "alice");
}

#[tokio::test]
async fn login_with_a_password_gets_a_token_and_ends_the_session() {
    let server = server().await;
    let registered = reqwest::Client::new()
        .post(server.url.join("/api/v1/auth/register").expect("a url"))
        .json(&serde_json::json!({
            "username": "alice",
            "email": "alice@example.com",
            "password": "a perfectly adequate passphrase",
        }))
        .send()
        .await
        .expect("registered");
    assert!(registered.status().is_success());

    let client = Client::new(server.url.clone(), None).expect("a client");
    let token = login::with_password(&client, "alice", "a perfectly adequate passphrase", || {
        Err(Error::Usage("no code was asked for".to_owned()))
    })
    .await
    .expect("a token");
    let account = Client::new(server.url.clone(), Some(token))
        .expect("a client")
        .account()
        .await
        .expect("the account");
    assert_eq!(account.username, "alice");

    let refused = login::with_password(&client, "alice", "not the password", || {
        Err(Error::Usage("no code was asked for".to_owned()))
    })
    .await;
    assert!(matches!(refused, Err(Error::Server { status: 401, .. })));
}

#[tokio::test]
async fn ls_lists_your_shares_and_rm_removes_one() {
    let server = server().await;
    let registered = reqwest::Client::new()
        .post(server.url.join("/api/v1/auth/register").expect("a url"))
        .json(&serde_json::json!({
            "username": "alice",
            "email": "alice@example.com",
            "password": "a perfectly adequate passphrase",
        }))
        .send()
        .await
        .expect("registered");
    assert!(registered.status().is_success());
    let anonymous = Client::new(server.url.clone(), None).expect("a client");
    let token = login::with_password(
        &anonymous,
        "alice",
        "a perfectly adequate passphrase",
        || Err(Error::Usage("no code was asked for".to_owned())),
    )
    .await
    .expect("a token");
    let client = Client::new(server.url.clone(), Some(token)).expect("a client");

    let dir = tempfile::tempdir().expect("temp dir");
    let mut codes = Vec::new();
    for name in ["one.txt", "two.txt"] {
        let path = write(dir.path(), name, b"hello");
        let sent = send::send(
            &client,
            Content::Files(vec![path]),
            Options::default(),
            &mut |_, _, _| {},
        )
        .await
        .expect("send");
        codes.push(link::parse(&sent.link, None).expect("a link").code);
    }

    let mine = shares::all_mine(&client).await.expect("listed");
    let mut listed: Vec<_> = mine.iter().map(|share| share.code.clone()).collect();
    listed.sort();
    let mut sent = codes.clone();
    sent.sort();
    assert_eq!(listed, sent);
    let table = shares::table(&mine, time::OffsetDateTime::now_utc());
    assert!(table.contains("one.txt"), "{table}");

    let gone = codes.first().expect("a code");
    client.delete_share(gone).await.expect("deleted");
    let left = shares::all_mine(&client).await.expect("listed");
    assert_eq!(left.len(), 1);
    assert!(left.iter().all(|share| &share.code != gone));
}
