use crate::client::GitLabClient;
use crate::config::{
    RunnerDiscoveryMode, RunnerTarget, RunnerTargetKind, DEFAULT_MAX_ENRICHMENT_REQUESTS,
    MAX_MAX_ENRICHMENT_REQUESTS, MIN_MAX_ENRICHMENT_REQUESTS,
};
use crate::metrics::{LiveQueryMetrics, QueryRequestCounts};
use crate::models::runner::{
    apply_runner_filters, parse_manager_contacted_at, ContactThreshold, Runner, RunnerFilters,
};
use anyhow::Result;
use chrono::{DateTime, Utc};
use futures::{
    stream::{self, StreamExt},
    TryStreamExt,
};
use serde::Serialize;
use std::{collections::BTreeMap, future::Future, sync::Arc, time::Instant};
use tokio::sync::Semaphore;

const CONFIGURED_TARGET_CONCURRENCY: usize = 4;

struct TargetFetchResult {
    runners: Vec<Runner>,
    list_requests: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum QueryProfile {
    Summary,
    Detail,
    Managers,
    Full,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct EnrichmentProfile {
    detail: bool,
    managers: bool,
}

impl QueryProfile {
    const fn enrichment(self) -> EnrichmentProfile {
        match self {
            Self::Summary => EnrichmentProfile {
                detail: false,
                managers: false,
            },
            Self::Detail => EnrichmentProfile {
                detail: true,
                managers: false,
            },
            Self::Managers => EnrichmentProfile {
                detail: false,
                managers: true,
            },
            Self::Full => EnrichmentProfile {
                detail: true,
                managers: true,
            },
        }
    }
}

impl EnrichmentProfile {
    fn for_filters(filters: &RunnerFilters) -> Self {
        let selected_versions = filters
            .selected_versions
            .as_ref()
            .is_some_and(|versions| !versions.is_empty());
        Self {
            detail: filters
                .popup_tags
                .as_ref()
                .is_some_and(|tags| !tags.is_empty())
                || filters.version_prefix.is_some()
                || selected_versions
                || filters.older_than_secs.is_some(),
            managers: selected_versions,
        }
    }

    const fn union(self, other: Self) -> Self {
        Self {
            detail: self.detail || other.detail,
            managers: self.managers || other.managers,
        }
    }

    const fn missing_from(self, available: Self) -> Self {
        Self {
            detail: self.detail && !available.detail,
            managers: self.managers && !available.managers,
        }
    }

    const fn is_empty(self) -> bool {
        !self.detail && !self.managers
    }
}

pub struct Conductor {
    client: GitLabClient,
    discovery_mode: RunnerDiscoveryMode,
    runner_targets: Vec<RunnerTarget>,
    max_enrichment_requests: usize,
    pub demo_mode: bool,
}

#[derive(Clone, Serialize)]
pub struct QueryOutcome {
    pub runners: Vec<Runner>,
    pub metrics: LiveQueryMetrics,
    /// True when AllRunners mode fell back from /runners/all to /runners due to 403.
    pub all_runners_fell_back: bool,
}

pub struct EnrichmentProgress {
    pub runner: Runner,
    pub metrics: LiveQueryMetrics,
}

/// Returns true when an anyhow error wraps a reqwest 403 Forbidden response.
fn is_forbidden(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<reqwest::Error>()
            .and_then(|e| e.status())
            .is_some_and(|s| s == reqwest::StatusCode::FORBIDDEN)
    })
}

