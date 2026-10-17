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
    config, get, link, login, output,
    send::{self, Content, Options},
    shares,
    style::{self, DIM, ERROR, LINK, OK, SPINNER, STRONG},
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
        /// Also draw the link as a QR code, for a phone to scan.
        #[arg(long)]
        qr: bool,
        /// Also put the link on the clipboard. Over SSH, the clipboard of the
        /// machine the terminal runs on, if the terminal allows it.
        #[arg(long)]
        copy: bool,
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
    /// List your own shares: code, expiry, retrievals left, name.
    Ls {
        /// The instance. Defaults to the one `tess login` saved.
        #[arg(long, env = "TESSERA_SERVER")]
        server: Option<Url>,
        /// An API token. Defaults to the one `tess login` saved.
        #[arg(long, env = "TESSERA_TOKEN", hide_env_values = true)]
        token: Option<String>,
    },
    /// Delete one of your own shares, by its code or its link.
    Rm {
        /// The code, or the whole link.
        share: String,
        /// The instance, when only a code is given. Defaults to the one
        /// `tess login` saved.
        #[arg(long, env = "TESSERA_SERVER")]
        server: Option<Url>,
        /// An API token. Defaults to the one `tess login` saved.
        #[arg(long, env = "TESSERA_TOKEN", hide_env_values = true)]
        token: Option<String>,
    },
    /// Print a completion script for a shell, to source from its startup file.
    Completions {
        /// The shell to write it for.
        shell: clap_complete::Shell,
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
            anstream::eprintln!("{ERROR}error:{ERROR:#} {error}");
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
            qr,
            copy,
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
            hand_on(&sent.link, qr, copy)?;
            Ok(())
        }
        Command::Get {
            link,
            server,
            output,
        } => fetch(&link, server, &output).await,
        Command::Ls { server, token } => list(server, token).await,
        Command::Rm {
            share,
            server,
            token,
        } => remove(&share, server, token).await,
        Command::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut <Cli as clap::CommandFactory>::command(),
                "tess",
                &mut std::io::stdout(),
            );
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

