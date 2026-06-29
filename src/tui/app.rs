use crate::client::GitLabClient;
use crate::conductor::{Conductor, QueryOutcome};
use crate::config::{
    format_runner_targets, parse_runner_targets, AppConfig, RotationWaitConfig, RunnerDiscoveryMode,
};
use crate::metrics::LiveQueryMetrics;
use crate::models::manager::RunnerManager;
use crate::models::runner::{
    apply_runner_filters, benchmark_runner_processing, extract_runner_tags,
    extract_runner_versions, parse_manager_contacted_at, parse_stale_cutoff, sort_runners,
    ContactThreshold, LocalBenchmarkSnapshot, Runner, RunnerFilters, RunnerSortKey, TagFilterMode,
};
use anyhow::{Context, Result};
use chrono::{DateTime, Local, Utc};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::{ListState, ScrollbarState, TableState};
use std::collections::HashMap;
use std::fmt;
use std::time::Instant;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum Tab {
    Runners,
    Health,
    Offline,
    Uncontacted,
    Empty,
    Rotating,
    Workers,
}

impl Tab {
    pub const ALL: &[Tab] = &[
        Tab::Runners,
        Tab::Health,
        Tab::Offline,
        Tab::Uncontacted,
        Tab::Empty,
        Tab::Rotating,
        Tab::Workers,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Runners => "Runners",
            Tab::Health => "Health",
            Tab::Offline => "Offline",
            Tab::Uncontacted => "Stale",
            Tab::Empty => "Idle",
            Tab::Rotating => "Rotating",
            Tab::Workers => "Workers",
        }
    }

    pub fn shortcut(self) -> char {
        match self {
            Tab::Runners => '1',
            Tab::Health => '2',
            Tab::Offline => '3',
            Tab::Uncontacted => '4',
            Tab::Empty => '5',
            Tab::Rotating => '6',
            Tab::Workers => '7',
        }
    }

    pub fn from_shortcut(shortcut: char) -> Option<Self> {
        match shortcut {
            '1' => Some(Tab::Runners),
            '2' => Some(Tab::Health),
            '3' => Some(Tab::Offline),
            '4' => Some(Tab::Uncontacted),
            '5' => Some(Tab::Empty),
            '6' => Some(Tab::Rotating),
            '7' => Some(Tab::Workers),
            _ => None,
        }
    }

    pub fn results_view_type(self) -> ResultsViewType {
        match self {
            Tab::Health => ResultsViewType::HealthCheck,
            Tab::Rotating => ResultsViewType::Rotation,
            Tab::Workers => ResultsViewType::Workers,
            _ => ResultsViewType::Runners,
        }
    }

    pub fn loading_label(self) -> &'static str {
        match self {
            Tab::Runners => "Loading runners",
            Tab::Health => "Loading health data",
            Tab::Offline => "Loading offline runners",
            Tab::Uncontacted => "Loading uncontacted runners",
            Tab::Empty => "Loading runners without managers",
            Tab::Rotating => "Loading rotating runners",
            Tab::Workers => "Loading workers",
        }
    }

    pub fn empty_label(self) -> &'static str {
        match self {
            Tab::Runners => "No runners found for the current tag filter.",
            Tab::Health => "No runners found for the current tag filter.",
            Tab::Offline => "No offline runners matched the current tag filter.",
            Tab::Uncontacted => "No uncontacted runners matched the current tag filter.",
            Tab::Empty => "No runners without managers matched the current tag filter.",
            Tab::Rotating => "No rotating runners matched the current tag filter.",
            Tab::Workers => "No worker rows matched the current tag filter.",
        }
    }

    fn query_mode(self) -> TabQueryMode {
        match self {
            Tab::Runners | Tab::Health | Tab::Workers => TabQueryMode::FetchRunners,
            Tab::Offline => TabQueryMode::Offline,
            Tab::Uncontacted => TabQueryMode::Uncontacted,
            Tab::Empty => TabQueryMode::Empty,
            Tab::Rotating => TabQueryMode::Rotating,
        }
    }
}