impl Conductor {
    #[allow(dead_code)]
    pub fn new(client: GitLabClient, runner_targets: Vec<RunnerTarget>) -> Self {
        Self::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            runner_targets,
        )
    }

    pub fn new_with_mode(
        client: GitLabClient,
        discovery_mode: RunnerDiscoveryMode,
        runner_targets: Vec<RunnerTarget>,
    ) -> Self {
        Self::new_with_mode_and_enrichment_limit(
            client,
            discovery_mode,
            runner_targets,
            DEFAULT_MAX_ENRICHMENT_REQUESTS,
        )
    }

    pub fn new_with_mode_and_enrichment_limit(
        client: GitLabClient,
        discovery_mode: RunnerDiscoveryMode,
        runner_targets: Vec<RunnerTarget>,
        max_enrichment_requests: usize,
    ) -> Self {
        assert!(
            (MIN_MAX_ENRICHMENT_REQUESTS..=MAX_MAX_ENRICHMENT_REQUESTS)
                .contains(&max_enrichment_requests),
            "maximum enrichment requests must be within the validated config range"
        );
        Self {
            client,
            discovery_mode,
            runner_targets,
            max_enrichment_requests,
            demo_mode: false,
        }
    }

    pub async fn validate_token(&self) -> Result<()> {
        if self.demo_mode {
            return Ok(());
        }
        self.client.validate_token().await
    }

    pub fn discovery_mode(&self) -> RunnerDiscoveryMode {
        self.discovery_mode
    }

    pub fn client(&self) -> &GitLabClient {
        &self.client
    }

    #[allow(dead_code)]
    pub async fn fetch_runners(&self, filters: RunnerFilters) -> Result<Vec<Runner>> {
        Ok(self.fetch_runners_with_metrics(filters).await?.runners)
    }

    pub async fn fetch_runners_with_metrics(&self, filters: RunnerFilters) -> Result<QueryOutcome> {
        self.fetch_runners_with_profile_and_metrics(filters, QueryProfile::Full)
            .await
    }

    pub async fn fetch_runners_with_profile_and_metrics(
        &self,
        filters: RunnerFilters,
        profile: QueryProfile,
    ) -> Result<QueryOutcome> {
        self.execute_query(filters, profile, profile, |runners| runners)
            .await
    }

    async fn execute_query<F>(
        &self,
        filters: RunnerFilters,
        base_profile: QueryProfile,
        output_profile: QueryProfile,
        select_results: F,
    ) -> Result<QueryOutcome>
    where
        F: FnOnce(Vec<Runner>) -> Vec<Runner>,
    {
        let started_at = Utc::now();
        let started = Instant::now();
        let initial_profile = base_profile
            .enrichment()
            .union(EnrichmentProfile::for_filters(&filters));
        let (runners, mut request_counts, all_runners_fell_back) =
            self.discover_runners(&filters).await?;
        let runners = self
            .enrich_runners(runners, initial_profile, &mut request_counts)
            .await;

        // `tag_list` is enforced by every GitLab list endpoint. Avoid forcing a detail request
        // solely to repeat that filter locally; demo fixtures still need local tag filtering.
        let mut local_filters = filters;
        if !self.demo_mode {
            local_filters.tag_list = None;
        }
        let runners = apply_runner_filters(&runners, &local_filters, Utc::now());
        let runners = select_results(runners);
        let final_profile = output_profile.enrichment().missing_from(initial_profile);
        let runners = self
            .enrich_runners(runners, final_profile, &mut request_counts)
            .await;
        let finished_at = Utc::now();
        let metrics = LiveQueryMetrics::success(
            started_at,
            finished_at,
            started.elapsed().as_millis(),
            runners.len(),
            self.discovery_mode,
            request_counts,
        );
        Ok(QueryOutcome {
            runners,
            metrics,
            all_runners_fell_back,
        })
    }

    pub async fn fetch_runner_summaries_with_metrics(
        &self,
        filters: RunnerFilters,
    ) -> Result<QueryOutcome> {
        self.fetch_runners_with_profile_and_metrics(filters, QueryProfile::Summary)
            .await
    }

    /// Enriches summary rows and publishes a complete snapshot after each runner finishes.
    ///
    /// Snapshots retain every discovered runner so the caller can update visible fields without
    /// rows disappearing while client-side filters are still waiting on detail data. The returned
    /// outcome applies all client-side filters and contains the final request counts.
    pub async fn stream_runner_summary_enrichment<F, Fut>(
        &self,
        summaries: QueryOutcome,
        filters: RunnerFilters,
        profile: QueryProfile,
        mut publish: F,
    ) -> QueryOutcome
    where
        F: FnMut(EnrichmentProgress) -> Fut,
        Fut: Future<Output = bool>,
    {
        debug_assert_eq!(summaries.metrics.request_counts.detail_requests, 0);
        debug_assert_eq!(summaries.metrics.request_counts.manager_requests, 0);

        let started_at = summaries.metrics.started_at;
        let mut request_counts = summaries.metrics.request_counts;
        let required_profile = profile
            .enrichment()
            .union(EnrichmentProfile::for_filters(&filters));
        let all_runners_fell_back = summaries.all_runners_fell_back;
        let mut runners = summaries.runners;

        if !required_profile.is_empty() && !runners.is_empty() && !self.demo_mode {
            let enrichment_permits = Arc::new(Semaphore::new(self.max_enrichment_requests));
            let runners_to_enrich = runners.clone();
            let enrichments = stream::iter(runners_to_enrich.into_iter().enumerate().map(
                |(index, runner)| {
                    let client = self.client.clone();
                    let enrichment_permits = Arc::clone(&enrichment_permits);
                    async move {
                        let runner = Self::enrich_runner(
                            client,
                            enrichment_permits,
                            runner,
                            required_profile,
                        )
                        .await;
                        (index, runner)
                    }
                },
            ))
            .buffer_unordered(self.max_enrichment_requests);
            tokio::pin!(enrichments);

            while let Some((index, runner)) = enrichments.next().await {
                runners[index] = runner.clone();
                if required_profile.detail {
                    request_counts.detail_requests += 1;
                }
                if required_profile.managers {
                    request_counts.manager_requests += 1;
                }

                let progress = EnrichmentProgress {
                    runner,
                    metrics: self.success_metrics(
                        started_at,
                        runners.len(),
                        request_counts.clone(),
                    ),
                };
                if !publish(progress).await {
                    break;
                }
            }
        }

        let mut local_filters = filters;
        if !self.demo_mode {
            local_filters.tag_list = None;
        }
        let runners = apply_runner_filters(&runners, &local_filters, Utc::now());
        let metrics = self.success_metrics(started_at, runners.len(), request_counts);

        QueryOutcome {
            runners,
            metrics,
            all_runners_fell_back,
        }
    }

    fn success_metrics(
        &self,
        started_at: DateTime<Utc>,
        result_count: usize,
        request_counts: QueryRequestCounts,
    ) -> LiveQueryMetrics {
        let finished_at = Utc::now();
        let duration_millis = finished_at
            .signed_duration_since(started_at)
            .num_milliseconds()
            .max(0) as u128;
        LiveQueryMetrics::success(
            started_at,
            finished_at,
            duration_millis,
            result_count,
            self.discovery_mode,
            request_counts,
        )
    }

    async fn discover_runners(
        &self,
        filters: &RunnerFilters,
    ) -> Result<(Vec<Runner>, QueryRequestCounts, bool)> {
        if self.demo_mode {
            return Ok((
                crate::fixtures::demo_runners(),
                QueryRequestCounts::default(),
                false,
            ));
        }

        let mut runner_map = BTreeMap::new();
        let per_page = 100;
        let mut request_counts = QueryRequestCounts::default();
        let mut all_runners_fell_back = false;

        match self.discovery_mode {
            RunnerDiscoveryMode::AllRunners => {
                // Try /runners/all (admin/auditor only). Fall back silently to /runners on 403
                // so that non-admin tokens still work without the user changing settings.
                request_counts.list_requests += 1;
                let first = self.client.fetch_all_runners(filters, 1, per_page).await;
                let use_admin = match &first {
                    Ok(_) => true,
                    Err(e) => !is_forbidden(e),
                };

                if use_admin {
                    let first_page = first?;
                    let mut next_page = first_page.next_page(1, per_page);
                    for runner in first_page.runners {
                        runner_map.entry(runner.id).or_insert(runner);
                    }

                    while let Some(page) = next_page {
                        request_counts.list_requests += 1;
                        let runner_page = self
                            .client
                            .fetch_all_runners(filters, page, per_page)
                            .await?;
                        if runner_page.runners.is_empty() {
                            break;
                        }
                        next_page = runner_page.next_page(page, per_page);
                        for runner in runner_page.runners {
                            runner_map.entry(runner.id).or_insert(runner);
                        }
                    }
                } else {
                    // 403 from /runners/all — fall back to /runners
                    all_runners_fell_back = true;
                    let mut page = 1;
                    loop {
                        request_counts.list_requests += 1;
                        let runner_page = self
                            .client
                            .fetch_available_runners(filters, page, per_page)
                            .await?;
                        if runner_page.runners.is_empty() {
                            break;
                        }
                        let next_page = runner_page.next_page(page, per_page);
                        for runner in runner_page.runners {
                            runner_map.entry(runner.id).or_insert(runner);
                        }
                        let Some(next_page) = next_page else {
                            break;
                        };
                        page = next_page;
                    }
                }
            }
            RunnerDiscoveryMode::VisibleRunners => {
                let mut page = 1;

                loop {
                    request_counts.list_requests += 1;
                    let runner_page = self
                        .client
                        .fetch_available_runners(filters, page, per_page)
                        .await?;
                    if runner_page.runners.is_empty() {
                        break;
                    }

                    let next_page = runner_page.next_page(page, per_page);
                    for runner in runner_page.runners {
                        runner_map.entry(runner.id).or_insert(runner);
                    }

                    let Some(next_page) = next_page else {
                        break;
                    };
                    page = next_page;
                }
            }
            RunnerDiscoveryMode::ConfiguredTargets => {
                let client = self.client.clone();
                let target_filters = filters.clone();
                let target_fetches = self.runner_targets.iter().cloned().map(move |target| {
                    Self::fetch_configured_target(
                        client.clone(),
                        target,
                        target_filters.clone(),
                        per_page,
                    )
                });
                let target_results =
                    collect_bounded_in_input_order(target_fetches, CONFIGURED_TARGET_CONCURRENCY)
                        .await?;
                request_counts.list_requests +=
                    merge_configured_target_results(target_results, &mut runner_map);
            }
        }

        Ok((
            runner_map.into_values().collect(),
            request_counts,
            all_runners_fell_back,
        ))
    }
    async fn enrich_runners(
        &self,
        runners: Vec<Runner>,
        profile: EnrichmentProfile,
        request_counts: &mut QueryRequestCounts,
    ) -> Vec<Runner> {
        if profile.is_empty() || runners.is_empty() || self.demo_mode {
            return runners;
        }

        if profile.detail {
            request_counts.detail_requests += runners.len();
        }
        if profile.managers {
            request_counts.manager_requests += runners.len();
        }

        let enrichment_permits = Arc::new(Semaphore::new(self.max_enrichment_requests));
        stream::iter(runners.into_iter().map(|runner| {
            let client = self.client.clone();
            let enrichment_permits = Arc::clone(&enrichment_permits);
            Self::enrich_runner(client, enrichment_permits, runner, profile)
        }))
        .buffer_unordered(self.max_enrichment_requests)
        .collect()
        .await
    }

    async fn enrich_runner(
        client: GitLabClient,
        enrichment_permits: Arc<Semaphore>,
        runner: Runner,
        profile: EnrichmentProfile,
    ) -> Runner {
        let runner_id = runner.id;

        let (detail_res, managers_res) = tokio::join!(
            async {
                if profile.detail {
                    Some(
                        with_request_permit(
                            Arc::clone(&enrichment_permits),
                            client.fetch_runner_detail(runner_id),
                        )
                        .await,
                    )
                } else {
                    None
                }
            },
            async {
                if profile.managers {
                    Some(
                        with_request_permit(
                            Arc::clone(&enrichment_permits),
                            client.fetch_runner_managers(runner_id),
                        )
                        .await,
                    )
                } else {
                    None
                }
            }
        );

        let existing_managers = runner.managers.clone();
        let mut enriched = match detail_res {
            Some(Ok(detail)) => detail,
            Some(Err(error)) => {
                tracing::warn!(runner_id, error = %error, "Failed to fetch runner detail, using list data");
                runner
            }
            None => runner,
        };

        match managers_res {
            Some(Ok(managers)) => enriched.managers = managers,
            Some(Err(error)) => {
                tracing::warn!(runner_id, error = %error, "Failed to fetch runner managers, keeping existing manager data");
                enriched.managers = existing_managers;
            }
            None => enriched.managers = existing_managers,
        }

        enriched
    }

    async fn fetch_configured_target(
        client: GitLabClient,
        target: RunnerTarget,
        filters: RunnerFilters,
        per_page: u32,
    ) -> Result<TargetFetchResult> {
        let mut page = 1;
        let mut list_requests = 0;
        let mut runners = Vec::new();

        loop {
            list_requests += 1;
            let runner_page = match target.kind {
                RunnerTargetKind::Group => {
                    client
                        .fetch_group_runners(&target.id, &filters, page, per_page)
                        .await?
                }
                RunnerTargetKind::Project => {
                    client
                        .fetch_project_runners(&target.id, &filters, page, per_page)
                        .await?
                }
            };
            if runner_page.runners.is_empty() {
                break;
            }

            let next_page = runner_page.next_page(page, per_page);
            runners.extend(runner_page.runners);

            let Some(next_page) = next_page else {
                break;
            };
            page = next_page;
        }

        Ok(TargetFetchResult {
            runners,
            list_requests,
        })
    }

    pub async fn list_offline_runners_with_metrics(
        &self,
        filters: RunnerFilters,
    ) -> Result<QueryOutcome> {
        self.execute_query(
            filters,
            QueryProfile::Managers,
            QueryProfile::Full,
            filter_offline_runners,
        )
        .await
    }

    pub async fn list_uncontacted_runners_with_metrics(
        &self,
        filters: RunnerFilters,
        threshold: ContactThreshold,
    ) -> Result<QueryOutcome> {
        self.execute_query(
            filters,
            QueryProfile::Managers,
            QueryProfile::Full,
            move |runners| filter_uncontacted_runners(runners, Utc::now(), threshold),
        )
        .await
    }

    /// Returns (online_count, total_count) - reserved for potential status aggregation
    #[allow(dead_code)]
    pub async fn check_runner_statuses(&self, filters: RunnerFilters) -> Result<(usize, usize)> {
        let runners = self
            .fetch_runners_with_profile_and_metrics(filters, QueryProfile::Managers)
            .await?
            .runners;
        let total = runners.len();
        let online = runners
            .iter()
            .filter(|r| r.managers.iter().any(|m| m.status == "online"))
            .count();
        Ok((online, total))
    }

    pub async fn list_runners_without_managers_with_metrics(
        &self,
        filters: RunnerFilters,
    ) -> Result<QueryOutcome> {
        self.execute_query(
            filters,
            QueryProfile::Managers,
            QueryProfile::Full,
            filter_runners_without_managers,
        )
        .await
    }

    pub async fn detect_rotating_runners_with_metrics(
        &self,
        filters: RunnerFilters,
    ) -> Result<QueryOutcome> {
        self.execute_query(
            filters,
            QueryProfile::Managers,
            QueryProfile::Full,
            filter_rotating_runners,
        )
        .await
    }
}

