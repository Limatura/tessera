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
    config::{Config, RegistrationPolicy},
    db::{
        self,
        repository::{
            invite::InviteRepository,
            role::{Role, RoleRepository},
            setting::SettingRepository,
            user::UserRepository,
        },
    },
    settings,
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
    /// Mint an invite code and print it.
    Invite {
        /// How many accounts it may create.
        uses: i64,
        /// How long it lasts, in days, or `None` for one that never expires.
        expires_in_days: Option<u64>,
        /// What to call it, so a list of them means something.
        note: Option<String>,
    },
    /// Read or change settings without the admin panel.
    ///
    /// # Why this exists alongside the panel
    ///
    /// Two instances cannot use the panel at all. One has `features.accounts`
    /// off — a file drop behind a VPN, which has no accounts and so no session
    /// to sign in with. The other has been configured into a state where the
    /// panel does not work, which is exactly when the way to undo it must not
    /// be the panel.
    Settings(SettingsAction),
    /// Read or change storage quotas without the admin panel.
    ///
    /// The instance-wide ceiling is a setting, so it is `settings set
    /// storage.max_total_bytes`. These are the per-role and per-account
    /// figures, which are rows rather than settings.
    Quota(QuotaAction),
    /// Print usage and exit successfully.
    Help,
}

/// What `quota` was asked to do.
#[derive(Debug, PartialEq, Eq)]
pub enum QuotaAction {
    /// Print every role and every account that carries a figure.
    List,
    /// Set or clear one.
    Set {
        /// Which figure.
        limit: QuotaLimit,
        /// Which one to change.
        subject: QuotaSubject,
        /// Bytes, `0` for no ceiling, or `None` to clear it so the next layer
        /// answers again.
        bytes: Option<u64>,
    },
}

/// Which of the two figures a role or account carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaLimit {
    /// Total storage held.
    Total,
    /// Largest single file.
    PerFile,
}

/// Whose quota `quota set` is about.
#[derive(Debug, PartialEq, Eq)]
pub enum QuotaSubject {
    /// A role, by name.
    Role(String),
    /// An account, by username.
    User(String),
}

/// What `settings` was asked to do.
#[derive(Debug, PartialEq, Eq)]
pub enum SettingsAction {
    /// Print every setting, its value and where that value came from.
    List,
    /// Store one setting.
    Set {
        /// Dotted key, the same spelling the file and the environment use.
        key: String,
        /// The new value, as JSON. A bare word is read as a string.
        value: String,
    },
    /// Remove one stored setting, so it falls back to the layer below.
    Reset {
        /// Dotted key.
        key: String,
    },
    /// Remove every stored setting.
    ///
    /// The way back from a panel that has been used to make the instance
    /// unusable.
    ResetAll,
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
        "invite" => parse_invite(args),
        "settings" => parse_settings(args),
        "quota" => parse_quota(args),
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

fn parse_invite<I: Iterator<Item = String>>(mut args: I) -> Result<Command, String> {
    let mut uses = 1_i64;
    let mut expires_in_days = None;
    let mut note = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--uses" | "-n" => {
                uses = value(&mut args, "--uses")?
                    .parse()
                    .map_err(|_| "invite: --uses needs a whole number".to_owned())?;
            }
            "--expires-in-days" | "-d" => {
                expires_in_days = Some(
                    value(&mut args, "--expires-in-days")?
                        .parse()
                        .map_err(|_| "invite: --expires-in-days needs a whole number".to_owned())?,
                );
            }
            "--note" => note = Some(value(&mut args, "--note")?),
            other => return Err(format!("invite: unknown argument `{other}`")),
        }
    }

    if uses < 1 {
        return Err("invite: --uses must be at least 1".to_owned());
    }
    Ok(Command::Invite {
        uses,
        expires_in_days,
        note,
    })
}

/// Said when more than one of the password flags is given.
const CONFLICT: &str =
    "reset-password: give only one of --password, --password-stdin or --generate";

