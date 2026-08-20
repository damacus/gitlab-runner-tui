use crate::client::normalize_host;
use crate::config::AppConfig;
use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

const KEYCHAIN_SERVICE: &str = "gitlab-runner-tui";
const CONTAINER_MARKER: &str = "GITLAB_RUNNER_TUI_CONTAINER";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenStorage {
    SystemKeychain,
    ConfigFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSource {
    Environment,
    ConfigFile,
    SystemKeychain,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogoutResult {
    pub config_rewritten: bool,
    pub keychain_removed: bool,
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

pub fn credential_source(
    config: &AppConfig,
    host: &str,
    environment_token: Option<&str>,
) -> Result<CredentialSource> {
    credential_source_with_loader(config, host, environment_token, load_token)
}

fn credential_source_with_loader<F>(
    config: &AppConfig,
    host: &str,
    environment_token: Option<&str>,
    load_keychain_token: F,
) -> Result<CredentialSource>
where
    F: FnOnce(&str) -> Result<Option<String>>,
{
    if environment_token.is_some() {
        return Ok(CredentialSource::Environment);
    }
    if config.gitlab_token.is_some() {
        return Ok(CredentialSource::ConfigFile);
    }
    if load_keychain_token(host)?.is_some() {
        return Ok(CredentialSource::SystemKeychain);
    }
    Ok(CredentialSource::Missing)
}

pub fn save_config(config: &AppConfig, host: &str, token: &str) -> Result<PathBuf> {
    let storage = TokenStorage::detect();

    if storage == TokenStorage::SystemKeychain {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &credential_account(host)?)
            .context("Failed to open the system keychain")?;
        entry
            .set_password(token)
            .context("Failed to save the GitLab token in the system keychain")?;
    }

    config_for_storage(config, token, storage).save_to_canonical_path()
}

pub fn migrate_legacy_config_token(config: &AppConfig, host: &str) -> Result<Option<PathBuf>> {
    migrate_legacy_config_token_with(config, host, TokenStorage::detect(), save_config)
}

pub fn logout(config: &AppConfig, host: &str, config_path: &Path) -> Result<LogoutResult> {
    logout_with(
        config,
        host,
        config_path,
        TokenStorage::detect(),
        delete_keychain_token,
    )
}

fn logout_with<F>(
    config: &AppConfig,
    host: &str,
    config_path: &Path,
    storage: TokenStorage,
    delete: F,
) -> Result<LogoutResult>
where
    F: FnOnce(&str) -> Result<bool>,
{
    let config_rewritten = if config_path.exists() {
        let mut persisted = config.clone();
        persisted.gitlab_token = None;
        persisted.save_to_path(config_path)?;
        true
    } else {
        false
    };

    let keychain_removed = match storage {
        TokenStorage::SystemKeychain => delete(host)?,
        TokenStorage::ConfigFile => false,
    };

    Ok(LogoutResult {
        config_rewritten,
        keychain_removed,
    })
}

fn delete_keychain_token(host: &str) -> Result<bool> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &credential_account(host)?)
        .context("Failed to open the system keychain")?;
    match entry.delete_credential() {
        Ok(()) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(error) => {
            Err(error).context("Failed to remove the GitLab token from the system keychain")
        }
    }
}

fn migrate_legacy_config_token_with<F>(
    config: &AppConfig,
    host: &str,
    storage: TokenStorage,
    save: F,
) -> Result<Option<PathBuf>>
where
    F: FnOnce(&AppConfig, &str, &str) -> Result<PathBuf>,
{
    if storage == TokenStorage::ConfigFile {
        return Ok(None);
    }

    let Some(token) = config.gitlab_token.as_deref() else {
        return Ok(None);
    };

    save(config, host, token).map(Some)
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
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = TEST_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "gitlab-runner-tui-credentials-{}-{sequence}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            Self(path)
        }

        fn config_path(&self) -> PathBuf {
            self.0.join("config.toml")
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

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

    #[test]
    fn legacy_token_migration_uses_the_effective_host() {
        let config = AppConfig {
            gitlab_token: Some("legacy-token".to_string()),
            ..AppConfig::default()
        };

        let migrated = migrate_legacy_config_token_with(
            &config,
            "https://override.example.com",
            TokenStorage::SystemKeychain,
            |saved_config, host, token| {
                assert!(saved_config.gitlab_host.is_none());
                assert_eq!(host, "https://override.example.com");
                assert_eq!(token, "legacy-token");
                Ok(PathBuf::from("config.toml"))
            },
        )
        .unwrap();

        assert_eq!(migrated, Some(PathBuf::from("config.toml")));
    }

    #[test]
    fn credential_source_uses_runtime_precedence_without_exposing_the_token() {
        let config = AppConfig {
            gitlab_token: Some("config-token".to_string()),
            ..AppConfig::default()
        };

        assert_eq!(
            credential_source_with_loader(
                &config,
                "https://gitlab.example.com",
                Some("environment-token"),
                |_| panic!("keychain must not be read when GITLAB_TOKEN is set")
            )
            .unwrap(),
            CredentialSource::Environment
        );
        assert_eq!(
            credential_source_with_loader(&config, "https://gitlab.example.com", None, |_| panic!(
                "keychain must not be read when config.toml has a token"
            ))
            .unwrap(),
            CredentialSource::ConfigFile
        );
        assert_eq!(
            credential_source_with_loader(
                &AppConfig::default(),
                "https://gitlab.example.com",
                None,
                |_| Ok(Some("keychain-token".to_string()))
            )
            .unwrap(),
            CredentialSource::SystemKeychain
        );
    }

    #[test]
    fn logout_rewrites_config_without_the_plaintext_token() {
        let directory = TestDirectory::new();
        let config_path = directory.config_path();
        let config = AppConfig {
            gitlab_host: Some("https://gitlab.example.com".to_string()),
            gitlab_token: Some("plaintext-token".to_string()),
            ..AppConfig::default()
        };
        config.save_to_path(&config_path).unwrap();

        let result = logout_with(
            &config,
            "https://gitlab.example.com",
            &config_path,
            TokenStorage::SystemKeychain,
            |host| {
                assert_eq!(host, "https://gitlab.example.com");
                Ok(true)
            },
        )
        .unwrap();
        let persisted = AppConfig::load_from_path(&config_path).unwrap();

        assert_eq!(
            result,
            LogoutResult {
                config_rewritten: true,
                keychain_removed: true
            }
        );
        assert!(persisted.gitlab_token.is_none());
        assert!(!std::fs::read_to_string(config_path)
            .unwrap()
            .contains("plaintext-token"));
    }
}
