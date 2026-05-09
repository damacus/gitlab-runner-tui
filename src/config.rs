use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RunnerDiscoveryMode {
    ConfiguredTargets,
    VisibleRunners,
    /// Calls /api/v4/runners/all (admin only); falls back to /api/v4/runners on 403.
    #[default]
    AllRunners,
}

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
    pub uncontacted_threshold_secs: u64,
    pub gitlab_host: Option<String>,
    pub gitlab_token: Option<String>,
    pub discovery_mode: RunnerDiscoveryMode,
    pub runner_targets: Vec<RunnerTarget>,
}

impl std::fmt::Debug for AppConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppConfig")
            .field("poll_interval_secs", &self.poll_interval_secs)
            .field("poll_timeout_secs", &self.poll_timeout_secs)
            .field(
                "uncontacted_threshold_secs",
                &self.uncontacted_threshold_secs,
            )
            .field("gitlab_host", &self.gitlab_host)
            .field(
                "gitlab_token",
                &self.gitlab_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("discovery_mode", &self.discovery_mode)
            .field("runner_targets", &self.runner_targets)
            .finish()
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: 30,
            poll_timeout_secs: 1800,
            uncontacted_threshold_secs: 3600,
            gitlab_host: None,
            gitlab_token: None,
            discovery_mode: RunnerDiscoveryMode::AllRunners,
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

    pub fn has_runner_targets(&self) -> bool {
        !self.runner_targets.is_empty()
    }

    pub fn requires_runner_targets(&self) -> bool {
        self.discovery_mode == RunnerDiscoveryMode::ConfiguredTargets
    }

    pub fn validate_runtime_settings(&self) -> Result<()> {
        if self.requires_runner_targets() && !self.has_runner_targets() {
            anyhow::bail!("Configured target discovery mode requires at least one runner target");
        }

        if self.poll_interval_secs == 0 {
            anyhow::bail!("Poll interval must be greater than zero seconds");
        }

        if self.uncontacted_threshold_secs == 0 {
            anyhow::bail!("Uncontacted threshold must be greater than zero seconds");
        }

        Ok(())
    }
}

