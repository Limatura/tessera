//! The config file `tess login` writes: which instance, and the token for it,
//! and how `tess ui` should behave, which a person writes by hand.
//!
//! TOML, at `$XDG_CONFIG_HOME/tessera/config.toml`, which is
//! `~/.config/tessera/config.toml` unless set otherwise, on macOS too, and the
//! roaming application data directory on Windows.
//!
//! The file holds a credential, so it is written readable by its owner only.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

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
    /// `[ui]`: how `tess ui` behaves.
    #[serde(default, skip_serializing_if = "Ui::is_default")]
    pub ui: Ui,
}

/// The `[ui]` table. Every key may be left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ui {
    /// Which set of keys to start from.
    pub keys: Preset,
    /// Whether the view takes the mouse. Selecting text still copies it.
    pub mouse: bool,
    /// Whether files whose names start with a dot are listed.
    pub show_hidden: bool,
    /// Keys for actions, by action name, replacing the preset's keys for that
    /// action: `send = "ctrl-s"`, or several, `up = ["k", "up"]`.
    pub bindings: BTreeMap<String, Keys>,
}

impl Default for Ui {
    fn default() -> Self {
        Self {
            keys: Preset::default(),
            mouse: true,
            show_hidden: false,
            bindings: BTreeMap::new(),
        }
    }
}

impl Ui {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// A set of keys to start from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    /// Arrows and the mouse; typing a name filters the list, and commands are
    /// on Ctrl and the function keys.
    #[default]
    Standard,
    /// `hjkl` to move and single letters for commands; `/` to filter.
    Vim,
}

/// One key, or several, for an action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Keys {
    /// One key.
    One(String),
    /// Several, any of which does it.
    Many(Vec<String>),
}

impl Keys {
    /// Each key, as written.
    #[must_use]
    pub fn each(&self) -> &[String] {
        match self {
            Self::One(key) => std::slice::from_ref(key),
            Self::Many(keys) => keys,
        }
    }
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
    let text = toml::to_string(config).map_err(|error| Error::Config {
        path: path.to_owned(),
        message: error.to_string(),
    })?;
    write_private(path, text.as_bytes())
}

/// Writes `bytes` to `path`, creating its directory, readable by its owner
/// only: for files that hold a credential.
///
/// # Errors
///
/// When the directory or the file cannot be written.
pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
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
    std::io::Write::write_all(&mut file, bytes)?;
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
            ..Config::default()
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
            ..Config::default()
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

    #[test]
    fn a_ui_table_written_by_hand_reads() {
        let config: Config = toml::from_str(
            r#"
            server = "https://share.example"

            [ui]
            keys = "vim"
            mouse = false

            [ui.bindings]
            send = "ctrl-s"
            up = ["k", "up"]
            "#,
        )
        .expect("a config");
        assert_eq!(config.ui.keys, Preset::Vim);
        assert!(!config.ui.mouse);
        assert!(!config.ui.show_hidden);
        assert_eq!(
            config.ui.bindings.get("up").map(Keys::each),
            Some(["k".to_owned(), "up".to_owned()].as_slice())
        );
    }

    #[test]
    fn a_config_without_ui_writes_without_it() {
        let config = Config {
            server: Some(url("https://share.example")),
            ..Config::default()
        };
        let text = toml::to_string(&config).expect("written");
        assert!(!text.contains("[ui]"), "{text}");
    }
}
