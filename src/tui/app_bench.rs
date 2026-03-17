#[cfg(test)]
mod tests {
    use crate::models::manager::RunnerManager;
    use crate::models::runner::{
        benchmark_runner_processing, LocalBenchmarkSnapshot, Runner, RunnerFilters, RunnerSortKey,
    };
    use chrono::Utc;

    fn build_runners(total: u64) -> Vec<Runner> {
        let mut runners = vec![];
        for i in 0..total {
            let mut managers = vec![];
            for j in 0..10 {
                managers.push(RunnerManager {
                    id: j,
                    system_id: format!("test-host-{}", j),
                    created_at: "2024-01-15T10:30:00.000Z".to_string(),
                    contacted_at: Some("2024-01-20T14:22:00.000Z".to_string()),
                    ip_address: Some("10.0.1.1".to_string()),
                    status: "online".to_string(),
                    version: Some(if j % 2 == 0 { "17.5.0" } else { "17.4.0" }.to_string()),
                    revision: None,
                    platform: None,
                    architecture: None,
                });
            }
            runners.push(Runner {
                id: i,
                description: Some("test".to_string()),
                ip_address: Some("10.0.1.1".to_string()),
                active: true,
                paused: false,
                is_shared: false,
                runner_type: "project_type".to_string(),
                status: "online".to_string(),
                tag_list: vec!["alm".to_string(), "prod".to_string()],
                version: Some(if i % 2 == 0 { "17.5.0" } else { "17.4.0" }.to_string()),
                revision: None,
                created_at: Some("2024-01-20T14:22:00.000Z".to_string()),
                managers,
            });
        }
        runners
    }

    #[test]
    fn test_benchmark_runner_processing_reports_10_50_100_samples() {
        let runners = build_runners(100);
        let filters = RunnerFilters {
            selected_versions: Some(vec!["17.5.0".to_string()]),
            ..RunnerFilters::default()
        };

        let snapshot = benchmark_runner_processing(
            &runners,
            &filters,
            RunnerSortKey::LastContactOldestFirst,
            Utc::now(),
        );

        assert_eq!(
            snapshot
                .measurements
                .iter()
                .map(|measurement| measurement.sample_size)
                .collect::<Vec<_>>(),
            vec![10, 50, 100]
        );
        assert!(snapshot
            .measurements
            .iter()
            .all(|measurement| measurement.filtered_count <= measurement.sample_size));
    }

    #[test]
    fn test_benchmark_snapshot_is_empty_when_no_runners_loaded() {
        let snapshot = benchmark_runner_processing(
            &[],
            &RunnerFilters::default(),
            RunnerSortKey::None,
            Utc::now(),
        );

        assert_eq!(snapshot, LocalBenchmarkSnapshot::default());
    }
}
