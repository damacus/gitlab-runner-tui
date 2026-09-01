use anyhow::{Context, Result};
use etcetera::{choose_base_strategy, BaseStrategy};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub const DEFAULT_MAX_ENRICHMENT_REQUESTS: usize = 10;
pub const MIN_MAX_ENRICHMENT_REQUESTS: usize = 2;
pub const MAX_MAX_ENRICHMENT_REQUESTS: usize = 64;

const CONFIG_DIRECTORY_NAME: &str = "gitlab-runner-tui";
const CONFIG_FILE_NAME: &str = "config.toml";

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfigPlatform {
    Linux,
    MacOs,
    Windows,
}

#[cfg(test)]
fn resolve_config_path(platform: ConfigPlatform, home_dir: &Path, config_dir: &Path) -> PathBuf {
    let config_dir = match platform {
        ConfigPlatform::MacOs => home_dir.join("Library").join("Application Support"),
        ConfigPlatform::Linux | ConfigPlatform::Windows => config_dir.to_path_buf(),
    };

    config_path(config_dir)
}

fn config_path(config_dir: PathBuf) -> PathBuf {
    config_dir
        .join(CONFIG_DIRECTORY_NAME)
        .join(CONFIG_FILE_NAME)
}

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
    pub max_enrichment_requests: usize,
    pub gitlab_host: Option<String>,
    pub gitlab_token: Option<String>,
    pub discovery_mode: RunnerDiscoveryMode,
    pub runner_targets: Vec<RunnerTarget>,
    pub rotation_wait: RotationWaitConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct RotationWaitConfig {
    pub timezone: Option<String>,
    pub rotation_window_start: Option<String>,
    pub active_contacted_within_secs: u64,
    pub missing_runner_grace_polls: u64,
    pub completion_stability_polls: u64,
}

impl Default for RotationWaitConfig {
    fn default() -> Self {
        Self {
            timezone: None,
            rotation_window_start: None,
            active_contacted_within_secs: 3600,
            missing_runner_grace_polls: 2,
            completion_stability_polls: 2,
        }
    }
}

