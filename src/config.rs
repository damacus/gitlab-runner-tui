use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RunnerTargetKind {
    Group,
    Project,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct RunnerTarget {
    pub kind: RunnerTargetKind,
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, PartialEq)]
#[serde(default)]
pub struct AppConfig {
    pub poll_interval_secs: u64,
    pub poll_timeout_secs: u64,
    pub gitlab_host: Option<String>,
    pub gitlab_token: Option<String>,
    pub runner_targets: Vec<RunnerTarget>,
}

impl std::fmt::Debug for AppConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppConfig")
            .field("poll_interval_secs", &self.poll_interval_secs)
            .field("poll_timeout_secs", &self.poll_timeout_secs)
            .field("gitlab_host", &self.gitlab_host)
            .field(
                "gitlab_token",
                &self.gitlab_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("runner_targets", &self.runner_targets)
            .finish()
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: 30,
            poll_timeout_secs: 1800,
            gitlab_host: None,
            gitlab_token: None,
            runner_targets: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        // Try CWD config.toml first, then canonical and legacy config directories.
        let paths = config_paths();

        for path in paths {
            if path.exists() {
                let contents = std::fs::read_to_string(&path)?;
                let config: AppConfig = toml::from_str(&contents)?;
                return Ok(config);
            }
        }

        Ok(AppConfig::default())
    }

    pub fn canonical_path() -> Result<PathBuf> {
        dirs::config_dir()
            .map(|config_dir| config_dir.join("gitlab-runner-tui").join("config.toml"))
            .context("Could not determine a config directory for gitlab-runner-tui")
    }

    pub fn save_to_canonical_path(&self) -> Result<PathBuf> {
        let path = Self::canonical_path()?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let contents = toml::to_string_pretty(self)?;
        std::fs::write(&path, contents)?;

        Ok(path)
    }

    #[cfg(test)]
    pub fn load_from_str(toml_str: &str) -> Result<Self> {
        let config: AppConfig = toml::from_str(toml_str)?;
        Ok(config)
    }

    #[cfg(test)]
    pub fn has_runner_targets(&self) -> bool {
        !self.runner_targets.is_empty()
    }
}

