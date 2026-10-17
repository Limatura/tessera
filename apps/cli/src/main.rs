//! The `tess` command.

use std::{
    io::{IsTerminal as _, Read as _, Write as _},
    path::PathBuf,
    process::ExitCode,
};

use clap::{Parser, Subcommand};
use tessera_cli::{
    Error, Result,
    client::Client,
    config, get, link, login,
    send::{self, Content, Options},
};
use url::Url;

/// Send and fetch Tessera shares.
#[derive(Debug, Parser)]
#[command(name = "tess", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Upload files, or a note from standard input, and print the link.
    Send {
        /// The files to put in one share. `-` reads a note from standard input.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// The instance to upload to. Defaults to the one `tess login` saved.
        #[arg(long, env = "TESSERA_SERVER")]
        server: Option<Url>,
        /// An API token, to upload as an account instead of anonymously.
        /// Defaults to the one `tess login` saved, for that instance.
        #[arg(long, env = "TESSERA_TOKEN", hide_env_values = true)]
        token: Option<String>,
        /// Seal the content on this machine. The server never sees the key:
        /// it is the part of the link after `#`, or with --password, the
        /// password itself.
        #[arg(long)]
        encrypt: bool,
        // Help text as an attribute, not a doc comment: the variable's name is
        // what a person needs to read, and doc lints want it in backticks.
        #[arg(long, help = "Ask for a password, or read it from TESSERA_PASSWORD")]
        password: bool,
        /// How many hours the share lasts.
        #[arg(long, value_name = "HOURS")]
        expires: Option<u64>,
        /// How many times it may be retrieved.
        #[arg(long, value_name = "N")]
        max_uses: Option<u32>,
    },
    /// Download a share, from its link or from its code with --server.
    Get {
        /// The whole link, or the retrieval code.
        link: String,
        /// The instance, when only a code is given. Defaults to the one
        /// `tess login` saved.
        #[arg(long, env = "TESSERA_SERVER")]
        server: Option<Url>,
        /// The directory to write files into. A note goes to standard output.
        #[arg(short, long, default_value = ".")]
        output: PathBuf,
    },
    /// Sign in to an instance by approving this machine in a browser, and
    /// save it as the default for `send` and `get`.
    Login {
        /// The instance to sign in to.
        server: Url,
        /// Only print the link, without trying to open a browser.
        #[arg(long)]
        no_browser: bool,
        /// Sign in with a password here instead, when there is no browser
        /// anywhere to approve from.
        #[arg(long, conflicts_with_all = ["no_browser", "with_token"])]
        password: bool,
        /// Read an API token from standard input instead, for scripts and CI.
        #[arg(long, conflicts_with = "no_browser")]
        with_token: bool,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    match run(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // Nowhere left to report a failure to write the failure.
            writeln!(std::io::stderr(), "tess: {error}").ok();
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Send {
            paths,
            server,
            token,
            encrypt,
            password,
            expires,
            max_uses,
        } => {
            let content = if paths.iter().any(|path| path.as_os_str() == "-") {
                if paths.len() > 1 {
                    return Err(Error::Usage(
                        "a note from standard input is a share of its own; send files separately"
                            .to_owned(),
                    ));
                }
                let mut note = Vec::new();
                std::io::stdin().read_to_end(&mut note)?;
                Content::Note(note)
            } else {
                Content::Files(paths)
            };
            let password = if password {
                Some(new_password()?)
            } else {
                None
            };
            let client = sending_client(server, token)?;
            let sent = send::send(
                &client,
                content,
                Options {
                    encrypt,
                    password,
                    expires_in_hours: expires,
                    max_uses,
                },
                &mut progress,
            )
            .await?;
            clear_progress();
            writeln!(std::io::stdout(), "{}", sent.link)?;
            Ok(())
        }
        Command::Get {
            link,
            server,
            output,
        } => {
            let server = match server {
                Some(server) => Some(server),
                None => config::load(&config::default_path()?)?.server,
            };
            let target = link::parse(&link, server.as_ref())?;
            let client = Client::new(target.server.clone(), None)?;
            let mut password = std::env::var("TESSERA_PASSWORD").ok();
            let fetched = loop {
                match get::get(
                    &client,
                    &target.code,
                    target.key.clone(),
                    password.as_deref(),
                    &output,
                    &mut progress,
                )
                .await
                {
                    Err(Error::PasswordRequired) if password.is_none() => {
                        password = Some(rpassword::prompt_password("Password: ")?);
                    }
                    other => break other?,
                }
            };
            clear_progress();
            let mut stdout = std::io::stdout();
            if let Some(note) = fetched.note {
                stdout.write_all(&note)?;
            }
            for file in fetched.files {
                writeln!(stdout, "{}", file.display())?;
            }
            Ok(())
        }
        Command::Login {
            server,
            no_browser,
            password,
            with_token,
        } => log_in(server, no_browser, password, with_token).await,
    }
}

