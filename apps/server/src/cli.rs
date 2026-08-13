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
        /// The new password, or `None` to generate one and print it.
        password: Option<String>,
    },
    /// Print usage and exit successfully.
    Help,
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
    let mut password = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--username" | "-u" => username = Some(value(&mut args, "--username")?),
            "--password" | "-p" => password = Some(value(&mut args, "--password")?),
            other => return Err(format!("reset-password: unknown argument `{other}`")),
        }
    }

    let Some(username) = username else {
        return Err("reset-password: --username is required".to_owned());
    };
    Ok(Command::ResetPassword { username, password })
}

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
    -p, --password <value>   The new password. Generated and printed when omitted,
                             and the account is then asked to change it at next sign-in.

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
    chosen: Option<String>,
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

    let (password, generated) = choose_password(chosen, config)?;

    let hash = password::hash(password).await.context("could not hash")?;
    let username = username.as_str();
    // `must_change` only when we chose it. One the operator typed is one they
    // already know; one printed here has been through a terminal and a scroll
    // buffer, and should not stay in force.
    users
        .set_password(
            user.id,
            hash,
            generated.is_some(),
            OffsetDateTime::now_utc(),
        )
        .await
        .context("could not store the new password")?;
    db::close(db).await;

    report(username, generated.as_deref());
    Ok(())
}

/// The new password, and a copy of it when we were the one who chose it.
///
/// Split out because the two cases are held to different rules, and reading
/// that difference matters more than saving a function.
fn choose_password(
    chosen: Option<String>,
    config: &Config,
) -> anyhow::Result<(Password, Option<String>)> {
    let Some(chosen) = chosen else {
        let generated = Secret::generate_session().expose().to_owned();
        // Parsed against the default policy rather than the configured one: an
        // operator who set `min_length` to 64 should not make the rescue path
        // fail on a password nobody has to type twice.
        let password = Password::parse(generated.clone(), PasswordPolicy::default())
            .map_err(|_| anyhow::anyhow!("could not generate a password"))?;
        return Ok((password, Some(generated)));
    };

    // Held to the instance's own rule, so the CLI cannot install a password the
    // login form would then refuse to accept.
    let password = Password::parse(chosen, config.auth.password)
        .map_err(|error| anyhow::anyhow!("that password is not acceptable: {error}"))?;
    Ok((password, None))
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
fn report(username: &str, generated: Option<&str>) {
    match generated {
        Some(printed) => println!(
            "Password for `{username}` set to:\n\n    {printed}\n\n\
             Sign in with it and choose a new one — this account is now marked as owing a change."
        ),
        None => println!("Password for `{username}` set."),
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

    #[test]
    fn a_password_may_be_given_or_left_to_us() {
        assert_eq!(
            parsed(&["reset-password", "--username", "admin"]),
            Ok(Command::ResetPassword {
                username: "admin".to_owned(),
                password: None
            })
        );
        assert_eq!(
            parsed(&["reset-password", "-u", "admin", "-p", "hunter2"]),
            Ok(Command::ResetPassword {
                username: "admin".to_owned(),
                password: Some("hunter2".to_owned())
            })
        );
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
