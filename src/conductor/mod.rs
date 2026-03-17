use crate::client::GitLabClient;
use crate::config::{RunnerTarget, RunnerTargetKind};
use crate::models::runner::{Runner, RunnerFilters};
use anyhow::Result;
use chrono::{DateTime, Utc};
use futures::stream::{self, StreamExt};
use std::collections::BTreeMap;

pub struct Conductor {
    client: GitLabClient,
    runner_targets: Vec<RunnerTarget>,
}

impl Conductor {
    pub fn new(client: GitLabClient, runner_targets: Vec<RunnerTarget>) -> Self {
        Self {
            client,
            runner_targets,
        }
    }

    pub async fn validate_token(&self) -> Result<()> {
        self.client.validate_token().await
    }

    pub async fn fetch_runners(&self, filters: RunnerFilters) -> Result<Vec<Runner>> {
        let mut runner_map = BTreeMap::new();
        let per_page = 100;

        if self.runner_targets.is_empty() {
            let mut page = 1;

            loop {
                let runners = self
                    .client
                    .fetch_available_runners(&filters, page, per_page)
                    .await?;
                if runners.is_empty() {
                    break;
                }

                let count = runners.len();
                for runner in runners {
                    runner_map.entry(runner.id).or_insert(runner);
                }

                if count < per_page as usize {
                    break;
                }
                page += 1;
            }
        } else {
            for target in &self.runner_targets {
                let mut page = 1;

                loop {
                    let runners = match target.kind {
                        RunnerTargetKind::Group => {
                            self.client
                                .fetch_group_runners(&target.id, &filters, page, per_page)
                                .await?
                        }
                        RunnerTargetKind::Project => {
                            self.client
                                .fetch_project_runners(&target.id, &filters, page, per_page)
                                .await?
                        }
                    };
                    if runners.is_empty() {
                        break;
                    }

                    let count = runners.len();
                    for runner in runners {
                        if let Some(existing) = runner_map.get_mut(&runner.id) {
                            merge_runner(existing, runner);
                        } else {
                            runner_map.insert(runner.id, runner);
                        }
                    }

                    if count < per_page as usize {
                        break;
                    }
                    page += 1;
                }
            }
        }

        let runners: Vec<Runner> = runner_map.into_values().collect();

        let enriched: Vec<Runner> = stream::iter(runners.into_iter().map(|r| {
            let client = self.client.clone();
            async move {
                let runner_id = r.id;

                let (detail_res, managers_res) = tokio::join!(
                    client.fetch_runner_detail(runner_id),
                    client.fetch_runner_managers(runner_id)
                );

                let mut detail = match detail_res {
                    Ok(d) => d,
                    Err(e) => {
                        tracing::warn!(runner_id, error = %e, "Failed to fetch runner detail, using list data");
                        r
                    }
                };

                match managers_res {
                    Ok(managers) => detail.managers = managers,
                    Err(e) => {
                        tracing::warn!(runner_id, error = %e, "Failed to fetch runner managers, keeping runner without manager data");
                    }
                }

                detail
            }
        }))
        .buffer_unordered(10)
        .collect()
        .await;

        Ok(enriched)
    }

    pub async fn list_offline_runners(&self, filters: RunnerFilters) -> Result<Vec<Runner>> {
        let runners = self.fetch_runners(filters).await?;
        Ok(filter_offline_runners(runners))
    }

    pub async fn list_uncontacted_runners(
        &self,
        filters: RunnerFilters,
        threshold_secs: u64,
    ) -> Result<Vec<Runner>> {
        let runners = self.fetch_runners(filters).await?;
        Ok(filter_uncontacted_runners(
            runners,
            Utc::now(),
            threshold_secs,
        ))
    }

    /// Returns (online_count, total_count) - reserved for potential status aggregation
    #[allow(dead_code)]
    pub async fn check_runner_statuses(&self, filters: RunnerFilters) -> Result<(usize, usize)> {
        let runners = self.fetch_runners(filters).await?;
        let total = runners.len();
        let online = runners
            .iter()
            .filter(|r| r.managers.iter().any(|m| m.status == "online"))
            .count();
        Ok((online, total))
    }

    pub async fn list_runners_without_managers(
        &self,
        filters: RunnerFilters,
    ) -> Result<Vec<Runner>> {
        let runners = self.fetch_runners(filters).await?;
        Ok(filter_runners_without_managers(runners))
    }

    pub async fn detect_rotating_runners(&self, filters: RunnerFilters) -> Result<Vec<Runner>> {
        let runners = self.fetch_runners(filters).await?;
        Ok(filter_rotating_runners(runners))
    }
}

fn is_runner_offline(runner: &Runner) -> bool {
    !runner.managers.is_empty() && !runner.managers.iter().any(|m| m.status == "online")
}

