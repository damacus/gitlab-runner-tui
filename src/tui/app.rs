use crate::conductor::Conductor;
use crate::config::AppConfig;
use crate::models::manager::RunnerManager;
use crate::models::runner::{Runner, RunnerFilters};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::TableState;
use std::fmt;
use std::time::Instant;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
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
            Tab::Uncontacted => "Uncontacted",
            Tab::Empty => "Empty",
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
            Tab::Uncontacted => TabQueryMode::Uncontacted {
                threshold_secs: UNCONTACTED_THRESHOLD_SECS,
            },
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
    Help,
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
enum TabQueryMode {
    FetchRunners,
    Offline,
    Uncontacted { threshold_secs: u64 },
    Empty,
    Rotating,
}

/// Flattened row for workers view: runner info + manager info
#[derive(Debug, Clone)]
pub struct ManagerRow {
    pub runner_id: u64,
    pub runner_tags: Vec<String>,
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

pub struct App {
    pub conductor: Conductor,
    pub config: AppConfig,
    pub mode: AppMode,
    pub should_quit: bool,
    pub runners: Vec<Runner>,
    pub manager_rows: Vec<ManagerRow>,
    pub health_summary: Option<HealthSummary>,

    pub tabs: &'static [Tab],
    pub active_tab_index: usize,
    pub loaded_tab: Option<Tab>,

    pub filter_input: String,
    pub table_state: TableState,

    pub is_loading: bool,
    pub error_message: Option<String>,
    pub spinner_frame: usize,

    pub polling_active: bool,
    pub poll_started_at: Option<Instant>,
    pub last_poll_at: Option<Instant>,
}

const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
const UNCONTACTED_THRESHOLD_SECS: u64 = 3600;

impl App {
    pub fn new(conductor: Conductor, config: AppConfig) -> Self {
        Self {
            conductor,
            config,
            mode: AppMode::default(),
            should_quit: false,
            runners: Vec::new(),
            manager_rows: Vec::new(),
            health_summary: None,
            tabs: Tab::ALL,
            active_tab_index: 0,
            loaded_tab: None,
            filter_input: String::new(),
            table_state: TableState::default(),
            is_loading: false,
            error_message: None,
            spinner_frame: 0,
            polling_active: false,
            poll_started_at: None,
            last_poll_at: None,
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

    pub fn next_tab(&mut self) {
        self.active_tab_index = (self.active_tab_index + 1) % self.tabs.len();
        self.on_tab_changed();
    }

    pub fn previous_tab(&mut self) {
        if self.active_tab_index == 0 {
            self.active_tab_index = self.tabs.len() - 1;
        } else {
            self.active_tab_index -= 1;
        }
        self.on_tab_changed();
    }

    pub fn select_tab(&mut self, tab: Tab) {
        if let Some(index) = self.tabs.iter().position(|candidate| *candidate == tab) {
            self.active_tab_index = index;
            self.on_tab_changed();
        }
    }

    fn on_tab_changed(&mut self) {
        self.error_message = None;
        self.table_state.select(None);
    }

    pub fn focus_filter(&mut self) {
        self.mode = AppMode::FilterInput;
        self.error_message = None;
    }

    pub fn has_loaded_active_tab(&self) -> bool {
        self.loaded_tab == Some(self.active_tab())
    }

    fn build_filters(&self) -> RunnerFilters {
        RunnerFilters {
            tag_list: self.filter_tags(),
            ..RunnerFilters::default()
        }
    }

    pub fn selected_runner(&self) -> Option<&Runner> {
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
            Tab::Uncontacted => format!("Uncontacted ({})", self.runners.len()),
            Tab::Empty => format!("Empty ({})", self.runners.len()),
            Tab::Rotating => format!("Rotating ({})", self.runners.len()),
            Tab::Workers => format!("Workers ({})", self.manager_rows.len()),
        }
    }

    pub async fn execute_search(&mut self) {
        self.is_loading = true;
        self.error_message = None;

        let tab = self.active_tab();
        let filters = self.build_filters();

        let result = match tab.query_mode() {
            TabQueryMode::FetchRunners => self.conductor.fetch_runners(filters).await,
            TabQueryMode::Offline => self.conductor.list_offline_runners(filters).await,
            TabQueryMode::Uncontacted { threshold_secs } => {
                self.conductor
                    .list_uncontacted_runners(filters, threshold_secs)
                    .await
            }
            TabQueryMode::Empty => self.conductor.list_runners_without_managers(filters).await,
            TabQueryMode::Rotating => self.conductor.detect_rotating_runners(filters).await,
        };

        self.is_loading = false;

        match result {
            Ok(runners) => {
                self.loaded_tab = Some(tab);
                self.renders_from_runners(tab, runners);
            }
            Err(error) => {
                self.loaded_tab = None;
                self.runners.clear();
                self.manager_rows.clear();
                self.health_summary = None;
                self.table_state.select(None);
                self.error_message = Some(format!("{:#}", error));
            }
        }
    }

    fn renders_from_runners(&mut self, tab: Tab, runners: Vec<Runner>) {
        self.runners.clear();
        self.manager_rows.clear();
        self.health_summary = None;

        match tab {
            Tab::Workers => {
                self.manager_rows = runners
                    .into_iter()
                    .flat_map(|mut runner| {
                        let tags = std::mem::take(&mut runner.tag_list);
                        runner.managers.into_iter().map(move |manager| ManagerRow {
                            runner_id: runner.id,
                            runner_tags: tags.clone(),
                            manager,
                        })
                    })
                    .collect();
            }
            Tab::Health => {
                let online_count = runners
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
                    total_count: runners.len(),
                });
                self.runners = runners;
            }
            _ => {
                self.runners = runners;
            }
        }

