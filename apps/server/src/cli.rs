//! Everything the binary does that is not serving.
//!
//! # Why the parsing is by hand
//!
//! There are two subcommands and three flags between them. `clap` would add
//! roughly half a megabyte to a binary whose whole pitch is that it is one small
//! file, and this module is the second place that decision has come up — the
//! health probe is hand-rolled for the same reason, rather than pulling in an
//! HTTP client. If a third subcommand arrives, revisit it; two is not enough to
//! pay for a dependency.
//!
//! # Why password reset is a subcommand and not an endpoint
//!
//! The case that matters is the only administrator having forgotten theirs.
//! That is not rare on a single-person instance, and the machine it happens on
//! usually has no mail server, so anything involving email is unavailable
//! exactly when it is needed. A rescue path has to work when the service does
//! not: no network, no session, no running process.
//!
//! The consequence is stated plainly rather than hidden: **whoever can run this
//! binary against the database can take any account.** For self-hosted software
//! that is already true — the database is a file they own — so this makes an
//! existing property usable rather than granting a new one.

use std::io::IsTerminal as _;

use anyhow::{Context as _, bail};
use tessera_core::{Password, PasswordPolicy, Username};
use time::OffsetDateTime;

use crate::{
    auth::{password, secret::Secret},
    config::Config,
    db::{self, repository::user::UserRepository},
};

/// Argument that turns the binary into its own health probe.
pub const HEALTH_CHECK_FLAG: &str = "--health-check";

/// What the process was asked to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// Run the server. The default, and what happens with no arguments.
    Serve,
    /// Probe the configured listener and exit on the result.
    HealthCheck,
    /// Set an account's password from the command line.
    ResetPassword {
        /// Whose password to set.
        username: String,
        /// Where the new password comes from.
        source: PasswordSource,
    },
    /// Print usage and exit successfully.
    Help,
}

/// What the command ended up setting, for the closing message.
#[derive(Debug)]
enum Chosen {
    /// The operator supplied it, one way or another.
    Supplied,
    /// We picked one. It is printed, and the account is asked to change it.
    Generated {
        /// The copy to show, since nobody else has seen it.
        printed: String,
        /// Whether this happened because there was no terminal to ask at,
        /// rather than because `--generate` asked for it.
        for_want_of_a_terminal: bool,
    },
}

/// Where `reset-password` gets the new password.
///
/// # Why the command line is not the default
///
/// A password in an argument is visible twice over: it stays in the shell
/// history file, and for as long as the process runs it is in `/proc/<pid>/cmdline`,
/// which `ps` reads and every other user on the machine can see. For a tool
/// whose whole purpose is rescuing an administrator account, that is a poor
/// place to leave the credential.
///
/// So the flag stays — automation exists and refusing it would only make people
/// write worse workarounds — but it is no longer where an operator lands by
/// default, and `--help` says what it costs.
#[derive(Debug, PartialEq, Eq)]
pub enum PasswordSource {
    /// Typed at the terminal if there is one, generated if there is not.
    ///
    /// Resolved when the command runs rather than when it is parsed, so that
    /// reading the arguments stays a pure function.
    Ask,
    /// `--password <value>`. Visible to `ps`; see the type's own docs.
    Argument(String),
    /// `--password-stdin`. The form automation should use: nothing reaches the
    /// argument list or the history file.
    Stdin,
    /// `--generate`. Chosen for us and printed, and the account is asked to
    /// change it at next sign-in.
    Generate,
}

/// Reads the command line.
///
/// # Errors
///
/// A message for the operator when the arguments do not describe a command.
/// Unknown arguments are refused rather than ignored: a mistyped flag that is
/// silently dropped would look like the command ran and did nothing.
pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Command, String> {
    let mut args = args.into_iter();
    let Some(first) = args.next() else {
        return Ok(Command::Serve);
    };

    match first.as_str() {
        HEALTH_CHECK_FLAG => Ok(Command::HealthCheck),
        "-h" | "--help" | "help" => Ok(Command::Help),
        "reset-password" => parse_reset_password(args),
        other => Err(format!(
            "unknown argument `{other}`\n\nRun `tessera-server --help` to see what this accepts."
        )),
    }
}