async fn with_request_permit<T, F>(semaphore: Arc<Semaphore>, future: F) -> T
where
    F: Future<Output = T>,
{
    let _permit = semaphore
        .acquire_owned()
        .await
        .expect("enrichment request semaphore is never closed");
    future.await
}

async fn collect_bounded_in_input_order<I, F, T>(futures: I, concurrency: usize) -> Result<Vec<T>>
where
    I: IntoIterator<Item = F>,
    F: Future<Output = Result<T>>,
{
    assert!(concurrency > 0, "target concurrency must be non-zero");

    let mut completed: Vec<(usize, T)> = stream::iter(
        futures
            .into_iter()
            .enumerate()
            .map(|(index, future)| async move { future.await.map(|value| (index, value)) }),
    )
    .buffer_unordered(concurrency)
    .try_collect()
    .await?;
    completed.sort_by_key(|(index, _)| *index);

    Ok(completed.into_iter().map(|(_, value)| value).collect())
}

fn merge_configured_target_results(
    target_results: Vec<TargetFetchResult>,
    runner_map: &mut BTreeMap<u64, Runner>,
) -> usize {
    let mut list_requests = 0;

    for target_result in target_results {
        list_requests += target_result.list_requests;
        for runner in target_result.runners {
            if let Some(existing) = runner_map.get_mut(&runner.id) {
                merge_runner(existing, runner);
            } else {
                runner_map.insert(runner.id, runner);
            }
        }
    }

    list_requests
}

fn is_runner_offline(runner: &Runner) -> bool {
    !runner.managers.is_empty() && !runner.managers.iter().any(|m| m.status == "online")
}

fn is_runner_uncontacted(runner: &Runner, now: DateTime<Utc>, threshold: ContactThreshold) -> bool {
    if runner.managers.is_empty() {
        return false;
    }

    // Runner is uncontacted if all managers are missing/stale/invalid relative to threshold.
    runner
        .managers
        .iter()
        .all(|m| threshold.is_contact_stale(parse_manager_contacted_at(m), now))
}

fn filter_offline_runners(runners: Vec<Runner>) -> Vec<Runner> {
    runners.into_iter().filter(is_runner_offline).collect()
}

fn filter_uncontacted_runners(
    runners: Vec<Runner>,
    now: DateTime<Utc>,
    threshold: ContactThreshold,
) -> Vec<Runner> {
    runners
        .into_iter()
        .filter(|r| is_runner_uncontacted(r, now, threshold))
        .collect()
}

fn filter_runners_without_managers(runners: Vec<Runner>) -> Vec<Runner> {
    runners
        .into_iter()
        .filter(|r| r.managers.is_empty())
        .collect()
}

fn filter_rotating_runners(runners: Vec<Runner>) -> Vec<Runner> {
    runners
        .into_iter()
        .filter(|r| r.managers.len() > 1)
        .collect()
}