impl std::fmt::Debug for AppConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppConfig")
            .field("poll_interval_secs", &self.poll_interval_secs)
            .field("poll_timeout_secs", &self.poll_timeout_secs)
            .field("max_enrichment_requests", &self.max_enrichment_requests)
            .field("gitlab_host", &self.gitlab_host)
            .field(
                "gitlab_token",
                &self.gitlab_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("discovery_mode", &self.discovery_mode)
            .field("runner_targets", &self.runner_targets)
            .field("rotation_wait", &self.rotation_wait)
            .finish()
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: 30,
            poll_timeout_secs: 1800,
            max_enrichment_requests: DEFAULT_MAX_ENRICHMENT_REQUESTS,
            gitlab_host: None,
            gitlab_token: None,
            discovery_mode: RunnerDiscoveryMode::AllRunners,
            runner_targets: Vec::new(),
            rotation_wait: RotationWaitConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        Self::load_from_path(&Self::canonical_path()?)
    }

    pub fn load_from_path(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(AppConfig::default());
        }

        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file {}", path.display()))?;
        toml::from_str(&contents)
            .with_context(|| format!("Failed to parse config file {}", path.display()))
    }

    pub fn canonical_path() -> Result<PathBuf> {
        let base_strategy = choose_base_strategy()
            .context("Could not determine a config directory for gitlab-runner-tui")?;

        #[cfg(target_os = "macos")]
        let config_dir = base_strategy
            .home_dir()
            .join("Library")
            .join("Application Support");
        #[cfg(not(target_os = "macos"))]
        let config_dir = base_strategy.config_dir();

        Ok(config_path(config_dir))
    }

    pub fn save_to_canonical_path(&self) -> Result<PathBuf> {
        let path = Self::canonical_path()?;
        self.save_to_path(&path)?;
        Ok(path)
    }

    pub(crate) fn save_to_path(&self, path: &Path) -> Result<()> {
        self.save_to_path_with_hook(path, |_| Ok(()))
    }

    fn save_to_path_with_hook<F>(&self, path: &Path, before_replace: F) -> Result<()>
    where
        F: FnOnce(&Path) -> Result<()>,
    {
        reject_symlink_target(path)?;
        let parent = path
            .parent()
            .context("Config path must have a parent directory")?;
        create_secure_config_dir(parent)?;

        let contents = toml::to_string_pretty(self)?;
        let temporary_path = temporary_path_for(path)?;
        let mut temporary_file = open_secure_temporary_file(&temporary_path)?;
        let mut temporary_guard = TemporaryFileGuard::new(temporary_path.clone());
        temporary_file.write_all(contents.as_bytes())?;
        temporary_file.flush()?;
        temporary_file.sync_all()?;
        drop(temporary_file);

        before_replace(&temporary_path)?;
        reject_symlink_target(path)?;
        atomic_replace(&temporary_path, path)?;
        temporary_guard.disarm();
        sync_parent_directory(parent)?;
        Ok(())
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

        if self.poll_timeout_secs == 0 {
            anyhow::bail!("Poll timeout must be greater than zero seconds");
        }

        self.validate_enrichment_request_limit()?;

        if self.rotation_wait.active_contacted_within_secs == 0 {
            anyhow::bail!(
                "Rotation wait active contact threshold must be greater than zero seconds"
            );
        }

        if self.rotation_wait.missing_runner_grace_polls == 0 {
            anyhow::bail!("Rotation wait missing runner grace polls must be greater than zero");
        }

        if self.rotation_wait.completion_stability_polls == 0 {
            anyhow::bail!("Rotation wait completion stability polls must be greater than zero");
        }

        Ok(())
    }

    pub fn validate_enrichment_request_limit(&self) -> Result<()> {
        if !(MIN_MAX_ENRICHMENT_REQUESTS..=MAX_MAX_ENRICHMENT_REQUESTS)
            .contains(&self.max_enrichment_requests)
        {
            anyhow::bail!(
                "Maximum enrichment requests must be between {MIN_MAX_ENRICHMENT_REQUESTS} and {MAX_MAX_ENRICHMENT_REQUESTS}"
            );
        }

        Ok(())
    }
}

fn reject_symlink_target(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            anyhow::bail!(
                "Refusing to write config through symlink {}",
                path.display()
            )
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error)
            .with_context(|| format!("Failed to inspect config target {}", path.display())),
    }
}

fn create_secure_config_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)
        .with_context(|| format!("Failed to create config directory {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .with_context(|| format!("Failed to secure config directory {}", path.display()))?;
    }

    Ok(())
}

fn temporary_path_for(path: &Path) -> Result<PathBuf> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("Config path must have a valid file name")?;
    let sequence = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    Ok(path.with_file_name(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        sequence
    )))
}