/// A client for `send`: flags and environment first, then what `tess login`
/// saved. The saved token goes only to the instance it came from.
fn sending_client(server: Option<Url>, token: Option<String>) -> Result<Client> {
    let saved = config::load(&config::default_path()?)?;
    let server = server.or_else(|| saved.server.clone()).ok_or_else(|| {
        Error::Usage("no instance to send to; give --server, or run `tess login` first".to_owned())
    })?;
    let token = token.or_else(|| saved.token_for(&server).map(str::to_owned));
    Client::new(server, token)
}

/// Gets a token one of three ways, checks it, and saves it.
async fn log_in(server: Url, no_browser: bool, password: bool, with_token: bool) -> Result<()> {
    let client = Client::new(server.clone(), None)?;
    let token = if with_token {
        let mut token = String::new();
        std::io::stdin().read_to_string(&mut token)?;
        token.trim().to_owned()
    } else if password {
        sign_in_with_password(&client).await?
    } else {
        sign_in_with_browser(&client, no_browser).await?
    };
    // Asked once more with the token itself, so a pasted typo is
    // reported here and not on the first `send`.
    let account = Client::new(server.clone(), Some(token.clone()))?
        .account()
        .await
        .map_err(|error| match error {
            Error::Server { status: 401, .. } => {
                Error::Usage("that token is not accepted by this instance".to_owned())
            }
            other => other,
        })?;
    save_sign_in(server, token, &account.username)
}

/// Shows the link and code, and waits for someone to approve them.
async fn sign_in_with_browser(client: &Client, no_browser: bool) -> Result<String> {
    let started = client.start_device_sign_in(&login::client_name()).await?;
    let url = login::approval_url(client, &started)?;
    let mut stderr = std::io::stderr();
    writeln!(
        stderr,
        "Open {url}\nand check that it shows the code {}.",
        started.user_code
    )?;
    if !no_browser && open_browser(&url) {
        writeln!(stderr, "Opened it in your browser.")?;
    }
    writeln!(stderr, "Waiting for approval...")?;
    login::wait_for_approval(client, &started).await
}

/// Asks for a username and password, and a code when the account has one.
async fn sign_in_with_password(client: &Client) -> Result<String> {
    let identifier = prompt("Username or email: ")?;
    let password = rpassword::prompt_password("Password: ")?;
    login::with_password(client, &identifier, &password, || {
        prompt("Code from your authenticator app: ")
    })
    .await
    .map_err(|error| match error {
        // The server does not say which accounts have no password, so this
        // cannot either; it can say where those accounts go instead.
        Error::Server {
            code: Some(code), ..
        } if code == "invalid_credentials" => Error::Usage(
            "wrong username or password. An account that signs in with a passkey or a \
             provider has no password here; run `tess login` without --password"
                .to_owned(),
        ),
        other => other,
    })
}

/// One line typed on the terminal, after `question` on standard error.
fn prompt(question: &str) -> Result<String> {
    let mut stderr = std::io::stderr();
    write!(stderr, "{question}")?;
    stderr.flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(line.trim().to_owned())
}

/// Saves the instance and its token as the defaults.
fn save_sign_in(server: Url, token: String, username: &str) -> Result<()> {
    let path = config::default_path()?;
    let mut saved = config::load(&path)?;
    let message = format!(
        "Signed in to {server} as {username}. Saved to {}.",
        path.display()
    );
    saved.server = Some(server);
    saved.token = Some(token);
    config::save(&path, &saved)?;
    writeln!(std::io::stderr(), "{message}")?;
    Ok(())
}

/// Tries to open `url` in a browser, and says whether something took it.
///
/// Over SSH, or on a server with no desktop, nothing will; the link printed
/// above is the way then.
fn open_browser(url: &Url) -> bool {
    let url = url.as_str();
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        let mut command = std::process::Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    } else {
        if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
            return false;
        }
        std::process::Command::new("xdg-open")
    };
    command
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// A password for a new share: from the environment, or typed twice.
fn new_password() -> Result<String> {
    if let Ok(password) = std::env::var("TESSERA_PASSWORD") {
        return Ok(password);
    }
    let first = rpassword::prompt_password("Password: ")?;
    let again = rpassword::prompt_password("Again: ")?;
    if first != again {
        return Err(Error::Usage("the two passwords differ".to_owned()));
    }
    Ok(first)
}

/// One line on standard error, rewritten in place, when it is a terminal.
fn progress(name: &str, done: u64, total: u64) {
    let mut stderr = std::io::stderr();
    if !stderr.is_terminal() {
        return;
    }
    let percent = done.saturating_mul(100).checked_div(total).unwrap_or(100);
    // Progress is a courtesy; a terminal that will not take it changes nothing.
    write!(stderr, "\r\x1b[2K{name}: {percent}%").ok();
    stderr.flush().ok();
}

fn clear_progress() {
    let mut stderr = std::io::stderr();
    if stderr.is_terminal() {
        write!(stderr, "\r\x1b[2K").ok();
    }
}
