use crate::client::normalize_host;
use crate::config::AppConfig;
use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::path::PathBuf;

const KEYCHAIN_SERVICE: &str = "gitlab-runner-tui";
const CONTAINER_MARKER: &str = "GITLAB_RUNNER_TUI_CONTAINER";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenStorage {
    SystemKeychain,
    ConfigFile,
}

impl TokenStorage {
    fn detect() -> Self {
        Self::from_container_marker(std::env::var_os(CONTAINER_MARKER).as_deref())
    }

    fn from_container_marker(marker: Option<&OsStr>) -> Self {
        match marker.and_then(OsStr::to_str) {
            Some("1") => Self::ConfigFile,
            _ => Self::SystemKeychain,
        }
    }
}

pub fn load_token(host: &str) -> Result<Option<String>> {
    if TokenStorage::detect() == TokenStorage::ConfigFile {
        return Ok(None);
    }

    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &credential_account(host)?)
        .context("Failed to open the system keychain")?;
    match entry.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => {
            Err(error).context("Failed to read the GitLab token from the system keychain")
        }
    }
}

pub fn save_config(config: &AppConfig, token: &str) -> Result<PathBuf> {
    let storage = TokenStorage::detect();

    if storage == TokenStorage::SystemKeychain {
        let host = config
            .gitlab_host
            .as_deref()
            .context("GitLab host is required before saving credentials")?;
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &credential_account(host)?)
            .context("Failed to open the system keychain")?;
        entry
            .set_password(token)
            .context("Failed to save the GitLab token in the system keychain")?;
    }

    config_for_storage(config, token, storage).save_to_canonical_path()
}

pub fn migrate_legacy_config_token(config: &AppConfig) -> Result<Option<PathBuf>> {
    if TokenStorage::detect() == TokenStorage::ConfigFile {
        return Ok(None);
    }

    let Some(token) = config.gitlab_token.as_deref() else {
        return Ok(None);
    };

    save_config(config, token).map(Some)
}

fn config_for_storage(config: &AppConfig, token: &str, storage: TokenStorage) -> AppConfig {
    let mut persisted = config.clone();
    persisted.gitlab_token = match storage {
        TokenStorage::SystemKeychain => None,
        TokenStorage::ConfigFile => Some(token.to_string()),
    };
    persisted
}

fn credential_account(host: &str) -> Result<String> {
    normalize_host(host, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_runs_default_to_the_system_keychain() {
        assert_eq!(
            TokenStorage::from_container_marker(None),
            TokenStorage::SystemKeychain
        );
        assert_eq!(
            TokenStorage::from_container_marker(Some(OsStr::new("0"))),
            TokenStorage::SystemKeychain
        );
    }

    #[test]
    fn official_container_marker_selects_config_storage() {
        assert_eq!(
            TokenStorage::from_container_marker(Some(OsStr::new("1"))),
            TokenStorage::ConfigFile
        );
    }

    #[test]
    fn system_keychain_config_does_not_serialize_the_token() {
        let config = AppConfig {
            gitlab_host: Some("https://gitlab.example.com".to_string()),
            gitlab_token: Some("legacy-token".to_string()),
            ..AppConfig::default()
        };

        let persisted =
            config_for_storage(&config, "replacement-token", TokenStorage::SystemKeychain);
        let serialized = toml::to_string(&persisted).unwrap();

        assert_eq!(persisted.gitlab_host, config.gitlab_host);
        assert!(persisted.gitlab_token.is_none());
        assert!(!serialized.contains("gitlab_token"));
        assert!(!serialized.contains("replacement-token"));
        assert!(!serialized.contains("legacy-token"));
    }

    #[test]
    fn container_config_keeps_the_token_for_the_mounted_volume() {
        let config = AppConfig {
            gitlab_host: Some("https://gitlab.example.com".to_string()),
            ..AppConfig::default()
        };

        let persisted = config_for_storage(&config, "container-token", TokenStorage::ConfigFile);

        assert_eq!(persisted.gitlab_token.as_deref(), Some("container-token"));
    }

    #[test]
    fn keychain_account_ignores_surrounding_space_and_trailing_slashes() {
        assert_eq!(
            credential_account(" https://GITLAB.example.com/// ").unwrap(),
            "https://gitlab.example.com"
        );
        assert_eq!(
            credential_account("gitlab.example.com").unwrap(),
            "https://gitlab.example.com"
        );
    }
}