fn is_runner_uncontacted(runner: &Runner, now: DateTime<Utc>, threshold_secs: u64) -> bool {
    if runner.managers.is_empty() {
        return false;
    }

    // Runner is uncontacted if all managers are missing/stale/invalid relative to threshold.
    runner
        .managers
        .iter()
        .all(|m| is_manager_stale(m.contacted_at.as_deref(), now, threshold_secs))
}

fn is_manager_stale(contacted_at: Option<&str>, now: DateTime<Utc>, threshold_secs: u64) -> bool {
    match contacted_at {
        Some(contacted_at_str) => match DateTime::parse_from_rfc3339(contacted_at_str) {
            Ok(contacted_at) => {
                let duration = now.signed_duration_since(contacted_at);
                duration.num_seconds() > threshold_secs as i64
            }
            Err(_) => true, // Unparseable timestamp treated as stale
        },
        None => true, // Missing contacted_at treated as stale
    }
}

fn filter_offline_runners(runners: Vec<Runner>) -> Vec<Runner> {
    runners.into_iter().filter(is_runner_offline).collect()
}

fn filter_uncontacted_runners(
    runners: Vec<Runner>,
    now: DateTime<Utc>,
    threshold_secs: u64,
) -> Vec<Runner> {
    runners
        .into_iter()
        .filter(|r| is_runner_uncontacted(r, now, threshold_secs))
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
                "version": "17.5.0",
                "revision": "abc123",
                "tag_list": [{}]
            }}"#,
            id,
            id,
            status,
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
            GitLabClient::new("http://example.com".to_string(), "token".to_string()).unwrap();
        let _conductor = Conductor::new(client, vec![group_target("123")]);
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
        let conductor = Conductor::new(
            client,
            vec![group_target("123"), project_target("my-org/app")],
        );

        let runners = conductor
            .fetch_runners(RunnerFilters::default())
            .await
            .unwrap();
        let ids: Vec<u64> = runners.iter().map(|runner| runner.id).collect();

        assert_eq!(ids, vec![1, 2, 3]);

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
        let conductor = Conductor::new(client, Vec::new());

        let runners = conductor
            .fetch_runners(RunnerFilters::default())
            .await
            .unwrap();

        assert!(runners.is_empty());
        list_mock.assert_async().await;
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
        let conductor = Conductor::new(client, vec![group_target("123")]);

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
        let conductor = Conductor::new(client, vec![group_target("123")]);

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
        let conductor = Conductor::new(client, vec![group_target("123")]);

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
        let conductor = Conductor::new(client, vec![group_target("123")]);

        let offline = conductor
            .list_offline_runners(RunnerFilters::default())
            .await
            .unwrap();

        // Only runner 2 has an offline manager
        assert_eq!(offline.len(), 1);
        assert_eq!(offline[0].id, 2);

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
        let conductor = Conductor::new(client, vec![group_target("123")]);

        let offline = conductor
            .list_offline_runners(RunnerFilters::default())
            .await
            .unwrap();

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
        let conductor = Conductor::new(client, vec![group_target("123")]);

        let empty = conductor
            .list_runners_without_managers(RunnerFilters::default())
            .await
            .unwrap();

        // Only runner 2 has no managers
        assert_eq!(empty.len(), 1);
        assert_eq!(empty[0].id, 2);

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
        let conductor = Conductor::new(client, vec![group_target("123")]);

        let rotating = conductor
            .detect_rotating_runners(RunnerFilters::default())
            .await
            .unwrap();

        assert_eq!(rotating.len(), 1);
        assert_eq!(rotating[0].id, 1);
        assert_eq!(rotating[0].managers.len(), 2);

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
        let conductor = Conductor::new(client, vec![group_target("123")]);

        let rotating = conductor
            .detect_rotating_runners(RunnerFilters::default())
            .await
            .unwrap();

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
        let conductor = Conductor::new(client, vec![group_target("123")]);

        let rotating = conductor
            .detect_rotating_runners(RunnerFilters::default())
            .await
            .unwrap();

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
        let conductor = Conductor::new(client, vec![group_target("123")]);

        let rotating = conductor
            .detect_rotating_runners(RunnerFilters::default())
            .await
            .unwrap();

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
        let conductor = Conductor::new(client, vec![group_target("123")]);

        let uncontacted = conductor
            .list_uncontacted_runners(RunnerFilters::default(), 60)
            .await
            .unwrap();

        assert_eq!(uncontacted.len(), 1);
        assert_eq!(uncontacted[0].id, 1);

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
        let conductor = Conductor::new(client, vec![group_target("123")]);

        let uncontacted = conductor
            .list_uncontacted_runners(RunnerFilters::default(), 60)
            .await
            .unwrap();

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

        let filtered = filter_uncontacted_runners(runners, now, 60);
        let ids: Vec<u64> = filtered.into_iter().map(|r| r.id).collect();

        assert_eq!(ids, vec![1, 4]);
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