fn merge_runner(existing: &mut Runner, incoming: Runner) {
    if existing.description.is_none() {
        existing.description = incoming.description.clone();
    }
    if existing.created_at.is_none() {
        existing.created_at = incoming.created_at.clone();
    }
    if existing.ip_address.is_none() {
        existing.ip_address = incoming.ip_address.clone();
    }
    if existing.version.is_none() {
        existing.version = incoming.version.clone();
    }
    if existing.revision.is_none() {
        existing.revision = incoming.revision.clone();
    }

    for tag in incoming.tag_list {
        if !existing.tag_list.contains(&tag) {
            existing.tag_list.push(tag);
        }
    }

    for manager in incoming.managers {
        if !existing
            .managers
            .iter()
            .any(|current| current.id == manager.id)
        {
            existing.managers.push(manager);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{RunnerTarget, RunnerTargetKind};
    use crate::models::manager::RunnerManager;
    use mockito::{Matcher, Server};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    };
    use tokio::sync::Semaphore;

    fn list_response_body(id: u64, status: &str) -> String {
        format!(
            r#"{{
                "id": {},
                "runner_type": "group_type",
                "active": true,
                "paused": false,
                "description": "Runner {}",
                "ip_address": "",
                "is_shared": false,
                "status": "{}",
                "name": null,
                "online": {}
            }}"#,
            id,
            id,
            status,
            status == "online"
        )
    }

    fn detail_response_body(id: u64, status: &str, tags: &[&str]) -> String {
        detail_response_body_with_version(id, status, tags, "17.5.0")
    }

    fn detail_response_body_with_version(
        id: u64,
        status: &str,
        tags: &[&str],
        version: &str,
    ) -> String {
        let tags_json: Vec<String> = tags.iter().map(|t| format!("\"{}\"", t)).collect();
        format!(
            r#"{{
                "id": {},
                "runner_type": "group_type",
                "active": true,
                "paused": false,
                "description": "Runner {}",
                "ip_address": "",
                "is_shared": false,
                "status": "{}",
                "version": "{}",
                "revision": "abc123",
                "tag_list": [{}]
            }}"#,
            id,
            id,
            status,
            version,
            tags_json.join(", ")
        )
    }

    fn manager_response_body(id: u64, runner_id: u64, status: &str) -> String {
        format!(
            r#"{{
                "id": {},
                "system_id": "host-{}",
                "created_at": "2024-01-15T10:30:00.000Z",
                "contacted_at": "2024-01-20T14:22:00.000Z",
                "ip_address": "10.0.1.1",
                "status": "{}",
                "version": "17.5.0",
                "revision": "abc123"
            }}"#,
            id, runner_id, status
        )
    }

    fn manager_response_body_with_contacted_at(
        id: u64,
        runner_id: u64,
        status: &str,
        contacted_at: Option<&str>,
    ) -> String {
        let contacted_at_json = match contacted_at {
            Some(value) => format!("\"{}\"", value),
            None => "null".to_string(),
        };

        format!(
            r#"{{
                "id": {},
                "system_id": "host-{}",
                "created_at": "2024-01-15T10:30:00.000Z",
                "contacted_at": {},
                "ip_address": "10.0.1.1",
                "status": "{}",
                "version": "17.5.0",
                "revision": "abc123"
            }}"#,
            id, runner_id, contacted_at_json, status
        )
    }

    type RunnerSpec<'a> = (u64, &'a str, &'a [&'a str], &'a [(u64, &'a str)]);

    fn group_target(id: &str) -> RunnerTarget {
        RunnerTarget {
            kind: RunnerTargetKind::Group,
            id: id.to_string(),
            label: None,
        }
    }

    fn project_target(id: &str) -> RunnerTarget {
        RunnerTarget {
            kind: RunnerTargetKind::Project,
            id: id.to_string(),
            label: None,
        }
    }

    fn repeated_list_body(id: u64, count: usize) -> String {
        let runner = list_response_body(id, "online");
        format!("[{}]", vec![runner; count].join(","))
    }

    async fn setup_simple_enrichment_mocks(
        server: &mut Server,
        runner_ids: &[u64],
    ) -> Vec<mockito::Mock> {
        let mut mocks = Vec::new();
        for runner_id in runner_ids {
            mocks.push(
                server
                    .mock("GET", format!("/api/v4/runners/{runner_id}").as_str())
                    .with_status(200)
                    .with_body(detail_response_body(*runner_id, "online", &[]))
                    .create_async()
                    .await,
            );
            mocks.push(
                server
                    .mock(
                        "GET",
                        format!("/api/v4/runners/{runner_id}/managers").as_str(),
                    )
                    .with_status(200)
                    .with_body("[]")
                    .create_async()
                    .await,
            );
        }
        mocks
    }

    async fn wait_for_counter(counter: &AtomicUsize, expected: usize) {
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while counter.load(Ordering::SeqCst) < expected {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("bounded fetches should start promptly");
    }

    async fn setup_runner_mocks(
        server: &mut Server,
        runners: &[RunnerSpec<'_>],
    ) -> Vec<mockito::Mock> {
        let mut mocks = Vec::new();

        // List endpoint
        let list_bodies: Vec<String> = runners
            .iter()
            .map(|(id, status, _, _)| list_response_body(*id, status))
            .collect();
        let list_body = format!("[{}]", list_bodies.join(","));

        mocks.push(
            server
                .mock("GET", "/api/v4/groups/123/runners")
                .match_query(Matcher::AllOf(vec![
                    Matcher::UrlEncoded("per_page".into(), "100".into()),
                    Matcher::UrlEncoded("page".into(), "1".into()),
                ]))
                .with_status(200)
                .with_body(list_body)
                .create_async()
                .await,
        );

        // Detail + manager endpoints per runner
        for (id, status, tags, managers) in runners {
            mocks.push(
                server
                    .mock("GET", format!("/api/v4/runners/{}", id).as_str())
                    .with_status(200)
                    .with_body(detail_response_body(*id, status, tags))
                    .expect_at_most(1)
                    .create_async()
                    .await,
            );

            let manager_bodies: Vec<String> = managers
                .iter()
                .map(|(mid, mstatus)| manager_response_body(*mid, *id, mstatus))
                .collect();
            let managers_body = format!("[{}]", manager_bodies.join(","));
            mocks.push(
                server
                    .mock("GET", format!("/api/v4/runners/{}/managers", id).as_str())
                    .with_status(200)
                    .with_body(managers_body)
                    .create_async()
                    .await,
            );
        }

        mocks
    }

    #[test]
    fn test_new() {
        let client =
            GitLabClient::new("https://example.com".to_string(), "token".to_string()).unwrap();
        let _conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123")],
        );
    }

    #[tokio::test]
    async fn summary_status_filter_uses_only_list_request() {
        let mut server = Server::new_async().await;
        let list = server
            .mock("GET", "/api/v4/groups/123/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
                Matcher::UrlEncoded("status".into(), "online".into()),
            ]))
            .with_status(200)
            .with_body(format!("[{}]", list_response_body(1, "online")))
            .create_async()
            .await;
        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123")],
        );

        let outcome = conductor
            .fetch_runners_with_profile_and_metrics(
                RunnerFilters {
                    status: Some("online".to_string()),
                    ..RunnerFilters::default()
                },
                QueryProfile::Summary,
            )
            .await
            .unwrap();

        assert_eq!(outcome.runners.len(), 1);
        assert_eq!(outcome.metrics.request_counts.list_requests, 1);
        assert_eq!(outcome.metrics.request_counts.detail_requests, 0);
        assert_eq!(outcome.metrics.request_counts.manager_requests, 0);
        list.assert_async().await;
    }

    #[tokio::test]
    async fn version_filter_requests_detail_but_not_managers() {
        let mut server = Server::new_async().await;
        let list = server
            .mock("GET", "/api/v4/groups/123/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .with_status(200)
            .with_body(format!("[{}]", list_response_body(1, "online")))
            .create_async()
            .await;
        let detail = server
            .mock("GET", "/api/v4/runners/1")
            .with_status(200)
            .with_body(detail_response_body_with_version(
                1,
                "online",
                &[],
                "17.5.0",
            ))
            .create_async()
            .await;
        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123")],
        );

        let outcome = conductor
            .fetch_runners_with_profile_and_metrics(
                RunnerFilters {
                    version_prefix: Some("17.5".to_string()),
                    ..RunnerFilters::default()
                },
                QueryProfile::Summary,
            )
            .await
            .unwrap();

        assert_eq!(outcome.runners.len(), 1);
        assert_eq!(outcome.metrics.request_counts.detail_requests, 1);
        assert_eq!(outcome.metrics.request_counts.manager_requests, 0);
        list.assert_async().await;
        detail.assert_async().await;
    }

    #[tokio::test]
    async fn manager_contact_profile_skips_detail_request() {
        let mut server = Server::new_async().await;
        let list = server
            .mock("GET", "/api/v4/groups/123/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .with_status(200)
            .with_body(format!("[{}]", list_response_body(1, "online")))
            .create_async()
            .await;
        let managers = server
            .mock("GET", "/api/v4/runners/1/managers")
            .with_status(200)
            .with_body(format!(
                "[{}]",
                manager_response_body_with_contacted_at(
                    10,
                    1,
                    "online",
                    Some("2024-01-20T14:22:00.000Z"),
                )
            ))
            .create_async()
            .await;
        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123")],
        );

        let outcome = conductor
            .fetch_runners_with_profile_and_metrics(
                RunnerFilters::default(),
                QueryProfile::Managers,
            )
            .await
            .unwrap();

        assert_eq!(outcome.runners[0].managers.len(), 1);
        assert_eq!(outcome.metrics.request_counts.detail_requests, 0);
        assert_eq!(outcome.metrics.request_counts.manager_requests, 1);
        list.assert_async().await;
        managers.assert_async().await;
    }

    #[tokio::test]
    async fn combined_filters_deduplicate_detail_and_manager_calls() {
        let mut server = Server::new_async().await;
        let list = server
            .mock("GET", "/api/v4/groups/123/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .with_status(200)
            .with_body(format!("[{}]", list_response_body(1, "online")))
            .create_async()
            .await;
        let detail = server
            .mock("GET", "/api/v4/runners/1")
            .with_status(200)
            .with_body(detail_response_body(1, "online", &["prod"]))
            .expect(1)
            .create_async()
            .await;
        let managers = server
            .mock("GET", "/api/v4/runners/1/managers")
            .with_status(200)
            .with_body(format!("[{}]", manager_response_body(10, 1, "online")))
            .expect(1)
            .create_async()
            .await;
        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123")],
        );

        let outcome = conductor
            .fetch_runners_with_profile_and_metrics(
                RunnerFilters {
                    popup_tags: Some(vec!["prod".to_string()]),
                    selected_versions: Some(vec!["17.5.0".to_string()]),
                    ..RunnerFilters::default()
                },
                QueryProfile::Full,
            )
            .await
            .unwrap();

        assert_eq!(outcome.runners.len(), 1);
        assert_eq!(outcome.metrics.request_counts.detail_requests, 1);
        assert_eq!(outcome.metrics.request_counts.manager_requests, 1);
        list.assert_async().await;
        detail.assert_async().await;
        managers.assert_async().await;
    }

    #[tokio::test]
    async fn full_detail_profile_fetches_both_enrichments() {
        let mut server = Server::new_async().await;
        let list = server
            .mock("GET", "/api/v4/groups/123/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .with_status(200)
            .with_body(format!("[{}]", list_response_body(1, "online")))
            .create_async()
            .await;
        let enrichments = setup_simple_enrichment_mocks(&mut server, &[1]).await;
        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123")],
        );

        let outcome = conductor
            .fetch_runners_with_profile_and_metrics(RunnerFilters::default(), QueryProfile::Full)
            .await
            .unwrap();

        assert_eq!(outcome.metrics.request_counts.detail_requests, 1);
        assert_eq!(outcome.metrics.request_counts.manager_requests, 1);
        list.assert_async().await;
        for enrichment in enrichments {
            enrichment.assert_async().await;
        }
    }

    #[tokio::test]
    async fn thousand_runner_summary_has_zero_enrichment_requests() {
        let mut server = Server::new_async().await;
        let body = format!(
            "[{}]",
            (1..=1000)
                .map(|id| list_response_body(id, "online"))
                .collect::<Vec<_>>()
                .join(",")
        );
        let list = server
            .mock("GET", "/api/v4/groups/123/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .with_status(200)
            .with_header("x-next-page", "")
            .with_body(body)
            .create_async()
            .await;
        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123")],
        );

        let outcome = conductor
            .fetch_runners_with_profile_and_metrics(RunnerFilters::default(), QueryProfile::Summary)
            .await
            .unwrap();

        assert_eq!(outcome.runners.len(), 1000);
        assert_eq!(outcome.metrics.request_counts.list_requests, 1);
        assert_eq!(outcome.metrics.request_counts.detail_requests, 0);
        assert_eq!(outcome.metrics.request_counts.manager_requests, 0);
        list.assert_async().await;
    }

    #[tokio::test]
    async fn bounded_target_collector_runs_independent_targets_concurrently() {
        let started = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new(Semaphore::new(0));
        let task_started = Arc::clone(&started);
        let task_gate = Arc::clone(&gate);
        let futures = (0..2).map(move |index| {
            let started = Arc::clone(&task_started);
            let gate = Arc::clone(&task_gate);
            async move {
                started.fetch_add(1, Ordering::SeqCst);
                let _permit = gate.acquire().await.expect("test gate should remain open");
                Ok::<_, anyhow::Error>(index)
            }
        });
        let collector = tokio::spawn(collect_bounded_in_input_order(futures, 2));

        wait_for_counter(&started, 2).await;
        assert_eq!(started.load(Ordering::SeqCst), 2);
        gate.add_permits(2);

        assert_eq!(collector.await.unwrap().unwrap(), vec![0, 1]);
    }

    #[tokio::test]
    async fn configured_target_concurrency_never_exceeds_named_limit() {
        let total = CONFIGURED_TARGET_CONCURRENCY + 2;
        let started = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new(Semaphore::new(0));
        let task_started = Arc::clone(&started);
        let task_gate = Arc::clone(&gate);
        let futures = (0..total).map(move |index| {
            let started = Arc::clone(&task_started);
            let gate = Arc::clone(&task_gate);
            async move {
                started.fetch_add(1, Ordering::SeqCst);
                let _permit = gate.acquire().await.expect("test gate should remain open");
                Ok::<_, anyhow::Error>(index)
            }
        });
        let collector = tokio::spawn(collect_bounded_in_input_order(
            futures,
            CONFIGURED_TARGET_CONCURRENCY,
        ));

        wait_for_counter(&started, CONFIGURED_TARGET_CONCURRENCY).await;
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert_eq!(
            started.load(Ordering::SeqCst),
            CONFIGURED_TARGET_CONCURRENCY
        );
        gate.add_permits(total);

        assert_eq!(
            collector.await.unwrap().unwrap(),
            (0..total).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn enrichment_detail_and_manager_requests_respect_request_budget() {
        const REQUEST_BUDGET: usize = 3;
        let semaphore = Arc::new(Semaphore::new(REQUEST_BUDGET));
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));

        let runner_enrichments = (0..8).map(|_| {
            let semaphore = Arc::clone(&semaphore);
            let active = Arc::clone(&active);
            let maximum = Arc::clone(&maximum);
            async move {
                let request = || {
                    let active = Arc::clone(&active);
                    let maximum = Arc::clone(&maximum);
                    async move {
                        let now_active = active.fetch_add(1, Ordering::SeqCst) + 1;
                        maximum.fetch_max(now_active, Ordering::SeqCst);
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                        active.fetch_sub(1, Ordering::SeqCst);
                    }
                };

                tokio::join!(
                    with_request_permit(Arc::clone(&semaphore), request()),
                    with_request_permit(Arc::clone(&semaphore), request())
                );
            }
        });

        stream::iter(runner_enrichments)
            .buffer_unordered(8)
            .collect::<Vec<_>>()
            .await;

        assert_eq!(active.load(Ordering::SeqCst), 0);
        assert_eq!(maximum.load(Ordering::SeqCst), REQUEST_BUDGET);
    }

    #[tokio::test]
    async fn completion_order_does_not_change_duplicate_merge_or_output_order() {
        let mut first_duplicate = test_runner(1, &[("online", None)]);
        first_duplicate.description = Some("first target".to_string());
        first_duplicate.tag_list = vec!["first".to_string()];
        let mut second_duplicate = test_runner(1, &[("online", None)]);
        second_duplicate.description = Some("second target".to_string());
        second_duplicate.tag_list = vec!["second".to_string()];
        let runner_two = test_runner(2, &[]);
        let completion_order = Arc::new(Mutex::new(Vec::new()));
        let inputs = vec![
            (
                0,
                std::time::Duration::from_millis(30),
                TargetFetchResult {
                    runners: vec![runner_two, first_duplicate],
                    list_requests: 2,
                },
            ),
            (
                1,
                std::time::Duration::ZERO,
                TargetFetchResult {
                    runners: vec![second_duplicate],
                    list_requests: 1,
                },
            ),
        ];
        let futures = inputs.into_iter().map(|(index, delay, result)| {
            let completion_order = Arc::clone(&completion_order);
            async move {
                tokio::time::sleep(delay).await;
                completion_order.lock().unwrap().push(index);
                Ok::<_, anyhow::Error>(result)
            }
        });

        let results = collect_bounded_in_input_order(futures, 2).await.unwrap();
        assert_eq!(*completion_order.lock().unwrap(), vec![1, 0]);

        let mut runner_map = BTreeMap::new();
        let list_requests = merge_configured_target_results(results, &mut runner_map);
        assert_eq!(list_requests, 3);
        assert_eq!(runner_map.keys().copied().collect::<Vec<_>>(), vec![1, 2]);
        let duplicate = runner_map.get(&1).unwrap();
        assert_eq!(duplicate.description.as_deref(), Some("first target"));
        assert_eq!(duplicate.tag_list, vec!["first", "second"]);
    }

    #[tokio::test]
    async fn bounded_target_collector_returns_first_error_without_partial_results() {
        let inputs = [(0, false), (1, true), (2, false)];
        let futures = inputs.into_iter().map(|(index, should_fail)| async move {
            if should_fail {
                anyhow::bail!("target {index} failed");
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            Ok(index)
        });

        let error = tokio::time::timeout(
            std::time::Duration::from_millis(250),
            collect_bounded_in_input_order(futures, 2),
        )
        .await
        .expect("target errors should propagate without waiting for slower targets")
        .unwrap_err();

        assert!(error.to_string().contains("target 1 failed"));
    }

    #[tokio::test]
    async fn test_fetch_runners_merges_and_dedupes_across_targets() {
        let mut server = Server::new_async().await;

        let group_list = server
            .mock("GET", "/api/v4/groups/123/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .with_status(200)
            .with_body(format!(
                "[{},{}]",
                list_response_body(1, "online"),
                list_response_body(2, "offline")
            ))
            .create_async()
            .await;

        let project_list = server
            .mock("GET", "/api/v4/projects/my-org%2Fapp/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .with_status(200)
            .with_body(format!(
                "[{},{}]",
                list_response_body(1, "online"),
                list_response_body(3, "online")
            ))
            .create_async()
            .await;

        let detail_1 = server
            .mock("GET", "/api/v4/runners/1")
            .with_status(200)
            .with_body(detail_response_body(1, "online", &["prod"]))
            .expect(1)
            .create_async()
            .await;
        let detail_2 = server
            .mock("GET", "/api/v4/runners/2")
            .with_status(200)
            .with_body(detail_response_body(2, "offline", &["staging"]))
            .create_async()
            .await;
        let detail_3 = server
            .mock("GET", "/api/v4/runners/3")
            .with_status(200)
            .with_body(detail_response_body(3, "online", &["qa"]))
            .create_async()
            .await;

        let managers_1 = server
            .mock("GET", "/api/v4/runners/1/managers")
            .with_status(200)
            .with_body(format!("[{}]", manager_response_body(10, 1, "online")))
            .expect(1)
            .create_async()
            .await;
        let managers_2 = server
            .mock("GET", "/api/v4/runners/2/managers")
            .with_status(200)
            .with_body(format!("[{}]", manager_response_body(20, 2, "offline")))
            .create_async()
            .await;
        let managers_3 = server
            .mock("GET", "/api/v4/runners/3/managers")
            .with_status(200)
            .with_body(format!("[{}]", manager_response_body(30, 3, "online")))
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123"), project_target("my-org/app")],
        );

        let outcome = conductor
            .fetch_runners_with_metrics(RunnerFilters::default())
            .await
            .unwrap();
        let ids: Vec<u64> = outcome.runners.iter().map(|runner| runner.id).collect();

        assert_eq!(ids, vec![1, 2, 3]);
        assert_eq!(outcome.metrics.request_counts.list_requests, 2);

        group_list.assert_async().await;
        project_list.assert_async().await;
        detail_1.assert_async().await;
        detail_2.assert_async().await;
        detail_3.assert_async().await;
        managers_1.assert_async().await;
        managers_2.assert_async().await;
        managers_3.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_runner_summaries_merges_targets_without_enrichment() {
        let mut server = Server::new_async().await;

        let group_list = server
            .mock("GET", "/api/v4/groups/123/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .with_status(200)
            .with_body(format!(
                "[{},{}]",
                list_response_body(1, "online"),
                list_response_body(2, "offline")
            ))
            .create_async()
            .await;

        let project_list = server
            .mock("GET", "/api/v4/projects/my-org%2Fapp/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .with_status(200)
            .with_body(format!(
                "[{},{}]",
                list_response_body(1, "online"),
                list_response_body(3, "online")
            ))
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123"), project_target("my-org/app")],
        );

        let outcome = conductor
            .fetch_runner_summaries_with_metrics(RunnerFilters::default())
            .await
            .unwrap();
        let ids: Vec<u64> = outcome.runners.iter().map(|runner| runner.id).collect();

        assert_eq!(ids, vec![1, 2, 3]);
        assert_eq!(outcome.metrics.request_counts.list_requests, 2);
        assert_eq!(outcome.metrics.request_counts.detail_requests, 0);
        assert_eq!(outcome.metrics.request_counts.manager_requests, 0);

        group_list.assert_async().await;
        project_list.assert_async().await;
    }

    #[tokio::test]
    async fn configured_target_failure_discards_partial_success() {
        let mut server = Server::new_async().await;
        let successful_target = server
            .mock("GET", "/api/v4/groups/123/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .with_status(200)
            .with_header("x-next-page", "")
            .with_body(format!("[{}]", list_response_body(1, "online")))
            .create_async()
            .await;
        let failing_target = server
            .mock("GET", "/api/v4/projects/456/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .with_status(500)
            .with_body(r#"{"message":"target failed"}"#)
            .create_async()
            .await;
        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123"), project_target("456")],
        );

        let error = conductor
            .fetch_runners_with_metrics(RunnerFilters::default())
            .await
            .err()
            .expect("one failed target should fail the entire query");

        assert!(format!("{error:#}").contains("500"));
        successful_target.assert_async().await;
        failing_target.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_runners_applies_version_filter_client_side() {
        let mut server = Server::new_async().await;

        let list = server
            .mock("GET", "/api/v4/groups/123/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .with_status(200)
            .with_body(format!(
                "[{},{}]",
                list_response_body(1, "online"),
                list_response_body(2, "online")
            ))
            .create_async()
            .await;

        let detail_1 = server
            .mock("GET", "/api/v4/runners/1")
            .with_status(200)
            .with_body(detail_response_body_with_version(
                1,
                "online",
                &["prod"],
                "16.11.2",
            ))
            .create_async()
            .await;
        let detail_2 = server
            .mock("GET", "/api/v4/runners/2")
            .with_status(200)
            .with_body(detail_response_body_with_version(
                2,
                "online",
                &["prod"],
                "17.5.0",
            ))
            .create_async()
            .await;
        let managers_1 = server
            .mock("GET", "/api/v4/runners/1/managers")
            .with_status(200)
            .with_body("[]")
            .create_async()
            .await;
        let managers_2 = server
            .mock("GET", "/api/v4/runners/2/managers")
            .with_status(200)
            .with_body("[]")
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123")],
        );
        let filters = RunnerFilters {
            version_prefix: Some("16.11".to_string()),
            ..RunnerFilters::default()
        };

        let outcome = conductor.fetch_runners_with_metrics(filters).await.unwrap();

        assert_eq!(outcome.runners.len(), 1);
        assert_eq!(outcome.runners[0].id, 1);
        assert_eq!(outcome.metrics.result_count, 1);
        assert_eq!(outcome.metrics.request_counts.detail_requests, 2);

        list.assert_async().await;
        detail_1.assert_async().await;
        detail_2.assert_async().await;
        managers_1.assert_async().await;
        managers_2.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_runners_without_targets_uses_available_runner_endpoint() {
        let mut server = Server::new_async().await;

        let list_mock = server
            .mock("GET", "/api/v4/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .with_status(200)
            .with_body("[]")
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor =
            Conductor::new_with_mode(client, RunnerDiscoveryMode::VisibleRunners, Vec::new());

        let runners = conductor
            .fetch_runners(RunnerFilters::default())
            .await
            .unwrap();

        assert!(runners.is_empty());
        list_mock.assert_async().await;
    }

    #[tokio::test]
    async fn exact_full_terminal_page_uses_explicit_completion_without_probe() {
        let mut server = Server::new_async().await;
        let list = server
            .mock("GET", "/api/v4/groups/123/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .with_status(200)
            .with_header("x-next-page", "")
            .with_body(repeated_list_body(1, 100))
            .expect(1)
            .create_async()
            .await;
        let enrichment = setup_simple_enrichment_mocks(&mut server, &[1]).await;
        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123")],
        );

        let outcome = conductor
            .fetch_runners_with_metrics(RunnerFilters::default())
            .await
            .unwrap();

        assert_eq!(outcome.runners.len(), 1);
        assert_eq!(outcome.metrics.request_counts.list_requests, 1);
        list.assert_async().await;
        for mock in enrichment {
            mock.assert_async().await;
        }
    }

    #[tokio::test]
    async fn visible_runners_follow_explicit_link_next_page() {
        let mut server = Server::new_async().await;
        let next_link = format!(
            "<{}/api/v4/runners?per_page=100&page=2>; rel=\"next\"",
            server.url()
        );
        let first = server
            .mock("GET", "/api/v4/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .with_status(200)
            .with_header("link", next_link.as_str())
            .with_body(format!("[{}]", list_response_body(1, "online")))
            .create_async()
            .await;
        let second = server
            .mock("GET", "/api/v4/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "2".into()),
            ]))
            .with_status(200)
            .with_header("x-next-page", "")
            .with_body(format!("[{}]", list_response_body(2, "online")))
            .create_async()
            .await;
        let enrichment = setup_simple_enrichment_mocks(&mut server, &[1, 2]).await;
        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor =
            Conductor::new_with_mode(client, RunnerDiscoveryMode::VisibleRunners, Vec::new());

        let outcome = conductor
            .fetch_runners_with_metrics(RunnerFilters::default())
            .await
            .unwrap();
        let mut runner_ids = outcome
            .runners
            .iter()
            .map(|runner| runner.id)
            .collect::<Vec<_>>();
        runner_ids.sort_unstable();

        assert_eq!(runner_ids, vec![1, 2]);
        assert_eq!(outcome.metrics.request_counts.list_requests, 2);
        first.assert_async().await;
        second.assert_async().await;
        for mock in enrichment {
            mock.assert_async().await;
        }
    }

    #[tokio::test]
    async fn all_runners_without_headers_keep_length_based_fallback() {
        let mut server = Server::new_async().await;
        let first = server
            .mock("GET", "/api/v4/runners/all")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .with_status(200)
            .with_body(repeated_list_body(1, 100))
            .create_async()
            .await;
        let fallback_probe = server
            .mock("GET", "/api/v4/runners/all")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "2".into()),
            ]))
            .with_status(200)
            .with_body("[]")
            .create_async()
            .await;
        let enrichment = setup_simple_enrichment_mocks(&mut server, &[1]).await;
        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor =
            Conductor::new_with_mode(client, RunnerDiscoveryMode::AllRunners, Vec::new());

        let outcome = conductor
            .fetch_runners_with_metrics(RunnerFilters::default())
            .await
            .unwrap();

        assert_eq!(outcome.runners.len(), 1);
        assert_eq!(outcome.metrics.request_counts.list_requests, 2);
        first.assert_async().await;
        fallback_probe.assert_async().await;
        for mock in enrichment {
            mock.assert_async().await;
        }
    }

    #[tokio::test]
    async fn self_loop_pagination_header_stops_without_repeating_request() {
        let mut server = Server::new_async().await;
        let list = server
            .mock("GET", "/api/v4/groups/123/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .with_status(200)
            .with_header("x-next-page", "1")
            .with_body(repeated_list_body(1, 100))
            .expect(1)
            .create_async()
            .await;
        let enrichment = setup_simple_enrichment_mocks(&mut server, &[1]).await;
        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123")],
        );

        let outcome = conductor
            .fetch_runners_with_metrics(RunnerFilters::default())
            .await
            .unwrap();

        assert_eq!(outcome.runners.len(), 1);
        assert_eq!(outcome.metrics.request_counts.list_requests, 1);
        list.assert_async().await;
        for mock in enrichment {
            mock.assert_async().await;
        }
    }

    #[tokio::test]
    async fn test_check_runner_statuses() {
        let mut server = Server::new_async().await;
        let mocks = setup_runner_mocks(
            &mut server,
            &[
                // Runner 1: one online manager -> online
                (1, "online", &["prod"], &[(10, "online")]),
                // Runner 2: only offline managers -> offline
                (
                    2,
                    "offline",
                    &["staging"],
                    &[(20, "offline"), (21, "offline")],
                ),
                // Runner 3: multiple managers, one online -> online
                (3, "online", &["dev"], &[(30, "offline"), (31, "online")]),
                // Runner 4: no managers -> offline (no online manager)
                (4, "online", &["test"], &[]),
            ],
        )
        .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123")],
        );

        let (online, total) = conductor
            .check_runner_statuses(RunnerFilters::default())
            .await
            .unwrap();

        // Total 4 runners
        assert_eq!(total, 4);
        // Runners 1 and 3 are online, so 2 online runners
        assert_eq!(online, 2);

        for mock in &mocks {
            mock.assert_async().await;
        }
    }

    #[tokio::test]
    async fn test_enrichment_adds_tags_from_detail() {
        let mut server = Server::new_async().await;
        let mocks = setup_runner_mocks(
            &mut server,
            &[(1, "online", &["alm", "production"], &[(10, "online")])],
        )
        .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123")],
        );

        let runners = conductor
            .fetch_runners(RunnerFilters::default())
            .await
            .unwrap();

        assert_eq!(runners.len(), 1);
        assert_eq!(runners[0].tag_list, vec!["alm", "production"]);
        assert_eq!(runners[0].version, Some("17.5.0".to_string()));
        assert_eq!(runners[0].managers.len(), 1);

        for mock in &mocks {
            mock.assert_async().await;
        }
    }

    #[tokio::test]
    async fn test_enrichment_degrades_gracefully_on_detail_failure() {
        let mut server = Server::new_async().await;

        // List returns one runner
        let list_mock = server
            .mock("GET", "/api/v4/groups/123/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .with_status(200)
            .with_body(format!("[{}]", list_response_body(1, "online")))
            .create_async()
            .await;

        // Detail returns 500
        let detail_mock = server
            .mock("GET", "/api/v4/runners/1")
            .with_status(500)
            .with_body(r#"{"message":"Internal Server Error"}"#)
            .create_async()
            .await;

        // Managers still succeeds
        let managers_mock = server
            .mock("GET", "/api/v4/runners/1/managers")
            .with_status(200)
            .with_body("[]")
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123")],
        );

        let runners = conductor
            .fetch_runners(RunnerFilters::default())
            .await
            .unwrap();

        // Should still get the runner, just without enriched tags
        assert_eq!(runners.len(), 1);
        assert_eq!(runners[0].id, 1);
        assert!(runners[0].tag_list.is_empty());

        list_mock.assert_async().await;
        detail_mock.assert_async().await;
        managers_mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_list_offline_runners() {
        let mut server = Server::new_async().await;
        let mocks = setup_runner_mocks(
            &mut server,
            &[
                (1, "online", &["prod"], &[(10, "online")]),
                (2, "offline", &["staging"], &[(20, "offline")]),
                (3, "online", &["dev"], &[]),
            ],
        )
        .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123")],
        );

        let outcome = conductor
            .list_offline_runners_with_metrics(RunnerFilters::default())
            .await
            .unwrap();
        let offline = outcome.runners;

        // Only runner 2 has an offline manager
        assert_eq!(offline.len(), 1);
        assert_eq!(offline[0].id, 2);
        assert_eq!(outcome.metrics.request_counts.detail_requests, 1);
        assert_eq!(outcome.metrics.request_counts.manager_requests, 3);

        for mock in &mocks {
            mock.assert_async().await;
        }
    }

    #[tokio::test]
    async fn test_list_offline_runners_with_multiple_managers() {
        let mut server = Server::new_async().await;
        let mocks = setup_runner_mocks(
            &mut server,
            &[
                // Runner 1: two managers, one online + one offline → NOT offline
                (1, "online", &["prod"], &[(10, "online"), (11, "offline")]),
                // Runner 2: two managers, both offline → IS offline
                (
                    2,
                    "offline",
                    &["staging"],
                    &[(20, "offline"), (21, "offline")],
                ),
                // Runner 3: one manager, online → NOT offline
                (3, "online", &["dev"], &[(30, "online")]),
            ],
        )
        .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123")],
        );

        let offline = conductor
            .list_offline_runners_with_metrics(RunnerFilters::default())
            .await
            .unwrap()
            .runners;

        // Only runner 2 should be offline (all managers offline)
        assert_eq!(offline.len(), 1);
        assert_eq!(offline[0].id, 2);

        for mock in &mocks {
            mock.assert_async().await;
        }
    }

    #[tokio::test]
    async fn test_list_runners_without_managers() {
        let mut server = Server::new_async().await;
        let mocks = setup_runner_mocks(
            &mut server,
            &[
                (1, "online", &["prod"], &[(10, "online")]),
                (2, "online", &["staging"], &[]),
            ],
        )
        .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123")],
        );

        let outcome = conductor
            .list_runners_without_managers_with_metrics(RunnerFilters::default())
            .await
            .unwrap();
        let empty = outcome.runners;

        // Only runner 2 has no managers
        assert_eq!(empty.len(), 1);
        assert_eq!(empty[0].id, 2);
        assert_eq!(outcome.metrics.request_counts.detail_requests, 1);
        assert_eq!(outcome.metrics.request_counts.manager_requests, 2);

        for mock in &mocks {
            mock.assert_async().await;
        }
    }

    #[tokio::test]
    async fn test_detect_rotating_runners_finds_multi_manager() {
        let mut server = Server::new_async().await;
        let mocks = setup_runner_mocks(
            &mut server,
            &[
                // Runner 1: two managers (rotation in progress)
                (1, "online", &["prod"], &[(10, "online"), (11, "offline")]),
                // Runner 2: single manager (no rotation)
                (2, "online", &["staging"], &[(20, "online")]),
            ],
        )
        .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123")],
        );

        let outcome = conductor
            .detect_rotating_runners_with_metrics(RunnerFilters::default())
            .await
            .unwrap();
        let rotating = outcome.runners;

        assert_eq!(rotating.len(), 1);
        assert_eq!(rotating[0].id, 1);
        assert_eq!(rotating[0].managers.len(), 2);
        assert_eq!(outcome.metrics.request_counts.detail_requests, 1);
        assert_eq!(outcome.metrics.request_counts.manager_requests, 2);

        for mock in &mocks {
            mock.assert_async().await;
        }
    }

    #[tokio::test]
    async fn test_detect_rotating_runners_empty_when_no_rotation() {
        let mut server = Server::new_async().await;
        let mocks = setup_runner_mocks(
            &mut server,
            &[
                (1, "online", &["prod"], &[(10, "online")]),
                (2, "online", &["staging"], &[(20, "online")]),
            ],
        )
        .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123")],
        );

        let rotating = conductor
            .detect_rotating_runners_with_metrics(RunnerFilters::default())
            .await
            .unwrap()
            .runners;

        assert!(rotating.is_empty());

        for mock in &mocks {
            mock.assert_async().await;
        }
    }

    #[tokio::test]
    async fn test_detect_rotating_runners_excludes_no_managers() {
        let mut server = Server::new_async().await;
        let mocks = setup_runner_mocks(
            &mut server,
            &[
                (1, "online", &["prod"], &[]),
                (
                    2,
                    "online",
                    &["staging"],
                    &[(20, "offline"), (21, "online")],
                ),
            ],
        )
        .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123")],
        );

        let rotating = conductor
            .detect_rotating_runners_with_metrics(RunnerFilters::default())
            .await
            .unwrap()
            .runners;

        assert_eq!(rotating.len(), 1);
        assert_eq!(rotating[0].id, 2);

        for mock in &mocks {
            mock.assert_async().await;
        }
    }

    #[tokio::test]
    async fn test_detect_rotating_runners_three_managers() {
        let mut server = Server::new_async().await;
        let mocks = setup_runner_mocks(
            &mut server,
            &[(
                1,
                "online",
                &["prod"],
                &[(10, "offline"), (11, "stale"), (12, "online")],
            )],
        )
        .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123")],
        );

        let rotating = conductor
            .detect_rotating_runners_with_metrics(RunnerFilters::default())
            .await
            .unwrap()
            .runners;

        assert_eq!(rotating.len(), 1);
        assert_eq!(rotating[0].managers.len(), 3);

        for mock in &mocks {
            mock.assert_async().await;
        }
    }

    #[tokio::test]
    async fn test_list_uncontacted_runners_respects_threshold_and_all_manager_rule() {
        let mut server = Server::new_async().await;

        let stale_contact = (Utc::now() - chrono::Duration::seconds(120)).to_rfc3339();
        let recent_contact = (Utc::now() - chrono::Duration::seconds(10)).to_rfc3339();

        let list_mock = server
            .mock("GET", "/api/v4/groups/123/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .with_status(200)
            .with_body(format!(
                "[{},{}]",
                list_response_body(1, "online"),
                list_response_body(2, "online")
            ))
            .create_async()
            .await;

        let detail_1 = server
            .mock("GET", "/api/v4/runners/1")
            .with_status(200)
            .with_body(detail_response_body(1, "online", &["prod"]))
            .create_async()
            .await;
        let detail_2 = server
            .mock("GET", "/api/v4/runners/2")
            .with_status(200)
            .with_body(detail_response_body(2, "online", &["staging"]))
            .expect(0)
            .create_async()
            .await;

        let managers_1 = server
            .mock("GET", "/api/v4/runners/1/managers")
            .with_status(200)
            .with_body(format!(
                "[{},{}]",
                manager_response_body_with_contacted_at(10, 1, "offline", Some(&stale_contact)),
                manager_response_body_with_contacted_at(11, 1, "offline", Some(&stale_contact))
            ))
            .create_async()
            .await;
        let managers_2 = server
            .mock("GET", "/api/v4/runners/2/managers")
            .with_status(200)
            .with_body(format!(
                "[{},{}]",
                manager_response_body_with_contacted_at(20, 2, "offline", Some(&stale_contact)),
                manager_response_body_with_contacted_at(21, 2, "online", Some(&recent_contact))
            ))
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123")],
        );

        let outcome = conductor
            .list_uncontacted_runners_with_metrics(
                RunnerFilters::default(),
                ContactThreshold::OlderThanSecs(60),
            )
            .await
            .unwrap();
        let uncontacted = outcome.runners;

        assert_eq!(uncontacted.len(), 1);
        assert_eq!(uncontacted[0].id, 1);
        assert_eq!(outcome.metrics.request_counts.detail_requests, 1);
        assert_eq!(outcome.metrics.request_counts.manager_requests, 2);

        list_mock.assert_async().await;
        detail_1.assert_async().await;
        detail_2.assert_async().await;
        managers_1.assert_async().await;
        managers_2.assert_async().await;
    }

    #[tokio::test]
    async fn test_list_uncontacted_runners_treats_missing_or_invalid_timestamps_as_uncontacted() {
        let mut server = Server::new_async().await;

        let recent_contact = (Utc::now() - chrono::Duration::seconds(5)).to_rfc3339();

        let list_mock = server
            .mock("GET", "/api/v4/groups/123/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .with_status(200)
            .with_body(format!(
                "[{},{}]",
                list_response_body(3, "online"),
                list_response_body(4, "online")
            ))
            .create_async()
            .await;

        let detail_3 = server
            .mock("GET", "/api/v4/runners/3")
            .with_status(200)
            .with_body(detail_response_body(3, "online", &["qa"]))
            .create_async()
            .await;
        let detail_4 = server
            .mock("GET", "/api/v4/runners/4")
            .with_status(200)
            .with_body(detail_response_body(4, "online", &["qa"]))
            .expect(0)
            .create_async()
            .await;

        let managers_3 = server
            .mock("GET", "/api/v4/runners/3/managers")
            .with_status(200)
            .with_body(format!(
                "[{},{}]",
                manager_response_body_with_contacted_at(30, 3, "offline", None),
                manager_response_body_with_contacted_at(31, 3, "offline", Some("not-a-timestamp"))
            ))
            .create_async()
            .await;
        let managers_4 = server
            .mock("GET", "/api/v4/runners/4/managers")
            .with_status(200)
            .with_body(format!(
                "[{},{}]",
                manager_response_body_with_contacted_at(40, 4, "offline", Some(&recent_contact)),
                manager_response_body_with_contacted_at(41, 4, "offline", None)
            ))
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![group_target("123")],
        );

        let uncontacted = conductor
            .list_uncontacted_runners_with_metrics(
                RunnerFilters::default(),
                ContactThreshold::OlderThanSecs(60),
            )
            .await
            .unwrap()
            .runners;

        assert_eq!(uncontacted.len(), 1);
        assert_eq!(uncontacted[0].id, 3);

        list_mock.assert_async().await;
        detail_3.assert_async().await;
        detail_4.assert_async().await;
        managers_3.assert_async().await;
        managers_4.assert_async().await;
    }

    fn test_runner(id: u64, manager_specs: &[(&str, Option<&str>)]) -> Runner {
        let managers = manager_specs
            .iter()
            .enumerate()
            .map(|(idx, (status, contacted_at))| RunnerManager {
                id: id * 100 + idx as u64,
                system_id: format!("host-{}-{}", id, idx),
                created_at: "2024-01-15T10:30:00.000Z".to_string(),
                contacted_at: contacted_at.map(|c| c.to_string()),
                ip_address: Some("10.0.1.1".to_string()),
                status: (*status).to_string(),
                version: Some("17.5.0".to_string()),
                revision: Some("abc123".to_string()),
                platform: None,
                architecture: None,
            })
            .collect();

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
            tag_list: vec!["test".to_string()],
            managers,
            groups: vec![],
        }
    }

    #[test]
    fn test_filter_offline_runners_socket_free() {
        let runners = vec![
            test_runner(1, &[("online", Some("2024-01-20T14:22:00.000Z"))]),
            test_runner(2, &[("offline", Some("2024-01-20T14:22:00.000Z"))]),
            test_runner(
                3,
                &[
                    ("offline", Some("2024-01-20T14:22:00.000Z")),
                    ("stale", None),
                ],
            ),
            test_runner(4, &[]),
        ];

        let filtered = filter_offline_runners(runners);
        let ids: Vec<u64> = filtered.into_iter().map(|r| r.id).collect();

        assert_eq!(ids, vec![2, 3]);
    }

    #[test]
    fn test_filter_uncontacted_runners_socket_free() {
        let now = Utc::now();
        let stale = (now - chrono::Duration::seconds(120)).to_rfc3339();
        let recent = (now - chrono::Duration::seconds(10)).to_rfc3339();

        let runners = vec![
            // all stale -> uncontacted
            test_runner(1, &[("offline", Some(&stale)), ("offline", None)]),
            // one recent contact -> not uncontacted
            test_runner(2, &[("online", Some(&recent)), ("offline", Some(&stale))]),
            // empty managers -> not uncontacted
            test_runner(3, &[]),
            // invalid timestamp treated as stale -> uncontacted
            test_runner(4, &[("offline", Some("not-a-time"))]),
        ];

        let filtered =
            filter_uncontacted_runners(runners, now, ContactThreshold::OlderThanSecs(60));
        let ids: Vec<u64> = filtered.into_iter().map(|r| r.id).collect();

        assert_eq!(ids, vec![1, 4]);
    }

    #[test]
    fn test_filter_uncontacted_runners_cutoff_requires_contact_after_cutoff() {
        let now = Utc::now();
        let cutoff = now - chrono::Duration::seconds(60);
        let before = (cutoff - chrono::Duration::seconds(1)).to_rfc3339();
        let equal = cutoff.to_rfc3339();
        let after = (cutoff + chrono::Duration::seconds(1)).to_rfc3339();

        let runners = vec![
            test_runner(1, &[("online", Some(&before)), ("offline", Some(&equal))]),
            test_runner(2, &[("online", Some(&after)), ("offline", Some(&before))]),
            test_runner(3, &[("offline", Some("not-a-time"))]),
            test_runner(4, &[]),
        ];

        let filtered = filter_uncontacted_runners(runners, now, ContactThreshold::Since(cutoff));
        let ids: Vec<u64> = filtered.into_iter().map(|r| r.id).collect();

        assert_eq!(ids, vec![1, 3]);
    }

    #[test]
    fn test_filter_runners_without_managers_socket_free() {
        let runners = vec![test_runner(1, &[("online", None)]), test_runner(2, &[])];

        let filtered = filter_runners_without_managers(runners);
        let ids: Vec<u64> = filtered.into_iter().map(|r| r.id).collect();

        assert_eq!(ids, vec![2]);
    }

    #[test]
    fn test_filter_rotating_runners_socket_free() {
        let runners = vec![
            test_runner(1, &[("online", None)]),
            test_runner(2, &[("online", None), ("offline", None)]),
            test_runner(3, &[]),
        ];

        let filtered = filter_rotating_runners(runners);
        let ids: Vec<u64> = filtered.into_iter().map(|r| r.id).collect();

        assert_eq!(ids, vec![2]);
    }
}