fn config_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // CWD config.toml
    if let Ok(cwd) = std::env::current_dir() {
        paths.push(cwd.join("config.toml"));
    }

    // ~/.config/gitlab-runner-tui/config.toml (canonical)
    // ~/.config/igor/config.toml (legacy fallback for backward compatibility)
    if let Some(config_dir) = dirs::config_dir() {
        paths.push(config_dir.join("gitlab-runner-tui").join("config.toml"));
        paths.push(config_dir.join("igor").join("config.toml"));
    }

    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.poll_interval_secs, 30);
        assert_eq!(config.poll_timeout_secs, 1800);
        assert!(config.gitlab_host.is_none());
        assert!(config.gitlab_token.is_none());
        assert!(config.runner_targets.is_empty());
    }

    #[test]
    fn test_load_from_full_toml() {
        let toml_str = r#"
            poll_interval_secs = 60
            poll_timeout_secs = 900
            gitlab_host = "https://gitlab.example.com"
            gitlab_token = "glpat-test-token"
            
            [[runner_targets]]
            kind = "group"
            id = "my-org/platform"
            label = "Platform"
        "#;

        let config = AppConfig::load_from_str(toml_str).unwrap();
        assert_eq!(config.poll_interval_secs, 60);
        assert_eq!(config.poll_timeout_secs, 900);
        assert_eq!(
            config.gitlab_host,
            Some("https://gitlab.example.com".to_string())
        );
        assert_eq!(config.gitlab_token, Some("glpat-test-token".to_string()));
        assert_eq!(
            config.runner_targets,
            vec![RunnerTarget {
                kind: RunnerTargetKind::Group,
                id: "my-org/platform".to_string(),
                label: Some("Platform".to_string()),
            }]
        );
    }

    #[test]
    fn test_load_from_partial_toml_uses_defaults() {
        let toml_str = r#"
            poll_interval_secs = 10
        "#;

        let config = AppConfig::load_from_str(toml_str).unwrap();
        assert_eq!(config.poll_interval_secs, 10);
        assert_eq!(config.poll_timeout_secs, 1800);
        assert!(config.gitlab_host.is_none());
        assert!(config.gitlab_token.is_none());
        assert!(config.runner_targets.is_empty());
    }

    #[test]
    fn test_load_from_empty_toml_uses_defaults() {
        let toml_str = "";
        let config = AppConfig::load_from_str(toml_str).unwrap();
        assert_eq!(config, AppConfig::default());
    }

    #[test]
    fn test_load_from_invalid_toml_returns_error() {
        let toml_str = "this is not valid toml [[[";
        let result = AppConfig::load_from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_from_toml_with_only_host() {
        let toml_str = r#"
            gitlab_host = "https://gitlab.com"
        "#;

        let config = AppConfig::load_from_str(toml_str).unwrap();
        assert_eq!(config.gitlab_host, Some("https://gitlab.com".to_string()));
        assert_eq!(config.poll_interval_secs, 30);
        assert!(config.runner_targets.is_empty());
    }

    #[test]
    fn test_load_from_toml_with_mixed_runner_targets() {
        let toml_str = r#"
            [[runner_targets]]
            kind = "group"
            id = "my-org/platform"

            [[runner_targets]]
            kind = "project"
            id = "12345"
            label = "App Project"
        "#;

        let config = AppConfig::load_from_str(toml_str).unwrap();
        assert_eq!(
            config.runner_targets,
            vec![
                RunnerTarget {
                    kind: RunnerTargetKind::Group,
                    id: "my-org/platform".to_string(),
                    label: None,
                },
                RunnerTarget {
                    kind: RunnerTargetKind::Project,
                    id: "12345".to_string(),
                    label: Some("App Project".to_string()),
                }
            ]
        );
    }

    #[test]
    fn test_has_runner_targets() {
        let mut config = AppConfig::default();
        assert!(!config.has_runner_targets());

        config.runner_targets.push(RunnerTarget {
            kind: RunnerTargetKind::Group,
            id: "my-org/platform".to_string(),
            label: None,
        });

        assert!(config.has_runner_targets());
    }

    #[test]
    fn test_config_paths_includes_cwd() {
        let paths = config_paths();
        assert!(!paths.is_empty());
        // First path should be CWD/config.toml
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(paths[0], cwd.join("config.toml"));
    }

    #[test]
    fn test_config_paths_includes_canonical_and_legacy_dirs() {
        let paths = config_paths();

        if let Some(config_dir) = dirs::config_dir() {
            assert!(paths.contains(&config_dir.join("gitlab-runner-tui").join("config.toml")));
            assert!(paths.contains(&config_dir.join("igor").join("config.toml")));
        }
    }

    #[test]
    fn test_canonical_path_uses_gitlab_runner_tui_dir() {
        if let Some(config_dir) = dirs::config_dir() {
            assert_eq!(
                AppConfig::canonical_path().unwrap(),
                config_dir.join("gitlab-runner-tui").join("config.toml")
            );
        }
    }

    #[test]
    fn test_debug_redacts_token() {
        let config = AppConfig {
            poll_interval_secs: 30,
            poll_timeout_secs: 1800,
            gitlab_host: Some("https://gitlab.com".to_string()),
            gitlab_token: Some("glpat-secret-token".to_string()),
            runner_targets: vec![RunnerTarget {
                kind: RunnerTargetKind::Group,
                id: "my-org/platform".to_string(),
                label: Some("Platform".to_string()),
            }],
        };

        let debug_output = format!("{:?}", config);

        assert!(!debug_output.contains("glpat-secret-token"));
        assert!(debug_output.contains("[REDACTED]"));
        assert!(debug_output.contains("https://gitlab.com"));
    }
}