fn parse_reset_password<I: Iterator<Item = String>>(mut args: I) -> Result<Command, String> {
    let mut username = None;
    let mut source = PasswordSource::Ask;

    // Two password flags are refused rather than last-one-wins: together they
    // mean the operator believes something about the command that is not true,
    // and quietly picking one would set a password they did not intend on the
    // one account they cannot get back into.
    while let Some(arg) = args.next() {
        let chosen = match arg.as_str() {
            "--username" | "-u" => {
                username = Some(value(&mut args, "--username")?);
                continue;
            }
            "--password" | "-p" => PasswordSource::Argument(value(&mut args, "--password")?),
            "--password-stdin" => PasswordSource::Stdin,
            "--generate" => PasswordSource::Generate,
            other => return Err(format!("reset-password: unknown argument `{other}`")),
        };
        if source != PasswordSource::Ask {
            return Err(CONFLICT.to_owned());
        }
        source = chosen;
    }

    let Some(username) = username else {
        return Err("reset-password: --username is required".to_owned());
    };
    Ok(Command::ResetPassword { username, source })
}

/// Said when more than one of the password flags is given.
const CONFLICT: &str =
    "reset-password: give only one of --password, --password-stdin or --generate";

fn value<I: Iterator<Item = String>>(args: &mut I, flag: &str) -> Result<String, String> {
    args.next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{flag} needs a value"))
}

/// What `--help` prints.
pub const USAGE: &str = "\
tessera-server — a self-hosted file courier

USAGE:
    tessera-server                       Run the server (the usual case)
    tessera-server --health-check        Probe the configured listener, exit 0 if healthy
    tessera-server reset-password ...    Set an account's password

RESET-PASSWORD:
    -u, --username <name>    Whose password to set. Required.
        --password-stdin     Read the new password from standard input. What
                             automation should use.
        --generate           Choose one, print it, and ask the account to change
                             it at next sign-in.
    -p, --password <value>   The new password, on the command line. Visible to
                             `ps` while this runs, and left in the shell history —
                             prefer --password-stdin.

    With none of those: asks for it at the terminal, twice, without echoing. If
    standard input is not a terminal — a script, a cron job, a pipe — one is
    generated and printed instead, which is what this did before these flags
    existed. Piping a password in without --password-stdin therefore sets a
    generated one; the output says so.

    Reads the same configuration as the server and writes straight to the
    database, so it works with the service stopped. It does not need the network.

    This is the way back in when the only administrator has forgotten theirs.
    It follows that anyone who can run this against the database can take any
    account — which was already true of anyone holding the database file.

Configuration is read from tessera.yaml (or TESSERA_CONFIG_FILE), with
TESSERA_-prefixed environment variables taking precedence.
";

/// Sets an account's password, without the server running.
///
/// # Errors
///
/// When the database cannot be opened, the name does not match an account, or
/// the chosen password fails the instance's own policy.
pub async fn reset_password(
    config: &Config,
    username: &str,
    source: PasswordSource,
) -> anyhow::Result<()> {
    // Privileged parsing, because the account this exists to rescue is normally
    // `admin` — which is on the reserved list precisely so nobody else can
    // register it, and which the ordinary parser therefore refuses.
    let username = Username::parse_privileged(username)
        .map_err(|error| anyhow::anyhow!("`{username}` is not a usable account name: {error}"))?;

    let db = db::connect(&config.database)
        .await
        .with_context(|| format!("could not open the database at `{}`", config.database.url))?;
    // Deliberately no migrations. A rescue tool that quietly rewrites the schema
    // is the last thing you want to run against a database you are already
    // unsure about; if the schema is behind, start the server once and try again.
    let users = UserRepository::new(db.clone());

    let found = users
        .find_by_username(&username)
        .await
        .context("could not look the account up")?;
    let Some(user) = found else {
        db::close(db).await;
        bail!("no account called `{username}`");
    };

    let (password, chosen) = choose_password(source, config)?;

    let hash = password::hash(password).await.context("could not hash")?;
    let username = username.as_str();
    // `must_change` only when we chose it. One the operator supplied is one they
    // already know; one printed here has been through a terminal and a scroll
    // buffer, and should not stay in force.
    let generated = matches!(chosen, Chosen::Generated { .. });
    users
        .set_password(user.id, hash, generated, OffsetDateTime::now_utc())
        .await
        .context("could not store the new password")?;
    db::close(db).await;

    report(username, &chosen);
    Ok(())
}

