//! The config file `tess login` writes: which instance, and the token for it.
//!
//! TOML, at `$XDG_CONFIG_HOME/tessera/config.toml`, which is
//! `~/.config/tessera/config.toml` unless set otherwise, on macOS too, and the
//! roaming application data directory on Windows.
//!
//! The file holds a credential, so it is written readable by its owner only.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::{Error, Result};

/// What the file holds. Both fields are optional so a person can write one by
/// hand with only a server in it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// The instance `send` and `get` use when no `--server` is given.
    pub server: Option<Url>,
    /// An API token for that instance, and only that one.
    pub token: Option<String>,
}

impl Config {
    /// The token to send to `server`: the saved one, only when `server` is the
    /// instance it was issued by.
    ///
    /// A token given to one instance must never reach another, which
    /// `--server` pointing somewhere else would otherwise do.
    #[must_use]
    pub fn token_for(&self, server: &Url) -> Option<&str> {
        let saved = self.server.as_ref()?;
        (saved.origin() == server.origin())
            .then_some(self.token.as_deref())
            .flatten()
    }
}

/// Where the file lives, whether or not it exists.
///
/// # Errors
///
/// When there is no home directory to put it in.
pub fn default_path() -> Result<PathBuf> {
    use etcetera::BaseStrategy as _;

    let strategy = etcetera::choose_base_strategy().map_err(|error| Error::Config {
        path: PathBuf::from("config.toml"),
        message: format!("no directory to keep it in: {error}"),
    })?;
    Ok(strategy.config_dir().join("tessera").join("config.toml"))
}

/// Reads the file at `path`. A file that does not exist is an empty config.
///
/// # Errors
///
/// When the file exists and cannot be read or does not parse.
pub fn load(path: &Path) -> Result<Config> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Config::default());
        }
        Err(error) => return Err(error.into()),
    };
    toml::from_str(&text).map_err(|error| Error::Config {
        path: path.to_owned(),
        message: error.to_string(),
    })
}

/// Writes `config` to `path`, creating its directory, readable by its owner
/// only.
///
/// # Errors
///
/// When the directory or the file cannot be written.
pub fn save(path: &Path, config: &Config) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = toml::to_string(config).map_err(|error| Error::Config {
        path: path.to_owned(),
        message: error.to_string(),
    })?;

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
        options.mode(0o600);
        // `mode` applies only to a file being created. One written before, by
        // hand or by an older version, is tightened here too.
        if let Ok(metadata) = std::fs::metadata(path) {
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            std::fs::set_permissions(path, permissions)?;
        }
    }
    let mut file = options.open(path)?;
    std::io::Write::write_all(&mut file, text.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(text: &str) -> Url {
        Url::parse(text).expect("a url")
    }

    #[test]
    fn a_token_goes_only_to_its_own_instance() {
        let config = Config {
            server: Some(url("https://share.example")),
            token: Some("tsk_secret".to_owned()),
        };
        assert_eq!(
            config.token_for(&url("https://share.example/")),
            Some("tsk_secret")
        );
        assert_eq!(config.token_for(&url("https://elsewhere.example")), None);
        assert_eq!(config.token_for(&url("http://share.example")), None);
    }

    #[test]
    fn a_missing_file_is_an_empty_config() {
        let dir = tempfile::tempdir().expect("a temp dir");
        assert_eq!(
            load(&dir.path().join("config.toml")).expect("a config"),
            Config::default()
        );
    }

    #[test]
    fn what_is_saved_loads_back() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = dir.path().join("tessera").join("config.toml");
        let config = Config {
            server: Some(url("https://share.example")),
            token: Some("tsk_secret".to_owned()),
        };
        save(&path, &config).expect("saved");
        assert_eq!(load(&path).expect("loaded"), config);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(&path)
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }
}