fn value<I: Iterator<Item = String>>(args: &mut I, flag: &str) -> Result<String, String> {
    args.next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{flag} needs a value"))
}

fn parse_settings<I: Iterator<Item = String>>(mut args: I) -> Result<Command, String> {
    let action = args
        .next()
        .ok_or_else(|| "settings: say what to do — list, set, reset or reset-all".to_owned())?;

    let command = match action.as_str() {
        "list" => Command::Settings(SettingsAction::List),
        "set" => {
            let key = args
                .next()
                .ok_or_else(|| "settings set: needs a key, e.g. share.code_length".to_owned())?;
            let value = args
                .next()
                .ok_or_else(|| format!("settings set: `{key}` needs a value"))?;
            Command::Settings(SettingsAction::Set { key, value })
        }
        "reset" => {
            let key = args
                .next()
                .ok_or_else(|| "settings reset: needs a key".to_owned())?;
            Command::Settings(SettingsAction::Reset { key })
        }
        "reset-all" => Command::Settings(SettingsAction::ResetAll),
        other => {
            return Err(format!(
                "settings: unknown action `{other}` — expected list, set, reset or reset-all"
            ));
        }
    };

    // Anything after what each action takes is a mistake, and a silently
    // ignored argument looks like the command did what was asked.
    if let Some(extra) = args.next() {
        return Err(format!("settings: unexpected argument `{extra}`"));
    }
    Ok(command)
}

fn parse_quota<I: Iterator<Item = String>>(mut args: I) -> Result<Command, String> {
    let action = args
        .next()
        .ok_or_else(|| "quota: say what to do — list, set or set-file".to_owned())?;

    let command = match action.as_str() {
        "list" => Command::Quota(QuotaAction::List),
        "set" | "set-file" => {
            let limit = if action == "set-file" {
                QuotaLimit::PerFile
            } else {
                QuotaLimit::Total
            };
            let kind = args
                .next()
                .ok_or_else(|| format!("quota {action}: needs `role` or `user`"))?;
            let name = args
                .next()
                .ok_or_else(|| format!("quota set {kind}: needs a name"))?;
            let subject = match kind.as_str() {
                "role" => QuotaSubject::Role(name),
                "user" => QuotaSubject::User(name),
                other => {
                    return Err(format!(
                        "quota set: `{other}` is not a subject — expected role or user"
                    ));
                }
            };
            let value = args
                .next()
                .ok_or_else(|| "quota set: needs bytes, 0, or none".to_owned())?;
            // `none` and `0` are different instructions and the difference is
            // the whole design, so neither is spelled as the other: `none`
            // hands the question to the next layer, `0` answers it with "no
            // ceiling".
            let bytes = match value.as_str() {
                "none" | "null" => None,
                other => Some(other.parse::<u64>().map_err(|_| {
                    format!("quota set: `{other}` is not a number of bytes, 0, or none")
                })?),
            };
            Command::Quota(QuotaAction::Set {
                limit,
                subject,
                bytes,
            })
        }
        other => {
            return Err(format!(
                "quota: unknown action `{other}` — expected list, set or set-file"
            ));
        }
    };

    if let Some(extra) = args.next() {
        return Err(format!("quota: unexpected argument `{extra}`"));
    }
    Ok(command)
}

/// What `--help` prints.
pub const USAGE: &str = "\
tessera-server — a self-hosted file courier

USAGE:
    tessera-server                       Run the server (the usual case)
    tessera-server --health-check        Probe the configured listener, exit 0 if healthy
    tessera-server reset-password ...    Set an account's password
    tessera-server invite ...            Mint an invite code
    tessera-server settings ...          Read or change settings without the panel
    tessera-server quota ...             Read or change per-role and per-account quotas

SETTINGS:
    settings list            Every setting, its value, and where that value
                             came from. Read-only ones are marked.
    settings set <key> <value>
                             Store one. The value is read as JSON, so 10 is a
                             number and true is a boolean; anything that is not
                             JSON is taken as a string.
    settings reset <key>     Forget one, so it falls back to the environment,
                             the file, or the default.
    settings reset-all       Forget all of them. The way back from a panel that
                             has been used to make the instance unusable.

    Writes the same table the admin panel does, so the two always agree. Takes
    effect when the server next starts — a running one is not asked to reload.

