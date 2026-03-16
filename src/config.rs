use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const CONFIG_FILE: &str = ".bears.yml";

const DEFAULT_ID_LENGTH: u8 = 4;
const MIN_ID_LENGTH: u8 = 3;
const MAX_ID_LENGTH: u8 = 8;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(rename = "id-length", default = "default_id_length")]
    pub id_length: u8,
}

fn default_id_length() -> u8 {
    DEFAULT_ID_LENGTH
}

impl Default for Config {
    fn default() -> Self {
        Self {
            id_length: DEFAULT_ID_LENGTH,
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
    let config: Config = serde_yaml::from_str(&content)?;
    config.validate()?;
    Ok(config)
}

/// Write the default config to `.bears.yml`.
pub fn create_default(base: &Path) -> Result<PathBuf> {
    let path = config_path(base);
    let config = Config::default();
    let content = serde_yaml::to_string(&config)?;
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
        assert_eq!(config.id_length, 4);
    }

    #[test]
    fn test_load_missing_file_returns_default() {
        let tmp = TempDir::new().unwrap();
        let config = load(tmp.path()).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn test_create_and_load() {
        let tmp = TempDir::new().unwrap();
        create_default(tmp.path()).unwrap();
        let config = load(tmp.path()).unwrap();
        assert_eq!(config.id_length, 4);
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
        let config = Config { id_length: 2 };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_too_large() {
        let config = Config { id_length: 10 };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_bounds() {
        Config { id_length: 4 }.validate().unwrap();
        Config { id_length: 8 }.validate().unwrap();
    }

    #[test]
    fn test_load_invalid_id_length() {
        let tmp = TempDir::new().unwrap();
        fs::write(config_path(tmp.path()), "id-length: 2\n").unwrap();
        assert!(load(tmp.path()).is_err());
    }

    #[test]
    fn test_missing_id_length_uses_default() {
        let tmp = TempDir::new().unwrap();
        fs::write(config_path(tmp.path()), "{}\n").unwrap();
        let config = load(tmp.path()).unwrap();
        assert_eq!(config.id_length, DEFAULT_ID_LENGTH);
    }
}
