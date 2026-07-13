#[cfg(test)]
mod tests {
    use crate::models::manager::RunnerManager;
    use crate::models::runner::{
        apply_runner_filters, benchmark_runner_processing, reset_runner_clone_count,
        runner_clone_count, sort_runners, LocalBenchmarkSnapshot, Runner, RunnerFilters,
        RunnerSortKey,
    };
    use chrono::Utc;
    use std::time::Instant;

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
                groups: vec![],
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

        let snapshot =
            benchmark_runner_processing(&runners, &filters, RunnerSortKey::LastContact, Utc::now());

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

    #[test]
    fn projected_view_benchmarks_cover_one_thousand_and_ten_thousand_without_deep_clones() {
        let runners = build_runners(10_000);
        let filters = RunnerFilters {
            selected_versions: Some(vec!["17.5.0".to_string()]),
            ..RunnerFilters::default()
        };

        let mut legacy_evidence = Vec::new();
        for sample_size in [1_000, 10_000] {
            reset_runner_clone_count();
            let legacy_started = Instant::now();
            let mut legacy_view =
                apply_runner_filters(&runners[..sample_size], &filters, Utc::now());
            sort_runners(&mut legacy_view, RunnerSortKey::LastContact, Utc::now());
            let legacy_micros = legacy_started.elapsed().as_micros();
            let legacy_clones = runner_clone_count();
            assert_eq!(legacy_clones, sample_size);
            legacy_evidence.push((sample_size, legacy_micros, legacy_clones));
        }

        reset_runner_clone_count();
        let projected_started = Instant::now();
        let snapshot =
            benchmark_runner_processing(&runners, &filters, RunnerSortKey::LastContact, Utc::now());
        let projected_micros = projected_started.elapsed().as_micros();

        for (sample_size, budget_micros) in [(1_000, 2_000_000_u128), (10_000, 10_000_000)] {
            let measurement = snapshot
                .measurements
                .iter()
                .find(|measurement| measurement.sample_size == sample_size)
                .expect("large projection benchmark");
            let total_micros = measurement.filter_duration_micros
                + measurement.sort_duration_micros
                + measurement.flatten_duration_micros;
            assert_eq!(measurement.deep_runner_clones, 0);
            assert!(
                total_micros < budget_micros,
                "{sample_size}-runner projection took {total_micros}us, budget {budget_micros}us"
            );
            let (_, legacy_micros, legacy_clones) = legacy_evidence
                .iter()
                .find(|(legacy_size, _, _)| *legacy_size == sample_size)
                .unwrap();
            eprintln!(
                "{sample_size} runners: legacy {legacy_micros}us/{legacy_clones} deep clones; projected {total_micros}us/0 deep clones"
            );
        }
        assert_eq!(runner_clone_count(), 0);
        eprintln!("all projected samples completed in {projected_micros}us");
    }
}