QUOTA:
    quota list               Every role, and every account that has a figure of
                             its own.
    quota set <role|user> <name> <bytes|0|none>
                             How much that role or account may hold in total.
    quota set-file <role|user> <name> <bytes|0|none>
                             Largest single file it may send.

    `none` clears the figure, so the next layer answers again: an account falls
    back to its role, a role to the instance. `0` is different — it means no
    limit at that layer.

    An account's own figure beats its role's. The instance figure still applies
    on top of both, so neither can hand out more than the instance allows:
    storage.max_total_bytes for the total, server.max_body_bytes for a single
    file. Set those with `settings set` and the environment respectively.

    Anonymous uploads are charged to the `guest` role, which is how an instance
    that allows them can still bound them.

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

INVITE:
    -n, --uses <count>           How many accounts this code may create. Default 1.
    -d, --expires-in-days <n>    How long it lasts. Never expires when omitted.
        --note <text>            What to call it, for `invites` listings later.

    Only useful with auth.registration set to `invite`. The code is printed once
    and only its digest is stored, so there is no way to read it back.

The settings a running server cannot pick up — the listening address, the
database, the log format — come from TESSERA_-prefixed environment variables,
with __ separating nesting levels. Everything else lives in the admin panel, or
in `settings set` above when there is no panel.
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

    let hash = password::hash(password, config.auth.hashing)
        .await
        .context("could not hash")?;
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

/// Mints an invite code and prints it, without the server running.
///
/// # Errors
///
/// Runs a `settings` subcommand against the database the server would use.
///
/// # Why this writes to the same table the panel does
///
/// So that the two agree. A separate file or a separate flag would be a second
/// answer to "what is this instance configured with", and the first thing
/// anybody would ask is which one wins.
///
/// # Errors
///
/// Returns an error if the database cannot be opened, the key is one the panel
/// would also refuse, or the resulting configuration will not load.
pub async fn settings(config: &Config, action: SettingsAction) -> anyhow::Result<()> {
    let db = db::connect(&config.database)
        .await
        .with_context(|| format!("could not open the database at `{}`", config.database.url))?;
    // No migrations, for the same reason `reset-password` runs none: a tool
    // that quietly rewrites the schema is the last thing to run against a
    // database you are unsure about. On an instance that has never started,
    // the table does not exist and the error says so.
    let store = SettingRepository::new(db.clone());

    let outcome = run_settings(&store, action).await;
    db::close(db).await;
    outcome
}

/// Reads or changes a per-role or per-account quota, without the server running.
///
/// # Errors
///
/// When the database cannot be opened, the name matches nothing, or the write
/// fails.
pub async fn quota(config: &Config, action: QuotaAction) -> anyhow::Result<()> {
    let db = db::connect(&config.database)
        .await
        .with_context(|| format!("could not open the database at `{}`", config.database.url))?;
    let roles = RoleRepository::new(db.clone());
    let users = UserRepository::new(db.clone());

    let outcome = run_quota(&roles, &users, action).await;
    db::close(db).await;
    outcome
}

