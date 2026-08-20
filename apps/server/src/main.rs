//! Process entry point.
//!
//! Start-up order is the whole point of this file: configuration is loaded and
//! validated *before* anything is opened, so a bad value fails the process
//! immediately instead of surfacing as a 500 on the first request that touches it.
//!
//! `anyhow` is used here and nowhere else — this is the boundary where a typed
//! error stops being actionable and becomes a message for an operator.

use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context as _, bail};
use tessera_server::{
    api, auth, cli,
    config::Config,
    db,
    db::repository::{role::RoleRepository, user::UserRepository},
    maintenance,
    state::{AppState, ServiceStatus},
    storage, telemetry,
};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio_util::sync::CancellationToken;

/// How much of a health-probe response to read before giving up on finding a
/// status line. A well-formed one is about 15 bytes.
const STATUS_LINE_CAP: usize = 256;

// Usage and argument errors go to the terminal, not to the log: at the point
// they happen there is no telemetry subscriber installed, and an operator who
// mistyped a flag is reading the terminal anyway.
#[expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "the command line answers on the command line"
)]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Configuration. Nothing is opened before this succeeds.
    let config = Config::load().context("configuration is not usable")?;

    // Read after the configuration, because every subcommand needs it and a bad
    // config should say so once rather than per command.
    let command = match cli::parse(std::env::args().skip(1)) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };

    match command {
        cli::Command::Help => {
            print!("{}", cli::USAGE);
            return Ok(());
        }
        // The distroless runtime image has no shell and no curl, so `HEALTHCHECK`
        // re-executes this binary instead. It shares the configuration, which
        // means the probe always targets the port the server actually listens on.
        cli::Command::HealthCheck => return probe_health(&config).await,
        cli::Command::ResetPassword { username, source } => {
            return cli::reset_password(&config, &username, source).await;
        }
        cli::Command::Invite {
            uses,
            expires_in_days,
            note,
        } => return cli::mint_invite(&config, uses, expires_in_days, note).await,
        cli::Command::Serve => {}
    }

    // 2. Logging, so every step below is observable.
    telemetry::init(&config.telemetry).context("could not install the tracing subscriber")?;

    // 3. A commented reference file, for operators who would rather edit YAML
    // than compose environment variables. Best-effort by design: a read-only
    // filesystem is a fine way to run this, and the environment configures it
    // completely on its own.
    match Config::write_template_if_absent(&Config::discover_file()) {
        Ok(Some(path)) => tracing::info!(
            path = %path.display(),
            "wrote a reference configuration file; every line is commented out"
        ),
        Ok(None) => {}
        Err(error) => tracing::warn!(?error, "could not render the reference configuration"),
    }
    tracing::info!(
        version = tessera_server::VERSION,
        build_rev = tessera_server::BUILD_REV,
        docs = config.features.docs,
        // The largest thing this process reserves on purpose, and the one an
        // operator sizing a container needs to know about.
        hasher_peak_mib = auth::password::peak_memory_bytes() / (1024 * 1024),
        "starting tessera"
    );

    warn_about_settings_that_bite(&config);

    // 4. Dependencies.
    let db = db::connect(&config.database)
        .await
        .with_context(|| format!("could not open the database at `{}`", config.database.url))?;
    if config.database.run_migrations {
        db::migrate(&db)
            .await
            .context("could not apply migrations")?;
        tracing::info!(backend = db::backend_name(&db), "migrations applied");
    }

    let (store, multipart) = open_store(&config)?;

    // Before the listener binds, so an instance exposed to the internet never
    // spends a moment with no administrator for a passer-by to claim.
    if config.features.accounts {
        auth::bootstrap::ensure_admin(
            &UserRepository::new(db.clone()),
            &RoleRepository::new(db.clone()),
            &config.auth,
        )
        .await
        .context("could not create the first administrator")?;
    }

    // 5. Listener. Bound before announcing readiness so a port clash is fatal.
    let addr = SocketAddr::new(config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("could not bind {addr}"))?;
    let local_addr = listener.local_addr().unwrap_or(addr);

    let assets = resolve_assets(&config.server.assets_dir)?;

    let config = Arc::new(config);
    let state = AppState::new(Arc::clone(&config), db.clone(), store, multipart, assets);
    let app = api::build(state.clone());

    state.set_status(ServiceStatus::Ready);
    tracing::info!(address = %local_addr, "listening");
    if config.features.docs {
        // Worth a line of its own, because "the docs page is blank" is otherwise
        // a confusing thing to debug — and the cause is on the *reader's* side.
        // This server never fetches the renderer; the browser does.
        tracing::info!(
            url = %format_args!("http://{local_addr}{}", api::DOCS_PATH),
            "API reference enabled; the browser opening it loads the renderer from a CDN"
        );
    }

    let shutdown = CancellationToken::new();
    let signal_task = tokio::spawn(watch_for_shutdown(shutdown.clone()));
    let sweep_task = tokio::spawn(maintenance::sweep_expired(state.clone(), shutdown.clone()));

    let drain = shutdown.clone();
    let state_for_drain = state.clone();
    // Served exactly as `api::build` returns it — no outer service wrapper — so
    // that the integration tests exercise the same stack this process does.
    //
    // `with_connect_info` is what makes the peer address reachable from a
    // handler. Login throttling needs it: without the address, the only key
    // available is the account name, and keying on that alone lets anyone lock a
    // known user out by guessing at them.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        drain.cancelled().await;
        // Flip to draining first: an orchestrator polling `/health` sees 503
        // and stops routing while in-flight requests finish.
        state_for_drain.set_status(ServiceStatus::Draining);
        tracing::info!("draining in-flight requests");
    })
    .await
    .context("the HTTP server stopped unexpectedly")?;

    signal_task.abort();
    // Not aborted: the sweep watches the same token, so it is already returning,
    // and letting it finish means the pool is never closed mid-`DELETE`. A join
    // error here means the task panicked, which the sweep's own error handling
    // makes unreachable and which is not worth failing shutdown over.
    if let Err(error) = sweep_task.await {
        tracing::warn!(?error, "the housekeeping sweep did not stop cleanly");
    }
    // Closing the pool checkpoints the SQLite WAL, so the next start-up does not
    // have to replay it.
    db::close(db).await;
    tracing::info!("stopped");
    Ok(())
}