fn open_secure_temporary_file(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let file = options
        .open(path)
        .with_context(|| format!("Failed to create temporary config {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .with_context(|| format!("Failed to secure temporary config {}", path.display()))?;
    }

    Ok(file)
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> Result<()> {
    fs::rename(source, destination).with_context(|| {
        format!(
            "Failed to atomically replace config {}",
            destination.display()
        )
    })
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "Kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }

    let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination_wide: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();

    // SAFETY: both pointers reference NUL-terminated UTF-16 buffers that remain alive for the
    // duration of the call. The flags request an atomic replacement with durable write-through.
    let replaced = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        return Err(std::io::Error::last_os_error()).with_context(|| {
            format!(
                "Failed to atomically replace config {}",
                destination.display()
            )
        });
    }
    Ok(())
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .with_context(|| format!("Failed to sync config directory {}", path.display()))
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> Result<()> {
    Ok(())
}

struct TemporaryFileGuard {
    path: PathBuf,
    armed: bool,
}

impl TemporaryFileGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TemporaryFileGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let sequence = TEST_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "gitlab-runner-tui-{label}-{}-{sequence}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            Self(path)
        }

        fn config_path(&self) -> PathBuf {
            self.0.join("config.toml")
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.poll_interval_secs, 30);
        assert_eq!(config.poll_timeout_secs, 1800);
        assert_eq!(
            config.max_enrichment_requests,
            DEFAULT_MAX_ENRICHMENT_REQUESTS
        );
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
            max_enrichment_requests = 12
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
        assert_eq!(config.max_enrichment_requests, 12);
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
        assert_eq!(
            config.max_enrichment_requests,
            DEFAULT_MAX_ENRICHMENT_REQUESTS
        );
        assert_eq!(config.rotation_wait.active_contacted_within_secs, 3600);
        assert_eq!(config.rotation_wait.missing_runner_grace_polls, 2);
        assert_eq!(config.rotation_wait.completion_stability_polls, 2);
        assert_eq!(config.rotation_wait.timezone, None);
        assert_eq!(config.rotation_wait.rotation_window_start, None);
        assert!(config.gitlab_host.is_none());
        assert!(config.gitlab_token.is_none());
        assert_eq!(config.discovery_mode, RunnerDiscoveryMode::AllRunners);
        assert!(config.runner_targets.is_empty());
    }

    #[test]
    fn test_load_from_toml_with_rotation_wait_config() {
        let toml_str = r#"
            [rotation_wait]
            timezone = "Europe/London"
            rotation_window_start = "00:00"
            active_contacted_within_secs = 1200
            missing_runner_grace_polls = 3
            completion_stability_polls = 4
        "#;

        let config = AppConfig::load_from_str(toml_str).unwrap();

        assert_eq!(
            config.rotation_wait.timezone,
            Some("Europe/London".to_string())
        );
        assert_eq!(
            config.rotation_wait.rotation_window_start,
            Some("00:00".to_string())
        );
        assert_eq!(config.rotation_wait.active_contacted_within_secs, 1200);
        assert_eq!(config.rotation_wait.missing_runner_grace_polls, 3);
        assert_eq!(config.rotation_wait.completion_stability_polls, 4);
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
    fn test_validate_runtime_settings_rejects_too_small_enrichment_budget() {
        for max_enrichment_requests in [0, 1] {
            let config = AppConfig {
                max_enrichment_requests,
                discovery_mode: RunnerDiscoveryMode::VisibleRunners,
                ..AppConfig::default()
            };

            let error = config.validate_runtime_settings().unwrap_err().to_string();
            assert!(error.contains("Maximum enrichment requests"));
        }
    }

    #[test]
    fn test_validate_runtime_settings_rejects_excessive_enrichment_budget() {
        let config = AppConfig {
            max_enrichment_requests: MAX_MAX_ENRICHMENT_REQUESTS + 1,
            discovery_mode: RunnerDiscoveryMode::VisibleRunners,
            ..AppConfig::default()
        };

        let error = config.validate_runtime_settings().unwrap_err().to_string();
        assert!(error.contains("Maximum enrichment requests"));
    }

    #[test]
    fn test_load_from_explicit_path() {
        let directory = TestDirectory::new("explicit-load");
        fs::create_dir_all(&directory.0).unwrap();
        fs::write(
            directory.config_path(),
            "gitlab_host = \"https://explicit.example.com\"\n",
        )
        .unwrap();

        let config = AppConfig::load_from_path(&directory.config_path()).unwrap();

        assert_eq!(
            config.gitlab_host,
            Some("https://explicit.example.com".to_string())
        );
    }

    #[test]
    fn test_missing_explicit_path_uses_defaults() {
        let directory = TestDirectory::new("missing-load");

        let config = AppConfig::load_from_path(&directory.config_path()).unwrap();

        assert_eq!(config, AppConfig::default());
    }

    #[test]
    fn resolves_linux_xdg_config_path() {
        assert_eq!(
            resolve_config_path(
                ConfigPlatform::Linux,
                Path::new("/home/alice"),
                Path::new("/home/alice/.config"),
            ),
            Path::new("/home/alice/.config/gitlab-runner-tui/config.toml"),
        );
    }

    #[test]
    fn resolves_windows_roaming_application_data_path() {
        assert_eq!(
            resolve_config_path(
                ConfigPlatform::Windows,
                Path::new("C:/Users/Alice"),
                Path::new("C:/Users/Alice/AppData/Roaming"),
            ),
            Path::new("C:/Users/Alice/AppData/Roaming/gitlab-runner-tui/config.toml"),
        );
    }

    #[test]
    fn resolves_macos_application_support_path() {
        assert_eq!(
            resolve_config_path(
                ConfigPlatform::MacOs,
                Path::new("/Users/alice"),
                Path::new("/Users/alice/.config"),
            ),
            Path::new("/Users/alice/Library/Application Support/gitlab-runner-tui/config.toml"),
        );
    }

    #[test]
    fn test_debug_redacts_token() {
        let config = AppConfig {
            poll_interval_secs: 30,
            poll_timeout_secs: 1800,
            max_enrichment_requests: DEFAULT_MAX_ENRICHMENT_REQUESTS,
            gitlab_host: Some("https://gitlab.com".to_string()),
            gitlab_token: Some("glpat-secret-token".to_string()),
            discovery_mode: RunnerDiscoveryMode::ConfiguredTargets,
            runner_targets: vec![RunnerTarget {
                kind: RunnerTargetKind::Group,
                id: "my-org/platform".to_string(),
                label: Some("Platform".to_string()),
            }],
            rotation_wait: RotationWaitConfig::default(),
        };

        let debug_output = format!("{:?}", config);

        assert!(!debug_output.contains("glpat-secret-token"));
        assert!(debug_output.contains("[REDACTED]"));
        assert!(debug_output.contains("https://gitlab.com"));
    }

    #[test]
    fn test_atomic_save_replaces_existing_config() {
        let directory = TestDirectory::new("atomic-replace");
        let path = directory.config_path();
        let original = AppConfig {
            gitlab_host: Some("https://old.example.com".to_string()),
            ..AppConfig::default()
        };
        original.save_to_path(&path).unwrap();

        let replacement = AppConfig {
            gitlab_host: Some("https://new.example.com".to_string()),
            gitlab_token: Some("replacement-token".to_string()),
            ..AppConfig::default()
        };
        replacement.save_to_path(&path).unwrap();

        assert_eq!(AppConfig::load_from_path(&path).unwrap(), replacement);
    }

    #[test]
    fn test_simulated_failure_preserves_existing_config() {
        let directory = TestDirectory::new("failed-replace");
        fs::create_dir_all(&directory.0).unwrap();
        let path = directory.config_path();
        let original = b"gitlab_host = \"https://old.example.com\"\n";
        fs::write(&path, original).unwrap();
        let replacement = AppConfig {
            gitlab_host: Some("https://new.example.com".to_string()),
            ..AppConfig::default()
        };

        let error = replacement
            .save_to_path_with_hook(&path, |temporary_path| {
                assert!(temporary_path.exists());
                anyhow::bail!("simulated write failure")
            })
            .unwrap_err();

        assert!(error.to_string().contains("simulated write failure"));
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn test_saved_config_and_directory_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let directory = TestDirectory::new("permissions");
        let path = directory.config_path();
        AppConfig {
            gitlab_token: Some("sensitive-token".to_string()),
            ..AppConfig::default()
        }
        .save_to_path(&path)
        .unwrap();

        assert_eq!(
            fs::metadata(&directory.0).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_save_rejects_symlink_target() {
        use std::os::unix::fs::symlink;

        let directory = TestDirectory::new("symlink");
        fs::create_dir_all(&directory.0).unwrap();
        let real_path = directory.0.join("real-config.toml");
        let symlink_path = directory.config_path();
        fs::write(&real_path, "original").unwrap();
        symlink(&real_path, &symlink_path).unwrap();

        let error = AppConfig::default()
            .save_to_path(&symlink_path)
            .unwrap_err();

        assert!(error.to_string().contains("symlink"));
        assert_eq!(fs::read_to_string(real_path).unwrap(), "original");
    }
}
