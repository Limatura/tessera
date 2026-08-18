//! The administrator that exists before anyone has registered.
//!
//! # Why this is needed at all
//!
//! Registration is open by default and the first account to register becomes the
//! administrator — which is the right shape for someone setting up their own
//! instance, and the wrong shape for anything else. An instance with registration
//! closed has no way to create its first account, and an instance exposed to the
//! internet before its owner reaches the login page hands administration to a
//! stranger. So the account is created at start-up, before the listener binds.

use tessera_core::{
    AccountState, EmailAddress, Password, PasswordPolicy, RoleId, UserId, Username,
};
use time::OffsetDateTime;

use crate::{
    auth::{password, secret::Secret},
    config::AuthConfig,
    db::repository::{
        role::RoleRepository,
        user::{User, UserRepository},
    },
    error::{Error, Result},
};

/// Creates the first administrator if the instance has no accounts at all.
///
/// Does nothing once any account exists — including one created by registration,
/// since that account is already an administrator. This is therefore safe to run
/// on every start-up, which is the only way it can be correct: a flag file or a
/// separate `init` command is one more thing to lose.
///
/// # Errors
///
/// Returns [`Error`] if the database is unreachable, if the configured
/// credentials fail their rules, or if hashing fails.
pub async fn ensure_admin(
    users: &UserRepository,
    roles: &RoleRepository,
    auth: &AuthConfig,
) -> Result<()> {
    // Checked whether or not an administrator is created below: the setting is
    // read at every registration, so a name nobody seeded turns the first
    // stranger's sign-up into a refusal with no explanation anywhere they can
    // see. Refusing to start says it once, to the person who can fix it.
    if roles.find_by_name(&auth.default_role).await?.is_none() {
        return Err(Error::bad_request(format!(
            "auth.default_role: no role is called `{}`",
            auth.default_role
        )));
    }

    if users.any_exist().await? {
        return Ok(());
    }

    // Privileged parsing: the shipped default is `admin`, which is on the
    // reserved list precisely so that nobody *else* can register it.
    let username = Username::parse_privileged(&auth.admin_username)
        .map_err(|error| Error::bad_request(format!("auth.admin_username: {error}")))?;
    let email = EmailAddress::parse(&auth.admin_email)
        .map_err(|error| Error::bad_request(format!("auth.admin_email: {error}")))?;

    let (password, generated) = admin_password(auth)?;
    let now = OffsetDateTime::now_utc();

    let user = User {
        id: UserId::generate(),
        username: username.clone(),
        email,
        password_hash: password::hash(password).await?,
        is_admin: true,
        // Only when we chose the password. One the operator set is one they
        // already know, and forcing a change on it is a pointless step; one we
        // printed has been through a log file and should not stay in force.
        must_change_password: generated.is_some(),
        state: AccountState::Active,
        deleted_at: None,
        deleted_by: None,
        email_verified_at: None,
        created_at: now,
        updated_at: now,
    };
    // The first account administers the instance, which is the whole point of
    // this function — so it goes straight into the role that says so.
    users.insert(&user, RoleId::parse(RoleId::ADMIN)?).await?;

    if let Some(printed) = generated {
        // Printed once, at `warn` so it survives a default log filter, and never
        // again — the digest is all that is stored, so this is the only moment
        // the value exists.
        tracing::warn!(
            username = %username,
            password = %printed,
            "created the first administrator with a generated password; sign in and change it"
        );
    } else {
        tracing::info!(username = %username, "created the first administrator");
    }
    Ok(())
}

/// The password to give the first administrator, and a copy to print if we were
/// the one who chose it.
fn admin_password(auth: &AuthConfig) -> Result<(Password, Option<String>)> {
    if auth.admin_password.is_empty() {
        let generated = Secret::generate_session().expose().to_owned();
        // Parsed against a policy that only has to accept our own output: an
        // operator who set `min_length` to 64 should not make start-up fail on a
        // password nobody types.
        let password = Password::parse(generated.clone(), PasswordPolicy::default())
            .map_err(|_| Error::bad_request("could not generate an administrator password"))?;
        return Ok((password, Some(generated)));
    }

    let password = Password::parse(auth.admin_password.clone(), auth.password)
        .map_err(|error| Error::bad_request(format!("auth.admin_password: {error}")))?;
    Ok((password, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> AuthConfig {
        crate::config::Config::default().auth
    }

    #[test]
    fn an_empty_configured_password_is_generated_and_returned_for_printing() {
        let (password, printed) = admin_password(&config()).expect("generated");
        let printed = printed.expect("a generated password must be reported");
        assert_eq!(password.expose(), printed);
        assert!(
            printed.len() >= PasswordPolicy::default().min_length,
            "a generated password must satisfy the default policy"
        );
    }

    #[test]
    fn a_configured_password_is_used_and_not_printed() {
        let mut auth = config();
        auth.admin_password = "a configured passphrase".to_owned();

        let (password, printed) = admin_password(&auth).expect("accepted");
        assert_eq!(password.expose(), "a configured passphrase");
        assert!(
            printed.is_none(),
            "a configured password must not be logged"
        );
    }

    /// An operator who sets a three-character password should be told at
    /// start-up, not discover it when the account cannot be used.
    #[test]
    fn a_configured_password_must_satisfy_the_instance_policy() {
        let mut auth = config();
        auth.admin_password = "short".to_owned();
        assert!(admin_password(&auth).is_err());
    }
}