/// Issues `GET /health` against the locally configured listener and succeeds
/// only on a 200.
///
/// Hand-rolled over a raw socket rather than pulling in an HTTP client: a probe
/// that needs `reqwest` would add a TLS stack and several megabytes to an image
/// whose entire selling point is that it is small.
async fn probe_health(config: &Config) -> anyhow::Result<()> {
    // `0.0.0.0` / `::` mean "every interface"; they are not connectable targets.
    let host = match config.server.host {
        IpAddr::V4(v4) if v4.is_unspecified() => IpAddr::from([127, 0, 0, 1]),
        IpAddr::V6(v6) if v6.is_unspecified() => IpAddr::from([0, 0, 0, 0, 0, 0, 0, 1]),
        other => other,
    };
    let addr = SocketAddr::new(host, config.server.port);

    let status_line = tokio::time::timeout(Duration::from_secs(2), async {
        let mut socket = tokio::net::TcpStream::connect(addr).await?;
        socket
            .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await?;

        // Only the status line matters, but TCP is free to deliver it in
        // fragments, so read until a newline shows up rather than trusting one
        // `read` to have produced the whole thing.
        let mut head = Vec::with_capacity(STATUS_LINE_CAP);
        let mut chunk = [0_u8; 64];
        while head.len() < STATUS_LINE_CAP && !head.contains(&b'\n') {
            let read = socket.read(&mut chunk).await?;
            if read == 0 {
                break; // peer closed
            }
            head.extend_from_slice(chunk.get(..read).unwrap_or_default());
        }
        Ok::<_, std::io::Error>(String::from_utf8_lossy(&head).into_owned())
    })
    .await
    .with_context(|| format!("health probe to {addr} timed out"))?
    .with_context(|| format!("health probe to {addr} failed"))?;

    if status_line.starts_with("HTTP/1.1 200") {
        Ok(())
    } else {
        let reported = status_line.lines().next().unwrap_or("<no response>");
        bail!("health probe to {addr} returned `{reported}`");
    }
}