async fn run_quota(
    roles: &RoleRepository,
    users: &UserRepository,
    action: QuotaAction,
) -> anyhow::Result<()> {
    match action {
        QuotaAction::List => {
            let listed = roles.list().await.context("could not read the roles")?;
            let overridden = users
                .with_quota()
                .await
                .context("could not read the accounts")?;
            print_quotas(&listed, &overridden);
        }
        QuotaAction::Set {
            limit,
            subject,
            bytes,
        } => {
            let now = OffsetDateTime::now_utc();
            let name = match subject {
                QuotaSubject::Role(name) => {
                    let role = roles
                        .find_by_name(&name)
                        .await
                        .context("could not read the roles")?
                        .with_context(|| format!("no role is called `{name}`"))?;
                    match limit {
                        QuotaLimit::Total => roles.set_quota(role.id, bytes, now).await,
                        QuotaLimit::PerFile => roles.set_max_file_bytes(role.id, bytes, now).await,
                    }
                    .context("could not store the figure")?;
                    format!("role {name}")
                }
                QuotaSubject::User(name) => {
                    let username = Username::parse(&name)
                        .with_context(|| format!("`{name}` is not a username"))?;
                    let user = users
                        .find_by_username(&username)
                        .await
                        .context("could not read the accounts")?
                        .with_context(|| format!("no account is called `{name}`"))?;
                    match limit {
                        QuotaLimit::Total => users.set_quota(user.id, bytes, now).await,
                        QuotaLimit::PerFile => users.set_max_file_bytes(user.id, bytes, now).await,
                    }
                    .context("could not store the figure")?;
                    format!("user {name}")
                }
            };
            report_quota_stored(&name, limit, bytes);
        }
    }
    Ok(())
}

async fn run_settings(store: &SettingRepository, action: SettingsAction) -> anyhow::Result<()> {
    match action {
        SettingsAction::List => {
            let stored = store.all().await.context("could not read the settings")?;
            print_settings(&settings::describe(&stored));
        }
        SettingsAction::Set { key, value } => {
            refuse_unless_settable(&key)?;
            let parsed = parse_value(&value);
            let mut candidate = store.all().await.context("could not read the settings")?;
            candidate.insert(key.clone(), parsed.clone());

            // The same proof the endpoint takes before storing: a value that
            // will not load should be refused here, not dropped silently at the
            // next start-up.
            let (_, rejected) = Config::load_with_overrides(&candidate)
                .context("that value leaves the configuration unusable")?;
            if let Some(bad) = rejected.into_iter().find(|bad| bad.key == key) {
                anyhow::bail!("`{key}` will not be accepted: {}", bad.reason);
            }

            store
                .set(&key, &parsed, OffsetDateTime::now_utc())
                .await
                .context("could not store the setting")?;
            report_setting_stored(&key, &parsed);
        }
        SettingsAction::Reset { key } => {
            let removed = store.clear(&key).await.context("could not clear it")?;
            report_setting_cleared(&key, removed);
        }
        SettingsAction::ResetAll => {
            let removed = store.clear_all().await.context("could not clear them")?;
            report_all_settings_cleared(removed);
        }
    }
    Ok(())
}

/// Refuses a key the panel would refuse, with the same reason.
///
/// The two surfaces have to agree: a setting the panel calls unchangeable and
/// the command line accepts would be a value nobody could explain afterwards.
fn refuse_unless_settable(key: &str) -> anyhow::Result<()> {
    match settings::admits(key) {
        Ok(()) => Ok(()),
        Err(settings::Refusal::Unknown) => {
            anyhow::bail!("`{key}` is not a setting this version has")
        }
        Err(settings::Refusal::Secret) => anyhow::bail!(
            "`{key}` holds a credential; set it in the environment rather than the database"
        ),
        Err(settings::Refusal::FixedAtBoot) => {
            anyhow::bail!("`{key}` cannot change while the server runs; set it in the environment")
        }
    }
}

/// Reads a value the way an operator typed it.
///
/// JSON first, so `10`, `true` and `["a","b"]` arrive as what they look like.
/// Anything that is not JSON is taken as a string, because quoting every
/// hostname at a shell prompt is the kind of friction that makes a tool
/// unpleasant — and `s3://…` is not valid JSON.
fn parse_value(raw: &str) -> serde_json::Value {
    serde_json::from_str(raw).unwrap_or_else(|_| serde_json::Value::String(raw.to_owned()))
}

/// Says what was stored, and that a running server has not noticed.
///
/// On stdout rather than through the log, for the same reason the invite code
/// is: this is a command-line tool's result, and `telemetry.filter` set to
/// `warn` must not be able to swallow it.
#[expect(
    clippy::print_stdout,
    reason = "a command-line tool's result belongs on stdout, not in the log"
)]
fn report_setting_stored(key: &str, value: &serde_json::Value) {
    println!("{key} = {value}");
    println!("\nThe running server picks this up on its next start.");
}

