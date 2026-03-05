use anyhow::Result;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct AppConfig {
    pub poll_interval_secs: u64,
    pub poll_timeout_secs: u64,
    pub gitlab_host: Option<String>,
    pub gitlab_token: Option<String>,
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
        }
    }
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        // Try ~/.config/gitlab-runner-tui/config.toml
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

    #[cfg(test)]
    pub fn load_from_str(toml_str: &str) -> Result<Self> {
        let config: AppConfig = toml::from_str(toml_str)?;
        Ok(config)
    }
}

fn config_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // ~/.config/gitlab-runner-tui/config.toml
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
        assert!(config.gitlab_host.is_none());
        assert!(config.gitlab_token.is_none());
    }

    #[test]
    fn test_load_from_full_toml() {
        let toml_str = r#"
            poll_interval_secs = 60
            poll_timeout_secs = 900
            gitlab_host = "https://gitlab.example.com"
            gitlab_token = "glpat-test-token"
        "#;

        let config = AppConfig::load_from_str(toml_str).unwrap();
        assert_eq!(config.poll_interval_secs, 60);
        assert_eq!(config.poll_timeout_secs, 900);
        assert_eq!(
            config.gitlab_host,
            Some("https://gitlab.example.com".to_string())
        );
        assert_eq!(config.gitlab_token, Some("glpat-test-token".to_string()));
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
    }

    #[test]
    fn test_config_paths_includes_config_dir() {
        let paths = config_paths();
        if let Some(config_dir) = dirs::config_dir() {
            assert_eq!(paths.len(), 1);
            assert_eq!(
                paths[0],
                config_dir.join("gitlab-runner-tui").join("config.toml")
            );
        } else {
            assert!(paths.is_empty());
        }
    }

    #[test]
    fn test_debug_redacts_token() {
        let config = AppConfig {
            poll_interval_secs: 30,
            poll_timeout_secs: 1800,
            gitlab_host: Some("https://gitlab.com".to_string()),
            gitlab_token: Some("glpat-secret-token".to_string()),
        };

        let debug_output = format!("{:?}", config);

        // Ensure sensitive token is not present
        assert!(!debug_output.contains("glpat-secret-token"));

        // Ensure redacted string is present
        assert!(debug_output.contains("[REDACTED]"));

        // Ensure other non-sensitive fields are still visible
        assert!(debug_output.contains("https://gitlab.com"));
    }
}