        let result_count = self.active_result_len();
        self.table_state.select((result_count > 0).then_some(0));
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

        let index = match self.table_state.selected() {
            Some(selected) if selected >= len - 1 => 0,
            Some(selected) => selected + 1,
            None => 0,
        };

        self.table_state.select(Some(index));
    }

    pub fn previous_result(&mut self) {
        let len = self.active_result_len();
        if len == 0 {
            return;
        }

        let index = match self.table_state.selected() {
            Some(0) => len - 1,
            Some(selected) => selected - 1,
            None => 0,
        };

        self.table_state.select(Some(index));
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
            self.last_poll_at = Some(now);
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

        self.last_poll_at
            .map(|last_poll| last_poll.elapsed().as_secs() >= self.config.poll_interval_secs)
            .unwrap_or(false)
    }

    pub async fn tick(&mut self) {
        if self.is_loading {
            self.advance_spinner();
        }

        if self.should_poll_now() {
            self.last_poll_at = Some(Instant::now());
            self.execute_search().await;
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
                    self.execute_search().await;
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

        if self.mode == AppMode::Help {
            self.mode = AppMode::Dashboard;
            return;
        }

        match key.code {
            KeyCode::Char('?') => {
                self.mode = AppMode::Help;
            }
            KeyCode::Char('q') => {
                self.should_quit = true;
            }
            KeyCode::Char('p') => {
                self.toggle_polling();
            }
            KeyCode::Char('r') => {
                self.execute_search().await;
            }
            KeyCode::Tab => {
                self.next_tab();
            }
            KeyCode::BackTab => {
                self.previous_tab();
            }
            KeyCode::Char(shortcut @ '1'..='7') => {
                if let Some(tab) = Tab::from_shortcut(shortcut) {
                    self.select_tab(tab);
                }
            }
            KeyCode::Enter => {
                self.execute_search().await;
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
            KeyCode::Backspace => {
                self.error_message = None;
            }
            KeyCode::Char('/') | KeyCode::Char('f') => {
                self.focus_filter();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::GitLabClient;
    use crate::config::{RunnerTarget, RunnerTargetKind};
    use crate::models::manager::RunnerManager;
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
        App::new(Conductor::new(client, test_runner_targets()), config)
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
        assert_eq!(
            Tab::Uncontacted.query_mode(),
            TabQueryMode::Uncontacted {
                threshold_secs: UNCONTACTED_THRESHOLD_SECS
            }
        );
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
        let mut app = test_app();
        app.handle_key(KeyEvent::new(KeyCode::Char('4'), KeyModifiers::NONE))
            .await;
        assert_eq!(app.active_tab(), Tab::Uncontacted);

        app.handle_key(KeyEvent::new(KeyCode::Char('7'), KeyModifiers::NONE))
            .await;
        assert_eq!(app.active_tab(), Tab::Workers);
    }

    #[tokio::test]
    async fn test_slash_focuses_filter_mode() {
        let mut app = test_app();

        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE))
            .await;

        assert_eq!(app.mode, AppMode::FilterInput);
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
    fn test_selected_runner_for_runner_tabs() {
        let mut app = test_app();
        app.loaded_tab = Some(Tab::Runners);
        app.runners = vec![test_runner(42, vec![test_manager(1, "online")])];
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
            runner_tags: vec!["prod".to_string()],
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
        app.runners = vec![test_runner(42, vec![test_manager(1, "online")])];
        app.table_state.select(Some(0));
        assert!(app
            .compact_selection_summary()
            .expect("runner summary")
            .contains("Runner 42"));

        app.select_tab(Tab::Workers);
        app.loaded_tab = Some(Tab::Workers);
        app.manager_rows = vec![ManagerRow {
            runner_id: 42,
            runner_tags: vec!["prod".to_string()],
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
        let conductor = Conductor::new(client, test_runner_targets());
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
}
