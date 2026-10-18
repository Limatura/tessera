//! `tracing` subscriber setup.

use std::sync::OnceLock;

use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::{
    config::{LogFormat, TelemetryConfig},
    log_files::FileLayer,
    state::AppState,
};

/// The layer copying lines into the log file, kept so [`attach`] can point it
/// at the files once the state exists.
static FILE: OnceLock<FileLayer> = OnceLock::new();

/// Failure to install the global subscriber.
#[derive(Debug, thiserror::Error)]
pub enum TelemetryError {
    /// A subscriber was already installed for this process.
    #[error("a tracing subscriber is already installed: {0}")]
    AlreadyInitialised(#[from] tracing_subscriber::util::TryInitError),
}

/// Installs the global `tracing` subscriber.
///
/// The filter directive has already been validated by
/// [`crate::config::Config::validate`], so a parse failure here would be a bug
/// and not an operator error, so it degrades to the directive's default instead
/// of aborting start-up.
///
/// # Errors
///
/// Returns [`TelemetryError::AlreadyInitialised`] if called twice in one process.
pub fn init(config: &TelemetryConfig) -> Result<(), TelemetryError> {
    let filter = EnvFilter::try_new(&config.filter).unwrap_or_else(|_| EnvFilter::new("info"));
    let file = FILE.get_or_init(FileLayer::new).clone();
    let registry = tracing_subscriber::registry().with(filter).with(file);

    match config.format {
        LogFormat::Json => registry
            .with(
                fmt::layer()
                    .json()
                    .flatten_event(true)
                    .with_current_span(true)
                    .with_span_list(false),
            )
            .try_init()?,
        LogFormat::Pretty => {
            registry
                .with(fmt::layer().with_ansi(config.ansi).with_target(true))
                .try_init()?;
        }
    }

    Ok(())
}

/// Starts writing the log file, under the state's `server.data_dir`.
///
/// Called once the state exists, which is after the database has been read:
/// until then the stored `logs.*` settings are not known. Lines logged before
/// this are held and written now.
pub fn attach(state: &AppState) {
    let Some(file) = FILE.get() else {
        return;
    };
    if let Err(error) = file.attach(&state.logs, state.config_handle()) {
        tracing::warn!(%error, "could not start writing the log file");
    }
}