/// Says whether there was anything to clear.
///
/// Worth telling apart: "cleared" and "there was nothing here" look the same to
/// somebody who mistyped the key.
#[expect(
    clippy::print_stdout,
    reason = "a command-line tool's result belongs on stdout, not in the log"
)]
fn report_setting_cleared(key: &str, removed: bool) {
    if removed {
        println!("{key} cleared; it falls back to the environment, the file or the default.");
    } else {
        println!("{key} was not set here, so nothing changed.");
    }
}

/// Says how much the rescue took away.
#[expect(
    clippy::print_stdout,
    reason = "a command-line tool's result belongs on stdout, not in the log"
)]
fn report_all_settings_cleared(removed: u64) {
    println!("cleared {removed} stored setting(s).");
    println!("Everything now comes from the environment, the file, or the defaults.");
}

/// Says what was set, in the same three-way language the argument takes.
#[expect(
    clippy::print_stdout,
    reason = "a command-line tool's result belongs on stdout, not in the log"
)]
fn report_quota_stored(subject: &str, limit: QuotaLimit, bytes: Option<u64>) {
    let what = match limit {
        QuotaLimit::Total => "total",
        QuotaLimit::PerFile => "per file",
    };
    match bytes {
        None => println!("{subject} ({what}): cleared; the next layer applies again."),
        Some(0) => println!("{subject} ({what}): no limit of its own."),
        Some(bytes) => println!("{subject} ({what}): {bytes} bytes."),
    }
    println!("\nTakes effect on the next request; the server is not restarted.");
}

/// Prints every role, and every account that overrides its role.
///
/// Accounts without a figure are left out: on an instance with a thousand
/// accounts and three overrides, listing all thousand buries the three.
#[expect(
    clippy::print_stdout,
    reason = "a command-line tool's result belongs on stdout, not in the log"
)]
fn print_quotas(roles: &[Role], overridden: &[(String, Option<u64>)]) {
    println!("ROLES{:>17}{:>28}", "TOTAL", "PER FILE");
    for role in roles {
        println!(
            "  {:<20}{:<28}{}",
            role.name,
            describe_quota(role.quota_bytes),
            describe_quota(role.max_file_bytes),
        );
    }
    if overridden.is_empty() {
        println!("\nNo account overrides its role.");
    } else {
        println!("\nACCOUNTS (overriding their role)");
        for (username, bytes) in overridden {
            println!("  {username:<20}{}", describe_quota(*bytes));
        }
    }
    println!("\nOn top of all of these: storage.max_total_bytes, in `settings list`.");
}

/// The three answers a quota can give, in words.
fn describe_quota(bytes: Option<u64>) -> String {
    match bytes {
        None => "(the next layer decides)".to_owned(),
        Some(0) => "no ceiling".to_owned(),
        Some(bytes) => format!("{bytes} bytes"),
    }
}

/// Prints the list a person reads.
#[expect(
    clippy::print_stdout,
    reason = "a command-line tool's result belongs on stdout, not in the log"
)]
fn print_settings(views: &[settings::View]) {
    let width = views.iter().map(|view| view.key.len()).max().unwrap_or(0);
    for view in views {
        let source = match view.source {
            settings::Source::Default => "default",
            settings::Source::Environment => "environment",
            settings::Source::Panel => "stored",
        };
        // The ones that cannot be changed are marked rather than hidden, for
        // the same reason the panel lists them: leaving them out is what makes
        // somebody ask where the port setting went.
        let fixed = if view.editable { "" } else { "  (read-only)" };
        // `describe` already withheld the value of anything that holds a
        // credential; this only spells the hole as something a person reads.
        let value = if matches!(view.refusal, Some(settings::Refusal::Secret)) {
            "<withheld>".to_owned()
        } else {
            view.value.to_string()
        };
        println!(
            "{:width$}  {value}  [{source}]{fixed}",
            view.key,
            width = width
        );
    }
}