/// Downloads a share, asking for its password if it has one.
async fn fetch(link: &str, server: Option<Url>, output: &std::path::Path) -> Result<()> {
    let server = match server {
        Some(server) => Some(server),
        None => config::load(&config::default_path()?)?.server,
    };
    let target = link::parse(link, server.as_ref())?;
    let client = Client::new(target.server.clone(), None)?;
    let mut password = std::env::var("TESSERA_PASSWORD").ok();
    let fetched = loop {
        match get::get(
            &client,
            &target.code,
            target.key.clone(),
            password.as_deref(),
            output,
            &mut progress,
        )
        .await
        {
            Err(Error::PasswordRequired) if password.is_none() => {
                password = Some(rpassword::prompt_password("Password: ")?);
            }
            Err(Error::Server { status: 404, .. }) => {
                return Err(Error::Usage(format!(
                    "no share with the code {}; it may have expired or been used up",
                    target.code
                )));
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

/// The QR code on standard error and the clipboard, as asked: standard output
/// carries only the link, so it can still be piped.
fn hand_on(link: &str, qr: bool, copy: bool) -> Result<()> {
    if qr {
        // Straight to the terminal, not through `anstream`: the colours are
        // what makes it scan, so they stay even with `NO_COLOR`.
        write!(std::io::stderr(), "\n{}\n", output::qr(link)?)?;
    }
    if copy {
        match output::copy(link)? {
            output::Copied::Here => anstream::eprintln!("{OK}✓{OK:#} Copied to the clipboard"),
            output::Copied::ToTerminal => anstream::eprintln!(
                "{OK}✓{OK:#} Copied through the terminal {DIM}(if your terminal allows it){DIM:#}"
            ),
        }
    }
    Ok(())
}

/// Prints the account's shares.
async fn list(server: Option<Url>, token: Option<String>) -> Result<()> {
    let client = signed_in_client(server, token)?;
    let mine = shares::all_mine(&client).await?;
    if mine.is_empty() {
        anstream::eprintln!("{DIM}No shares yet.{DIM:#}");
        return Ok(());
    }
    anstream::print!("{}", shares::table(&mine, time::OffsetDateTime::now_utc()));
    Ok(())
}

/// Deletes one of the account's shares, named by its code or its link.
async fn remove(share: &str, server: Option<Url>, token: Option<String>) -> Result<()> {
    // A link names its own instance; only a bare code needs one.
    let server = if Url::parse(share).is_ok() {
        None
    } else {
        server
    };
    let client = signed_in_client(server, token)?;
    let target = link::parse(share, Some(client.server()))?;
    if target.server.origin() != client.server().origin() {
        return Err(Error::Usage(format!(
            "that link is on {}, and you are signed in to {}",
            target.server,
            client.server()
        )));
    }
    client.delete_share(&target.code).await?;
    anstream::eprintln!("{OK}✓{OK:#} Deleted {STRONG}{}{STRONG:#}", target.code);
    Ok(())
}

/// A client for `ls` and `rm`, which act as an account and so need a token.
fn signed_in_client(server: Option<Url>, token: Option<String>) -> Result<Client> {
    let client = sending_client(server, token)?;
    if !client.signed_in() {
        return Err(Error::Usage(format!(
            "not signed in to {}; run `tess login {}` first",
            client.server(),
            client.server()
        )));
    }
    Ok(client)
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
    anstream::eprintln!(
        "\nOpen the link and check that it shows the same code:\n\n  \
         {DIM}Code{DIM:#}  {STRONG}{}{STRONG:#}\n  {DIM}Link{DIM:#}  {LINK}{url}{LINK:#}\n",
        started.user_code
    );
    let stderr = std::io::stderr();
    if !no_browser && open_browser(&url) {
        anstream::eprintln!("{DIM}Opened it in your browser.{DIM:#}\n");
    } else if stderr.is_terminal() {
        // No browser here, so the approving is done on another device, and a
        // phone would rather scan than be typed into.
        anstream::eprintln!("{DIM}Or scan it with your phone:{DIM:#}\n");
        writeln!(&stderr, "{}", output::qr(url.as_str())?)?;
    }
    spin(
        "Waiting for approval",
        login::wait_for_approval(client, &started),
    )
    .await
}

/// Shows a spinner beside `label` on a terminal until `work` is done.
async fn spin<T>(label: &str, work: impl Future<Output = T>) -> T {
    let mut stderr = std::io::stderr();
    if !stderr.is_terminal() {
        return work.await;
    }
    let mut work = std::pin::pin!(work);
    let mut ticks = tokio::time::interval(std::time::Duration::from_millis(100));
    let mut frames = SPINNER.iter().cycle();
    loop {
        tokio::select! {
            done = &mut work => {
                write!(stderr, "\r\x1b[2K").ok();
                return done;
            }
            _ = ticks.tick() => {
                let frame = frames.next().copied().unwrap_or(' ');
                anstream::eprint!("\r\x1b[2K{DIM}{frame} {label}…{DIM:#}");
            }
        }
    }
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
    let host = host_of(&server);
    saved.server = Some(server);
    saved.token = Some(token);
    config::save(&path, &saved)?;
    anstream::eprintln!(
        "{OK}✓{OK:#} Signed in to {STRONG}{host}{STRONG:#} as {STRONG}{username}{STRONG:#}\n  \
         {DIM}Saved to {}{DIM:#}",
        tilde(&path)
    );
    Ok(())
}

/// `path` with the home directory written as `~`, the way people write it.
fn tilde(path: &std::path::Path) -> String {
    std::env::home_dir()
        .and_then(|home| path.strip_prefix(home).ok())
        .map_or_else(
            || path.display().to_string(),
            |rest| format!("~/{}", rest.display()),
        )
}

/// `share.example.com`, or with its port when it has one: how a person names
/// an instance.
fn host_of(server: &Url) -> String {
    match (server.host_str(), server.port()) {
        (Some(host), Some(port)) => format!("{host}:{port}"),
        (Some(host), None) => host.to_owned(),
        (None, _) => server.to_string(),
    }
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

/// One line on standard error, rewritten in place, when it is a terminal:
/// the name, a bar, and how much of how much.
fn progress(name: &str, done: u64, total: u64) {
    let mut stderr = std::io::stderr();
    if !stderr.is_terminal() {
        return;
    }
    let percent = done.saturating_mul(100).checked_div(total).unwrap_or(100);
    let (filled, empty) = style::bar(done, total, 24);
    // Progress is a courtesy; a terminal that will not take it changes nothing.
    anstream::eprint!(
        "\r\x1b[2K{name}  {filled}{DIM}{empty}{DIM:#}  {percent:>3}%  {DIM}{} / {}{DIM:#}",
        style::bytes(done),
        style::bytes(total)
    );
    stderr.flush().ok();
}

fn clear_progress() {
    let mut stderr = std::io::stderr();
    if stderr.is_terminal() {
        write!(stderr, "\r\x1b[2K").ok();
    }
}