/// The new password, and a copy of it when we were the one who chose it.
///
/// Split out because the cases are held to different rules, and reading that
/// difference matters more than saving a function.
///
/// # Errors
///
/// When the terminal cannot be read, the two attempts do not match, or the
/// password fails the instance's policy.
fn choose_password(source: PasswordSource, config: &Config) -> anyhow::Result<(Password, Chosen)> {
    let typed = match source {
        PasswordSource::Generate => return generated_password(false),
        PasswordSource::Argument(value) => value,
        PasswordSource::Stdin => read_from_stdin()?,
        // The test is on *stdin*, while the prompt below reads from the terminal
        // device directly — they are not the same question, and the stricter one
        // is deliberate. Asking whether a terminal exists would make
        // `echo secret | tessera-server reset-password -u admin` stop at a prompt,
        // and a script run from a terminal would hang there with nobody watching.
        // A password printed to a log is a visible mistake; a job that never
        // returns is not.
        //
        // The cost is that the same forgotten `--password-stdin` sets a password
        // nobody asked for. `report` says so, and says which flag was missing.
        PasswordSource::Ask if !std::io::stdin().is_terminal() => {
            return generated_password(true);
        }
        PasswordSource::Ask => read_from_terminal()?,
    };

    // Held to the instance's own rule, so the CLI cannot install a password the
    // login form would then refuse to accept.
    let password = Password::parse(typed, config.auth.password)
        .map_err(|error| anyhow::anyhow!("that password is not acceptable: {error}"))?;
    Ok((password, Chosen::Supplied))
}

/// One we chose, returned alongside a copy to print.
fn generated_password(for_want_of_a_terminal: bool) -> anyhow::Result<(Password, Chosen)> {
    let generated = Secret::generate_session().expose().to_owned();
    // Parsed against the default policy rather than the configured one: an
    // operator who set `min_length` to 64 should not make the rescue path fail
    // on a password nobody has to type twice.
    let password = Password::parse(generated.clone(), PasswordPolicy::default())
        .map_err(|_| anyhow::anyhow!("could not generate a password"))?;
    Ok((
        password,
        Chosen::Generated {
            printed: generated,
            for_want_of_a_terminal,
        },
    ))
}

/// Asks twice, without echoing, and insists the two agree.
///
/// Twice because there is nothing to check a typo against: this writes straight
/// to the database with the service stopped, so a mistyped password is only
/// discovered at the next sign-in, by which point the operator has no reason to
/// suspect the tool.
///
/// The prompt is written straight to the terminal rather than to stdout, which
/// is what keeps stdout the command's result and nothing else: redirecting it to
/// a file captures `Password for ... set.` and no prompt. Measured rather than
/// assumed — that is `rpassword`'s doing, not ours.
fn read_from_terminal() -> anyhow::Result<String> {
    let first = rpassword::prompt_password("New password: ")
        .context("could not read the password from the terminal")?;
    let again = rpassword::prompt_password("Again: ")
        .context("could not read the password from the terminal")?;
    if first != again {
        bail!("those did not match; nothing has been changed");
    }
    Ok(first)
}

/// Reads one line from standard input, for automation.
///
/// One line rather than the whole stream, so that `echo secret | ...` works
/// without the trailing newline becoming part of the password — a difference
/// nobody would see until the sign-in that fails.
fn read_from_stdin() -> anyhow::Result<String> {
    let mut raw = String::new();
    std::io::stdin()
        .read_line(&mut raw)
        .context("could not read the password from standard input")?;
    first_line(&raw).map(str::to_owned)
}

/// One line, with the line ending taken off.
///
/// Split out to be testable, because the failure it prevents is invisible:
/// keeping the newline gives a password that differs from the one the operator
/// piped in by one character, and nothing says so until a sign-in fails.
///
/// # Errors
///
/// When there was nothing but the line ending.
fn first_line(raw: &str) -> anyhow::Result<&str> {
    let line = raw.trim_end_matches(['\r', '\n']);
    if line.is_empty() {
        bail!("nothing arrived on standard input");
    }
    Ok(line)
}

