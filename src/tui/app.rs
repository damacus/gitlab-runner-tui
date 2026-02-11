use crate::conductor::Conductor;
use crate::models::manager::RunnerManager;
use crate::models::runner::Runner;
use ratatui::widgets::TableState;

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
    pub mode: AppMode,
    pub should_quit: bool,
    pub runners: Vec<Runner>,
    pub manager_rows: Vec<ManagerRow>,
    pub results_view_type: ResultsViewType,
    pub health_summary: Option<HealthSummary>,

    pub commands: Vec<&'static str>,
    pub selected_command_index: usize,

    pub input_buffer: String,
    pub table_state: TableState,

    // Loading and error state
    pub is_loading: bool,
    pub error_message: Option<String>,
    pub spinner_frame: usize,
}

const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

impl App {
    pub fn new(conductor: Conductor) -> Self {
        Self {
            conductor,
            mode: AppMode::default(),
            should_quit: false,
            runners: Vec::new(),
            manager_rows: Vec::new(),
            results_view_type: ResultsViewType::default(),
            health_summary: None,
            commands: vec!["fetch", "lights", "switch", "workers", "flames", "empty"],
            selected_command_index: 0,
            input_buffer: String::new(),
            table_state: TableState::default(),
            is_loading: false,
            error_message: None,
            spinner_frame: 0,
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

        let is_workers = command == "workers";
        let is_lights = command == "lights";

        let result = match command {
            "fetch" | "lights" | "workers" => self.conductor.fetch_runners(filters).await,
            "switch" => self.conductor.list_offline_runners(filters).await,
            "flames" => self.conductor.list_uncontacted_runners(filters, 3600).await,
            "empty" => self.conductor.list_runners_without_managers(filters).await,
            _ => {
                self.is_loading = false;
                return;
            }
        };

        self.is_loading = false;

        match result {
            Ok(runners) => {
                // Clear all previous results before populating new ones
                self.runners.clear();
                self.manager_rows.clear();
                self.health_summary = None;

                if is_workers {
                    // Flatten runners into manager rows
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
                } else if is_lights {
                    let online_count = runners
                        .iter()
                        .filter(|r| {
                            r.managers
                                .first()
                                .map(|m| m.status == "online")
                                .unwrap_or(false)
                        })
                        .count();
                    self.health_summary = Some(HealthSummary {
                        online_count,
                        total_count: runners.len(),
                    });
                    self.runners = runners;
                    self.results_view_type = ResultsViewType::HealthCheck;
                } else {
                    self.runners = runners;
                    self.results_view_type = ResultsViewType::Runners;
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
            ResultsViewType::Runners | ResultsViewType::HealthCheck => self.runners.len(),
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
            ResultsViewType::Runners | ResultsViewType::HealthCheck => self.runners.len(),
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

    pub fn tick(&mut self) {
        if self.is_loading {
            self.advance_spinner();
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
