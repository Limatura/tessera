//! The `tessera` command.

use std::{
    io::{IsTerminal as _, Read as _, Write as _},
    path::PathBuf,
    process::ExitCode,
};

use clap::{Parser, Subcommand};
use tessera_cli::{
    Error, Result,
    client::Client,
    get, link,
    send::{self, Content, Options},
};
use url::Url;

/// Send and fetch Tessera shares.
#[derive(Debug, Parser)]
#[command(name = "tessera", version, about)]
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
        /// The instance to upload to.
        #[arg(long, env = "TESSERA_SERVER")]
        server: Url,
        /// An API token, to upload as an account instead of anonymously.
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
        /// The instance, when only a code is given.
        #[arg(long, env = "TESSERA_SERVER")]
        server: Option<Url>,
        /// The directory to write files into. A note goes to standard output.
        #[arg(short, long, default_value = ".")]
        output: PathBuf,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    match run(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // Nowhere left to report a failure to write the failure.
            writeln!(std::io::stderr(), "tessera: {error}").ok();
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
            let client = Client::new(server, token)?;
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
    }
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
