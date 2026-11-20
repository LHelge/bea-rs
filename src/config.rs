use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const CONFIG_FILE: &str = ".bears.yml";

const DEFAULT_ID_LENGTH: u8 = 3;
const MIN_ID_LENGTH: u8 = 2;
const MAX_ID_LENGTH: u8 = 8;

const DEFAULT_CHILD_ID_LENGTH: u8 = 2;
const MIN_CHILD_ID_LENGTH: u8 = 1;
const MAX_CHILD_ID_LENGTH: u8 = 8;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    /// Length of a root ID (`a1b`), and of every ID before hierarchical IDs.
    #[serde(rename = "id-length", default = "default_id_length")]
    pub id_length: u8,
    /// Starting length of each child segment of a hierarchical ID (the `c2`
    /// in `a1b.c2`). A parent's new children get longer segments once its
    /// sibling space fills up; see [`crate::task::generate_child_id`].
    ///
    /// Written to `.bears.yml` only when it differs from the default, so a
    /// fresh config file keeps its one-line shape.
    #[serde(
        rename = "child-id-length",
        default = "default_child_id_length",
        skip_serializing_if = "is_default_child_id_length"
    )]
    pub child_id_length: u8,
}

fn default_id_length() -> u8 {
    DEFAULT_ID_LENGTH
}

fn default_child_id_length() -> u8 {
    DEFAULT_CHILD_ID_LENGTH
}

fn is_default_child_id_length(length: &u8) -> bool {
    *length == DEFAULT_CHILD_ID_LENGTH
}

impl Default for Config {
    fn default() -> Self {
        Self {
            id_length: DEFAULT_ID_LENGTH,
            child_id_length: DEFAULT_CHILD_ID_LENGTH,
        }
    }
}

impl Config {
    /// Validate the config values, returning an error on invalid settings.
    pub fn validate(&self) -> Result<()> {
        if self.id_length < MIN_ID_LENGTH || self.id_length > MAX_ID_LENGTH {
            return Err(Error::InvalidConfig {
                reason: format!(
                    "id-length must be between {MIN_ID_LENGTH} and {MAX_ID_LENGTH}, got {}",
                    self.id_length
                ),
            });
        }
        if self.child_id_length < MIN_CHILD_ID_LENGTH || self.child_id_length > MAX_CHILD_ID_LENGTH
        {
            return Err(Error::InvalidConfig {
                reason: format!(
                    "child-id-length must be between {MIN_CHILD_ID_LENGTH} and {MAX_CHILD_ID_LENGTH}, got {}",
                    self.child_id_length
                ),
            });
        }
        Ok(())
    }
}

/// Path to the config file.
pub fn config_path(base: &Path) -> PathBuf {
    base.join(CONFIG_FILE)
}

/// Load config from `.bears.yml`. Returns default config if file doesn't exist.
pub fn load(base: &Path) -> Result<Config> {
    let path = config_path(base);
    if !path.exists() {
        return Ok(Config::default());
    }
    let content = fs::read_to_string(&path)?;
    let config: Config = crate::task::from_yaml(&content)?;
    config.validate()?;
    Ok(config)
}

/// Write the default config to `.bears.yml`.
/// Leaves an existing config file untouched so re-running `bea init` is safe.
pub fn create_default(base: &Path) -> Result<PathBuf> {
    let path = config_path(base);
    if path.exists() {
        return Ok(path);
    }
    let config = Config::default();
    let content = crate::task::to_yaml(&config)?;
    fs::write(&path, content)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.id_length, 3);
        assert_eq!(config.child_id_length, 2);
    }

    #[test]
    fn test_load_missing_file_returns_default() {
        let tmp = TempDir::new().unwrap();
        let config = load(tmp.path()).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn test_create_default_file_content() {
        let tmp = TempDir::new().unwrap();
        let path = create_default(tmp.path()).unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), "id-length: 3\n");
    }

    #[test]
    fn test_create_and_load() {
        let tmp = TempDir::new().unwrap();
        create_default(tmp.path()).unwrap();
        let config = load(tmp.path()).unwrap();
        assert_eq!(config.id_length, 3);
    }

    #[test]
    fn test_create_default_preserves_existing_config() {
        let tmp = TempDir::new().unwrap();
        fs::write(config_path(tmp.path()), "id-length: 6\n").unwrap();
        create_default(tmp.path()).unwrap();
        let config = load(tmp.path()).unwrap();
        assert_eq!(config.id_length, 6, "re-init must not clobber config");
    }

    #[test]
    fn test_load_custom_id_length() {
        let tmp = TempDir::new().unwrap();
        fs::write(config_path(tmp.path()), "id-length: 6\n").unwrap();
        let config = load(tmp.path()).unwrap();
        assert_eq!(config.id_length, 6);
    }

    #[test]
    fn test_validate_too_small() {
        let config = Config {
            id_length: 1,
            ..Config::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_too_large() {
        let config = Config {
            id_length: 10,
            ..Config::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_bounds() {
        Config {
            id_length: 2,
            ..Config::default()
        }
        .validate()
        .unwrap();
        Config {
            id_length: 8,
            ..Config::default()
        }
        .validate()
        .unwrap();
    }

    #[test]
    fn test_load_invalid_id_length() {
        let tmp = TempDir::new().unwrap();
        fs::write(config_path(tmp.path()), "id-length: 1\n").unwrap();
        assert!(load(tmp.path()).is_err());
    }

    #[test]
    fn test_missing_id_length_uses_default() {
        let tmp = TempDir::new().unwrap();
        fs::write(config_path(tmp.path()), "{}\n").unwrap();
        let config = load(tmp.path()).unwrap();
        assert_eq!(config.id_length, DEFAULT_ID_LENGTH);
    }

    #[test]
    fn test_load_custom_child_id_length() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            config_path(tmp.path()),
            "id-length: 4\nchild-id-length: 3\n",
        )
        .unwrap();
        let config = load(tmp.path()).unwrap();
        assert_eq!(config.id_length, 4);
        assert_eq!(config.child_id_length, 3);
    }

    #[test]
    fn test_missing_child_id_length_uses_default() {
        let tmp = TempDir::new().unwrap();
        fs::write(config_path(tmp.path()), "id-length: 5\n").unwrap();
        let config = load(tmp.path()).unwrap();
        assert_eq!(config.child_id_length, DEFAULT_CHILD_ID_LENGTH);
    }

    #[test]
    fn test_validate_child_id_length_bounds() {
        let with = |child_id_length| Config {
            child_id_length,
            ..Config::default()
        };
        with(1).validate().unwrap();
        with(8).validate().unwrap();
        assert!(with(0).validate().is_err());
        let err = with(9).validate().unwrap_err().to_string();
        assert!(err.contains("child-id-length"), "{err}");
    }

    #[test]
    fn test_load_invalid_child_id_length() {
        let tmp = TempDir::new().unwrap();
        fs::write(config_path(tmp.path()), "child-id-length: 0\n").unwrap();
        assert!(load(tmp.path()).is_err());
    }

    #[test]
    fn test_non_default_child_id_length_is_written() {
        let config = Config {
            child_id_length: 3,
            ..Config::default()
        };
        assert_eq!(
            crate::task::to_yaml(&config).unwrap(),
            "id-length: 3\nchild-id-length: 3\n"
        );
    }
}