/// Mints an invite code and prints it, without the server running.
///
/// # Errors
///
/// When the database cannot be opened or the insert fails.
pub async fn mint_invite(
    config: &Config,
    uses: i64,
    expires_in_days: Option<u64>,
    note: Option<String>,
) -> anyhow::Result<()> {
    let db = db::connect(&config.database)
        .await
        .with_context(|| format!("could not open the database at `{}`", config.database.url))?;
    // Deliberately no migrations, for the same reason `reset-password` runs
    // none: a tool that quietly rewrites the schema is the last thing to run
    // against a database you are unsure about.
    let invites = InviteRepository::new(db.clone());

    let now = OffsetDateTime::now_utc();
    let expires_at = expires_in_days
        .map(|days| {
            i64::try_from(days)
                .map(time::Duration::days)
                .map(|span| now.saturating_add(span))
                .map_err(|_| anyhow::anyhow!("that is more days than a calendar has"))
        })
        .transpose()?;

    let issued = invites.issue(note, uses, now, expires_at).await;
    db::close(db).await;
    let (_, code) = issued.context("could not create the invite")?;

    report_invite(code.expose(), uses, expires_at);
    if config.auth.registration != RegistrationPolicy::Invite {
        // Not an error: an operator may well be setting up before flipping the
        // switch. But a code that does nothing is worth a word.
        report_invite_is_inert();
    }
    Ok(())
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
fn report_invite(code: &str, uses: i64, expires_at: Option<OffsetDateTime>) {
    let places = if uses == 1 {
        "one account".to_owned()
    } else {
        format!("{uses} accounts")
    };
    let until = expires_at.map_or_else(
        || "It does not expire.".to_owned(),
        |when| {
            let stamp = when
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| when.unix_timestamp().to_string());
            format!("It stops working at {stamp}.")
        },
    );
    println!(
        "Invite code:\n\n    {code}\n\n\
         Good for {places}. {until}\n\
         This is the only time it is shown — only its digest is stored."
    );
}

/// Says that the code will not do anything until the policy changes.
#[expect(
    clippy::print_stdout,
    reason = "a command-line tool's result belongs on stdout, not in the log"
)]
fn report_invite_is_inert() {
    println!(
        "\nNote: `auth.registration` is not `invite`, so registration does not \
         ask for a code yet and this one will not be checked."
    );
}

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

#[cfg(test)]
mod settings_tests {
    use super::{Command, SettingsAction, parse, parse_value};

    fn parsed(args: &[&str]) -> Result<Command, String> {
        parse(args.iter().map(|arg| (*arg).to_owned()))
    }

    #[test]
    fn the_four_actions_parse() {
        assert_eq!(
            parsed(&["settings", "list"]),
            Ok(Command::Settings(SettingsAction::List))
        );
        assert_eq!(
            parsed(&["settings", "set", "share.code_length", "10"]),
            Ok(Command::Settings(SettingsAction::Set {
                key: "share.code_length".to_owned(),
                value: "10".to_owned(),
            }))
        );
        assert_eq!(
            parsed(&["settings", "reset", "share.code_length"]),
            Ok(Command::Settings(SettingsAction::Reset {
                key: "share.code_length".to_owned(),
            }))
        );
        assert_eq!(
            parsed(&["settings", "reset-all"]),
            Ok(Command::Settings(SettingsAction::ResetAll))
        );
    }

    /// A silently ignored argument looks like the command did what was asked.
    #[test]
    fn anything_unexpected_is_refused() {
        assert!(parsed(&["settings"]).is_err(), "no action");
        assert!(parsed(&["settings", "frobnicate"]).is_err(), "unknown");
        assert!(parsed(&["settings", "set", "a.b"]).is_err(), "no value");
        assert!(
            parsed(&["settings", "reset-all", "please"]).is_err(),
            "trailing argument"
        );
    }

