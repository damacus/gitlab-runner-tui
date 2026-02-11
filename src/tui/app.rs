use crate::conductor::Conductor;
use crate::config::AppConfig;
use crate::models::manager::RunnerManager;
use crate::models::runner::Runner;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::widgets::TableState;
use std::fmt;
use std::time::Instant;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Command {
    Fetch,
    Lights,
    Switch,
    Workers,
    Flames,
    Empty,
    Rotate,
}

impl Command {
    pub const ALL: &[Command] = &[
        Command::Fetch,
        Command::Lights,
        Command::Switch,
        Command::Workers,
        Command::Flames,
        Command::Empty,
        Command::Rotate,
    ];
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Fetch => write!(f, "fetch"),
            Command::Lights => write!(f, "lights"),
            Command::Switch => write!(f, "switch"),
            Command::Workers => write!(f, "workers"),
            Command::Flames => write!(f, "flames"),
            Command::Empty => write!(f, "empty"),
            Command::Rotate => write!(f, "rotate"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum AppMode {
    #[default]
    CommandSelection,
    FilterInput,
    ResultsView,
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

/// Flattened row for workers view: runner info + manager info
#[derive(Debug, Clone)]
pub struct ManagerRow {
    pub runner_id: u64,
    pub runner_tags: Vec<String>,
    pub manager: RunnerManager,
}

/// Health check summary for lights command
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
    pub results_view_type: ResultsViewType,
    pub health_summary: Option<HealthSummary>,

    pub commands: &'static [Command],
    pub selected_command_index: usize,

    pub input_buffer: String,
    pub table_state: TableState,

    // Loading and error state
    pub is_loading: bool,
    pub error_message: Option<String>,
    pub spinner_frame: usize,

    // Polling state
    pub polling_active: bool,
    pub poll_started_at: Option<Instant>,
    pub last_poll_at: Option<Instant>,
}

const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

impl App {
    pub fn new(conductor: Conductor, config: AppConfig) -> Self {
        Self {
            conductor,
            config,
            mode: AppMode::default(),
            should_quit: false,
            runners: Vec::new(),
            manager_rows: Vec::new(),
            results_view_type: ResultsViewType::default(),
            health_summary: None,
            commands: Command::ALL,
            selected_command_index: 0,
            input_buffer: String::new(),
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

    /// Clear the current error message - reserved for error recovery flows
    #[allow(dead_code)]
    pub fn clear_error(&mut self) {
        self.error_message = None;
    }

    pub fn next_command(&mut self) {
        if self.selected_command_index < self.commands.len() - 1 {
            self.selected_command_index += 1;
        } else {
            self.selected_command_index = 0;
        }
    }

    pub fn previous_command(&mut self) {
        if self.selected_command_index > 0 {
            self.selected_command_index -= 1;
        } else {
            self.selected_command_index = self.commands.len() - 1;
        }
    }

    pub fn select_command(&mut self) {
        self.mode = AppMode::FilterInput;
        self.input_buffer.clear();
    }

    pub async fn execute_search(&mut self) {
        self.is_loading = true;
        self.error_message = None;

        let command = self.commands[self.selected_command_index];
        let mut filters = crate::models::runner::RunnerFilters::default();

        if !self.input_buffer.is_empty() {
            filters.tag_list = Some(
                self.input_buffer
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect(),
            );
        }

        let result = match command {
            Command::Fetch | Command::Lights | Command::Workers => {
                self.conductor.fetch_runners(filters).await
            }
            Command::Switch => self.conductor.list_offline_runners(filters).await,
            Command::Flames => self.conductor.list_uncontacted_runners(filters, 3600).await,
            Command::Empty => self.conductor.list_runners_without_managers(filters).await,
            Command::Rotate => self.conductor.detect_rotating_runners(filters).await,
        };

        self.is_loading = false;

        match result {
            Ok(runners) => {
                // Clear all previous results before populating new ones
                self.runners.clear();
                self.manager_rows.clear();
                self.health_summary = None;

                match command {
                    Command::Workers => {
                        self.manager_rows = runners
                            .iter()
                            .flat_map(|r| {
                                r.managers.iter().map(move |m| ManagerRow {
                                    runner_id: r.id,
                                    runner_tags: r.tag_list.clone(),
                                    manager: m.clone(),
                                })
                            })
                            .collect();
                        self.results_view_type = ResultsViewType::Workers;
                    }
                    Command::Lights => {
                        let online_count = runners
                            .iter()
                            .filter(|r| r.managers.iter().any(|m| m.status == "online"))
                            .count();
                        self.health_summary = Some(HealthSummary {
                            online_count,
                            total_count: runners.len(),
                        });
                        self.runners = runners;
                        self.results_view_type = ResultsViewType::HealthCheck;
                    }
                    Command::Rotate => {
                        self.runners = runners;
                        self.results_view_type = ResultsViewType::Rotation;
                    }
                    _ => {
                        self.runners = runners;
                        self.results_view_type = ResultsViewType::Runners;
                    }
                }
                self.mode = AppMode::ResultsView;
                if !self.runners.is_empty() || !self.manager_rows.is_empty() {
                    self.table_state.select(Some(0));
                }
            }
            Err(e) => {
                self.error_message = Some(format!("{:#}", e));
                self.mode = AppMode::ResultsView; // Show error in results view
            }
        }
    }

    pub fn next_result(&mut self) {
        let len = match self.results_view_type {
            ResultsViewType::Runners | ResultsViewType::HealthCheck | ResultsViewType::Rotation => {
                self.runners.len()
            }
            ResultsViewType::Workers => self.manager_rows.len(),
        };
        if len == 0 {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => {
                if i >= len - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    pub fn previous_result(&mut self) {
        let len = match self.results_view_type {
            ResultsViewType::Runners | ResultsViewType::HealthCheck | ResultsViewType::Rotation => {
                self.runners.len()
            }
            ResultsViewType::Workers => self.manager_rows.len(),
        };
        if len == 0 {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => {
                if i == 0 {
                    len - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    pub fn toggle_polling(&mut self) {
        if self.polling_active {
            self.polling_active = false;
            self.poll_started_at = None;
            self.last_poll_at = None;
        } else {
            self.polling_active = true;
            self.poll_started_at = Some(Instant::now());
            self.last_poll_at = Some(Instant::now());
        }
    }

    pub fn poll_elapsed_secs(&self) -> u64 {
        self.poll_started_at
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0)
    }

    pub fn poll_timed_out(&self) -> bool {
        self.poll_elapsed_secs() >= self.config.poll_timeout_secs
    }

    fn should_poll_now(&self) -> bool {
        if !self.polling_active || self.is_loading {
            return false;
        }
        if self.mode != AppMode::ResultsView {
            return false;
        }
        if self.poll_timed_out() {
            return false;
        }
        self.last_poll_at
            .map(|t| t.elapsed().as_secs() >= self.config.poll_interval_secs)
            .unwrap_or(false)
    }

    pub fn tick(&mut self) {
        if self.is_loading {
            self.advance_spinner();
        }
    }

    pub async fn handle_key(&mut self, key: KeyEvent) {
        // FilterInput mode: route all chars/backspace to input buffer first
        if self.mode == AppMode::FilterInput {
            match key.code {
                KeyCode::Enter => self.execute_search().await,
                KeyCode::Esc => {
                    self.error_message = None;
                    self.mode = AppMode::CommandSelection;
                }
                KeyCode::Backspace => {
                    self.input_buffer.pop();
                }
                KeyCode::Char(c) => {
                    self.input_buffer.push(c);
                }
                _ => {}
            }
            return;
        }

        // Help mode: any key closes help
        if self.mode == AppMode::Help {
            self.mode = AppMode::CommandSelection;
            return;
        }

        // CommandSelection and ResultsView modes
        match key.code {
            KeyCode::Char('?') => {
                self.mode = AppMode::Help;
            }
            KeyCode::Char('q') => {
                self.should_quit = true;
            }
            KeyCode::Char('p') if self.mode == AppMode::ResultsView => {
                self.toggle_polling();
            }
            KeyCode::Up | KeyCode::Char('k') => match self.mode {
                AppMode::CommandSelection => self.previous_command(),
                AppMode::ResultsView => self.previous_result(),
                _ => {}
            },
            KeyCode::Down | KeyCode::Char('j') => match self.mode {
                AppMode::CommandSelection => self.next_command(),
                AppMode::ResultsView => self.next_result(),
                _ => {}
            },
            KeyCode::Enter => {
                if self.mode == AppMode::CommandSelection {
                    self.select_command();
                }
            }
            KeyCode::Esc => match self.mode {
                AppMode::CommandSelection => self.should_quit = true,
                AppMode::ResultsView => {
                    self.error_message = None;
                    self.mode = AppMode::CommandSelection;
                }
                _ => self.mode = AppMode::CommandSelection,
            },
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let summary = HealthSummary {
            online_count: 0,
            total_count: 0,
        };
        assert!((summary.percentage() - 0.0).abs() < 0.001);
        assert!(!summary.is_healthy()); // Empty is not healthy
    }

    #[test]
    fn test_health_summary_default() {
        let summary = HealthSummary::default();
        assert_eq!(summary.online_count, 0);
        assert_eq!(summary.total_count, 0);
    }

    #[test]
    fn test_app_mode_default() {
        let mode = AppMode::default();
        assert_eq!(mode, AppMode::CommandSelection);
    }

    #[test]
    fn test_results_view_type_default() {
        let view_type = ResultsViewType::default();
        assert_eq!(view_type, ResultsViewType::Runners);
    }

    #[test]
    fn test_manager_row_creation() {
        let manager = RunnerManager {
            id: 1,
            system_id: "test-host".to_string(),
            created_at: "2024-01-15T10:30:00.000Z".to_string(),
            contacted_at: Some("2024-01-20T14:22:00.000Z".to_string()),
            ip_address: Some("10.0.1.1".to_string()),
            status: "online".to_string(),
            version: Some("17.5.0".to_string()),
            revision: None,
            platform: None,
            architecture: None,
        };

        let row = ManagerRow {
            runner_id: 12345,
            runner_tags: vec!["alm".to_string(), "prod".to_string()],
            manager: manager.clone(),
        };

        assert_eq!(row.runner_id, 12345);
        assert_eq!(row.runner_tags.len(), 2);
        assert_eq!(row.manager.system_id, "test-host");
    }
}
