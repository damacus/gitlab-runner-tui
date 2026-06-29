use crate::models::runner::{parse_manager_contacted_at, parse_manager_created_at, Runner};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RotationWaitOptions {
    pub rotation_window_start: DateTime<Utc>,
    pub active_contacted_within_secs: u64,
    pub missing_runner_grace_polls: u64,
    pub completion_stability_polls: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RotationWaitEventKind {
    Baseline,
    Progress,
    FleetChanged,
    Complete,
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RotationWaitEvent {
    pub event: RotationWaitEventKind,
    pub eligible_count: usize,
    pub completed_count: usize,
    pub pending_count: usize,
    pub stable_polls: u64,
    pub stale_excluded_count: usize,
    pub added_runner_ids: Vec<u64>,
    pub rotated_runner_ids: Vec<u64>,
    pub missing_runner_ids: Vec<u64>,
    pub removed_runner_ids: Vec<u64>,
    pub pending_runner_ids: Vec<u64>,
    pub is_complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TrackedRunner {
    baseline_system_ids: BTreeSet<String>,
    completed: bool,
    missing_polls: u64,
}

#[derive(Debug, Clone)]
pub struct RotationWaitState {
    options: RotationWaitOptions,
    tracked: BTreeMap<u64, TrackedRunner>,
    observed_baseline: bool,
    stable_complete_polls: u64,
    last_event: Option<RotationWaitEvent>,
}

impl RotationWaitState {
    pub fn new(options: RotationWaitOptions) -> Self {
        Self {
            options,
            tracked: BTreeMap::new(),
            observed_baseline: false,
            stable_complete_polls: 0,
            last_event: None,
        }
    }

    pub fn observe(&mut self, runners: &[Runner], now: DateTime<Utc>) -> RotationWaitEvent {
        let was_baseline = !self.observed_baseline;
        self.observed_baseline = true;

        let mut added_runner_ids = Vec::new();
        let mut rotated_runner_ids = Vec::new();
        let mut missing_runner_ids = Vec::new();
        let mut removed_runner_ids = Vec::new();
        let mut current_runner_ids = BTreeSet::new();
        let mut stale_excluded_count = 0;

        for runner in runners {
            if !self.is_eligible(runner, now) {
                stale_excluded_count += 1;
                continue;
            }

            current_runner_ids.insert(runner.id);
            let created_after_window = self.has_manager_created_after_window(runner);

            match self.tracked.get_mut(&runner.id) {
                Some(tracked) => {
                    tracked.missing_polls = 0;
                    if !tracked.completed
                        && (created_after_window || has_new_system_id(runner, tracked))
                    {
                        tracked.completed = true;
                        rotated_runner_ids.push(runner.id);
                    }
                }
                None => {
                    let completed = created_after_window;
                    let tracked = TrackedRunner {
                        baseline_system_ids: runner_system_ids(runner),
                        completed,
                        missing_polls: 0,
                    };
                    self.tracked.insert(runner.id, tracked);
                    added_runner_ids.push(runner.id);
                    if completed {
                        rotated_runner_ids.push(runner.id);
                    }
                }
            }
        }

        let tracked_ids: Vec<u64> = self.tracked.keys().copied().collect();
        for runner_id in tracked_ids {
            if current_runner_ids.contains(&runner_id) {
                continue;
            }

            if let Some(tracked) = self.tracked.get_mut(&runner_id) {
                tracked.missing_polls += 1;
                missing_runner_ids.push(runner_id);
                if tracked.missing_polls >= self.options.missing_runner_grace_polls {
                    self.tracked.remove(&runner_id);
                    removed_runner_ids.push(runner_id);
                }
            }
        }

        let pending_runner_ids: Vec<u64> = self
            .tracked
            .iter()
            .filter_map(|(runner_id, tracked)| (!tracked.completed).then_some(*runner_id))
            .collect();
        let eligible_count = self.tracked.len();
        let completed_count = eligible_count.saturating_sub(pending_runner_ids.len());

        if eligible_count > 0 && pending_runner_ids.is_empty() {
            self.stable_complete_polls += 1;
        } else {
            self.stable_complete_polls = 0;
        }

        let is_complete = self.stable_complete_polls >= self.options.completion_stability_polls;
        let event = if is_complete {
            RotationWaitEventKind::Complete
        } else if was_baseline {
            RotationWaitEventKind::Baseline
        } else if !rotated_runner_ids.is_empty() {
            RotationWaitEventKind::Progress
        } else if !added_runner_ids.is_empty()
            || !missing_runner_ids.is_empty()
            || !removed_runner_ids.is_empty()
        {
            RotationWaitEventKind::FleetChanged
        } else {
            RotationWaitEventKind::Progress
        };

        let event = RotationWaitEvent {
            event,
            eligible_count,
            completed_count,
            pending_count: pending_runner_ids.len(),
            stable_polls: self.stable_complete_polls,
            stale_excluded_count,
            added_runner_ids,
            rotated_runner_ids,
            missing_runner_ids,
            removed_runner_ids,
            pending_runner_ids,
            is_complete,
        };
        self.last_event = Some(event.clone());
        event
    }

    pub fn timeout_event(&self) -> RotationWaitEvent {
        let mut event = self.last_event.clone().unwrap_or(RotationWaitEvent {
            event: RotationWaitEventKind::Timeout,
            eligible_count: 0,
            completed_count: 0,
            pending_count: 0,
            stable_polls: 0,
            stale_excluded_count: 0,
            added_runner_ids: Vec::new(),
            rotated_runner_ids: Vec::new(),
            missing_runner_ids: Vec::new(),
            removed_runner_ids: Vec::new(),
            pending_runner_ids: Vec::new(),
            is_complete: false,
        });
        event.event = RotationWaitEventKind::Timeout;
        event.is_complete = false;
        event
    }

    fn is_eligible(&self, runner: &Runner, now: DateTime<Utc>) -> bool {
        runner.managers.iter().any(|manager| {
            parse_manager_contacted_at(manager).is_some_and(|contacted_at| {
                now.signed_duration_since(contacted_at).num_seconds()
                    <= self.options.active_contacted_within_secs as i64
            })
        })
    }

    fn has_manager_created_after_window(&self, runner: &Runner) -> bool {
        runner.managers.iter().any(|manager| {
            parse_manager_created_at(manager)
                .is_some_and(|created_at| created_at >= self.options.rotation_window_start)
        })
    }
}

fn has_new_system_id(runner: &Runner, tracked: &TrackedRunner) -> bool {
    runner
        .managers
        .iter()
        .any(|manager| !tracked.baseline_system_ids.contains(&manager.system_id))
}

fn runner_system_ids(runner: &Runner) -> BTreeSet<String> {
    runner
        .managers
        .iter()
        .map(|manager| manager.system_id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{manager::RunnerManager, runner::Runner};
    use chrono::{DateTime, Utc};

    fn runner(id: u64, system_ids: &[&str]) -> Runner {
        Runner {
            id,
            runner_type: "group_type".to_string(),
            active: true,
            paused: false,
            description: Some(format!("Runner {id}")),
            created_at: None,
            ip_address: None,
            is_shared: false,
            status: "online".to_string(),
            version: Some("17.5.0".to_string()),
            revision: Some("abc123".to_string()),
            tag_list: vec!["prod".to_string()],
            managers: system_ids
                .iter()
                .enumerate()
                .map(|(index, system_id)| manager(index as u64 + 1, system_id))
                .collect(),
            groups: Vec::new(),
        }
    }

    fn manager(id: u64, system_id: &str) -> RunnerManager {
        RunnerManager {
            id,
            system_id: system_id.to_string(),
            created_at: "2026-06-29T09:00:00Z".to_string(),
            contacted_at: Some("2026-06-29T10:04:00Z".to_string()),
            ip_address: None,
            status: "online".to_string(),
            version: Some("17.5.0".to_string()),
            revision: Some("abc123".to_string()),
            platform: Some("linux".to_string()),
            architecture: Some("amd64".to_string()),
        }
    }

    fn options() -> RotationWaitOptions {
        RotationWaitOptions {
            rotation_window_start: DateTime::parse_from_rfc3339("2026-06-29T10:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            active_contacted_within_secs: 3600,
            missing_runner_grace_polls: 2,
            completion_stability_polls: 2,
        }
    }

    fn observed_at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-06-29T10:05:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn waits_for_new_system_id_and_stability_before_completion() {
        let now = observed_at();
        let mut state = RotationWaitState::new(options());

        let first = state.observe(&[runner(1, &["old-system"])], now);
        assert_eq!(first.event, RotationWaitEventKind::Baseline);
        assert_eq!(first.eligible_count, 1);
        assert_eq!(first.pending_runner_ids, vec![1]);
        assert!(!first.is_complete);

        let second = state.observe(&[runner(1, &["new-system"])], now);
        assert_eq!(second.event, RotationWaitEventKind::Progress);
        assert_eq!(second.pending_runner_ids, Vec::<u64>::new());
        assert!(!second.is_complete);

        let third = state.observe(&[runner(1, &["new-system"])], now);
        assert_eq!(third.event, RotationWaitEventKind::Complete);
        assert!(third.is_complete);
    }

    #[test]
    fn excludes_stale_runners_and_never_completes_empty_fleet() {
        let mut stale = runner(1, &["old-system"]);
        stale.managers[0].contacted_at = Some("2026-06-29T08:00:00Z".to_string());
        let mut state = RotationWaitState::new(options());

        let event = state.observe(&[stale], observed_at());

        assert_eq!(event.eligible_count, 0);
        assert_eq!(event.stale_excluded_count, 1);
        assert_eq!(event.event, RotationWaitEventKind::Baseline);
        assert!(!event.is_complete);
    }

    #[test]
    fn removes_missing_runners_after_grace_polls() {
        let now = observed_at();
        let mut state = RotationWaitState::new(options());

        state.observe(&[runner(1, &["old-system"])], now);
        let missing_once = state.observe(&[], now);
        assert_eq!(missing_once.pending_runner_ids, vec![1]);
        assert_eq!(missing_once.removed_runner_ids, Vec::<u64>::new());

        let missing_twice = state.observe(&[], now);
        assert_eq!(missing_twice.eligible_count, 0);
        assert_eq!(missing_twice.removed_runner_ids, vec![1]);
        assert!(!missing_twice.is_complete);
    }

    #[test]
    fn newly_added_runner_joins_pending_fleet() {
        let now = observed_at();
        let mut state = RotationWaitState::new(options());

        state.observe(&[runner(1, &["old-one"])], now);
        let event = state.observe(&[runner(1, &["new-one"]), runner(2, &["old-two"])], now);

        assert_eq!(event.added_runner_ids, vec![2]);
        assert_eq!(event.rotated_runner_ids, vec![1]);
        assert_eq!(event.pending_runner_ids, vec![2]);
        assert_eq!(event.eligible_count, 2);
        assert!(!event.is_complete);
    }
}