    /// JSON first, so a number arrives as a number. Falling back to a string is
    /// what makes `settings set storage.url s3://bucket` work without the
    /// operator quoting it for JSON as well as for the shell.
    #[test]
    fn values_are_read_as_json_then_as_text() {
        assert_eq!(parse_value("10"), serde_json::json!(10));
        assert_eq!(parse_value("true"), serde_json::json!(true));
        assert_eq!(parse_value("[\"a\"]"), serde_json::json!(["a"]));
        assert_eq!(
            parse_value("s3://bucket/prefix"),
            serde_json::json!("s3://bucket/prefix")
        );
        assert_eq!(
            parse_value("smtp.example.com"),
            serde_json::json!("smtp.example.com")
        );
    }
}

#[cfg(test)]
mod quota_tests {
    use super::{Command, QuotaAction, QuotaLimit, QuotaSubject, parse};

    fn parsed(args: &[&str]) -> Result<Command, String> {
        parse(args.iter().map(|arg| (*arg).to_owned()))
    }

    #[test]
    fn both_actions_parse() {
        assert_eq!(
            parsed(&["quota", "list"]),
            Ok(Command::Quota(QuotaAction::List))
        );
        assert_eq!(
            parsed(&["quota", "set", "role", "member", "1024"]),
            Ok(Command::Quota(QuotaAction::Set {
                limit: QuotaLimit::Total,
                subject: QuotaSubject::Role("member".to_owned()),
                bytes: Some(1024),
            }))
        );
        assert_eq!(
            parsed(&["quota", "set", "user", "alice", "1024"]),
            Ok(Command::Quota(QuotaAction::Set {
                limit: QuotaLimit::Total,
                subject: QuotaSubject::User("alice".to_owned()),
                bytes: Some(1024),
            }))
        );
    }

    /// The whole design is that these are different instructions, so the
    /// command line has to be able to say both.
    #[test]
    fn none_and_zero_stay_apart() {
        let cleared = parsed(&["quota", "set", "role", "member", "none"]);
        let unlimited = parsed(&["quota", "set", "role", "member", "0"]);
        assert_eq!(
            cleared,
            Ok(Command::Quota(QuotaAction::Set {
                limit: QuotaLimit::Total,
                subject: QuotaSubject::Role("member".to_owned()),
                bytes: None,
            }))
        );
        assert_eq!(
            unlimited,
            Ok(Command::Quota(QuotaAction::Set {
                limit: QuotaLimit::Total,
                subject: QuotaSubject::Role("member".to_owned()),
                bytes: Some(0),
            }))
        );
        assert_ne!(cleared, unlimited);
    }

    /// A negative figure or a typo must not be read as one of the two words
    /// that mean something.
    #[test]
    fn a_value_that_is_neither_is_refused() {
        assert!(parsed(&["quota", "set", "role", "member", "-1"]).is_err());
        assert!(parsed(&["quota", "set", "role", "member", "lots"]).is_err());
        assert!(parsed(&["quota", "set", "group", "member", "0"]).is_err());
        assert!(parsed(&["quota", "reset", "role", "member"]).is_err());
    }

    /// The two figures are different things, so the command line has to be able
    /// to say which one — and `set` has to keep meaning what it meant.
    #[test]
    fn set_file_is_the_other_figure() {
        assert_eq!(
            parsed(&["quota", "set-file", "role", "guest", "104857600"]),
            Ok(Command::Quota(QuotaAction::Set {
                limit: QuotaLimit::PerFile,
                subject: QuotaSubject::Role("guest".to_owned()),
                bytes: Some(104_857_600),
            }))
        );
        assert!(parsed(&["quota", "set-file", "role", "guest", "none"]).is_ok());
        assert!(parsed(&["quota", "set-files", "role", "guest", "0"]).is_err());
    }

    /// A silently ignored argument looks like the command did what was asked.
    #[test]
    fn a_trailing_argument_is_refused() {
        assert!(parsed(&["quota", "list", "roles"]).is_err());
        assert!(parsed(&["quota", "set", "role", "member", "0", "please"]).is_err());
    }
}
