use crate::config::RunnerDiscoveryMode;
use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct QueryRequestCounts {
    pub list_requests: usize,
    pub detail_requests: usize,
    pub manager_requests: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct EnrichmentReuseCounts {
    pub detail_enrichments: usize,
    pub manager_enrichments: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveQueryMetrics {
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub duration_millis: u128,
    pub result_count: usize,
    pub discovery_mode: RunnerDiscoveryMode,
    pub request_counts: QueryRequestCounts,
    pub reused_enrichments: EnrichmentReuseCounts,
    pub succeeded: bool,
    pub error_message: Option<String>,
}

impl LiveQueryMetrics {
    pub fn success(
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
        duration_millis: u128,
        result_count: usize,
        discovery_mode: RunnerDiscoveryMode,
        request_counts: QueryRequestCounts,
    ) -> Self {
        Self {
            started_at,
            finished_at,
            duration_millis,
            result_count,
            discovery_mode,
            request_counts,
            reused_enrichments: EnrichmentReuseCounts::default(),
            succeeded: true,
            error_message: None,
        }
    }

    pub fn failure(
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
        duration_millis: u128,
        discovery_mode: RunnerDiscoveryMode,
        error_message: String,
    ) -> Self {
        Self {
            started_at,
            finished_at,
            duration_millis,
            result_count: 0,
            discovery_mode,
            request_counts: QueryRequestCounts::default(),
            reused_enrichments: EnrichmentReuseCounts::default(),
            succeeded: false,
            error_message: Some(error_message),
        }
    }

    pub fn success_with_reuse(
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
        duration_millis: u128,
        result_count: usize,
        discovery_mode: RunnerDiscoveryMode,
        request_counts: QueryRequestCounts,
        reused_enrichments: EnrichmentReuseCounts,
    ) -> Self {
        Self {
            started_at,
            finished_at,
            duration_millis,
            result_count,
            discovery_mode,
            request_counts,
            reused_enrichments,
            succeeded: true,
            error_message: None,
        }
    }
}