impl fmt::Display for Tab {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.title())
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum AppMode {
    #[default]
    Dashboard,
    FilterInput,
    StaleCutoffInput,
    FilterPopup,
    Settings,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FilterPopupSection {
    #[default]
    TagSearch,
    Selected,
    Status,
    Tags,
    Versions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectedFilterItem {
    TextTag(String),
    PopupTag(String),
    Version(String),
    Status(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResultsViewType {
    #[default]
    Runners,
    Workers,
    HealthCheck,
    Rotation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailLayoutMode {
    SidePanel,
    BottomPanel,
    Compact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollDisplayState {
    Live,
    Paused,
    Refreshing,
    TimedOut,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsField {
    Host,
    Token,
    DiscoveryMode,
    Targets,
    PollInterval,
    PollTimeout,
    Save,
    Cancel,
}

impl SettingsField {
    const ALL: [SettingsField; 8] = [
        SettingsField::Host,
        SettingsField::Token,
        SettingsField::DiscoveryMode,
        SettingsField::Targets,
        SettingsField::PollInterval,
        SettingsField::PollTimeout,
        SettingsField::Save,
        SettingsField::Cancel,
    ];

    fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|candidate| *candidate == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    fn previous(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|candidate| *candidate == self)
            .unwrap_or(0);
        Self::ALL[(index + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone)]
pub struct SettingsDraft {
    pub host: String,
    pub token: String,
    pub discovery_mode: RunnerDiscoveryMode,
    pub runner_targets_input: String,
    pub poll_interval_input: String,
    pub poll_timeout_input: String,
    pub rotation_wait: RotationWaitConfig,
    pub selected_field: SettingsField,
}

impl SettingsDraft {
    fn from_config(config: &AppConfig) -> Self {
        Self {
            host: config
                .gitlab_host
                .clone()
                .unwrap_or_else(|| "https://gitlab.com".to_string()),
            token: config.gitlab_token.clone().unwrap_or_default(),
            discovery_mode: config.discovery_mode,
            runner_targets_input: format_runner_targets(&config.runner_targets),
            poll_interval_input: config.poll_interval_secs.to_string(),
            poll_timeout_input: config.poll_timeout_secs.to_string(),
            rotation_wait: config.rotation_wait.clone(),
            selected_field: SettingsField::Host,
        }
    }

    fn validate(&self) -> Result<AppConfig> {
        let poll_interval_secs = self
            .poll_interval_input
            .trim()
            .parse::<u64>()
            .context("Poll interval must be a whole number of seconds")?;
        let poll_timeout_secs = self
            .poll_timeout_input
            .trim()
            .parse::<u64>()
            .context("Poll timeout must be a whole number of seconds")?;

        let runner_targets = if self.runner_targets_input.trim().is_empty() {
            Vec::new()
        } else {
            parse_runner_targets(&self.runner_targets_input)?
        };

        let config = AppConfig {
            poll_interval_secs,
            poll_timeout_secs,
            gitlab_host: Some(self.host.trim().to_string()),
            gitlab_token: Some(self.token.trim().to_string()),
            discovery_mode: self.discovery_mode,
            runner_targets,
            rotation_wait: self.rotation_wait.clone(),
        };
        config.validate_runtime_settings()?;
        Ok(config)
    }

    fn selected_value_mut(&mut self) -> Option<&mut String> {
        match self.selected_field {
            SettingsField::Host => Some(&mut self.host),
            SettingsField::Token => Some(&mut self.token),
            SettingsField::Targets => Some(&mut self.runner_targets_input),
            SettingsField::PollInterval => Some(&mut self.poll_interval_input),
            SettingsField::PollTimeout => Some(&mut self.poll_timeout_input),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TabQueryMode {
    FetchRunners,
    Offline,
    Uncontacted,
    Empty,
    Rotating,
}

/// Flattened row for workers view: runner info + manager info
#[derive(Debug, Clone)]
pub struct ManagerRow {
    pub runner_id: u64,
    pub manager: RunnerManager,
}

/// Health check summary for the health tab.
#[derive(Debug, Clone, Default)]
pub struct HealthSummary {
    pub online_count: usize,
    pub total_count: usize,
}

impl HealthSummary {
    pub fn percentage(&self) -> f64 {
        if self.total_count == 0 {
            0.0
        } else {
            (self.online_count as f64 / self.total_count as f64) * 100.0
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.online_count == self.total_count && self.total_count > 0
    }
}

/// Wrapper struct used for TUI rendering to prevent expensive heap allocations.
/// Pre-computes and caches expensive string operations (like joining tags)
/// so they do not need to be recalculated on every render loop tick.
#[derive(Debug, Clone)]
pub struct UIRunner {
    pub runner: Runner,
    pub formatted_tags: String,
    pub formatted_groups: Option<String>,
}

pub struct App {
    pub conductor: Conductor,
    pub config: AppConfig,
    pub mode: AppMode,
    pub should_quit: bool,
    pub raw_runners: Vec<Runner>,
    pub runners: Vec<UIRunner>,
    pub manager_rows: Vec<ManagerRow>,
    pub health_summary: Option<HealthSummary>,
    pub version_options: Vec<String>,

    pub tabs: &'static [Tab],
    pub active_tab_index: usize,
    pub loaded_tab: Option<Tab>,
    pub tab_counts: HashMap<Tab, usize>,

    pub filter_input: String,
    pub stale_cutoff_at: Option<DateTime<Utc>>,
    pub stale_cutoff_input: String,
    pub selected_versions: Vec<String>,
    pub version_list_state: ListState,
    pub filter_popup_section: FilterPopupSection,
    pub selected_filter_list_state: ListState,
    pub tag_search_input: String,
    pub tag_filter_mode: TagFilterMode,
    pub tag_options: Vec<String>,
    pub selected_tags: Vec<String>,
    pub tag_list_state: ListState,
    pub status_options: Vec<String>,
    pub selected_status: Option<String>,
    pub status_list_state: ListState,
    pub sort_key: RunnerSortKey,
    pub table_state: TableState,
    pub scroll_state: ScrollbarState,

    pub is_loading: bool,
    pub error_message: Option<String>,
    pub spinner_frame: usize,

    pub polling_active: bool,
    pub poll_started_at: Option<Instant>,
    pub last_poll_at: Option<Instant>,
    pub last_refresh_at: Option<Instant>,
    pub last_fetch_failed: bool,
    pending_search: Option<tokio::task::JoinHandle<(Tab, anyhow::Result<QueryOutcome>)>>,
    /// True after the last AllRunners fetch fell back from /runners/all to /runners (403).
    pub all_runners_fell_back: bool,
    pub settings_draft: SettingsDraft,
    pub settings_message: Option<String>,
    pub live_query_metrics: Option<LiveQueryMetrics>,
    pub local_benchmarks: Option<LocalBenchmarkSnapshot>,
    /// When true, all network operations are suppressed (used in --demo mode).
    pub demo_mode: bool,
}

const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
const UNCONTACTED_THRESHOLD_SECS: u64 = 3600;

impl App {
    pub fn new(conductor: Conductor, config: AppConfig) -> Self {
        let settings_draft = SettingsDraft::from_config(&config);
        Self {
            conductor,
            config,
            mode: AppMode::default(),
            should_quit: false,
            raw_runners: Vec::new(),
            runners: Vec::new(),
            manager_rows: Vec::new(),
            health_summary: None,
            version_options: Vec::new(),
            tabs: Tab::ALL,
            active_tab_index: 0,
            loaded_tab: None,
            tab_counts: HashMap::new(),
            filter_input: String::new(),
            stale_cutoff_at: None,
            stale_cutoff_input: String::new(),
            selected_versions: Vec::new(),
            version_list_state: ListState::default(),
            filter_popup_section: FilterPopupSection::default(),
            selected_filter_list_state: ListState::default(),
            tag_search_input: String::new(),
            tag_filter_mode: TagFilterMode::default(),
            tag_options: Vec::new(),
            selected_tags: Vec::new(),
            tag_list_state: ListState::default(),
            status_options: vec![
                "online".to_string(),
                "offline".to_string(),
                "stale".to_string(),
                "never_contacted".to_string(),
            ],
            selected_status: None,
            status_list_state: ListState::default(),
            sort_key: RunnerSortKey::None,
            table_state: TableState::default(),
            scroll_state: ScrollbarState::default(),
            is_loading: false,
            error_message: None,
            spinner_frame: 0,
            polling_active: false,
            poll_started_at: None,
            last_poll_at: None,
            last_refresh_at: None,
            last_fetch_failed: false,
            pending_search: None,
            all_runners_fell_back: false,
            settings_draft,
            settings_message: None,
            live_query_metrics: None,
            local_benchmarks: None,
            demo_mode: false,
        }
    }

    pub fn spinner_char(&self) -> char {
        SPINNER_FRAMES[self.spinner_frame % SPINNER_FRAMES.len()]
    }

    pub fn advance_spinner(&mut self) {
        self.spinner_frame = (self.spinner_frame + 1) % SPINNER_FRAMES.len();
    }

    pub fn active_tab(&self) -> Tab {
        self.tabs[self.active_tab_index]
    }

    pub fn current_results_view_type(&self) -> ResultsViewType {
        self.active_tab().results_view_type()
    }

    pub fn filter_tags(&self) -> Option<Vec<String>> {
        let tags: Vec<String> = self
            .filter_input
            .split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(ToOwned::to_owned)
            .collect();

        if tags.is_empty() {
            None
        } else {
            Some(tags)
        }
    }

    pub fn next_tab(&mut self) -> bool {
        let next = (self.active_tab_index + 1) % self.tabs.len();
        self.set_active_tab_index(next)
    }

    pub fn previous_tab(&mut self) -> bool {
        let next = if self.active_tab_index == 0 {
            self.tabs.len() - 1
        } else {
            self.active_tab_index - 1
        };
        self.set_active_tab_index(next)
    }

    pub fn select_tab(&mut self, tab: Tab) -> bool {
        if let Some(index) = self.tabs.iter().position(|candidate| *candidate == tab) {
            return self.set_active_tab_index(index);
        }
        false
    }

    fn set_active_tab_index(&mut self, index: usize) -> bool {
        if self.active_tab_index == index {
            return false;
        }

        self.active_tab_index = index;
        self.on_tab_changed();
        true
    }

    fn on_tab_changed(&mut self) {
        self.error_message = None;
        self.table_state.select(None);
        self.update_scroll_state();
    }

    pub fn focus_filter(&mut self) {
        self.mode = AppMode::FilterInput;
        self.error_message = None;
    }

    pub fn focus_stale_cutoff(&mut self) {
        self.stale_cutoff_input = self
            .stale_cutoff_at
            .map(format_stale_cutoff_input)
            .unwrap_or_default();
        self.mode = AppMode::StaleCutoffInput;
        self.error_message = None;
    }

    pub fn open_filter_popup(&mut self) {
        self.mode = AppMode::FilterPopup;
        self.filter_popup_section = FilterPopupSection::TagSearch;
        let selected_count = self.selected_filter_items().len();
        if selected_count == 0 {
            self.selected_filter_list_state.select(None);
        } else if self.selected_filter_list_state.selected().is_none() {
            self.selected_filter_list_state.select(Some(0));
        }
        if self.status_options.is_empty() {
            self.status_list_state.select(None);
        } else if let Some(current_status) = &self.selected_status {
            let selected = self
                .status_options
                .iter()
                .position(|status| status == current_status)
                .unwrap_or(0);
            self.status_list_state.select(Some(selected));
        } else if self.status_list_state.selected().is_none() {
            self.status_list_state.select(Some(0));
        }
        let filtered_count = self.filtered_tag_options().count();
        if filtered_count == 0 {
            self.tag_list_state.select(None);
        } else if self.tag_list_state.selected().is_none() {
            self.tag_list_state.select(Some(0));
        }
        if self.version_options.is_empty() {
            self.version_list_state.select(None);
        } else if self.version_list_state.selected().is_none() {
            self.version_list_state.select(Some(0));
        }
        self.error_message = None;
    }

    pub fn open_settings(&mut self) {
        self.settings_draft = SettingsDraft::from_config(&self.config);
        self.settings_message = None;
        self.refresh_local_benchmarks();
        self.mode = AppMode::Settings;
        self.error_message = None;
    }

    pub fn cycle_discovery_mode(&mut self) {
        let next = match self.config.discovery_mode {
            RunnerDiscoveryMode::AllRunners => RunnerDiscoveryMode::VisibleRunners,
            RunnerDiscoveryMode::VisibleRunners => RunnerDiscoveryMode::ConfiguredTargets,
            RunnerDiscoveryMode::ConfiguredTargets => RunnerDiscoveryMode::AllRunners,
        };
        self.config.discovery_mode = next;
        self.conductor = Conductor::new_with_mode(
            self.conductor.client().clone(),
            next,
            self.config.runner_targets.clone(),
        );
        self.settings_draft.discovery_mode = next;
        self.loaded_tab = None;
    }

    pub fn has_loaded_active_tab(&self) -> bool {
        self.loaded_tab == Some(self.active_tab())
    }

    fn build_filters(&self) -> RunnerFilters {
        RunnerFilters {
            tag_list: self.filter_tags(),
            popup_tags: (!self.selected_tags.is_empty()).then_some(self.selected_tags.clone()),
            popup_tag_mode: self.tag_filter_mode,
            status: self.selected_status.clone(),
            selected_versions: (!self.selected_versions.is_empty())
                .then_some(self.selected_versions.clone()),
            ..RunnerFilters::default()
        }
    }

    fn stale_contact_threshold(&self) -> ContactThreshold {
        self.stale_cutoff_at
            .map(ContactThreshold::Since)
            .unwrap_or(ContactThreshold::OlderThanSecs(UNCONTACTED_THRESHOLD_SECS))
    }

    pub fn stale_cutoff_summary(&self) -> String {
        self.stale_cutoff_at.map_or_else(
            || "older than 1h".to_string(),
            |cutoff| format!("since {}", format_stale_cutoff_label(cutoff)),
        )
    }

    pub fn sort_label(&self) -> &'static str {
        match self.effective_sort_key() {
            RunnerSortKey::None => "None",
            RunnerSortKey::Status => "Status",
            RunnerSortKey::Version => "Version",
            RunnerSortKey::LastContact => "Last contact",
            RunnerSortKey::Tags => "Tags",
            RunnerSortKey::Managers => "Managers",
        }
    }

    pub fn selected_versions_summary(&self) -> String {
        if self.selected_versions.is_empty() {
            "All versions".to_string()
        } else {
            self.selected_versions.join(", ")
        }
    }

    #[allow(dead_code)]
    pub fn selected_tags_summary(&self) -> String {
        if self.selected_tags.is_empty() {
            "all tags".to_string()
        } else {
            format!("{} selected", self.selected_tags.len())
        }
    }

    pub fn effective_sort_key(&self) -> RunnerSortKey {
        match self.current_results_view_type() {
            ResultsViewType::Workers => match self.sort_key {
                RunnerSortKey::Status => RunnerSortKey::Status,
                RunnerSortKey::LastContact => RunnerSortKey::LastContact,
                _ => RunnerSortKey::None,
            },
            _ => self.sort_key,
        }
    }

    pub fn selected_runner(&self) -> Option<&Runner> {
        self.selected_ui_runner().map(|u| &u.runner)
    }

    pub fn selected_ui_runner(&self) -> Option<&UIRunner> {
        if !self.has_loaded_active_tab() {
            return None;
        }

        match self.current_results_view_type() {
            ResultsViewType::Workers => None,
            _ => self
                .table_state
                .selected()
                .and_then(|index| self.runners.get(index)),
        }
    }

    pub fn selected_manager_row(&self) -> Option<&ManagerRow> {
        if !self.has_loaded_active_tab()
            || self.current_results_view_type() != ResultsViewType::Workers
        {
            return None;
        }

        self.table_state
            .selected()
            .and_then(|index| self.manager_rows.get(index))
    }

    pub fn compact_selection_summary(&self) -> Option<String> {
        match self.current_results_view_type() {
            ResultsViewType::Workers => self.selected_manager_row().map(|row| {
                format!(
                    "Worker {} on {} [{}]",
                    row.manager.id, row.manager.system_id, row.manager.status
                )
            }),
            _ => self.selected_runner().map(|runner| {
                format!(
                    "Runner {} [{}] managers={} version={}",
                    runner.id,
                    runner.status,
                    runner.managers.len(),
                    runner.version.as_deref().unwrap_or("-")
                )
            }),
        }
    }

    pub fn current_tab_title(&self) -> String {
        match self.active_tab() {
            Tab::Runners => format!("Runners ({})", self.runners.len()),
            Tab::Health => {
                if let Some(summary) = &self.health_summary {
                    format!(
                        "Health ({}/{} online, {:.1}%)",
                        summary.online_count,
                        summary.total_count,
                        summary.percentage()
                    )
                } else {
                    "Health".to_string()
                }
            }
            Tab::Offline => format!("Offline ({})", self.runners.len()),
            Tab::Uncontacted => format!(
                "Stale ({}, {})",
                self.runners.len(),
                self.stale_cutoff_summary()
            ),
            Tab::Empty => format!("Idle ({})", self.runners.len()),
            Tab::Rotating => format!("Rotating ({})", self.runners.len()),
            Tab::Workers => format!("Workers ({})", self.manager_rows.len()),
        }
    }

    pub fn poll_display_state(&self) -> PollDisplayState {
        if self.is_loading {
            PollDisplayState::Refreshing
        } else if self.last_fetch_failed {
            PollDisplayState::Error
        } else if self.polling_active && self.poll_timed_out() {
            PollDisplayState::TimedOut
        } else if self.polling_active {
            PollDisplayState::Live
        } else {
            PollDisplayState::Paused
        }
    }

    pub fn last_refresh_age_secs(&self) -> Option<u64> {
        self.last_refresh_at
            .map(|refresh| refresh.elapsed().as_secs())
    }

    pub fn next_poll_in_secs(&self) -> Option<u64> {
        if !self.polling_active || self.poll_timed_out() {
            return None;
        }

        let interval = self.config.poll_interval_secs;
        let elapsed = self
            .last_refresh_at
            .or(self.last_poll_at)?
            .elapsed()
            .as_secs();
        Some(interval.saturating_sub(elapsed.min(interval)))
    }

    /// Spawn a background task to fetch runners; the UI stays responsive during the fetch.
    /// Call `poll_pending_search()` on each tick to collect the result.
    pub fn start_search(&mut self) {
        if self.demo_mode {
            return;
        }
        if let Some(handle) = self.pending_search.take() {
            handle.abort();
        }
        self.is_loading = true;
        self.error_message = None;

        let client = self.conductor.client().clone();
        let mode = self.conductor.discovery_mode();
        let targets = self.config.runner_targets.clone();
        let filters = self.build_filters();
        let tab = self.active_tab();
        let threshold = self.stale_contact_threshold();

        self.pending_search = Some(tokio::spawn(async move {
            let conductor = Conductor::new_with_mode(client, mode, targets);
            let result = match tab.query_mode() {
                TabQueryMode::FetchRunners => conductor.fetch_runners_with_metrics(filters).await,
                TabQueryMode::Offline => conductor.list_offline_runners_with_metrics(filters).await,
                TabQueryMode::Uncontacted => {
                    conductor
                        .list_uncontacted_runners_with_metrics(filters, threshold)
                        .await
                }
                TabQueryMode::Empty => {
                    conductor
                        .list_runners_without_managers_with_metrics(filters)
                        .await
                }
                TabQueryMode::Rotating => {
                    conductor
                        .detect_rotating_runners_with_metrics(filters)
                        .await
                }
            };
            (tab, result)
        }));
    }

    /// Check whether the background search has finished and, if so, apply its result.
    pub async fn poll_pending_search(&mut self) {
        let finished = self
            .pending_search
            .as_ref()
            .is_some_and(|h| h.is_finished());
        if !finished {
            return;
        }
        let handle = self.pending_search.take().unwrap();
        match handle.await {
            Ok((tab, result)) => self.process_search_result(tab, result),
            Err(_) => {
                self.is_loading = false;
                self.error_message = Some("Search task failed unexpectedly".to_string());
            }
        }
    }

    /// Wait for the background search task to finish (test helper only).
    #[cfg(test)]
    pub async fn await_pending_search(&mut self) {
        if let Some(handle) = self.pending_search.take() {
            match handle.await {
                Ok((tab, result)) => self.process_search_result(tab, result),
                Err(_) => {
                    self.is_loading = false;
                    self.error_message = Some("Search task failed unexpectedly".to_string());
                }
            }
        }
    }

    /// Blocking search used in tests.
    #[cfg(test)]
    pub async fn execute_search(&mut self) {
        self.is_loading = true;
        self.error_message = None;
        let tab = self.active_tab();
        let filters = self.build_filters();
        let threshold = self.stale_contact_threshold();

        let result = match tab.query_mode() {
            TabQueryMode::FetchRunners => self.conductor.fetch_runners_with_metrics(filters).await,
            TabQueryMode::Offline => {
                self.conductor
                    .list_offline_runners_with_metrics(filters)
                    .await
            }
            TabQueryMode::Uncontacted => {
                self.conductor
                    .list_uncontacted_runners_with_metrics(filters, threshold)
                    .await
            }
            TabQueryMode::Empty => {
                self.conductor
                    .list_runners_without_managers_with_metrics(filters)
                    .await
            }
            TabQueryMode::Rotating => {
                self.conductor
                    .detect_rotating_runners_with_metrics(filters)
                    .await
            }
        };

        self.process_search_result(tab, result);
    }

    fn process_search_result(&mut self, tab: Tab, result: anyhow::Result<QueryOutcome>) {
        self.is_loading = false;
        match result {
            Ok(outcome) => {
                let now = Instant::now();
                self.loaded_tab = Some(tab);
                self.last_refresh_at = Some(now);
                if self.polling_active {
                    self.last_poll_at = Some(now);
                }
                self.last_fetch_failed = false;
                self.all_runners_fell_back = outcome.all_runners_fell_back;
                self.live_query_metrics = Some(outcome.metrics);
                self.renders_from_runners(tab, outcome.runners);
            }
            Err(error) => {
                self.loaded_tab = None;
                self.raw_runners.clear();
                self.runners.clear();
                self.manager_rows.clear();
                self.health_summary = None;
                self.version_options.clear();
                self.tag_options.clear();
                self.selected_tags.clear();
                self.selected_versions.clear();
                self.table_state.select(None);
                self.update_scroll_state();
                self.last_fetch_failed = true;
                self.live_query_metrics = Some(LiveQueryMetrics::failure(
                    Utc::now(),
                    Utc::now(),
                    0,
                    self.conductor.discovery_mode(),
                    error.to_string(),
                ));
                self.error_message = Some(format!("{:#}", error));
            }
        }
    }

    fn renders_from_runners(&mut self, tab: Tab, runners: Vec<Runner>) {
        self.raw_runners = runners;
        self.apply_view_state(tab);
    }

    pub fn seed_demo_data(&mut self, runners: Vec<Runner>) {
        self.renders_from_runners(Tab::Runners, runners);
        self.loaded_tab = Some(Tab::Runners);
        self.last_refresh_at = Some(Instant::now());
        self.is_loading = false;
        self.last_fetch_failed = false;
        self.all_runners_fell_back = false;
        self.live_query_metrics = None;
    }

    fn apply_view_state(&mut self, tab: Tab) {
        self.runners.clear();
        self.manager_rows.clear();
        self.health_summary = None;
        self.version_options = extract_runner_versions(&self.raw_runners);

        self.selected_versions
            .retain(|version| self.version_options.contains(version));

        self.tag_options = extract_runner_tags(&self.raw_runners);
        self.selected_tags
            .retain(|tag| self.tag_options.contains(tag));

        let now = Utc::now();
        let mut filtered = apply_runner_filters(&self.raw_runners, &self.build_filters(), now);

        match tab {
            Tab::Workers => {
                let mut manager_rows: Vec<ManagerRow> = filtered
                    .into_iter()
                    .flat_map(|runner| {
                        runner.managers.into_iter().map(move |manager| ManagerRow {
                            runner_id: runner.id,
                            manager,
                        })
                    })
                    .collect();
                let sort_key = self.effective_sort_key();
                if sort_key == RunnerSortKey::LastContact {
                    manager_rows.sort_by(|left, right| {
                        let left_contact = parse_manager_contacted_at(&left.manager);
                        let right_contact = parse_manager_contacted_at(&right.manager);
                        match (left_contact, right_contact) {
                            (Some(l), Some(r)) => l.cmp(&r),
                            (None, Some(_)) => std::cmp::Ordering::Less,
                            (Some(_), None) => std::cmp::Ordering::Greater,
                            (None, None) => std::cmp::Ordering::Equal,
                        }
                        .then_with(|| left.runner_id.cmp(&right.runner_id))
                        .then_with(|| left.manager.id.cmp(&right.manager.id))
                    });
                } else if sort_key == RunnerSortKey::Status {
                    manager_rows.sort_by(|left, right| {
                        left.manager
                            .status
                            .cmp(&right.manager.status)
                            .then_with(|| left.runner_id.cmp(&right.runner_id))
                            .then_with(|| left.manager.id.cmp(&right.manager.id))
                    });
                }
                self.manager_rows = manager_rows;
            }
            Tab::Health => {
                sort_runners(&mut filtered, self.effective_sort_key(), now);
                let online_count = filtered
                    .iter()
                    .filter(|runner| {
                        runner
                            .managers
                            .iter()
                            .any(|manager| manager.status == "online")
                    })
                    .count();
                self.health_summary = Some(HealthSummary {
                    online_count,
                    total_count: filtered.len(),
                });
                self.runners = filtered
                    .into_iter()
                    .map(|runner| {
                        let formatted_tags = runner.tag_list.join(", ");

                        // ⚡ Bolt: Pre-calculate formatted groups during state update
                        // This prevents creating `Vec<&str>` and joining strings 60 times a second
                        // in the hot UI render loop (inside `render_runner_detail`).
                        let formatted_groups = if !runner.groups.is_empty() {
                            let group_names: Vec<&str> =
                                runner.groups.iter().map(|g| g.name.as_str()).collect();
                            Some(group_names.join(", "))
                        } else {
                            None
                        };
                        UIRunner {
                            runner,
                            formatted_tags,
                            formatted_groups,
                        }
                    })
                    .collect();
            }
            _ => {
                sort_runners(&mut filtered, self.effective_sort_key(), now);
                self.runners = filtered
                    .into_iter()
                    .map(|runner| {
                        let formatted_tags = runner.tag_list.join(", ");

                        // ⚡ Bolt: Pre-calculate formatted groups during state update
                        // This prevents creating `Vec<&str>` and joining strings 60 times a second
                        // in the hot UI render loop (inside `render_runner_detail`).
                        let formatted_groups = if !runner.groups.is_empty() {
                            let group_names: Vec<&str> =
                                runner.groups.iter().map(|g| g.name.as_str()).collect();
                            Some(group_names.join(", "))
                        } else {
                            None
                        };
                        UIRunner {
                            runner,
                            formatted_tags,
                            formatted_groups,
                        }
                    })
                    .collect();
            }
        }

        let result_count = self.active_result_len();
        self.tab_counts.insert(tab, result_count);
        self.table_state.select((result_count > 0).then_some(0));
        self.update_scroll_state();
    }

    pub fn active_result_len(&self) -> usize {
        match self.current_results_view_type() {
            ResultsViewType::Workers => self.manager_rows.len(),
            _ => self.runners.len(),
        }
    }

    pub fn next_result(&mut self) {
        let len = self.active_result_len();
        if len == 0 {
            return;
        }
        if self.table_state.selected().is_some_and(|i| i >= len - 1) {
            self.table_state.select_first();
        } else {
            self.table_state.select_next();
        }
        self.update_scroll_state();
    }

    pub fn previous_result(&mut self) {
        let len = self.active_result_len();
        if len == 0 {
            return;
        }
        if self.table_state.selected().is_none_or(|i| i == 0) {
            self.table_state.select(Some(len - 1));
        } else {
            self.table_state.select_previous();
        }
        self.update_scroll_state();
    }

    fn update_scroll_state(&mut self) {
        let content_length = self.active_result_len();
        let position = self
            .table_state
            .offset()
            .min(content_length.saturating_sub(1));
        self.scroll_state = ScrollbarState::new(content_length).position(position);
    }

    pub fn cycle_sort_key(&mut self) {
        self.sort_key = match self.current_results_view_type() {
            ResultsViewType::Workers => match self.sort_key {
                RunnerSortKey::None => RunnerSortKey::Status,
                RunnerSortKey::Status => RunnerSortKey::LastContact,
                _ => RunnerSortKey::None,
            },
            _ => match self.sort_key {
                RunnerSortKey::None => RunnerSortKey::Status,
                RunnerSortKey::Status => RunnerSortKey::Version,
                RunnerSortKey::Version => RunnerSortKey::LastContact,
                RunnerSortKey::LastContact => RunnerSortKey::Tags,
                RunnerSortKey::Tags => RunnerSortKey::Managers,
                RunnerSortKey::Managers => RunnerSortKey::None,
            },
        };

        if self.has_loaded_active_tab() {
            self.apply_view_state(self.active_tab());
        }
    }

    pub fn toggle_selected_version(&mut self) {
        let Some(index) = self.version_list_state.selected() else {
            return;
        };
        let Some(version) = self.version_options.get(index).cloned() else {
            return;
        };

        if let Some(existing_index) = self
            .selected_versions
            .iter()
            .position(|selected| selected == &version)
        {
            self.selected_versions.remove(existing_index);
        } else {
            self.selected_versions.push(version);
            self.selected_versions.sort();
        }

        if self.has_loaded_active_tab() {
            self.apply_view_state(self.active_tab());
        }
    }

    pub fn filtered_tag_options(&self) -> impl Iterator<Item = &String> {
        let query = self.tag_search_input.to_lowercase();
        self.tag_options
            .iter()
            .filter(move |t| query.is_empty() || t.to_lowercase().contains(&query))
    }

    pub fn selected_filter_items(&self) -> Vec<SelectedFilterItem> {
        let mut items = Vec::new();
        if let Some(text_tags) = self.filter_tags() {
            items.extend(text_tags.into_iter().map(SelectedFilterItem::TextTag));
        }
        items.extend(
            self.selected_tags
                .iter()
                .cloned()
                .map(SelectedFilterItem::PopupTag),
        );
        items.extend(
            self.selected_versions
                .iter()
                .cloned()
                .map(SelectedFilterItem::Version),
        );
        if let Some(status) = &self.selected_status {
            items.push(SelectedFilterItem::Status(status.clone()));
        }
        items
    }

    pub fn toggle_selected_tag(&mut self) {
        let Some(index) = self.tag_list_state.selected() else {
            return;
        };
        let query = self.tag_search_input.to_lowercase();
        let Some(tag) = self
            .tag_options
            .iter()
            .filter(|t| query.is_empty() || t.to_lowercase().contains(&query))
            .nth(index)
            .cloned()
        else {
            return;
        };

        if let Some(existing_index) = self.selected_tags.iter().position(|t| t == &tag) {
            self.selected_tags.remove(existing_index);
        } else {
            self.selected_tags.push(tag);
            self.selected_tags.sort();
        }

        if self.has_loaded_active_tab() {
            self.apply_view_state(self.active_tab());
        }
    }

    pub fn toggle_selected_status(&mut self) {
        let Some(index) = self.status_list_state.selected() else {
            return;
        };
        let Some(status) = self.status_options.get(index).cloned() else {
            return;
        };

        if self.selected_status.as_deref() == Some(status.as_str()) {
            self.selected_status = None;
        } else {
            self.selected_status = Some(status);
        }

        if self.has_loaded_active_tab() {
            self.apply_view_state(self.active_tab());
        }
    }

    pub fn remove_selected_filter(&mut self) {
        let Some(index) = self.selected_filter_list_state.selected() else {
            return;
        };
        let items = self.selected_filter_items();
        let Some(item) = items.get(index).cloned() else {
            return;
        };

        match item {
            SelectedFilterItem::TextTag(tag) => {
                let mut tags = self.filter_tags().unwrap_or_default();
                if let Some(position) = tags.iter().position(|candidate| candidate == &tag) {
                    tags.remove(position);
                }
                self.filter_input = tags.join(",");
            }
            SelectedFilterItem::PopupTag(tag) => {
                self.selected_tags.retain(|candidate| candidate != &tag);
            }
            SelectedFilterItem::Version(version) => {
                self.selected_versions
                    .retain(|candidate| candidate != &version);
            }
            SelectedFilterItem::Status(_) => {
                self.selected_status = None;
            }
        }

        let remaining = self.selected_filter_items().len();
        if remaining == 0 {
            self.selected_filter_list_state.select(None);
        } else if index >= remaining {
            self.selected_filter_list_state.select(Some(remaining - 1));
        }

        if self.has_loaded_active_tab() {
            self.apply_view_state(self.active_tab());
        }
    }

    pub fn clear_all_filters(&mut self) {
        self.filter_input.clear();
        self.selected_tags.clear();
        self.selected_versions.clear();
        self.selected_status = None;
        self.tag_search_input.clear();
        self.tag_list_state.select(if self.tag_options.is_empty() {
            None
        } else {
            Some(0)
        });
        self.version_list_state
            .select(if self.version_options.is_empty() {
                None
            } else {
                Some(0)
            });
        self.selected_filter_list_state.select(None);
        self.status_list_state
            .select(if self.status_options.is_empty() {
                None
            } else {
                Some(0)
            });

        if self.has_loaded_active_tab() {
            self.apply_view_state(self.active_tab());
        }
    }

    pub fn refresh_local_benchmarks(&mut self) {
        self.local_benchmarks = Some(benchmark_runner_processing(
            &self.raw_runners,
            &self.build_filters(),
            self.effective_sort_key(),
            Utc::now(),
        ));
    }

    fn apply_stale_cutoff_input(&mut self) {
        match parse_stale_cutoff(&self.stale_cutoff_input, Local::now()) {
            Ok(cutoff) => {
                self.stale_cutoff_at = cutoff;
                self.mode = AppMode::Dashboard;
                self.error_message = None;
                self.start_search();
            }
            Err(message) => {
                self.error_message = Some(format!("Invalid stale cutoff: {message}"));
            }
        }
    }

    async fn save_settings(&mut self) {
        let result = async {
            let new_config = self.settings_draft.validate()?;
            let host = new_config
                .gitlab_host
                .clone()
                .context("GitLab host is required")?;
            let token = new_config
                .gitlab_token
                .clone()
                .context("GitLab token is required")?;

            let client = GitLabClient::new(host, token)?;
            let conductor = Conductor::new_with_mode(
                client,
                new_config.discovery_mode,
                new_config.runner_targets.clone(),
            );
            conductor.validate_token().await?;
            new_config.save_to_canonical_path()?;
            Ok::<(AppConfig, Conductor), anyhow::Error>((new_config, conductor))
        }
        .await;

        match result {
            Ok((new_config, conductor)) => {
                self.config = new_config;
                self.conductor = conductor;
                self.mode = AppMode::Dashboard;
                self.settings_message = None;
                self.loaded_tab = None;
                self.error_message = None;
                self.raw_runners.clear();
                self.runners.clear();
                self.manager_rows.clear();
                self.health_summary = None;
                self.version_options.clear();
                self.selected_versions.clear();
                self.tag_options.clear();
                self.selected_tags.clear();
                self.selected_status = None;
                self.table_state.select(None);
                self.update_scroll_state();
                self.start_search();
            }
            Err(error) => {
                self.settings_message = Some(format!("{:#}", error));
            }
        }
    }

    pub fn toggle_polling(&mut self) {
        if self.polling_active {
            self.polling_active = false;
            self.poll_started_at = None;
            self.last_poll_at = None;
        } else {
            let now = Instant::now();
            self.polling_active = true;
            self.poll_started_at = Some(now);
            self.last_poll_at = self.last_refresh_at.or(Some(now));
        }
    }

    pub fn poll_elapsed_secs(&self) -> u64 {
        self.poll_started_at
            .map(|started| started.elapsed().as_secs())
            .unwrap_or(0)
    }

    pub fn poll_timed_out(&self) -> bool {
        self.poll_elapsed_secs() >= self.config.poll_timeout_secs
    }

    fn should_poll_now(&self) -> bool {
        if !self.polling_active || self.is_loading || self.mode != AppMode::Dashboard {
            return false;
        }
        if self.poll_timed_out() {
            return false;
        }

        self.last_refresh_at
            .or(self.last_poll_at)
            .map(|last_poll| last_poll.elapsed().as_secs() >= self.config.poll_interval_secs)
            .unwrap_or(false)
    }

    pub async fn tick(&mut self) {
        if self.is_loading {
            self.advance_spinner();
        }

        self.poll_pending_search().await;

        if self.should_poll_now() {
            self.last_poll_at = Some(Instant::now());
            self.start_search();
        }

        if self.polling_active && self.poll_timed_out() {
            self.polling_active = false;
        }
    }

    pub async fn handle_key(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Char('c')) && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return;
        }

        if self.mode == AppMode::FilterInput {
            match key.code {
                KeyCode::Enter => {
                    self.mode = AppMode::Dashboard;
                    self.start_search();
                }
                KeyCode::Esc => {
                    self.mode = AppMode::Dashboard;
                }
                KeyCode::Backspace => {
                    self.filter_input.pop();
                }
                KeyCode::Char(character) => {
                    self.filter_input.push(character);
                }
                _ => {}
            }
            return;
        }

        if self.mode == AppMode::StaleCutoffInput {
            match key.code {
                KeyCode::Enter => self.apply_stale_cutoff_input(),
                KeyCode::Esc => {
                    self.mode = AppMode::Dashboard;
                    self.error_message = None;
                }
                KeyCode::Backspace => {
                    self.stale_cutoff_input.pop();
                }
                KeyCode::Char(character) => {
                    self.stale_cutoff_input.push(character);
                }
                _ => {}
            }
            return;
        }

        if self.mode == AppMode::FilterPopup {
            match self.filter_popup_section {
                FilterPopupSection::TagSearch => match key.code {
                    KeyCode::Esc => {
                        if !self.tag_search_input.is_empty() {
                            self.tag_search_input.clear();
                            let filtered_count = self.filtered_tag_options().count();
                            if filtered_count == 0 {
                                self.tag_list_state.select(None);
                            } else {
                                self.tag_list_state.select(Some(0));
                            }
                        } else {
                            self.mode = AppMode::Dashboard;
                        }
                    }
                    KeyCode::Char('f') if key.modifiers.is_empty() => {
                        self.mode = AppMode::Dashboard;
                    }
                    KeyCode::Tab => {
                        self.filter_popup_section = FilterPopupSection::Selected;
                    }
                    KeyCode::BackTab => {
                        self.filter_popup_section = FilterPopupSection::Versions;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.filter_popup_section = FilterPopupSection::Selected;
                    }
                    KeyCode::Backspace => {
                        self.tag_search_input.pop();
                        let filtered_count = self.filtered_tag_options().count();
                        if filtered_count == 0 {
                            self.tag_list_state.select(None);
                        } else {
                            self.tag_list_state.select(Some(0));
                        }
                    }
                    KeyCode::Char(c) => {
                        self.tag_search_input.push(c);
                        let filtered_count = self.filtered_tag_options().count();
                        if filtered_count == 0 {
                            self.tag_list_state.select(None);
                        } else {
                            self.tag_list_state.select(Some(0));
                        }
                    }
                    _ => {}
                },
                FilterPopupSection::Selected
                | FilterPopupSection::Status
                | FilterPopupSection::Tags
                | FilterPopupSection::Versions => match key.code {
                    KeyCode::Esc | KeyCode::Char('f') => {
                        self.mode = AppMode::Dashboard;
                    }
                    KeyCode::Tab => {
                        self.filter_popup_section = match self.filter_popup_section {
                            FilterPopupSection::Selected => FilterPopupSection::Status,
                            FilterPopupSection::Status => FilterPopupSection::Tags,
                            FilterPopupSection::Tags => FilterPopupSection::Versions,
                            FilterPopupSection::Versions => FilterPopupSection::TagSearch,
                            FilterPopupSection::TagSearch => unreachable!(),
                        };
                    }
                    KeyCode::BackTab => {
                        self.filter_popup_section = match self.filter_popup_section {
                            FilterPopupSection::Selected => FilterPopupSection::TagSearch,
                            FilterPopupSection::Status => FilterPopupSection::Selected,
                            FilterPopupSection::Tags => FilterPopupSection::Status,
                            FilterPopupSection::Versions => FilterPopupSection::Tags,
                            FilterPopupSection::TagSearch => unreachable!(),
                        };
                    }
                    KeyCode::Up | KeyCode::Char('k') => match self.filter_popup_section {
                        FilterPopupSection::Selected => {
                            if !self.selected_filter_items().is_empty() {
                                self.selected_filter_list_state.select_previous();
                            }
                        }
                        FilterPopupSection::Status => {
                            if !self.status_options.is_empty() {
                                self.status_list_state.select_previous();
                            }
                        }
                        FilterPopupSection::Tags => {
                            if self.filtered_tag_options().count() > 0 {
                                self.tag_list_state.select_previous();
                            }
                        }
                        FilterPopupSection::Versions => {
                            if !self.version_options.is_empty() {
                                self.version_list_state.select_previous();
                            }
                        }
                        FilterPopupSection::TagSearch => unreachable!(),
                    },
                    KeyCode::Down | KeyCode::Char('j') => match self.filter_popup_section {
                        FilterPopupSection::Selected => {
                            if !self.selected_filter_items().is_empty() {
                                self.selected_filter_list_state.select_next();
                            }
                        }
                        FilterPopupSection::Status => {
                            if !self.status_options.is_empty() {
                                self.status_list_state.select_next();
                            }
                        }
                        FilterPopupSection::Tags => {
                            if self.filtered_tag_options().count() > 0 {
                                self.tag_list_state.select_next();
                            }
                        }
                        FilterPopupSection::Versions => {
                            if !self.version_options.is_empty() {
                                self.version_list_state.select_next();
                            }
                        }
                        FilterPopupSection::TagSearch => unreachable!(),
                    },
                    KeyCode::Char(' ') => match self.filter_popup_section {
                        FilterPopupSection::Selected => self.remove_selected_filter(),
                        FilterPopupSection::Status => self.toggle_selected_status(),
                        FilterPopupSection::Tags => self.toggle_selected_tag(),
                        FilterPopupSection::Versions => self.toggle_selected_version(),
                        FilterPopupSection::TagSearch => unreachable!(),
                    },
                    KeyCode::Char('m') => {
                        self.tag_filter_mode = self.tag_filter_mode.toggle();
                        if self.has_loaded_active_tab() {
                            self.apply_view_state(self.active_tab());
                        }
                    }
                    KeyCode::Char('c') => match self.filter_popup_section {
                        FilterPopupSection::Selected => {}
                        FilterPopupSection::Status => {
                            self.selected_status = None;
                            if self.has_loaded_active_tab() {
                                self.apply_view_state(self.active_tab());
                            }
                        }
                        FilterPopupSection::Tags => {
                            self.selected_tags.clear();
                            if self.has_loaded_active_tab() {
                                self.apply_view_state(self.active_tab());
                            }
                        }
                        FilterPopupSection::Versions => {
                            self.selected_versions.clear();
                            if self.has_loaded_active_tab() {
                                self.apply_view_state(self.active_tab());
                            }
                        }
                        FilterPopupSection::TagSearch => unreachable!(),
                    },
                    KeyCode::Char('a') => {
                        self.clear_all_filters();
                    }
                    _ => {}
                },
            }
            return;
        }

        if self.mode == AppMode::Settings {
            match key.code {
                KeyCode::Esc => {
                    self.mode = AppMode::Dashboard;
                    self.settings_draft = SettingsDraft::from_config(&self.config);
                }
                KeyCode::Up => {
                    self.settings_draft.selected_field =
                        self.settings_draft.selected_field.previous();
                }
                KeyCode::Down | KeyCode::Tab => {
                    self.settings_draft.selected_field = self.settings_draft.selected_field.next();
                }
                KeyCode::BackTab => {
                    self.settings_draft.selected_field =
                        self.settings_draft.selected_field.previous();
                }
                KeyCode::Left | KeyCode::Right
                    if matches!(
                        self.settings_draft.selected_field,
                        SettingsField::DiscoveryMode
                    ) =>
                {
                    self.settings_draft.discovery_mode = match self.settings_draft.discovery_mode {
                        RunnerDiscoveryMode::AllRunners => RunnerDiscoveryMode::VisibleRunners,
                        RunnerDiscoveryMode::VisibleRunners => {
                            RunnerDiscoveryMode::ConfiguredTargets
                        }
                        RunnerDiscoveryMode::ConfiguredTargets => RunnerDiscoveryMode::AllRunners,
                    };
                }
                KeyCode::Backspace => {
                    if let Some(value) = self.settings_draft.selected_value_mut() {
                        value.pop();
                    }
                }
                KeyCode::Char('b') => self.refresh_local_benchmarks(),
                KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.save_settings().await;
                }
                KeyCode::Enter
                    if matches!(self.settings_draft.selected_field, SettingsField::Save) =>
                {
                    self.save_settings().await;
                }
                KeyCode::Enter
                    if matches!(self.settings_draft.selected_field, SettingsField::Cancel) =>
                {
                    self.mode = AppMode::Dashboard;
                    self.settings_draft = SettingsDraft::from_config(&self.config);
                }
                KeyCode::Enter
                    if matches!(
                        self.settings_draft.selected_field,
                        SettingsField::DiscoveryMode
                    ) =>
                {
                    self.settings_draft.discovery_mode = match self.settings_draft.discovery_mode {
                        RunnerDiscoveryMode::AllRunners => RunnerDiscoveryMode::VisibleRunners,
                        RunnerDiscoveryMode::VisibleRunners => {
                            RunnerDiscoveryMode::ConfiguredTargets
                        }
                        RunnerDiscoveryMode::ConfiguredTargets => RunnerDiscoveryMode::AllRunners,
                    };
                }
                KeyCode::Char(character)
                    if matches!(
                        self.settings_draft.selected_field,
                        SettingsField::DiscoveryMode
                    ) =>
                {
                    match character {
                        't' | 'T' => {
                            self.settings_draft.discovery_mode =
                                RunnerDiscoveryMode::ConfiguredTargets
                        }
                        'v' | 'V' => {
                            self.settings_draft.discovery_mode = RunnerDiscoveryMode::VisibleRunners
                        }
                        'a' | 'A' => {
                            self.settings_draft.discovery_mode = RunnerDiscoveryMode::AllRunners
                        }
                        _ => {}
                    }
                }
                KeyCode::Char(character) => {
                    if let Some(value) = self.settings_draft.selected_value_mut() {
                        value.push(character);
                    }
                }
                _ => {}
            }
            return;
        }

        if self.mode == AppMode::Help {
            self.mode = AppMode::Dashboard;
            return;
        }

        match key.code {
            KeyCode::Char('?') => {
                self.mode = AppMode::Help;
            }
            KeyCode::Char('c') => {
                self.open_settings();
            }
            KeyCode::Char('q') => {
                self.should_quit = true;
            }
            KeyCode::Char('p') => {
                self.toggle_polling();
            }
            KeyCode::Char('r') => {
                self.start_search();
            }
            KeyCode::Char('d') => {
                self.cycle_discovery_mode();
                self.start_search();
            }
            KeyCode::Char('s') => {
                self.cycle_sort_key();
            }
            KeyCode::Char('t') => {
                self.focus_filter();
            }
            KeyCode::Char('a') if self.active_tab() == Tab::Uncontacted => {
                self.focus_stale_cutoff();
            }
            KeyCode::Char('/') | KeyCode::Char('f') => {
                self.open_filter_popup();
            }
            KeyCode::Tab if self.next_tab() => {
                self.start_search();
            }
            KeyCode::Tab => {}
            KeyCode::BackTab if self.previous_tab() => {
                self.start_search();
            }
            KeyCode::BackTab => {}
            KeyCode::Char(shortcut @ '1'..='7') => {
                if let Some(tab) = Tab::from_shortcut(shortcut) {
                    if self.select_tab(tab) {
                        self.start_search();
                    }
                }
            }
            KeyCode::Enter => {
                if let Some(runner) = self.selected_runner() {
                    let host = self
                        .config
                        .gitlab_host
                        .as_deref()
                        .unwrap_or("https://gitlab.com");
                    let url = runner_admin_url(host, runner.id);
                    open_in_browser(&url);
                } else {
                    self.start_search();
                }
            }
            KeyCode::Esc => {
                self.error_message = None;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.previous_result();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.next_result();
            }
            KeyCode::Home | KeyCode::Char('g') if self.active_result_len() > 0 => {
                self.table_state.select_first();
                self.update_scroll_state();
            }
            KeyCode::Home | KeyCode::Char('g') => {}
            KeyCode::End | KeyCode::Char('G') => {
                let len = self.active_result_len();
                if len > 0 {
                    self.table_state.select(Some(len - 1));
                    self.update_scroll_state();
                }
            }
            KeyCode::PageDown if self.active_result_len() > 0 => {
                self.table_state.scroll_down_by(10);
                self.update_scroll_state();
            }
            KeyCode::PageDown => {}
            KeyCode::PageUp if self.active_result_len() > 0 => {
                self.table_state.scroll_up_by(10);
                self.update_scroll_state();
            }
            KeyCode::PageUp => {}
            KeyCode::Backspace => {
                self.error_message = None;
            }
            _ => {}
        }
    }
}

pub fn detail_layout_mode(width: u16, height: u16) -> DetailLayoutMode {
    if width >= 130 {
        DetailLayoutMode::SidePanel
    } else if width >= 90 && height >= 26 {
        DetailLayoutMode::BottomPanel
    } else {
        DetailLayoutMode::Compact
    }
}

pub fn latest_runner_contact(runner: &Runner) -> Option<DateTime<Utc>> {
    runner
        .managers
        .iter()
        .filter_map(|manager| parse_contact_timestamp(manager.contacted_at.as_deref()))
        .max()
}

fn format_stale_cutoff_input(cutoff: DateTime<Utc>) -> String {
    let local_cutoff = cutoff.with_timezone(&Local);
    let today = Local::now().date_naive();
    if local_cutoff.date_naive() == today {
        local_cutoff.format("%H:%M:%S").to_string()
    } else {
        local_cutoff.to_rfc3339()
    }
}

fn format_stale_cutoff_label(cutoff: DateTime<Utc>) -> String {
    cutoff
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S %Z")
        .to_string()
}

pub fn open_in_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", url])
        .spawn();
}

pub fn runner_admin_url(host: &str, runner_id: u64) -> String {
    let host = normalize_browser_host(host);
    format!("{host}/admin/runners/{runner_id}")
}

fn normalize_browser_host(host: &str) -> String {
    let trimmed = host.trim().trim_end_matches('/');

    if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    }
}

pub fn latest_runner_contact_label(runner: &Runner, now: DateTime<Utc>) -> String {
    latest_runner_contact(runner)
        .map(|contact| relative_timestamp_label(contact, now))
        .unwrap_or_else(|| "Never".to_string())
}

#[cfg(test)]
pub fn latest_runner_contact_detail(runner: &Runner) -> String {
    latest_runner_contact(runner)
        .map(format_absolute_timestamp)
        .unwrap_or_else(|| "Never".to_string())
}

pub fn manager_contact_label(manager: &RunnerManager, now: DateTime<Utc>) -> String {
    parse_contact_timestamp(manager.contacted_at.as_deref())
        .map(|contact| relative_timestamp_label(contact, now))
        .unwrap_or_else(|| "Never".to_string())
}

pub fn manager_contact_detail(manager: &RunnerManager) -> String {
    parse_contact_timestamp(manager.contacted_at.as_deref())
        .map(format_absolute_timestamp)
        .unwrap_or_else(|| "Never".to_string())
}

fn parse_contact_timestamp(value: Option<&str>) -> Option<DateTime<Utc>> {
    value
        .and_then(|timestamp| DateTime::parse_from_rfc3339(timestamp).ok())
        .map(|timestamp| timestamp.with_timezone(&Utc))
}

fn relative_timestamp_label(timestamp: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let seconds = now.signed_duration_since(timestamp).num_seconds().max(0);

    match seconds {
        0..=89 => "just now".to_string(),
        90..=3599 => format!("{}m ago", seconds / 60),
        3600..=86_399 => format!("{}h ago", seconds / 3600),
        _ => format!("{}d ago", seconds / 86_400),
    }
}

fn format_absolute_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::GitLabClient;
    use crate::config::{RunnerTarget, RunnerTargetKind};
    use crate::models::manager::RunnerManager;
    use chrono::TimeZone;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn test_runner_targets() -> Vec<RunnerTarget> {
        vec![RunnerTarget {
            kind: RunnerTargetKind::Group,
            id: "my-org/platform".to_string(),
            label: None,
        }]
    }

    fn test_app() -> App {
        let client = GitLabClient::new("https://gitlab.com".to_string(), "token".to_string())
            .expect("client");
        let config = AppConfig {
            runner_targets: test_runner_targets(),
            ..AppConfig::default()
        };
        App::new(
            Conductor::new_with_mode(
                client,
                RunnerDiscoveryMode::ConfiguredTargets,
                test_runner_targets(),
            ),
            config,
        )
    }

    fn test_runner(id: u64, managers: Vec<RunnerManager>) -> Runner {
        Runner {
            id,
            runner_type: "group_type".to_string(),
            active: true,
            paused: false,
            description: Some(format!("Runner {}", id)),
            created_at: Some("2024-01-15T10:30:00.000Z".to_string()),
            ip_address: Some("10.0.1.1".to_string()),
            is_shared: false,
            status: "online".to_string(),
            version: Some("17.5.0".to_string()),
            revision: Some("abc123".to_string()),
            tag_list: vec!["prod".to_string(), "linux".to_string()],
            managers,
            groups: vec![],
        }
    }

    fn test_manager(id: u64, status: &str) -> RunnerManager {
        RunnerManager {
            id,
            system_id: format!("runner-host-{}", id),
            created_at: "2024-01-15T10:30:00.000Z".to_string(),
            contacted_at: Some("2024-01-20T14:22:00.000Z".to_string()),
            ip_address: Some("10.0.1.1".to_string()),
            status: status.to_string(),
            version: Some("17.5.0".to_string()),
            revision: Some("abc123".to_string()),
            platform: None,
            architecture: None,
        }
    }

    #[test]
    fn test_build_filters_merges_text_and_popup_tags() {
        let mut app = test_app();
        app.filter_input = "linux".to_owned();
        app.selected_tags = vec!["docker".to_owned()];
        let filters = app.build_filters();
        assert_eq!(filters.tag_list, Some(vec!["linux".to_owned()]));
        assert_eq!(filters.popup_tags, Some(vec!["docker".to_owned()]));
    }

    #[test]
    fn test_build_filters_popup_tags_only() {
        let mut app = test_app();
        app.selected_tags = vec!["docker".to_owned()];
        let filters = app.build_filters();
        assert!(filters.tag_list.is_none());
        assert_eq!(filters.popup_tags, Some(vec!["docker".to_owned()]));
    }

    #[test]
    fn test_build_filters_text_tags_only() {
        let mut app = test_app();
        app.filter_input = "linux".to_owned();
        let filters = app.build_filters();
        assert_eq!(filters.tag_list, Some(vec!["linux".to_owned()]));
    }

    #[test]
    fn test_build_filters_no_tags_returns_none() {
        let app = test_app();
        let filters = app.build_filters();
        assert!(filters.tag_list.is_none());
    }

    #[test]
    fn test_build_filters_duplicate_tag_is_harmless() {
        let mut app = test_app();
        app.filter_input = "docker".to_owned();
        app.selected_tags = vec!["docker".to_owned()];
        let filters = app.build_filters();
        // text tags go to tag_list, popup tags go to popup_tags — no merging
        assert_eq!(filters.tag_list, Some(vec!["docker".to_owned()]));
        assert_eq!(filters.popup_tags, Some(vec!["docker".to_owned()]));
    }

    #[test]
    fn test_open_filter_popup_selects_first_item_when_tags_available() {
        let mut app = test_app();
        app.filter_input = "prod".to_owned();
        app.tag_options = vec!["docker".to_owned(), "linux".to_owned()];
        app.tag_list_state.select(None);
        app.open_filter_popup();
        assert_eq!(app.mode, AppMode::FilterPopup);
        assert_eq!(app.selected_filter_list_state.selected(), Some(0));
        assert_eq!(app.tag_list_state.selected(), Some(0));
    }

    #[test]
    fn test_open_filter_popup_none_when_tags_empty() {
        let mut app = test_app();
        app.tag_options = vec![];
        app.open_filter_popup();
        assert_eq!(app.tag_list_state.selected(), None);
    }

    #[test]
    fn test_toggle_selected_tag_adds_and_removes() {
        let mut app = test_app();
        app.tag_options = vec!["docker".to_owned(), "linux".to_owned()];
        app.tag_list_state.select(Some(0));
        app.toggle_selected_tag();
        assert_eq!(app.selected_tags, vec!["docker"]);
        app.toggle_selected_tag();
        assert!(app.selected_tags.is_empty());
    }

    #[test]
    fn test_build_filters_includes_status() {
        let mut app = test_app();
        app.selected_status = Some("offline".to_owned());
        let filters = app.build_filters();
        assert_eq!(filters.status, Some("offline".to_owned()));
    }

    #[test]
    fn test_toggle_selected_status_adds_and_removes() {
        let mut app = test_app();
        app.status_list_state.select(Some(1));
        app.toggle_selected_status();
        assert_eq!(app.selected_status, Some("offline".to_owned()));
        app.toggle_selected_status();
        assert_eq!(app.selected_status, None);
    }

    #[test]
    fn test_remove_selected_filter_removes_text_tag() {
        let mut app = test_app();
        app.filter_input = "prod,linux".to_owned();
        app.selected_filter_list_state.select(Some(0));
        app.remove_selected_filter();
        assert_eq!(app.filter_input, "linux");
    }

    #[test]
    fn test_clear_all_filters_resets_everything() {
        let mut app = test_app();
        app.filter_input = "prod".to_owned();
        app.selected_tags = vec!["docker".to_owned()];
        app.selected_versions = vec!["17.5.0".to_owned()];
        app.selected_status = Some("online".to_owned());
        app.tag_search_input = "dock".to_owned();
        app.clear_all_filters();
        assert!(app.filter_input.is_empty());
        assert!(app.selected_tags.is_empty());
        assert!(app.selected_versions.is_empty());
        assert!(app.selected_status.is_none());
        assert!(app.tag_search_input.is_empty());
    }

    #[test]
    fn test_selected_tags_summary_empty_and_non_empty() {
        let mut app = test_app();
        assert_eq!(app.selected_tags_summary(), "all tags");
        app.selected_tags = vec!["docker".to_owned(), "linux".to_owned()];
        assert_eq!(app.selected_tags_summary(), "2 selected");
    }

    #[test]
    fn test_app_initial_filter_popup_fields() {
        let app = test_app();
        assert_eq!(app.filter_popup_section, FilterPopupSection::TagSearch);
        assert!(app.tag_options.is_empty());
        assert!(app.selected_tags.is_empty());
        assert_eq!(app.tag_list_state.selected(), None);
    }

    #[test]
    fn test_health_summary_percentage_all_online() {
        let summary = HealthSummary {
            online_count: 10,
            total_count: 10,
        };
        assert!((summary.percentage() - 100.0).abs() < 0.001);
        assert!(summary.is_healthy());
    }

    #[test]
    fn test_health_summary_percentage_half_online() {
        let summary = HealthSummary {
            online_count: 5,
            total_count: 10,
        };
        assert!((summary.percentage() - 50.0).abs() < 0.001);
        assert!(!summary.is_healthy());
    }

    #[test]
    fn test_health_summary_percentage_none_online() {
        let summary = HealthSummary {
            online_count: 0,
            total_count: 10,
        };
        assert!((summary.percentage() - 0.0).abs() < 0.001);
        assert!(!summary.is_healthy());
    }

    #[test]
    fn test_health_summary_percentage_empty() {
        let summary = HealthSummary::default();
        assert!((summary.percentage() - 0.0).abs() < 0.001);
        assert!(!summary.is_healthy());
    }

    #[test]
    fn test_app_mode_default() {
        assert_eq!(AppMode::default(), AppMode::Dashboard);
    }

    #[test]
    fn test_results_view_type_mapping() {
        assert_eq!(Tab::Runners.results_view_type(), ResultsViewType::Runners);
        assert_eq!(
            Tab::Health.results_view_type(),
            ResultsViewType::HealthCheck
        );
        assert_eq!(Tab::Rotating.results_view_type(), ResultsViewType::Rotation);
        assert_eq!(Tab::Workers.results_view_type(), ResultsViewType::Workers);
    }

    #[test]
    fn test_tab_shortcut_roundtrip_mapping() {
        for tab in Tab::ALL {
            let shortcut = tab.shortcut();
            assert_eq!(Tab::from_shortcut(shortcut), Some(*tab));
        }
        assert_eq!(Tab::from_shortcut('0'), None);
    }

    #[test]
    fn test_active_tab_query_mapping() {
        assert_eq!(Tab::Runners.query_mode(), TabQueryMode::FetchRunners);
        assert_eq!(Tab::Health.query_mode(), TabQueryMode::FetchRunners);
        assert_eq!(Tab::Workers.query_mode(), TabQueryMode::FetchRunners);
        assert_eq!(Tab::Offline.query_mode(), TabQueryMode::Offline);
        assert_eq!(Tab::Uncontacted.query_mode(), TabQueryMode::Uncontacted);
        assert_eq!(Tab::Empty.query_mode(), TabQueryMode::Empty);
        assert_eq!(Tab::Rotating.query_mode(), TabQueryMode::Rotating);
    }

    #[test]
    fn test_tab_switching_wraps() {
        let mut app = test_app();
        assert_eq!(app.active_tab(), Tab::Runners);
        app.previous_tab();
        assert_eq!(app.active_tab(), Tab::Workers);
        app.next_tab();
        assert_eq!(app.active_tab(), Tab::Runners);
    }

    #[tokio::test]
    async fn test_direct_tab_hotkeys_select_expected_tab() {
        let client =
            GitLabClient::new("http://127.0.0.1:1".to_string(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            test_runner_targets(),
        );
        let config = AppConfig {
            runner_targets: test_runner_targets(),
            ..AppConfig::default()
        };
        let mut app = App::new(conductor, config);
        app.handle_key(KeyEvent::new(KeyCode::Char('4'), KeyModifiers::NONE))
            .await;
        app.await_pending_search().await;
        assert_eq!(app.active_tab(), Tab::Uncontacted);
        assert!(app.error_message.is_some());

        app.handle_key(KeyEvent::new(KeyCode::Char('7'), KeyModifiers::NONE))
            .await;
        app.await_pending_search().await;
        assert_eq!(app.active_tab(), Tab::Workers);
        assert!(app.error_message.is_some());
    }

    #[tokio::test]
    async fn test_tab_wrap_navigation_auto_loads() {
        let client =
            GitLabClient::new("http://127.0.0.1:1".to_string(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            test_runner_targets(),
        );
        let config = AppConfig {
            runner_targets: test_runner_targets(),
            ..AppConfig::default()
        };
        let mut app = App::new(conductor, config);

        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .await;
        app.await_pending_search().await;

        assert_eq!(app.active_tab(), Tab::Health);
        assert!(app.error_message.is_some());
    }

    #[tokio::test]
    async fn test_slash_focuses_filter_mode() {
        let mut app = test_app();

        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE))
            .await;

        assert_eq!(app.mode, AppMode::FilterPopup);
    }

    #[tokio::test]
    async fn test_f_key_opens_filter_popup() {
        let mut app = test_app();
        app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE))
            .await;
        assert_eq!(app.mode, AppMode::FilterPopup);
    }

    #[tokio::test]
    async fn test_slash_key_opens_filter_popup() {
        let mut app = test_app();
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE))
            .await;
        assert_eq!(app.mode, AppMode::FilterPopup);
    }

    #[tokio::test]
    async fn test_t_key_opens_filter_input() {
        let mut app = test_app();
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE))
            .await;
        assert_eq!(app.mode, AppMode::FilterInput);
    }

    #[tokio::test]
    async fn test_a_key_opens_stale_cutoff_input_on_stale_tab() {
        let mut app = test_app();
        app.select_tab(Tab::Uncontacted);
        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE))
            .await;

        assert_eq!(app.mode, AppMode::StaleCutoffInput);
        assert!(app.stale_cutoff_input.is_empty());
    }

    #[tokio::test]
    async fn test_stale_cutoff_enter_applies_and_refreshes() {
        let mut app = test_app();
        app.demo_mode = true;
        app.select_tab(Tab::Uncontacted);
        app.focus_stale_cutoff();
        app.stale_cutoff_input = "11:00".to_string();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await;

        assert_eq!(app.mode, AppMode::Dashboard);
        assert!(matches!(
            app.stale_contact_threshold(),
            ContactThreshold::Since(_)
        ));
        assert!(app.error_message.is_none());
    }

    #[tokio::test]
    async fn test_stale_cutoff_esc_cancels_edit() {
        let mut app = test_app();
        let existing = Utc.with_ymd_and_hms(2026, 5, 12, 10, 0, 0).unwrap();
        app.stale_cutoff_at = Some(existing);
        app.focus_stale_cutoff();
        app.stale_cutoff_input = "12:00".to_string();
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .await;

        assert_eq!(app.mode, AppMode::Dashboard);
        assert_eq!(app.stale_cutoff_at, Some(existing));
    }

    #[tokio::test]
    async fn test_blank_stale_cutoff_restores_default_threshold() {
        let mut app = test_app();
        app.demo_mode = true;
        app.stale_cutoff_at = Some(Utc.with_ymd_and_hms(2026, 5, 12, 10, 0, 0).unwrap());
        app.focus_stale_cutoff();
        app.stale_cutoff_input.clear();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await;

        assert_eq!(app.stale_cutoff_at, None);
        assert_eq!(
            app.stale_contact_threshold(),
            ContactThreshold::OlderThanSecs(UNCONTACTED_THRESHOLD_SECS)
        );
    }

    #[tokio::test]
    async fn test_v_key_does_nothing_in_dashboard() {
        let mut app = test_app();
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE))
            .await;
        assert_eq!(app.mode, AppMode::Dashboard);
    }

    #[tokio::test]
    async fn test_esc_closes_filter_popup() {
        let mut app = test_app();
        app.mode = AppMode::FilterPopup;
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .await;
        assert_eq!(app.mode, AppMode::Dashboard);
    }

    #[tokio::test]
    async fn test_f_key_closes_filter_popup() {
        let mut app = test_app();
        app.mode = AppMode::FilterPopup;
        app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE))
            .await;
        assert_eq!(app.mode, AppMode::Dashboard);
    }

    #[tokio::test]
    async fn test_slash_inside_filter_popup_is_noop() {
        let mut app = test_app();
        app.mode = AppMode::FilterPopup;
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE))
            .await;
        assert_eq!(app.mode, AppMode::FilterPopup);
    }

    #[tokio::test]
    async fn test_tab_switches_section_tags_to_versions() {
        let mut app = test_app();
        app.mode = AppMode::FilterPopup;
        app.filter_popup_section = FilterPopupSection::Tags;
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .await;
        assert_eq!(app.filter_popup_section, FilterPopupSection::Versions);
    }

    #[tokio::test]
    async fn test_tab_switches_section_selected_to_status() {
        let mut app = test_app();
        app.mode = AppMode::FilterPopup;
        app.filter_popup_section = FilterPopupSection::Selected;
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .await;
        assert_eq!(app.filter_popup_section, FilterPopupSection::Status);
    }

    #[tokio::test]
    async fn test_tab_switches_section_versions_to_tags() {
        let mut app = test_app();
        app.mode = AppMode::FilterPopup;
        app.filter_popup_section = FilterPopupSection::Versions;
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .await;
        assert_eq!(app.filter_popup_section, FilterPopupSection::TagSearch);
    }

    #[tokio::test]
    async fn test_c_clears_focused_tags_section_only() {
        let mut app = test_app();
        app.mode = AppMode::FilterPopup;
        app.filter_popup_section = FilterPopupSection::Tags;
        app.selected_tags = vec!["docker".to_owned()];
        app.selected_versions = vec!["17.5.0".to_owned()];
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE))
            .await;
        assert!(app.selected_tags.is_empty());
        assert_eq!(app.selected_versions, vec!["17.5.0".to_owned()]);
    }

    #[tokio::test]
    async fn test_c_clears_focused_versions_section_only() {
        let mut app = test_app();
        app.mode = AppMode::FilterPopup;
        app.filter_popup_section = FilterPopupSection::Versions;
        app.selected_tags = vec!["docker".to_owned()];
        app.selected_versions = vec!["17.5.0".to_owned()];
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE))
            .await;
        assert!(app.selected_versions.is_empty());
        assert_eq!(app.selected_tags, vec!["docker".to_owned()]);
    }

    #[tokio::test]
    async fn test_c_clears_focused_status_only() {
        let mut app = test_app();
        app.mode = AppMode::FilterPopup;
        app.filter_popup_section = FilterPopupSection::Status;
        app.selected_status = Some("online".to_owned());
        app.selected_tags = vec!["docker".to_owned()];
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE))
            .await;
        assert!(app.selected_status.is_none());
        assert_eq!(app.selected_tags, vec!["docker".to_owned()]);
    }

    #[tokio::test]
    async fn test_a_clears_all_filters_in_popup() {
        let mut app = test_app();
        app.mode = AppMode::FilterPopup;
        app.filter_popup_section = FilterPopupSection::Tags;
        app.filter_input = "prod".to_owned();
        app.selected_tags = vec!["docker".to_owned()];
        app.selected_versions = vec!["17.5.0".to_owned()];
        app.selected_status = Some("online".to_owned());
        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE))
            .await;
        assert!(app.filter_input.is_empty());
        assert!(app.selected_tags.is_empty());
        assert!(app.selected_versions.is_empty());
        assert!(app.selected_status.is_none());
    }

    #[tokio::test]
    async fn test_tag_search_accepts_a_without_clearing_filters() {
        let mut app = test_app();
        app.mode = AppMode::FilterPopup;
        app.filter_popup_section = FilterPopupSection::TagSearch;
        app.filter_input = "prod".to_owned();
        app.selected_tags = vec!["docker".to_owned()];
        app.selected_versions = vec!["17.5.0".to_owned()];
        app.selected_status = Some("online".to_owned());

        for character in ['e', 'x', 'a'] {
            app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))
                .await;
        }

        assert_eq!(app.tag_search_input, "exa");
        assert_eq!(app.filter_input, "prod");
        assert_eq!(app.selected_tags, vec!["docker".to_owned()]);
        assert_eq!(app.selected_versions, vec!["17.5.0".to_owned()]);
        assert_eq!(app.selected_status, Some("online".to_owned()));
    }

    #[tokio::test]
    async fn test_plain_character_does_not_force_filter_mode() {
        let mut app = test_app();

        app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))
            .await;

        assert_eq!(app.mode, AppMode::Dashboard);
        assert!(app.filter_input.is_empty());
    }

    #[tokio::test]
    async fn test_ctrl_c_quits_even_when_filter_is_focused() {
        let mut app = test_app();
        app.mode = AppMode::FilterInput;
        app.filter_input = "prod".to_string();

        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
            .await;

        assert!(app.should_quit);
    }

    #[test]
    fn test_filter_tags_trim_and_drop_empty_entries() {
        let mut app = test_app();
        app.filter_input = " prod, , qa ,, linux ".to_string();
        assert_eq!(
            app.filter_tags(),
            Some(vec![
                "prod".to_string(),
                "qa".to_string(),
                "linux".to_string()
            ])
        );
    }

    #[test]
    fn test_filter_tags_only_noise_returns_none() {
        let mut app = test_app();
        app.filter_input = " , ,,   ,".to_string();
        assert_eq!(app.filter_tags(), None);
    }

    #[test]
    fn test_filter_persists_across_tab_switches() {
        let mut app = test_app();
        app.filter_input = "prod,qa".to_string();
        app.next_tab();
        app.next_tab();
        assert_eq!(app.filter_input, "prod,qa");
    }

    #[test]
    fn test_toggle_polling_does_not_change_tab() {
        let mut app = test_app();
        app.select_tab(Tab::Rotating);
        app.toggle_polling();
        assert!(app.polling_active);
        assert_eq!(app.active_tab(), Tab::Rotating);
    }

    #[test]
    fn test_toggle_polling_off_clears_poll_state() {
        let mut app = test_app();
        app.toggle_polling();
        assert!(app.polling_active);
        assert!(app.poll_started_at.is_some());
        assert!(app.last_poll_at.is_some());

        app.toggle_polling();
        assert!(!app.polling_active);
        assert!(app.poll_started_at.is_none());
        assert!(app.last_poll_at.is_none());
    }

    #[tokio::test]
    async fn test_polling_stays_active_when_switching_tabs() {
        let client =
            GitLabClient::new("http://127.0.0.1:1".to_string(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            test_runner_targets(),
        );
        let config = AppConfig {
            runner_targets: test_runner_targets(),
            ..AppConfig::default()
        };
        let mut app = App::new(conductor, config);
        app.toggle_polling();

        app.handle_key(KeyEvent::new(KeyCode::Char('6'), KeyModifiers::NONE))
            .await;
        app.await_pending_search().await;

        assert!(app.polling_active);
        assert_eq!(app.active_tab(), Tab::Rotating);
        assert!(app.error_message.is_some());
    }

    #[tokio::test]
    async fn test_tick_disables_polling_when_timeout_reached() {
        let mut app = test_app();
        app.polling_active = true;
        app.poll_started_at = Some(Instant::now());
        app.last_poll_at = Some(Instant::now());
        app.config.poll_timeout_secs = 0;

        app.tick().await;

        assert!(!app.polling_active);
    }

    #[test]
    fn test_detail_layout_mode_breakpoints() {
        assert_eq!(detail_layout_mode(140, 30), DetailLayoutMode::SidePanel);
        assert_eq!(detail_layout_mode(100, 30), DetailLayoutMode::BottomPanel);
        assert_eq!(detail_layout_mode(80, 24), DetailLayoutMode::Compact);
    }

    #[test]
    fn test_detail_layout_mode_requires_height_for_bottom_panel() {
        assert_eq!(detail_layout_mode(100, 20), DetailLayoutMode::Compact);
    }

    #[test]
    fn test_runner_admin_url_adds_https_for_scheme_less_host() {
        assert_eq!(
            runner_admin_url("gitlab.example.com/", 7411),
            "https://gitlab.example.com/admin/runners/7411"
        );
    }

    #[test]
    fn test_runner_admin_url_preserves_existing_scheme() {
        assert_eq!(
            runner_admin_url("http://gitlab.example.com/", 7411),
            "http://gitlab.example.com/admin/runners/7411"
        );
    }

    #[test]
    fn test_selected_runner_for_runner_tabs() {
        let mut app = test_app();
        app.loaded_tab = Some(Tab::Runners);
        let runner = test_runner(42, vec![test_manager(1, "online")]);
        app.runners = vec![UIRunner {
            formatted_tags: runner.tag_list.join(", "),
            formatted_groups: None,
            runner,
        }];
        app.table_state.select(Some(0));

        let runner = app.selected_runner().expect("selected runner");
        assert_eq!(runner.id, 42);
        assert_eq!(runner.managers.len(), 1);
    }

    #[test]
    fn test_selected_worker_for_workers_tab() {
        let mut app = test_app();
        app.select_tab(Tab::Workers);
        app.loaded_tab = Some(Tab::Workers);
        app.manager_rows = vec![ManagerRow {
            runner_id: 42,
            manager: test_manager(7, "online"),
        }];
        app.table_state.select(Some(0));

        let worker = app.selected_manager_row().expect("selected worker");
        assert_eq!(worker.runner_id, 42);
        assert_eq!(worker.manager.id, 7);
    }

    #[test]
    fn test_compact_selection_summary_uses_active_tab_shape() {
        let mut app = test_app();
        app.loaded_tab = Some(Tab::Runners);
        let runner = test_runner(42, vec![test_manager(1, "online")]);
        app.runners = vec![UIRunner {
            formatted_tags: runner.tag_list.join(", "),
            formatted_groups: None,
            runner,
        }];
        app.table_state.select(Some(0));
        assert!(app
            .compact_selection_summary()
            .expect("runner summary")
            .contains("Runner 42"));

        app.select_tab(Tab::Workers);
        app.loaded_tab = Some(Tab::Workers);
        app.manager_rows = vec![ManagerRow {
            runner_id: 42,
            manager: test_manager(7, "online"),
        }];
        app.table_state.select(Some(0));
        assert!(app
            .compact_selection_summary()
            .expect("worker summary")
            .contains("Worker 7"));
    }

    #[tokio::test]
    async fn test_execute_search_handles_error() {
        let client =
            GitLabClient::new("http://127.0.0.1:1".to_string(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            test_runner_targets(),
        );
        let config = AppConfig {
            runner_targets: test_runner_targets(),
            ..AppConfig::default()
        };
        let mut app = App::new(conductor, config);

        app.execute_search().await;

        assert!(app.error_message.is_some());
        assert!(!app.is_loading);
        assert_eq!(app.loaded_tab, None);
    }

    #[test]
    fn test_latest_runner_contact_uses_newest_valid_manager_timestamp() {
        let runner = test_runner(
            42,
            vec![
                RunnerManager {
                    contacted_at: Some("invalid".to_string()),
                    ..test_manager(1, "offline")
                },
                RunnerManager {
                    contacted_at: Some("2024-01-20T14:22:00.000Z".to_string()),
                    ..test_manager(2, "online")
                },
                RunnerManager {
                    contacted_at: Some("2024-01-21T09:15:00.000Z".to_string()),
                    ..test_manager(3, "online")
                },
            ],
        );

        let contact = latest_runner_contact(&runner).expect("contact");
        assert_eq!(
            contact,
            Utc.with_ymd_and_hms(2024, 1, 21, 9, 15, 0).unwrap()
        );
    }

    #[test]
    fn test_latest_runner_contact_label_returns_never_without_contacts() {
        let runner = test_runner(
            42,
            vec![RunnerManager {
                contacted_at: None,
                ..test_manager(1, "offline")
            }],
        );

        assert_eq!(
            latest_runner_contact_label(
                &runner,
                Utc.with_ymd_and_hms(2024, 1, 22, 9, 15, 0).unwrap()
            ),
            "Never"
        );
        assert_eq!(latest_runner_contact_detail(&runner), "Never");
    }

    #[test]
    fn test_manager_contact_label_uses_relative_output() {
        let manager = RunnerManager {
            contacted_at: Some("2024-01-21T08:15:00.000Z".to_string()),
            ..test_manager(7, "online")
        };

        assert_eq!(
            manager_contact_label(
                &manager,
                Utc.with_ymd_and_hms(2024, 1, 21, 10, 15, 0).unwrap()
            ),
            "2h ago"
        );
        assert_eq!(manager_contact_detail(&manager), "2024-01-21 08:15:00 UTC");
    }

    #[test]
    fn test_demo_mode_suppresses_start_search() {
        let mut app = test_app();
        app.demo_mode = true;
        app.start_search();
        // is_loading must stay false — no background task was spawned
        assert!(!app.is_loading);
        assert!(app.pending_search.is_none());
    }

    #[test]
    fn test_seed_demo_data_populates_runners() {
        let mut app = test_app();
        let runners = vec![test_runner(1, vec![]), test_runner(2, vec![])];
        app.seed_demo_data(runners);
        assert_eq!(app.raw_runners.len(), 2);
        assert_eq!(app.loaded_tab, Some(Tab::Runners));
        assert!(!app.is_loading);
        assert!(app.last_refresh_at.is_some());
        assert!(!app.runners.is_empty());
    }
}
