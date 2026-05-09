use crate::models::{
    manager::RunnerManager,
    runner::{Runner, RunnerGroup},
};

pub fn demo_runners() -> Vec<Runner> {
    let now = chrono::Utc::now();
    let minutes_ago = |minutes: i64| {
        (now - chrono::Duration::minutes(minutes))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    };

    vec![
        // --- ONLINE runners (6) ---
        Runner {
            id: 100001,
            runner_type: "group_type".to_string(),
            active: true,
            paused: false,
            description: Some("prod-docker-01".to_string()),
            created_at: Some("2022-06-15T08:00:00.000Z".to_string()),
            ip_address: Some("10.0.1.10".to_string()),
            is_shared: false,
            status: "online".to_string(),
            version: Some("18.1.0".to_string()),
            revision: Some("a1b2c3d4".to_string()),
            tag_list: vec![
                "docker".to_string(),
                "linux".to_string(),
                "prod".to_string(),
            ],
            managers: vec![RunnerManager {
                id: 200001,
                system_id: "s_aabbccdd1122".to_string(),
                created_at: "2022-06-15T08:00:00.000Z".to_string(),
                contacted_at: Some(minutes_ago(5)),
                ip_address: Some("10.0.1.10".to_string()),
                status: "online".to_string(),
                version: Some("18.1.0".to_string()),
                revision: Some("a1b2c3d4".to_string()),
                platform: Some("linux".to_string()),
                architecture: Some("amd64".to_string()),
            }],
            groups: vec![RunnerGroup {
                id: 1,
                name: "platform".to_string(),
                web_url: "https://demo.gitlab.example.com/platform".to_string(),
            }],
        },
        Runner {
            id: 100002,
            runner_type: "group_type".to_string(),
            active: true,
            paused: false,
            description: Some("prod-docker-02".to_string()),
            created_at: Some("2022-06-15T08:00:00.000Z".to_string()),
            ip_address: Some("10.0.1.11".to_string()),
            is_shared: false,
            status: "online".to_string(),
            version: Some("18.1.0".to_string()),
            revision: Some("a1b2c3d4".to_string()),
            tag_list: vec![
                "docker".to_string(),
                "linux".to_string(),
                "prod".to_string(),
            ],
            managers: vec![RunnerManager {
                id: 200002,
                system_id: "s_bbccddee2233".to_string(),
                created_at: "2022-06-15T08:00:00.000Z".to_string(),
                contacted_at: Some(minutes_ago(8)),
                ip_address: Some("10.0.1.11".to_string()),
                status: "online".to_string(),
                version: Some("18.1.0".to_string()),
                revision: Some("a1b2c3d4".to_string()),
                platform: Some("linux".to_string()),
                architecture: Some("amd64".to_string()),
            }],
            groups: vec![RunnerGroup {
                id: 1,
                name: "platform".to_string(),
                web_url: "https://demo.gitlab.example.com/platform".to_string(),
            }],
        },
        Runner {
            id: 100003,
            runner_type: "instance_type".to_string(),
            active: true,
            paused: false,
            description: Some("shared-k8s-runner".to_string()),
            created_at: Some("2023-01-10T12:00:00.000Z".to_string()),
            ip_address: Some("10.0.2.5".to_string()),
            is_shared: true,
            status: "online".to_string(),
            version: Some("17.9.0".to_string()),
            revision: Some("deadbeef".to_string()),
            tag_list: vec!["k8s".to_string(), "linux".to_string()],
            managers: vec![
                RunnerManager {
                    id: 200003,
                    system_id: "s_ccddee334455".to_string(),
                    created_at: "2023-01-10T12:00:00.000Z".to_string(),
                    contacted_at: Some(minutes_ago(12)),
                    ip_address: Some("10.0.2.5".to_string()),
                    status: "online".to_string(),
                    version: Some("17.9.0".to_string()),
                    revision: Some("deadbeef".to_string()),
                    platform: Some("linux".to_string()),
                    architecture: Some("amd64".to_string()),
                },
                RunnerManager {
                    id: 200004,
                    system_id: "s_ddeeff445566".to_string(),
                    created_at: "2023-01-10T12:00:00.000Z".to_string(),
                    contacted_at: Some(minutes_ago(150)),
                    ip_address: Some("10.0.2.6".to_string()),
                    status: "online".to_string(),
                    version: Some("17.9.0".to_string()),
                    revision: Some("deadbeef".to_string()),
                    platform: Some("linux".to_string()),
                    architecture: Some("arm64".to_string()),
                },
            ],
            groups: vec![],
        },
        Runner {
            id: 100004,
            runner_type: "group_type".to_string(),
            active: true,
            paused: true,
            description: Some("platform-core-runner-01".to_string()),
            created_at: Some("2023-03-20T09:00:00.000Z".to_string()),
            ip_address: Some("10.0.3.1".to_string()),
            is_shared: false,
            status: "online".to_string(),
            version: Some("17.8.2".to_string()),
            revision: Some("f1e2d3c4".to_string()),
            tag_list: vec![
                "runner:type:local".to_string(),
                "platform".to_string(),
                "linux".to_string(),
            ],
            managers: vec![RunnerManager {
                id: 200005,
                system_id: "s_eeff00556677".to_string(),
                created_at: "2023-03-20T09:00:00.000Z".to_string(),
                contacted_at: Some(minutes_ago(3)),
                ip_address: Some("10.0.3.1".to_string()),
                status: "online".to_string(),
                version: Some("17.8.2".to_string()),
                revision: Some("f1e2d3c4".to_string()),
                platform: Some("linux".to_string()),
                architecture: Some("amd64".to_string()),
            }],
            groups: vec![RunnerGroup {
                id: 2,
                name: "platform".to_string(),
                web_url: "https://demo.gitlab.example.com/platform".to_string(),
            }],
        },
        Runner {
            id: 100005,
            runner_type: "group_type".to_string(),
            active: true,
            paused: false,
            description: Some("preproduction-runner".to_string()),
            created_at: Some("2021-11-05T14:00:00.000Z".to_string()),
            ip_address: Some("10.0.4.2".to_string()),
            is_shared: false,
            status: "online".to_string(),
            version: Some("16.11.5".to_string()),
            revision: Some("aabbccdd".to_string()),
            tag_list: vec![
                "linux".to_string(),
                "preproduction".to_string(),
                "local".to_string(),
            ],
            managers: vec![RunnerManager {
                id: 200006,
                system_id: "s_ff0011667788".to_string(),
                created_at: "2021-11-05T14:00:00.000Z".to_string(),
                contacted_at: Some(minutes_ago(190)),
                ip_address: Some("10.0.4.2".to_string()),
                status: "online".to_string(),
                version: Some("16.11.5".to_string()),
                revision: Some("aabbccdd".to_string()),
                platform: Some("linux".to_string()),
                architecture: Some("amd64".to_string()),
            }],
            groups: vec![],
        },
        Runner {
            id: 100006,
            runner_type: "project_type".to_string(),
            active: true,
            paused: false,
            description: Some("windows-build-runner".to_string()),
            created_at: Some("2024-02-01T10:00:00.000Z".to_string()),
            ip_address: Some("10.0.5.10".to_string()),
            is_shared: false,
            status: "online".to_string(),
            version: Some("18.0.1".to_string()),
            revision: Some("11223344".to_string()),
            tag_list: vec!["windows".to_string(), "dotnet".to_string()],
            managers: vec![RunnerManager {
                id: 200007,
                system_id: "s_001122778899".to_string(),
                created_at: "2024-02-01T10:00:00.000Z".to_string(),
                contacted_at: Some(minutes_ago(2)),
                ip_address: Some("10.0.5.10".to_string()),
                status: "online".to_string(),
                version: Some("18.0.1".to_string()),
                revision: Some("11223344".to_string()),
                platform: Some("windows".to_string()),
                architecture: Some("amd64".to_string()),
            }],
            groups: vec![],
        },
        // --- OFFLINE runners (3) ---
        Runner {
            id: 100007,
            runner_type: "group_type".to_string(),
            active: true,
            paused: false,
            description: Some("prod-docker-03".to_string()),
            created_at: Some("2022-08-01T08:00:00.000Z".to_string()),
            ip_address: Some("10.0.1.12".to_string()),
            is_shared: false,
            status: "offline".to_string(),
            version: Some("17.5.0".to_string()),
            revision: Some("55667788".to_string()),
            tag_list: vec![
                "docker".to_string(),
                "linux".to_string(),
                "prod".to_string(),
            ],
            managers: vec![RunnerManager {
                id: 200008,
                system_id: "s_112233889900".to_string(),
                created_at: "2022-08-01T08:00:00.000Z".to_string(),
                contacted_at: Some(minutes_ago(220)),
                ip_address: Some("10.0.1.12".to_string()),
                status: "offline".to_string(),
                version: Some("17.5.0".to_string()),
                revision: Some("55667788".to_string()),
                platform: Some("linux".to_string()),
                architecture: Some("amd64".to_string()),
            }],
            groups: vec![RunnerGroup {
                id: 1,
                name: "platform".to_string(),
                web_url: "https://demo.gitlab.example.com/platform".to_string(),
            }],
        },
        Runner {
            id: 100008,
            runner_type: "instance_type".to_string(),
            active: true,
            paused: false,
            description: Some("shared-runner-legacy".to_string()),
            created_at: Some("2020-05-12T00:00:00.000Z".to_string()),
            ip_address: None,
            is_shared: true,
            status: "offline".to_string(),
            version: Some("15.4.0".to_string()),
            revision: Some("oldoldold".to_string()),
            tag_list: vec!["legacy".to_string()],
            managers: vec![RunnerManager {
                id: 200009,
                system_id: "s_223344990011".to_string(),
                created_at: "2020-05-12T00:00:00.000Z".to_string(),
                contacted_at: Some(minutes_ago(1440)),
                ip_address: None,
                status: "offline".to_string(),
                version: Some("15.4.0".to_string()),
                revision: Some("oldoldold".to_string()),
                platform: Some("linux".to_string()),
                architecture: Some("amd64".to_string()),
            }],
            groups: vec![],
        },
        Runner {
            id: 100009,
            runner_type: "group_type".to_string(),
            active: true,
            paused: false,
            description: Some("platform-runner-offline".to_string()),
            created_at: Some("2023-07-19T11:00:00.000Z".to_string()),
            ip_address: Some("10.0.3.5".to_string()),
            is_shared: false,
            status: "offline".to_string(),
            version: Some("17.2.1".to_string()),
            revision: Some("aabbddeeff".to_string()),
            tag_list: vec!["runner:type:local".to_string(), "linux".to_string()],
            managers: vec![RunnerManager {
                id: 200010,
                system_id: "s_334455001122".to_string(),
                created_at: "2023-07-19T11:00:00.000Z".to_string(),
                contacted_at: Some(minutes_ago(70)),
                ip_address: Some("10.0.3.5".to_string()),
                status: "offline".to_string(),
                version: Some("17.2.1".to_string()),
                revision: Some("aabbddeeff".to_string()),
                platform: Some("linux".to_string()),
                architecture: Some("amd64".to_string()),
            }],
            groups: vec![RunnerGroup {
                id: 2,
                name: "platform".to_string(),
                web_url: "https://demo.gitlab.example.com/platform".to_string(),
            }],
        },
        // --- STALE runners (3) ---
        Runner {
            id: 100010,
            runner_type: "group_type".to_string(),
            active: true,
            paused: false,
            description: Some("stale-runner-01".to_string()),
            created_at: Some("2022-04-01T00:00:00.000Z".to_string()),
            ip_address: Some("10.0.6.1".to_string()),
            is_shared: false,
            status: "stale".to_string(),
            version: Some("17.0.0".to_string()),
            revision: Some("stale001".to_string()),
            tag_list: vec!["linux".to_string(), "prod".to_string()],
            managers: vec![RunnerManager {
                id: 200011,
                system_id: "s_445566112233".to_string(),
                created_at: "2022-04-01T00:00:00.000Z".to_string(),
                contacted_at: Some(minutes_ago(2880)),
                ip_address: Some("10.0.6.1".to_string()),
                status: "offline".to_string(),
                version: Some("17.0.0".to_string()),
                revision: Some("stale001".to_string()),
                platform: Some("linux".to_string()),
                architecture: Some("amd64".to_string()),
            }],
            groups: vec![],
        },
        Runner {
            id: 100011,
            runner_type: "group_type".to_string(),
            active: true,
            paused: false,
            description: Some("stale-runner-02".to_string()),
            created_at: Some("2021-09-14T00:00:00.000Z".to_string()),
            ip_address: None,
            is_shared: false,
            status: "stale".to_string(),
            version: Some("16.8.0".to_string()),
            revision: Some("stale002".to_string()),
            tag_list: vec!["windows".to_string()],
            managers: vec![RunnerManager {
                id: 200012,
                system_id: "s_556677223344".to_string(),
                created_at: "2021-09-14T00:00:00.000Z".to_string(),
                contacted_at: Some(minutes_ago(10080)),
                ip_address: None,
                status: "offline".to_string(),
                version: Some("16.8.0".to_string()),
                revision: Some("stale002".to_string()),
                platform: Some("windows".to_string()),
                architecture: Some("amd64".to_string()),
            }],
            groups: vec![],
        },
        Runner {
            id: 100012,
            runner_type: "instance_type".to_string(),
            active: true,
            paused: false,
            description: Some("stale-shared-runner".to_string()),
            created_at: Some("2020-11-30T00:00:00.000Z".to_string()),
            ip_address: Some("10.0.7.99".to_string()),
            is_shared: true,
            status: "stale".to_string(),
            version: Some("16.5.2".to_string()),
            revision: Some("stale003".to_string()),
            tag_list: vec!["linux".to_string(), "k8s".to_string()],
            managers: vec![RunnerManager {
                id: 200013,
                system_id: "s_667788334455".to_string(),
                created_at: "2020-11-30T00:00:00.000Z".to_string(),
                contacted_at: Some(minutes_ago(4320)),
                ip_address: Some("10.0.7.99".to_string()),
                status: "offline".to_string(),
                version: Some("16.5.2".to_string()),
                revision: Some("stale003".to_string()),
                platform: Some("linux".to_string()),
                architecture: Some("amd64".to_string()),
            }],
            groups: vec![],
        },
        // --- NEVER CONTACTED runners (2) ---
        Runner {
            id: 100013,
            runner_type: "group_type".to_string(),
            active: true,
            paused: false,
            description: Some("new-runner-not-yet-started".to_string()),
            created_at: Some("2026-03-18T15:00:00.000Z".to_string()),
            ip_address: None,
            is_shared: false,
            status: "never_contacted".to_string(),
            version: None,
            revision: None,
            tag_list: vec!["linux".to_string(), "prod".to_string()],
            managers: vec![],
            groups: vec![RunnerGroup {
                id: 1,
                name: "platform".to_string(),
                web_url: "https://demo.gitlab.example.com/platform".to_string(),
            }],
        },
        Runner {
            id: 100014,
            runner_type: "group_type".to_string(),
            active: true,
            paused: false,
            description: None,
            created_at: Some("2026-03-10T09:00:00.000Z".to_string()),
            ip_address: None,
            is_shared: false,
            status: "never_contacted".to_string(),
            version: None,
            revision: None,
            tag_list: vec![],
            managers: vec![],
            groups: vec![],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    #[test]
    fn demo_runners_covers_all_statuses() {
        let runners = demo_runners();
        assert_eq!(
            runners.len(),
            14,
            "fixture set must contain exactly 14 runners"
        );

        let statuses: Vec<&str> = runners.iter().map(|r| r.status.as_str()).collect();
        assert!(statuses.contains(&"online"), "need online runners");
        assert!(statuses.contains(&"offline"), "need offline runners");
        assert!(statuses.contains(&"stale"), "need stale runners");
        assert!(
            statuses.contains(&"never_contacted"),
            "need never_contacted runners"
        );

        let has_paused = runners.iter().any(|r| r.paused);
        assert!(has_paused, "need at least one paused runner");

        let has_groups = runners.iter().any(|r| !r.groups.is_empty());
        assert!(has_groups, "need at least one runner with a group");

        let has_managers = runners.iter().any(|r| !r.managers.is_empty());
        assert!(has_managers, "need runners with managers");
    }

    #[test]
    fn demo_runners_include_threshold_edge_case_for_stale_filtering() {
        let runners = demo_runners();
        let now = Utc::now();
        let threshold_secs = 3600i64;

        let mixed_runner = runners
            .iter()
            .find(|runner| runner.id == 100003)
            .expect("runner 100003 must exist");
        let manager_ages: Vec<i64> = mixed_runner
            .managers
            .iter()
            .filter_map(|manager| manager.contacted_at.as_deref())
            .filter_map(|contacted_at| DateTime::parse_from_rfc3339(contacted_at).ok())
            .map(|contacted_at| {
                now.signed_duration_since(contacted_at.with_timezone(&Utc))
                    .num_seconds()
            })
            .collect();

        assert!(
            manager_ages.iter().any(|age| *age <= threshold_secs),
            "edge-case runner must include at least one recently contacted manager"
        );
        assert!(
            manager_ages.iter().any(|age| *age > threshold_secs),
            "edge-case runner must include at least one stale manager"
        );
    }
}
