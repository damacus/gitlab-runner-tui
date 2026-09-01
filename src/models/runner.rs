use super::manager::RunnerManager;
use crate::time::{elapsed_seconds, parse_rfc3339, parse_user_cutoff};
use jiff::{Timestamp, Zoned};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, cmp::Reverse, time::Instant};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct RunnerGroup {
    pub id: u64,
    pub name: String,
    pub web_url: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
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
    #[serde(default)]
    pub groups: Vec<RunnerGroup>,
}

#[cfg(test)]
thread_local! {
    static RUNNER_CLONE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl Clone for Runner {
    fn clone(&self) -> Self {
        #[cfg(test)]
        RUNNER_CLONE_COUNT.with(|count| count.set(count.get() + 1));

        Self {
            id: self.id,
            runner_type: self.runner_type.clone(),
            active: self.active,
            paused: self.paused,
            description: self.description.clone(),
            created_at: self.created_at.clone(),
            ip_address: self.ip_address.clone(),
            is_shared: self.is_shared,
            status: self.status.clone(),
            version: self.version.clone(),
            revision: self.revision.clone(),
            tag_list: self.tag_list.clone(),
            managers: self.managers.clone(),
            groups: self.groups.clone(),
        }
    }
}

#[cfg(test)]
pub fn reset_runner_clone_count() {
    RUNNER_CLONE_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
pub fn runner_clone_count() -> usize {
    RUNNER_CLONE_COUNT.with(std::cell::Cell::get)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContactThreshold {
    OlderThanSecs(u64),
    Since(Timestamp),
}

impl ContactThreshold {
    pub fn is_contact_stale(self, contacted_at: Option<Timestamp>, now: Timestamp) -> bool {
        match contacted_at {
            Some(contacted_at) => match self {
                Self::OlderThanSecs(seconds) => elapsed_seconds(now, contacted_at) > seconds as i64,
                Self::Since(cutoff) => contacted_at <= cutoff,
            },
            None => true,
        }
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalBenchmarkMeasurement {
    pub sample_size: usize,
    pub filtered_count: usize,
    pub worker_row_count: usize,
    pub filter_duration_micros: u128,
    pub sort_duration_micros: u128,
    pub flatten_duration_micros: u128,
    pub deep_runner_clones: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocalBenchmarkSnapshot {
    pub measurements: Vec<LocalBenchmarkMeasurement>,
}

pub fn parse_runner_created_at(runner: &Runner) -> Option<Timestamp> {
    parse_timestamp(runner.created_at.as_deref())
}

pub fn parse_manager_created_at(manager: &RunnerManager) -> Option<Timestamp> {
    parse_timestamp(Some(manager.created_at.as_str()))
}

pub fn parse_manager_contacted_at(manager: &RunnerManager) -> Option<Timestamp> {
    parse_timestamp(manager.contacted_at.as_deref())
}

pub fn parse_stale_cutoff(input: &str, now: &Zoned) -> Result<Option<Timestamp>, String> {
    parse_user_cutoff(input, now)
}

pub fn latest_runner_contact_at(runner: &Runner) -> Option<Timestamp> {
    runner
        .managers
        .iter()
        .filter_map(parse_manager_contacted_at)
        .max()
}

pub fn runner_age_secs(runner: &Runner, now: Timestamp) -> Option<u64> {
    parse_runner_created_at(runner).map(|created_at| elapsed_seconds(now, created_at).max(0) as u64)
}

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

pub fn runner_matches_filters(runner: &Runner, filters: &RunnerFilters, now: Timestamp) -> bool {
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

pub fn apply_runner_filters(
    runners: &[Runner],
    filters: &RunnerFilters,
    now: Timestamp,
) -> Vec<Runner> {
    runners
        .iter()
        .filter(|runner| runner_matches_filters(runner, filters, now))
        .cloned()
        .collect()
}

#[cfg(test)]
pub fn sort_runners(runners: &mut [Runner], sort_key: RunnerSortKey, now: Timestamp) {
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
        RunnerSortKey::LastContact => {
            sort_runners_by_last_contact_with(runners, latest_runner_contact_at)
        }
        RunnerSortKey::Tags => runners.sort_by(|left, right| {
            // Compare tag_list vectors directly to avoid expensive allocations
            // caused by calling .join() within the O(N log N) sorting closure.
            left.tag_list
                .cmp(&right.tag_list)
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

pub fn sort_runner_indices(
    runners: &[Runner],
    indices: &mut [usize],
    sort_key: RunnerSortKey,
    now: Timestamp,
) {
    match sort_key {
        RunnerSortKey::None => {}
        RunnerSortKey::Status => indices.sort_by(|left, right| {
            let left = &runners[*left];
            let right = &runners[*right];
            left.status
                .cmp(&right.status)
                .then_with(|| left.id.cmp(&right.id))
        }),
        RunnerSortKey::Version => indices.sort_by(|left, right| {
            let left = &runners[*left];
            let right = &runners[*right];
            compare_versions_desc(
                left.version.as_deref().unwrap_or(""),
                right.version.as_deref().unwrap_or(""),
            )
            .then_with(|| left.id.cmp(&right.id))
        }),
        RunnerSortKey::LastContact => indices.sort_by_cached_key(|index| {
            let runner = &runners[*index];
            (latest_runner_contact_at(runner).map(Reverse), runner.id)
        }),
        RunnerSortKey::Tags => indices.sort_by(|left, right| {
            let left = &runners[*left];
            let right = &runners[*right];
            left.tag_list
                .cmp(&right.tag_list)
                .then_with(|| left.id.cmp(&right.id))
        }),
        RunnerSortKey::Managers => indices.sort_by(|left, right| {
            let left = &runners[*left];
            let right = &runners[*right];
            right
                .managers
                .len()
                .cmp(&left.managers.len())
                .then_with(|| left.id.cmp(&right.id))
        }),
    }

    let _ = now;
}

#[cfg(test)]
fn sort_runners_by_last_contact_with<F>(runners: &mut [Runner], mut contact_key: F)
where
    F: FnMut(&Runner) -> Option<Timestamp>,
{
    runners.sort_by_cached_key(|runner| (contact_key(runner).map(Reverse), runner.id));
}

#[cfg(test)]
pub fn sort_managers_by_last_contact(managers: &mut [RunnerManager], _now: Timestamp) {
    managers.sort_by(|left, right| {
        compare_option_datetimes(
            parse_manager_contacted_at(left),
            parse_manager_contacted_at(right),
        )
        .then_with(|| left.id.cmp(&right.id))
    });
}

pub fn benchmark_runner_processing(
    runners: &[Runner],
    filters: &RunnerFilters,
    sort_key: RunnerSortKey,
    now: Timestamp,
) -> LocalBenchmarkSnapshot {
    const SAMPLE_SIZES: [usize; 5] = [10, 50, 100, 1_000, 10_000];
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

        let filter_started = Instant::now();
        let mut filtered: Vec<usize> = runners[..sample_size]
            .iter()
            .enumerate()
            .filter_map(|(index, runner)| {
                runner_matches_filters(runner, filters, now).then_some(index)
            })
            .collect();
        let filter_duration_micros = filter_started.elapsed().as_micros();

        let sort_started = Instant::now();
        sort_runner_indices(runners, &mut filtered, sort_key, now);
        let sort_duration_micros = sort_started.elapsed().as_micros();

        let flatten_started = Instant::now();
        let worker_row_count = filtered
            .iter()
            .map(|index| runners[*index].managers.len())
            .sum();
        let _flattened: Vec<(usize, usize)> = filtered
            .iter()
            .flat_map(|runner_index| {
                runners[*runner_index]
                    .managers
                    .iter()
                    .enumerate()
                    .map(move |(manager_index, _)| (*runner_index, manager_index))
            })
            .collect();
        let flatten_duration_micros = flatten_started.elapsed().as_micros();

        measurements.push(LocalBenchmarkMeasurement {
            sample_size,
            filtered_count: filtered.len(),
            worker_row_count,
            filter_duration_micros,
            sort_duration_micros,
            flatten_duration_micros,
            deep_runner_clones: 0,
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

fn parse_timestamp(value: Option<&str>) -> Option<Timestamp> {
    value.and_then(|timestamp| parse_rfc3339(timestamp).ok())
}

#[cfg(test)]
fn compare_option_datetimes(left: Option<Timestamp>, right: Option<Timestamp>) -> Ordering {
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

// ⚡ Bolt: Compare iterators directly to prevent O(N log N) heap allocations
// during version sorting in the hot render loop.
fn compare_versions_asc(left: &str, right: &str) -> Ordering {
    let left_iter = left
        .split(['.', '-', '+'])
        .map(|segment| segment.parse::<u32>().unwrap_or(0));
    let right_iter = right
        .split(['.', '-', '+'])
        .map(|segment| segment.parse::<u32>().unwrap_or(0));

    left_iter.cmp(right_iter).then_with(|| left.cmp(right))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::manager::RunnerManager;
    use crate::time::{add_seconds, format_rfc3339, now as timestamp_now, system_time_zone};
    use jiff::tz::TimeZone;

    fn london_now(input: &str) -> Zoned {
        parse_rfc3339(input)
            .unwrap()
            .to_zoned(TimeZone::get("Europe/London").unwrap())
    }

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

    #[test]
    fn test_parse_stale_cutoff_accepts_hour_and_minute() {
        let now = london_now("2026-05-12T09:30:00+01:00");
        let cutoff = parse_stale_cutoff("11:00", &now).unwrap().unwrap();
        let local_cutoff = cutoff.to_zoned(now.time_zone().clone());

        assert_eq!(local_cutoff.date(), now.date());
        assert_eq!(local_cutoff.strftime("%H:%M:%S").to_string(), "11:00:00");
    }

    #[test]
    fn test_parse_stale_cutoff_accepts_hour_minute_and_second() {
        let now = london_now("2026-05-12T09:30:00+01:00");
        let cutoff = parse_stale_cutoff("11:00:30", &now).unwrap().unwrap();
        let local_cutoff = cutoff.to_zoned(now.time_zone().clone());

        assert_eq!(local_cutoff.date(), now.date());
        assert_eq!(local_cutoff.strftime("%H:%M:%S").to_string(), "11:00:30");
    }

    #[test]
    fn test_parse_stale_cutoff_accepts_rfc3339() {
        let now = timestamp_now().to_zoned(system_time_zone());
        let cutoff = parse_stale_cutoff("2026-05-12T11:00:00+01:00", &now)
            .unwrap()
            .unwrap();

        assert_eq!(format_rfc3339(cutoff), "2026-05-12T10:00:00+00:00");
    }

    #[test]
    fn test_parse_stale_cutoff_blank_clears_cutoff() {
        let now = timestamp_now().to_zoned(system_time_zone());
        assert_eq!(parse_stale_cutoff("   ", &now).unwrap(), None);
    }

    #[test]
    fn test_parse_stale_cutoff_rejects_invalid_input() {
        let now = timestamp_now().to_zoned(system_time_zone());
        assert!(parse_stale_cutoff("not-a-time", &now).is_err());
    }

    #[test]
    fn test_contact_threshold_cutoff_treats_equal_contact_as_stale() {
        let cutoff = parse_rfc3339("2026-05-12T10:00:00Z").unwrap();
        let after = add_seconds(cutoff, 1);
        let threshold = ContactThreshold::Since(cutoff);

        assert!(threshold.is_contact_stale(Some(cutoff), after));
        assert!(!threshold.is_contact_stale(Some(after), after));
        assert!(threshold.is_contact_stale(None, after));
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
            groups: vec![],
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

        assert!(runner_matches_filters(&runner, &filters, timestamp_now()));
    }

    #[test]
    fn test_runner_matches_older_than_requires_valid_created_at() {
        let now = parse_rfc3339("2024-02-20T00:00:00Z").unwrap();
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

        let filtered = apply_runner_filters(&[prod.clone(), qa], &filters, timestamp_now());
        assert_eq!(filtered, vec![prod]);
    }

    #[test]
    fn test_sort_runners_by_status() {
        let now = timestamp_now();
        let offline = create_test_runner(1, "offline", Some("offline"));
        let online = create_test_runner(2, "online", Some("online"));

        let mut runners = vec![online.clone(), offline.clone()];
        sort_runners(&mut runners, RunnerSortKey::Status, now);

        assert_eq!(runners[0].id, offline.id);
    }

    #[test]
    fn test_sort_runners_by_last_contact_handles_missing() {
        let now = timestamp_now();
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
    fn test_cached_last_contact_sort_matches_comparator_semantics() {
        let mut missing = create_test_runner(40, "online", Some("online"));
        missing.managers[0].contacted_at = None;

        let mut invalid = create_test_runner(30, "online", Some("online"));
        invalid.managers[0].contacted_at = Some("not-a-timestamp".to_string());

        let mut recent_first = create_test_runner(20, "online", Some("online"));
        recent_first.managers[0].contacted_at = Some("2024-03-01T00:00:00Z".to_string());

        let mut recent_tie = create_test_runner(10, "online", Some("online"));
        recent_tie.managers[0].contacted_at = Some("2024-03-01T00:00:00Z".to_string());

        let mut older = create_test_runner(50, "online", Some("online"));
        older.managers[0].contacted_at = Some("2024-01-01T00:00:00Z".to_string());

        let mut reference = [
            older.clone(),
            recent_first.clone(),
            invalid.clone(),
            missing.clone(),
            recent_tie.clone(),
        ];
        reference.sort_by(|left, right| {
            compare_option_datetimes(
                latest_runner_contact_at(left),
                latest_runner_contact_at(right),
            )
            .then_with(|| left.id.cmp(&right.id))
        });

        let mut cached = [older, recent_first, invalid, missing, recent_tie];
        sort_runners(&mut cached, RunnerSortKey::LastContact, timestamp_now());

        assert_eq!(
            cached.iter().map(|runner| runner.id).collect::<Vec<_>>(),
            reference.iter().map(|runner| runner.id).collect::<Vec<_>>()
        );
        assert_eq!(
            cached.iter().map(|runner| runner.id).collect::<Vec<_>>(),
            vec![30, 40, 10, 20, 50]
        );
    }

    #[test]
    fn test_cached_last_contact_sort_parses_each_manager_once() {
        use std::cell::Cell;

        const RUNNER_COUNT: u64 = 32;
        const MANAGERS_PER_RUNNER: u64 = 7;

        let mut runners: Vec<_> = (0..RUNNER_COUNT)
            .map(|runner_id| {
                let mut runner = create_test_runner(runner_id, "online", None);
                runner.managers = (0..MANAGERS_PER_RUNNER)
                    .map(|manager_id| RunnerManager {
                        id: runner_id * MANAGERS_PER_RUNNER + manager_id,
                        system_id: format!("host-{runner_id}-{manager_id}"),
                        created_at: "2024-01-01T00:00:00Z".to_string(),
                        contacted_at: Some(format!("2024-01-{:02}T00:00:00Z", manager_id + 1)),
                        ip_address: None,
                        status: "online".to_string(),
                        version: None,
                        revision: None,
                        platform: None,
                        architecture: None,
                    })
                    .collect();
                runner
            })
            .collect();
        let parse_count = Cell::new(0usize);

        sort_runners_by_last_contact_with(&mut runners, |runner| {
            runner
                .managers
                .iter()
                .filter_map(|manager| {
                    parse_count.set(parse_count.get() + 1);
                    parse_manager_contacted_at(manager)
                })
                .max()
        });

        assert_eq!(
            parse_count.get(),
            (RUNNER_COUNT * MANAGERS_PER_RUNNER) as usize
        );
    }

    #[test]
    fn test_sort_runners_by_version() {
        let now = timestamp_now();
        let mut older = create_test_runner(1, "online", Some("online"));
        older.version = Some("17.4.0".to_string());
        let mut newer = create_test_runner(2, "online", Some("online"));
        newer.version = Some("17.5.0".to_string());

        let mut runners = vec![older.clone(), newer.clone()];
        sort_runners(&mut runners, RunnerSortKey::Version, now);

        assert_eq!(runners[0].id, newer.id);
    }

    #[test]
    fn projected_index_sort_matches_owned_runner_sort_for_every_key() {
        let mut runners = vec![
            create_test_runner(4, "offline", Some("offline")),
            create_test_runner(2, "online", Some("online")),
            create_test_runner(3, "online", None),
            create_test_runner(1, "stale", Some("online")),
        ];
        runners[0].version = Some("17.1.0".to_string());
        runners[1].version = Some("18.0.0".to_string());
        runners[2].version = None;
        runners[0].tag_list = vec!["z".to_string()];
        runners[1].tag_list = vec!["a".to_string()];
        let extra_manager = runners[3].managers[0].clone();
        runners[3].managers.push(extra_manager);
        let now = timestamp_now();

        for sort_key in [
            RunnerSortKey::None,
            RunnerSortKey::Status,
            RunnerSortKey::Version,
            RunnerSortKey::LastContact,
            RunnerSortKey::Tags,
            RunnerSortKey::Managers,
        ] {
            let mut owned = runners.clone();
            sort_runners(&mut owned, sort_key, now);
            let mut indices: Vec<usize> = (0..runners.len()).collect();
            sort_runner_indices(&runners, &mut indices, sort_key, now);

            assert_eq!(
                indices
                    .into_iter()
                    .map(|index| runners[index].id)
                    .collect::<Vec<_>>(),
                owned
                    .into_iter()
                    .map(|runner| runner.id)
                    .collect::<Vec<_>>(),
                "projection order differs for {sort_key:?}"
            );
        }
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
        sort_managers_by_last_contact(&mut managers, timestamp_now());
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
            timestamp_now(),
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
