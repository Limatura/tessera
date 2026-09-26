//! `tessera-dev`: stand-ins for the outside services Tessera talks to.
//!
//! Signing in with an identity provider, with GitHub, and scanning uploads all
//! need a real service and a real secret to try out. This starts local fakes
//! of each on loopback ports and writes the settings that point a server at
//! them to `.dev/fakes.env`, so trying any of those features is one command
//! and no registration anywhere.
//!
//! Every fake answers what a real one would, but checks nothing: any client
//! secret works, and the sign-in page lets you be whoever you type in. None of
//! this is for anything but a development machine.
//!
//! ```text
//! pnpm dev:fakes          # this, in one terminal
//! pnpm server:dev:fakes   # the server, with .dev/fakes.env loaded, in another
//! ```

mod github;
mod oidc;
mod page;
mod scanner;

use std::{fmt::Write as _, net::SocketAddr, path::Path};

/// Where each fake listens. Fixed rather than picked by the system, so the
/// settings written for one run stay right for the next.
const OIDC_PORT: u16 = 9101;
const GITHUB_PORT: u16 = 9102;
const SCANNER_PORT: u16 = 9103;

/// What every fake accepts as a client, and what the server is told to send.
const CLIENT_ID: &str = "tessera-dev";
const CLIENT_SECRET: &str = "tessera-dev-secret";

/// Where the settings go, relative to where the command is run.
const ENV_FILE: &str = ".dev/fakes.env";

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let public_url = std::env::args()
        .skip_while(|arg| arg != "--public-url")
        .nth(1)
        .unwrap_or_else(|| "http://localhost:8787".to_owned());

    let oidc = format!("http://127.0.0.1:{OIDC_PORT}");
    let github = format!("http://127.0.0.1:{GITHUB_PORT}");
    let scanner = format!("http://127.0.0.1:{SCANNER_PORT}/scan");

    let settings = [
        ("TESSERA_SERVER__PUBLIC_URL", public_url.as_str()),
        ("TESSERA_AUTH__OIDC__ISSUER", oidc.as_str()),
        ("TESSERA_AUTH__OIDC__CLIENT_ID", CLIENT_ID),
        ("TESSERA_AUTH__OIDC__CLIENT_SECRET", CLIENT_SECRET),
        ("TESSERA_AUTH__GITHUB__CLIENT_ID", CLIENT_ID),
        ("TESSERA_AUTH__GITHUB__CLIENT_SECRET", CLIENT_SECRET),
        ("TESSERA_AUTH__GITHUB__BASE_URL", github.as_str()),
        ("TESSERA_SCAN__URL", scanner.as_str()),
        ("TESSERA_SCAN__AUTHORIZATION", "Bearer tessera-dev"),
    ];
    let mut file = String::from(
        "# Written by tessera-dev. Points a server at the local fakes; see apps/dev.\n",
    );
    // Quoted, because one value has a space in it and the file is read with
    // the shell's `.`. None of them has a quote of its own.
    for (name, value) in settings {
        writeln!(file, "{name}='{value}'").ok();
    }
    if let Some(parent) = Path::new(ENV_FILE).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(ENV_FILE, file)?;

    let listen = |port| tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port)));
    let (oidc_listener, github_listener, scanner_listener) = (
        listen(OIDC_PORT).await?,
        listen(GITHUB_PORT).await?,
        listen(SCANNER_PORT).await?,
    );
    tokio::spawn(async move { axum::serve(oidc_listener, oidc::router(oidc.clone())).await });
    tokio::spawn(async move { axum::serve(github_listener, github::router()).await });
    tokio::spawn(async move { axum::serve(scanner_listener, scanner::router()).await });

    announce(&public_url);
    tokio::signal::ctrl_c().await
}

#[expect(
    clippy::print_stdout,
    reason = "a development tool's instructions belong on the terminal"
)]
fn announce(public_url: &str) {
    println!(
        "\
Fakes running. Ctrl-C stops them.

  identity provider   http://127.0.0.1:{OIDC_PORT}
  GitHub              http://127.0.0.1:{GITHUB_PORT}
  malware scanner     http://127.0.0.1:{SCANNER_PORT}/scan

Settings written to {ENV_FILE}. Start the server with them:

  pnpm server:dev:fakes

It expects to be reached at {public_url}; pass --public-url to change that.

Settings saved in the admin panel win over these. If a sign-in goes somewhere
else, clear the auth.oidc.* or auth.github.* keys there first.

The scanner flags the EICAR test string and passes everything else. Upload it
to see a share switched off:
  X5O!P%@AP[4\\PZX54(P^)7CC)7}}$EICAR-STANDARD-ANTIVIRUS-TEST-FILE!$H+H*"
    );
}
