use super::manager::RunnerManager;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, time::Instant};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Runner {
    pub id: u64,
    pub runner_type: String,
    pub active: bool,
    pub paused: bool,
    pub description: Option<String>,
    pub created_at: Option<String>,
    pub ip_address: Option<String>,
    pub is_shared: bool,
    pub status: String,
    pub version: Option<String>,
    pub revision: Option<String>,
    #[serde(default)]
    pub tag_list: Vec<String>,
    #[serde(default)]
    pub managers: Vec<RunnerManager>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TagFilterMode {
    #[default]
    And,
    Or,
}

impl TagFilterMode {
    pub fn toggle(self) -> Self {
        match self {
            TagFilterMode::And => TagFilterMode::Or,
            TagFilterMode::Or => TagFilterMode::And,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct RunnerFilters {
    /// Text-filter tags — sent to the GitLab API (always AND).
    pub tag_list: Option<Vec<String>>,
    /// Popup-selected tags — filtered client-side with `popup_tag_mode`.
    pub popup_tags: Option<Vec<String>>,
    pub popup_tag_mode: TagFilterMode,
    pub status: Option<String>,
    pub version_prefix: Option<String>,
    pub selected_versions: Option<Vec<String>>,
    pub older_than_secs: Option<u64>,
    pub runner_type: Option<String>,
    pub paused: Option<bool>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunnerSortKey {
    #[default]
    None,
    Status,
    Version,
    LastContact,
    Tags,
    Managers,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalBenchmarkMeasurement {
    pub sample_size: usize,
    pub filtered_count: usize,
    pub worker_row_count: usize,
    pub filter_duration_micros: u128,
    pub sort_duration_micros: u128,
    pub flatten_duration_micros: u128,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocalBenchmarkSnapshot {
    pub measurements: Vec<LocalBenchmarkMeasurement>,
}

#[allow(dead_code)]
pub fn parse_runner_created_at(runner: &Runner) -> Option<DateTime<Utc>> {
    parse_timestamp(runner.created_at.as_deref())
}

#[allow(dead_code)]
pub fn parse_manager_created_at(manager: &RunnerManager) -> Option<DateTime<Utc>> {
    parse_timestamp(Some(manager.created_at.as_str()))
}

#[allow(dead_code)]
pub fn parse_manager_contacted_at(manager: &RunnerManager) -> Option<DateTime<Utc>> {
    parse_timestamp(manager.contacted_at.as_deref())
}

#[allow(dead_code)]
pub fn latest_runner_contact_at(runner: &Runner) -> Option<DateTime<Utc>> {
    runner
        .managers
        .iter()
        .filter_map(parse_manager_contacted_at)
        .max()
}

#[allow(dead_code)]
pub fn runner_age_secs(runner: &Runner, now: DateTime<Utc>) -> Option<u64> {
    parse_runner_created_at(runner)
        .map(|created_at| now.signed_duration_since(created_at).num_seconds().max(0) as u64)
}

#[allow(dead_code)]
pub fn extract_runner_versions(runners: &[Runner]) -> Vec<String> {
    let mut versions: Vec<String> = runners
        .iter()
        .flat_map(|runner| {
            runner.version.iter().chain(
                runner
                    .managers
                    .iter()
                    .filter_map(|manager| manager.version.as_ref()),
            )
        })
        .map(|version| version.trim())
        .filter(|version| !version.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    versions.sort_by(|left, right| compare_versions_desc(left, right));
    versions.dedup();
    versions
}

#[allow(dead_code)]
pub fn extract_runner_tags(runners: &[Runner]) -> Vec<String> {
    let mut tags: Vec<String> = runners
        .iter()
        .flat_map(|runner| runner.tag_list.iter())
        .map(|tag| tag.trim())
        .filter(|tag| !tag.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    tags.sort();
    tags.dedup();
    tags
}

#[allow(dead_code)]
pub fn runner_matches_filters(
    runner: &Runner,
    filters: &RunnerFilters,
    now: DateTime<Utc>,
) -> bool {
    if let Some(tags) = &filters.tag_list {
        if !tags
            .iter()
            .all(|tag| runner.tag_list.iter().any(|value| value == tag))
        {
            return false;
        }
    }

    if let Some(popup_tags) = &filters.popup_tags {
        let matched = match filters.popup_tag_mode {
            TagFilterMode::And => popup_tags
                .iter()
                .all(|tag| runner.tag_list.iter().any(|v| v == tag)),
            TagFilterMode::Or => popup_tags
                .iter()
                .any(|tag| runner.tag_list.iter().any(|v| v == tag)),
        };
        if !matched {
            return false;
        }
    }

    if let Some(status) = &filters.status {
        if runner.status != *status {
            return false;
        }
    }

    if let Some(prefix) = &filters.version_prefix {
        let version = runner.version.as_deref().unwrap_or_default();
        if !version.starts_with(prefix) {
            return false;
        }
    }

    if let Some(selected_versions) = &filters.selected_versions {
        if !selected_versions.is_empty() {
            let available_versions = runner_versions(runner);
            if !selected_versions.iter().any(|selected| {
                available_versions
                    .iter()
                    .any(|candidate| candidate == selected)
            }) {
                return false;
            }
        }
    }

    if let Some(older_than_secs) = filters.older_than_secs {
        match runner_age_secs(runner, now) {
            Some(age_secs) if age_secs >= older_than_secs => {}
            _ => return false,
        }
    }

    if let Some(runner_type) = &filters.runner_type {
        if runner.runner_type != *runner_type {
            return false;
        }
    }

    if let Some(paused) = filters.paused {
        if runner.paused != paused {
            return false;
        }
    }

    true
}

#[allow(dead_code)]
pub fn apply_runner_filters(
    runners: &[Runner],
    filters: &RunnerFilters,
    now: DateTime<Utc>,
) -> Vec<Runner> {
    runners
        .iter()
        .filter(|runner| runner_matches_filters(runner, filters, now))
        .cloned()
        .collect()
}

#[allow(dead_code)]
pub fn sort_runners(runners: &mut [Runner], sort_key: RunnerSortKey, now: DateTime<Utc>) {
    match sort_key {
        RunnerSortKey::None => {}
        RunnerSortKey::Status => runners.sort_by(|left, right| {
            left.status
                .cmp(&right.status)
                .then_with(|| left.id.cmp(&right.id))
        }),
        RunnerSortKey::Version => runners.sort_by(|left, right| {
            compare_versions_desc(
                left.version.as_deref().unwrap_or(""),
                right.version.as_deref().unwrap_or(""),
            )
            .then_with(|| left.id.cmp(&right.id))
        }),
        RunnerSortKey::LastContact => runners.sort_by(|left, right| {
            compare_option_datetimes(
                latest_runner_contact_at(left),
                latest_runner_contact_at(right),
            )
            .then_with(|| left.id.cmp(&right.id))
        }),
        RunnerSortKey::Tags => runners.sort_by(|left, right| {
            left.tag_list
                .join(", ")
                .cmp(&right.tag_list.join(", "))
                .then_with(|| left.id.cmp(&right.id))
        }),
        RunnerSortKey::Managers => runners.sort_by(|left, right| {
            right
                .managers
                .len()
                .cmp(&left.managers.len())
                .then_with(|| left.id.cmp(&right.id))
        }),
    }

    let _ = now;
}

#[allow(dead_code)]
pub fn sort_managers_by_last_contact(managers: &mut [RunnerManager], _now: DateTime<Utc>) {
    managers.sort_by(|left, right| {
        compare_option_datetimes(
            parse_manager_contacted_at(left),
            parse_manager_contacted_at(right),
        )
        .then_with(|| left.id.cmp(&right.id))
    });
}

#[allow(dead_code)]
pub fn benchmark_runner_processing(
    runners: &[Runner],
    filters: &RunnerFilters,
    sort_key: RunnerSortKey,
    now: DateTime<Utc>,
) -> LocalBenchmarkSnapshot {
    const SAMPLE_SIZES: [usize; 3] = [10, 50, 100];
    let mut measurements = Vec::new();
    let mut seen_sample_sizes = Vec::new();

    for sample_size in SAMPLE_SIZES {
        let sample_size = sample_size.min(runners.len());
        if sample_size == 0 {
            continue;
        }
        if seen_sample_sizes.contains(&sample_size) {
            continue;
        }
        seen_sample_sizes.push(sample_size);

        let sample = runners[..sample_size].to_vec();

        let filter_started = Instant::now();
        let filtered = apply_runner_filters(&sample, filters, now);
        let filter_duration_micros = filter_started.elapsed().as_micros();

        let sort_started = Instant::now();
        let mut sorted = filtered.clone();
        sort_runners(&mut sorted, sort_key, now);
        let sort_duration_micros = sort_started.elapsed().as_micros();

        let flatten_started = Instant::now();
        let worker_row_count = sorted.iter().map(|runner| runner.managers.len()).sum();
        let _flattened: Vec<(u64, u64)> = sorted
            .iter()
            .flat_map(|runner| {
                runner
                    .managers
                    .iter()
                    .map(move |manager| (runner.id, manager.id))
            })
            .collect();
        let flatten_duration_micros = flatten_started.elapsed().as_micros();

        measurements.push(LocalBenchmarkMeasurement {
            sample_size,
            filtered_count: sorted.len(),
            worker_row_count,
            filter_duration_micros,
            sort_duration_micros,
            flatten_duration_micros,
        });
    }

    LocalBenchmarkSnapshot { measurements }
}

fn runner_versions(runner: &Runner) -> Vec<&str> {
    runner
        .version
        .iter()
        .map(String::as_str)
        .chain(
            runner
                .managers
                .iter()
                .filter_map(|manager| manager.version.as_deref()),
        )
        .collect()
}

fn parse_timestamp(value: Option<&str>) -> Option<DateTime<Utc>> {
    value
        .and_then(|timestamp| DateTime::parse_from_rfc3339(timestamp).ok())
        .map(|timestamp| timestamp.with_timezone(&Utc))
}

fn compare_option_datetimes(left: Option<DateTime<Utc>>, right: Option<DateTime<Utc>>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => right.cmp(&left), // Newer first
        (None, Some(_)) => Ordering::Less,             // None first
        (Some(_), None) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn compare_versions_desc(left: &str, right: &str) -> Ordering {
    compare_versions_asc(right, left)
}

fn compare_versions_asc(left: &str, right: &str) -> Ordering {
    let left_key = version_sort_key(left);
    let right_key = version_sort_key(right);
    left_key.cmp(&right_key).then_with(|| left.cmp(right))
}

fn version_sort_key(version: &str) -> (Vec<u32>, String) {
    let numeric = version
        .split(['.', '-', '+'])
        .map(|segment| segment.parse::<u32>().unwrap_or(0))
        .collect();
    (numeric, version.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::manager::RunnerManager;

    #[test]
    fn test_runner_deserialization() {
        let json = r#"{
            "id": 12345,
            "runner_type": "group_type",
            "active": true,
            "paused": false,
            "description": "Production ALM Runner",
            "created_at": "2024-01-15T10:30:00.000Z",
            "ip_address": "10.0.1.50",
            "is_shared": false,
            "status": "online",
            "version": "17.5.0",
            "revision": "abc123def",
            "tag_list": ["alm", "production", "linux"],
            "managers": []
        }"#;

        let runner: Runner = serde_json::from_str(json).expect("Failed to deserialize runner");

        assert_eq!(runner.id, 12345);
        assert_eq!(runner.runner_type, "group_type");
        assert!(runner.active);
        assert_eq!(runner.status, "online");
        assert_eq!(runner.tag_list.len(), 3);
    }

    #[test]
    fn test_runner_with_managers() {
        let json = r#"{
            "id": 12345,
            "runner_type": "instance_type",
            "active": true,
            "paused": false,
            "description": null,
            "created_at": "2024-01-15T10:30:00.000Z",
            "ip_address": null,
            "is_shared": true,
            "status": "online",
            "version": "17.5.0",
            "revision": null,
            "tag_list": ["shared"],
            "managers": [{
                "id": 1,
                "system_id": "host-1",
                "created_at": "2024-01-15T10:30:00.000Z",
                "contacted_at": "2024-01-20T14:22:00.000Z",
                "ip_address": "10.0.1.1",
                "status": "online",
                "version": "17.5.0",
                "revision": "abc"
            }]
        }"#;

        let runner: Runner = serde_json::from_str(json).expect("Failed to deserialize runner");

        assert_eq!(runner.id, 12345);
        assert_eq!(runner.runner_type, "instance_type");
        assert!(runner.is_shared);
        assert_eq!(runner.managers.len(), 1);
        assert_eq!(runner.managers[0].system_id, "host-1");
    }

    #[test]
    fn test_runner_all_status_variants() {
        for status in &["online", "offline", "stale", "never_contacted"] {
            let json = format!(
                r#"{{
                    "id": 1,
                    "runner_type": "group_type",
                    "active": true,
                    "paused": false,
                    "description": null,
                    "created_at": "2024-01-15T10:30:00.000Z",
                    "ip_address": null,
                    "is_shared": false,
                    "status": "{}",
                    "version": null,
                    "revision": null,
                    "tag_list": [],
                    "managers": []
                }}"#,
                status
            );

            let runner: Runner = serde_json::from_str(&json).expect("Failed to deserialize");
            assert_eq!(runner.status, *status);
        }
    }

    #[test]
    fn test_runner_all_type_variants() {
        for runner_type in &["instance_type", "group_type", "project_type"] {
            let json = format!(
                r#"{{
                    "id": 1,
                    "runner_type": "{}",
                    "active": true,
                    "paused": false,
                    "description": null,
                    "created_at": "2024-01-15T10:30:00.000Z",
                    "ip_address": null,
                    "is_shared": false,
                    "status": "online",
                    "version": null,
                    "revision": null,
                    "tag_list": [],
                    "managers": []
                }}"#,
                runner_type
            );

            let runner: Runner = serde_json::from_str(&json).expect("Failed to deserialize");
            assert_eq!(runner.runner_type, *runner_type);
        }
    }

    #[test]
    fn test_runner_filters_default() {
        let filters = RunnerFilters::default();
        assert!(filters.tag_list.is_none());
        assert!(filters.status.is_none());
        assert!(filters.version_prefix.is_none());
        assert!(filters.selected_versions.is_none());
        assert!(filters.older_than_secs.is_none());
        assert!(filters.runner_type.is_none());
        assert!(filters.paused.is_none());
    }

    #[test]
    fn test_runner_filters_with_tags() {
        let filters = RunnerFilters {
            tag_list: Some(vec!["alm".to_string(), "production".to_string()]),
            ..RunnerFilters::default()
        };

        let tags = filters.tag_list.unwrap();
        assert_eq!(tags.len(), 2);
        assert!(tags.contains(&"alm".to_string()));
    }

    fn create_test_runner(id: u64, status: &str, manager_status: Option<&str>) -> Runner {
        let managers = match manager_status {
            Some(s) => vec![RunnerManager {
                id: id * 10,
                system_id: format!("host-{}", id),
                created_at: "2024-01-15T10:30:00.000Z".to_string(),
                contacted_at: Some("2024-01-20T14:22:00.000Z".to_string()),
                ip_address: Some("10.0.1.1".to_string()),
                status: s.to_string(),
                version: Some("17.5.0".to_string()),
                revision: None,
                platform: None,
                architecture: None,
            }],
            None => vec![],
        };

        Runner {
            id,
            runner_type: "group_type".to_string(),
            active: true,
            paused: false,
            description: None,
            created_at: Some("2024-01-15T10:30:00.000Z".to_string()),
            ip_address: None,
            is_shared: false,
            status: status.to_string(),
            version: Some("17.5.0".to_string()),
            revision: None,
            tag_list: vec!["alm".to_string()],
            managers,
        }
    }

    #[test]
    fn test_filter_runners_with_online_managers() {
        let runners = [
            create_test_runner(1, "online", Some("online")),
            create_test_runner(2, "online", Some("offline")),
            create_test_runner(3, "online", None),
        ];

        // Filter for runners with online managers
        let online: Vec<_> = runners
            .iter()
            .filter(|r| {
                r.managers
                    .first()
                    .map(|m| m.status == "online")
                    .unwrap_or(false)
            })
            .collect();

        assert_eq!(online.len(), 1);
        assert_eq!(online[0].id, 1);
    }

    #[test]
    fn test_filter_runners_without_managers() {
        let runners = [
            create_test_runner(1, "online", Some("online")),
            create_test_runner(2, "online", None),
            create_test_runner(3, "never_contacted", None),
        ];

        let empty: Vec<_> = runners.iter().filter(|r| r.managers.is_empty()).collect();

        assert_eq!(empty.len(), 2);
        assert!(empty.iter().any(|r| r.id == 2));
        assert!(empty.iter().any(|r| r.id == 3));
    }

    #[test]
    fn test_extract_runner_tags_deduplicates_and_sorts_alpha() {
        let mut r1 = create_test_runner(1, "online", None);
        r1.tag_list = vec!["linux".to_owned(), "docker".to_owned()];
        let mut r2 = create_test_runner(2, "online", None);
        r2.tag_list = vec!["docker".to_owned(), "prod".to_owned()];
        let tags = extract_runner_tags(&[r1, r2]);
        assert_eq!(tags, vec!["docker", "linux", "prod"]);
    }

    #[test]
    fn test_extract_runner_tags_trims_and_drops_empty() {
        let mut r = create_test_runner(1, "online", None);
        r.tag_list = vec!["  linux  ".to_owned(), "".to_owned(), " ".to_owned()];
        let tags = extract_runner_tags(&[r]);
        assert_eq!(tags, vec!["linux"]);
    }

    #[test]
    fn test_extract_runner_tags_empty_runners() {
        let tags = extract_runner_tags(&[]);
        assert!(tags.is_empty());
    }

    #[test]
    fn test_extract_runner_versions_deduplicates_and_sorts_desc() {
        let mut first = create_test_runner(1, "online", Some("online"));
        first.version = Some("16.11.1".to_string());
        first.managers[0].version = Some("17.5.0".to_string());

        let mut second = create_test_runner(2, "online", Some("online"));
        second.version = Some("17.4.1".to_string());
        second.managers[0].version = Some("17.5.0".to_string());

        let versions = extract_runner_versions(&[first, second]);
        assert_eq!(versions, vec!["17.5.0", "17.4.1", "16.11.1"]);
    }

    #[test]
    fn test_runner_matches_selected_versions_against_runner_or_manager() {
        let mut runner = create_test_runner(1, "online", Some("online"));
        runner.version = Some("17.4.0".to_string());
        runner.managers[0].version = Some("17.5.0".to_string());

        let filters = RunnerFilters {
            selected_versions: Some(vec!["17.5.0".to_string()]),
            ..RunnerFilters::default()
        };

        assert!(runner_matches_filters(&runner, &filters, Utc::now()));
    }

    #[test]
    fn test_runner_matches_older_than_requires_valid_created_at() {
        let now = DateTime::parse_from_rfc3339("2024-02-20T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let runner = create_test_runner(1, "online", Some("online"));
        let filters = RunnerFilters {
            older_than_secs: Some(60 * 60 * 24 * 7),
            ..RunnerFilters::default()
        };

        assert!(runner_matches_filters(&runner, &filters, now));

        let mut missing_created = create_test_runner(2, "online", Some("online"));
        missing_created.created_at = None;
        assert!(!runner_matches_filters(&missing_created, &filters, now));
    }

    #[test]
    fn test_apply_runner_filters_combines_tags_and_versions() {
        let mut prod = create_test_runner(1, "online", Some("online"));
        prod.tag_list.push("prod".to_string());
        prod.version = Some("17.5.0".to_string());

        let mut qa = create_test_runner(2, "online", Some("online"));
        qa.tag_list.push("qa".to_string());
        qa.version = Some("17.4.0".to_string());

        let filters = RunnerFilters {
            tag_list: Some(vec!["alm".to_string(), "prod".to_string()]),
            selected_versions: Some(vec!["17.5.0".to_string()]),
            ..RunnerFilters::default()
        };

        let filtered = apply_runner_filters(&[prod.clone(), qa], &filters, Utc::now());
        assert_eq!(filtered, vec![prod]);
    }

    #[test]
    fn test_sort_runners_by_status() {
        let now = Utc::now();
        let offline = create_test_runner(1, "offline", Some("offline"));
        let online = create_test_runner(2, "online", Some("online"));

        let mut runners = vec![online.clone(), offline.clone()];
        sort_runners(&mut runners, RunnerSortKey::Status, now);

        assert_eq!(runners[0].id, offline.id);
    }

    #[test]
    fn test_sort_runners_by_last_contact_handles_missing() {
        let now = Utc::now();
        let mut stale = create_test_runner(1, "online", Some("online"));
        stale.managers[0].contacted_at = Some("2024-01-01T00:00:00Z".to_string());

        let mut missing = create_test_runner(2, "online", Some("online"));
        missing.managers[0].contacted_at = None;

        let mut recent = create_test_runner(3, "online", Some("online"));
        recent.managers[0].contacted_at = Some("2024-02-01T00:00:00Z".to_string());

        let mut runners = vec![recent.clone(), stale.clone(), missing.clone()];
        sort_runners(&mut runners, RunnerSortKey::LastContact, now);

        // Expect: missing (None) first, then recent (Newer) second, then stale (Older) third
        assert_eq!(runners[0].id, missing.id);
        assert_eq!(runners[1].id, recent.id);
        assert_eq!(runners[2].id, stale.id);
    }

    #[test]
    fn test_sort_runners_by_version() {
        let now = Utc::now();
        let mut older = create_test_runner(1, "online", Some("online"));
        older.version = Some("17.4.0".to_string());
        let mut newer = create_test_runner(2, "online", Some("online"));
        newer.version = Some("17.5.0".to_string());

        let mut runners = vec![older.clone(), newer.clone()];
        sort_runners(&mut runners, RunnerSortKey::Version, now);

        assert_eq!(runners[0].id, newer.id);
    }

    #[test]
    fn test_sort_managers_by_last_contact() {
        let mut first = RunnerManager {
            id: 1,
            system_id: "one".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            contacted_at: Some("2024-02-01T00:00:00Z".to_string()),
            ip_address: None,
            status: "online".to_string(),
            version: None,
            revision: None,
            platform: None,
            architecture: None,
        };
        let second = RunnerManager {
            id: 2,
            contacted_at: None,
            ..first.clone()
        };
        first.contacted_at = Some("2024-01-01T00:00:00Z".to_string());
        let mut managers = vec![first.clone(), second.clone()];
        sort_managers_by_last_contact(&mut managers, Utc::now());
        assert_eq!(managers[0].id, second.id);
        assert_eq!(managers[1].id, first.id);
    }

    #[test]
    fn test_benchmark_runner_processing_uses_available_sample_sizes() {
        let runners: Vec<Runner> = (1..=12)
            .map(|id| create_test_runner(id, "online", Some("online")))
            .collect();

        let snapshot = benchmark_runner_processing(
            &runners,
            &RunnerFilters::default(),
            RunnerSortKey::Status,
            Utc::now(),
        );

        assert_eq!(snapshot.measurements.len(), 2);
        assert_eq!(snapshot.measurements[0].sample_size, 10);
        assert_eq!(snapshot.measurements[1].sample_size, 12);
        assert!(snapshot
            .measurements
            .iter()
            .all(|measurement| measurement.worker_row_count == measurement.filtered_count));
    }
}