pub fn format_runner_targets(targets: &[RunnerTarget]) -> String {
    targets
        .iter()
        .map(|target| {
            format!(
                "{}:{}",
                match target.kind {
                    RunnerTargetKind::Group => "group",
                    RunnerTargetKind::Project => "project",
                },
                target.id
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

pub fn parse_runner_targets(input: &str) -> Result<Vec<RunnerTarget>> {
    input
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(parse_runner_target)
        .collect()
}

fn parse_runner_target(entry: &str) -> Result<RunnerTarget> {
    let (kind, id) = entry
        .split_once(':')
        .context("Runner targets must use group:<id-or-path> or project:<id-or-path>")?;

    let id = id.trim();
    if id.is_empty() {
        anyhow::bail!("Runner target identifiers cannot be empty");
    }

    let kind = match kind.trim() {
        "group" => RunnerTargetKind::Group,
        "project" => RunnerTargetKind::Project,
        other => anyhow::bail!("Unsupported runner target kind: {other}"),
    };

    Ok(RunnerTarget {
        kind,
        id: id.to_string(),
        label: None,
    })
}

fn config_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // CWD config.toml
    if let Ok(cwd) = std::env::current_dir() {
        paths.push(cwd.join("config.toml"));
    }

    // ~/.config/gitlab-runner-tui/config.toml (canonical)
    if let Some(config_dir) = dirs::config_dir() {
        paths.push(config_dir.join("gitlab-runner-tui").join("config.toml"));
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
        assert_eq!(config.uncontacted_threshold_secs, 3600);
        assert!(config.gitlab_host.is_none());
        assert!(config.gitlab_token.is_none());
        assert_eq!(config.discovery_mode, RunnerDiscoveryMode::AllRunners);
        assert!(config.runner_targets.is_empty());
    }

    #[test]
    fn test_default_discovery_mode_matches_app_config_default() {
        // RunnerDiscoveryMode::default() and AppConfig::default().discovery_mode must agree
        // so that serde's #[serde(default)] fills missing fields with AllRunners, not VisibleRunners.
        assert_eq!(
            RunnerDiscoveryMode::default(),
            AppConfig::default().discovery_mode
        );
    }

    #[test]
    fn test_load_from_full_toml() {
        let toml_str = r#"
            poll_interval_secs = 60
            poll_timeout_secs = 900
            uncontacted_threshold_secs = 1200
            gitlab_host = "https://gitlab.example.com"
            gitlab_token = "glpat-test-token"
            discovery_mode = "visible_runners"
            
            [[runner_targets]]
            kind = "group"
            id = "my-org/platform"
            label = "Platform"
        "#;

        let config = AppConfig::load_from_str(toml_str).unwrap();
        assert_eq!(config.poll_interval_secs, 60);
        assert_eq!(config.poll_timeout_secs, 900);
        assert_eq!(config.uncontacted_threshold_secs, 1200);
        assert_eq!(
            config.gitlab_host,
            Some("https://gitlab.example.com".to_string())
        );
        assert_eq!(config.gitlab_token, Some("glpat-test-token".to_string()));
        assert_eq!(config.discovery_mode, RunnerDiscoveryMode::VisibleRunners);
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
        assert_eq!(config.uncontacted_threshold_secs, 3600);
        assert!(config.gitlab_host.is_none());
        assert!(config.gitlab_token.is_none());
        assert_eq!(config.discovery_mode, RunnerDiscoveryMode::AllRunners);
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
        assert_eq!(config.uncontacted_threshold_secs, 3600);
        assert_eq!(config.discovery_mode, RunnerDiscoveryMode::AllRunners);
        assert!(config.runner_targets.is_empty());
    }

    #[test]
    fn test_explicit_visible_runners_in_toml_is_respected() {
        // An existing config file that explicitly says visible_runners should NOT be upgraded.
        let toml_str = r#"discovery_mode = "visible_runners""#;
        let config = AppConfig::load_from_str(toml_str).unwrap();
        assert_eq!(config.discovery_mode, RunnerDiscoveryMode::VisibleRunners);
    }

    #[test]
    fn test_explicit_all_runners_in_toml_is_respected() {
        let toml_str = r#"discovery_mode = "all_runners""#;
        let config = AppConfig::load_from_str(toml_str).unwrap();
        assert_eq!(config.discovery_mode, RunnerDiscoveryMode::AllRunners);
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
    fn test_format_runner_targets() {
        let formatted = format_runner_targets(&[
            RunnerTarget {
                kind: RunnerTargetKind::Group,
                id: "org/platform".to_string(),
                label: None,
            },
            RunnerTarget {
                kind: RunnerTargetKind::Project,
                id: "123".to_string(),
                label: None,
            },
        ]);

        assert_eq!(formatted, "group:org/platform,project:123");
    }

    #[test]
    fn test_parse_runner_targets() {
        let targets = parse_runner_targets("group:org/platform, project:123").unwrap();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].kind, RunnerTargetKind::Group);
        assert_eq!(targets[1].kind, RunnerTargetKind::Project);
    }

    #[test]
    fn test_validate_runtime_settings_requires_targets_for_target_mode() {
        let config = AppConfig {
            discovery_mode: RunnerDiscoveryMode::ConfiguredTargets,
            ..AppConfig::default()
        };
        let error = config.validate_runtime_settings().unwrap_err().to_string();
        assert!(error.contains("requires at least one runner target"));
    }

    #[test]
    fn test_validate_runtime_settings_allows_visible_runners_without_targets() {
        let config = AppConfig {
            discovery_mode: RunnerDiscoveryMode::VisibleRunners,
            ..AppConfig::default()
        };

        assert!(config.validate_runtime_settings().is_ok());
    }

    #[test]
    fn test_validate_runtime_settings_rejects_zero_poll_interval() {
        let config = AppConfig {
            poll_interval_secs: 0,
            discovery_mode: RunnerDiscoveryMode::VisibleRunners,
            ..AppConfig::default()
        };

        let error = config.validate_runtime_settings().unwrap_err().to_string();
        assert!(error.contains("Poll interval"));
    }

    #[test]
    fn test_validate_runtime_settings_rejects_zero_uncontacted_threshold() {
        let config = AppConfig {
            uncontacted_threshold_secs: 0,
            discovery_mode: RunnerDiscoveryMode::VisibleRunners,
            ..AppConfig::default()
        };

        let error = config.validate_runtime_settings().unwrap_err().to_string();
        assert!(error.contains("Uncontacted threshold"));
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
            uncontacted_threshold_secs: 3600,
            gitlab_host: Some("https://gitlab.com".to_string()),
            gitlab_token: Some("glpat-secret-token".to_string()),
            discovery_mode: RunnerDiscoveryMode::ConfiguredTargets,
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