/// Resolves when the process is asked to stop, by `Ctrl-C` or by `SIGTERM`
/// (which is what `docker stop` and systemd send).
async fn watch_for_shutdown(token: CancellationToken) {
    let ctrl_c = async {
        if tokio::signal::ctrl_c().await.is_err() {
            // No console attached: fall back to SIGTERM alone.
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::warn!(?error, "could not install the SIGTERM handler");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => tracing::info!("received Ctrl-C"),
        () = terminate => tracing::info!("received SIGTERM"),
    }

    token.cancel();
}

/// Says something about the settings that are legal, quiet and dangerous.
///
/// Warnings rather than refusals: each of these is the right setting for
/// somebody. What they have in common is that nothing goes wrong at start-up and
/// nothing goes wrong in testing — the cost only shows up when a stranger finds
/// the instance.
fn warn_about_settings_that_bite(config: &Config) {
    // The combination that turns an instance into free storage for anyone who
    // can reach it. Neither half is wrong on its own — anonymous upload is the
    // point of the product, and no quota is right when the operator is the only
    // user — so this warns rather than refuses. `tessera.public.yaml` sets both.
    if config.features.anonymous_upload && config.storage.max_total_bytes == 0 {
        tracing::warn!(
            "anonymous uploads are enabled and `storage.max_total_bytes` is unset: anyone who \
         can reach this instance can fill its disk. Set a limit, or turn off \
         `features.anonymous_upload`, if this is reachable by people you do not know."
        );
    }

    warn_about_proxy_trust(&config.server);
}

/// The settings that decide whether `X-Forwarded-For` may be believed.
///
/// Its own function because getting this wrong does not look like a mistake: the
/// service starts, the logs fill with plausible addresses, and every throttle in
/// it counts against something the caller chose. Two hundred sign-in attempts,
/// none refused, against an instance that blocks the eleventh.
fn warn_about_proxy_trust(server: &tessera_server::config::ServerConfig) {
    if server.trusted_proxies.is_empty() && server.trusted_proxy {
        tracing::warn!(
            "`server.trusted_proxy` is deprecated and believes whatever connects to this \
         service. Replace it with `server.trusted_proxies`, naming the proxy — \
         `[\"127.0.0.1\", \"::1\"]` for one on this host, `[\"10.0.0.0/8\"]` for a container \
         network. Anyone who can reach this port directly can otherwise pick the address \
         their failed sign-ins are counted against."
        );
    }
    if server.proxy_trust().covers_everything() {
        tracing::warn!(
            "`server.trusted_proxies` contains a block covering every address, so every \
             `X-Forwarded-For` entry counts as one of ours and the whole header is believed. \
             Anyone who can reach this service can then pick the address their failed sign-ins \
             are counted against. Name the proxy instead."
        );
    }
    if !server.trusted_proxies.is_empty() && server.trusted_proxy {
        tracing::warn!(
            "both `server.trusted_proxies` and the deprecated `server.trusted_proxy` are set; \
         the list is used and the boolean is ignored"
        );
    }
}

/// Where the frontend comes from, decided before anything is served.
///
/// Fatal when it cannot be: a mistyped `assets_dir` should be answered while
/// somebody is still looking at the terminal, not by a blank page later.
fn resolve_assets(configured: &str) -> anyhow::Result<tessera_server::assets::Source> {
    let assets = tessera_server::assets::Source::resolve(configured)
        .map_err(|reason| anyhow::anyhow!("server.assets_dir — {reason}"))?;
    if let tessera_server::assets::Source::Directory(root) = &assets {
        tracing::info!(directory = %root.display(), "serving the frontend from disk");
    }
    Ok(assets)
}

/// Opens the blob store, and the same store as a multipart one when it is.
///
/// Split out so `main` stays under the line limit; the pair is what
/// [`AppState`] wants and what decides whether this instance can hold a
/// resumable upload open across requests.
fn open_store(config: &Config) -> anyhow::Result<storage::Stores> {
    storage::build_with_multipart(&config.storage).context("could not open the object store")
}