/// Says what happened, on stdout.
///
/// Printed rather than logged: this is the entire output of the command, and a
/// rescue tool whose result disappears because `telemetry.filter` is set to
/// `warn` would be worse than useless.
#[expect(
    clippy::print_stdout,
    reason = "a command-line tool's result belongs on stdout, not in the log"
)]
fn report(username: &str, chosen: &Chosen) {
    match chosen {
        Chosen::Supplied => println!("Password for `{username}` set."),
        Chosen::Generated {
            printed,
            for_want_of_a_terminal,
        } => {
            println!(
                "Password for `{username}` set to:\n\n    {printed}\n\n\
                 Sign in with it and choose a new one — this account is now marked as \
                 owing a change."
            );
            // The case this exists for: somebody piped a password in and forgot
            // the flag that reads it. Without this line the command looks like
            // it worked, and the password they meant to set is gone with no
            // indication that it was ever seen.
            if *for_want_of_a_terminal {
                println!(
                    "\nThere was no terminal to ask at, so this one was chosen here. \
                     If you meant to supply one, pass it with --password-stdin."
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(args: &[&str]) -> Result<Command, String> {
        parse(args.iter().map(|arg| (*arg).to_owned()))
    }

    #[test]
    fn no_arguments_runs_the_server() {
        assert_eq!(parsed(&[]), Ok(Command::Serve));
    }

    #[test]
    fn the_health_probe_is_still_reachable() {
        assert_eq!(parsed(&["--health-check"]), Ok(Command::HealthCheck));
    }

    #[test]
    fn a_username_is_required() {
        assert!(parsed(&["reset-password"]).is_err());
    }

    /// No flag means "ask", and where to ask is decided when the command runs.
    #[test]
    fn no_password_flag_means_ask() {
        assert_eq!(
            parsed(&["reset-password", "--username", "admin"]),
            Ok(Command::ResetPassword {
                username: "admin".to_owned(),
                source: PasswordSource::Ask
            })
        );
    }

    #[test]
    fn each_way_of_supplying_one_is_understood() {
        for (args, expected) in [
            (
                vec!["reset-password", "-u", "admin", "-p", "hunter2"],
                PasswordSource::Argument("hunter2".to_owned()),
            ),
            (
                vec!["reset-password", "-u", "admin", "--password-stdin"],
                PasswordSource::Stdin,
            ),
            (
                vec!["reset-password", "-u", "admin", "--generate"],
                PasswordSource::Generate,
            ),
        ] {
            assert_eq!(
                parsed(&args),
                Ok(Command::ResetPassword {
                    username: "admin".to_owned(),
                    source: expected
                }),
                "{args:?}"
            );
        }
    }

    /// Two of them together means the operator believes something about the
    /// command that is not true. Picking one quietly would set a password they
    /// did not intend, on the one account they cannot get back into.
    #[test]
    fn two_ways_at_once_are_refused() {
        for args in [
            vec!["reset-password", "-u", "a", "-p", "x", "--generate"],
            vec!["reset-password", "-u", "a", "--password-stdin", "-p", "x"],
            vec![
                "reset-password",
                "-u",
                "a",
                "--generate",
                "--password-stdin",
            ],
            vec!["reset-password", "-u", "a", "-p", "x", "-p", "y"],
        ] {
            assert!(parsed(&args).is_err(), "{args:?}");
        }
    }

    /// `echo secret | ...` adds a newline that is not part of the password, and
    /// keeping it would cost one sign-in and a lot of confusion.
    #[test]
    fn a_piped_password_loses_its_line_ending_and_nothing_else() {
        assert_eq!(first_line("hunter2\n").unwrap(), "hunter2");
        assert_eq!(first_line("hunter2\r\n").unwrap(), "hunter2");
        assert_eq!(first_line("hunter2").unwrap(), "hunter2");
        assert_eq!(
            first_line("  hunter2  \n").unwrap(),
            "  hunter2  ",
            "spaces are part of a password; only the line ending is not"
        );
    }

    #[test]
    fn an_empty_pipe_is_refused_rather_than_setting_nothing() {
        assert!(first_line("\n").is_err());
        assert!(first_line("").is_err());
    }

    /// A flag with no value would otherwise swallow the next flag as its value,
    /// so `-u -p secret` would look for an account called `-p`.
    #[test]
    fn a_flag_without_a_value_is_refused() {
        assert!(parsed(&["reset-password", "--username"]).is_err());
    }

    /// Silently ignoring an unknown flag would make a typo look like a command
    /// that ran and did nothing.
    #[test]
    fn unknown_arguments_are_refused() {
        assert!(parsed(&["--frobnicate"]).is_err());
        assert!(parsed(&["reset-password", "-u", "admin", "--force"]).is_err());
    }
}
