//! `tracing` subscriber setup.

use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::{LogFormat, TelemetryConfig};

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
/// rather than an operator error; it degrades to the directive's default instead
/// of aborting start-up.
///
/// # Errors
///
/// Returns [`TelemetryError::AlreadyInitialised`] if called twice in one process.
pub fn init(config: &TelemetryConfig) -> Result<(), TelemetryError> {
    let filter = EnvFilter::try_new(&config.filter).unwrap_or_else(|_| EnvFilter::new("info"));
    let registry = tracing_subscriber::registry().with(filter);

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
