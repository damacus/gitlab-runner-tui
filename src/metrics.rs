use crate::config::RunnerDiscoveryMode;
use jiff::Timestamp;
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
    #[serde(with = "crate::time::serde_timestamp")]
    pub started_at: Timestamp,
    #[serde(with = "crate::time::serde_timestamp")]
    pub finished_at: Timestamp,
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
        started_at: Timestamp,
        finished_at: Timestamp,
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
        started_at: Timestamp,
        finished_at: Timestamp,
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
        started_at: Timestamp,
        finished_at: Timestamp,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::parse_rfc3339;

    #[test]
    fn timestamps_remain_rfc3339_strings_in_metrics_json() {
        let metrics = LiveQueryMetrics::success(
            parse_rfc3339("2026-05-12T10:00:00.120Z").unwrap(),
            parse_rfc3339("2026-05-12T10:00:00.123400Z").unwrap(),
            125,
            2,
            RunnerDiscoveryMode::AllRunners,
            QueryRequestCounts::default(),
        );

        let json = serde_json::to_string(&metrics).unwrap();

        assert_eq!(
            json,
            r#"{"started_at":"2026-05-12T10:00:00.120Z","finished_at":"2026-05-12T10:00:00.123400Z","duration_millis":125,"result_count":2,"discovery_mode":"all_runners","request_counts":{"list_requests":0,"detail_requests":0,"manager_requests":0},"reused_enrichments":{"detail_enrichments":0,"manager_enrichments":0},"succeeded":true,"error_message":null}"#
        );
    }
}
