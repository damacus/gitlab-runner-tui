#[cfg(test)]
mod tests {
    use crate::models::manager::RunnerManager;
    use crate::models::runner::Runner;
    use crate::tui::app::ManagerRow;

    #[test]
    fn test_benchmark_workers() {
        let mut runners = vec![];
        for i in 0..1000 {
            let mut managers = vec![];
            for j in 0..10 {
                managers.push(RunnerManager {
                    id: j,
                    system_id: format!("test-host-{}", j),
                    created_at: "2024-01-15T10:30:00.000Z".to_string(),
                    contacted_at: Some("2024-01-20T14:22:00.000Z".to_string()),
                    ip_address: Some("10.0.1.1".to_string()),
                    status: "online".to_string(),
                    version: Some("17.5.0".to_string()),
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
                version: Some("17.5.0".to_string()),
                revision: None,
                created_at: Some("2024-01-20T14:22:00.000Z".to_string()),
                managers,
            });
        }

        let runners_clone = runners.clone();

        // BASELINE
        let start = std::time::Instant::now();
        let _manager_rows: Vec<ManagerRow> = runners_clone
            .iter()
            .flat_map(|r| {
                r.managers.iter().map(move |m| ManagerRow {
                    runner_id: r.id,
                    runner_tags_str: r.tag_list.join(", "),
                    manager: m.clone(),
                })
            })
            .collect();
        let duration = start.elapsed();
        println!("Benchmark baseline (iter/clone): {:?}", duration);

        // IMPROVED
        let start2 = std::time::Instant::now();
        let _manager_rows2: Vec<ManagerRow> = runners
            .into_iter()
            .flat_map(|mut r| {
                let tags = std::mem::take(&mut r.tag_list);
                let tags_str = tags.join(", ");
                r.managers.into_iter().map(move |m| ManagerRow {
                    runner_id: r.id,
                    runner_tags_str: tags_str.clone(),
                    manager: m,
                })
            })
            .collect();
        let duration2 = start2.elapsed();
        println!("Benchmark improved (into_iter): {:?}", duration2);
    }
}
