use crate::client::GitLabClient;
use crate::models::runner::{Runner, RunnerFilters};
use anyhow::Result;
use chrono::{DateTime, Utc};

pub struct Conductor {
    client: GitLabClient,
}

impl Conductor {
    pub fn new(client: GitLabClient) -> Self {
        Self { client }
    }

    pub async fn fetch_runners(&self, filters: RunnerFilters) -> Result<Vec<Runner>> {
        let mut all_runners = Vec::new();
        let mut page = 1;
        let per_page = 100;

        loop {
            let mut runners = self.client.fetch_runners(&filters, page, per_page).await?;
            if runners.is_empty() {
                break;
            }

            // For each runner, fetch managers
            for runner in &mut runners {
                let managers = self.client.fetch_runner_managers(runner.id).await?;
                // Sort managers by ID descending (proxy for creation time if created_at is string)
                // Or parse created_at if needed. For now, rely on API order or simple sort.
                // Managers usually returned ordered.
                runner.managers = managers;
            }

            let count = runners.len();
            all_runners.append(&mut runners);

            if count < per_page as usize {
                break;
            }
            page += 1;
        }

        // Apply client-side filters
        if let Some(tags) = &filters.tag_list {
            all_runners.retain(|r| tags.iter().any(|t| r.tag_list.contains(t)));
        }

        if let Some(prefix) = &filters.version_prefix {
            all_runners.retain(|r| {
                r.version
                    .as_deref()
                    .map(|v| v.starts_with(prefix))
                    .unwrap_or(false)
            });
        }

        Ok(all_runners)
    }

    pub async fn list_offline_runners(&self, filters: RunnerFilters) -> Result<Vec<Runner>> {
        let runners = self.fetch_runners(filters).await?;
        let offline = runners
            .into_iter()
            .filter(|r| {
                if let Some(manager) = r.managers.first() {
                    manager.status != "online"
                } else {
                    false // No manager means we can't determine status from manager, or it's "never_contacted"
                }
            })
            .collect();
        Ok(offline)
    }

    pub async fn list_uncontacted_runners(
        &self,
        filters: RunnerFilters,
        threshold_secs: u64,
    ) -> Result<Vec<Runner>> {
        let runners = self.fetch_runners(filters).await?;
        let now = Utc::now();

        let uncontacted = runners
            .into_iter()
            .filter(|r| {
                if let Some(manager) = r.managers.first() {
                    if let Some(contacted_at_str) = &manager.contacted_at {
                        if let Ok(contacted_at) = DateTime::parse_from_rfc3339(contacted_at_str) {
                            let duration = now.signed_duration_since(contacted_at);
                            return duration.num_seconds() > threshold_secs as i64;
                        }
                    }
                    true // If contacted_at is missing or unparseable, treat as uncontacted? Or maybe safe to ignore. Spec says "managers[0].contacted_at.is_none() OR ..."
                } else {
                    false
                }
            })
            .collect();
        Ok(uncontacted)
    }

    /// Returns (online_count, total_count) - reserved for potential status aggregation
    #[allow(dead_code)]
    pub async fn check_runner_statuses(&self, filters: RunnerFilters) -> Result<(usize, usize)> {
        let runners = self.fetch_runners(filters).await?;
        let total = runners.len();
        let online = runners
            .iter()
            .filter(|r| {
                if let Some(manager) = r.managers.first() {
                    manager.status == "online"
                } else {
                    false
                }
            })
            .count();
        Ok((online, total))
    }

    pub async fn list_runners_without_managers(
        &self,
        filters: RunnerFilters,
    ) -> Result<Vec<Runner>> {
        let runners = self.fetch_runners(filters).await?;
        let empty = runners
            .into_iter()
            .filter(|r| r.managers.is_empty())
            .collect();
        Ok(empty)
    }

    pub async fn detect_rotating_runners(&self, filters: RunnerFilters) -> Result<Vec<Runner>> {
        let runners = self.fetch_runners(filters).await?;
        let rotating = runners
            .into_iter()
            .filter(|r| r.managers.len() > 1)
            .collect();
        Ok(rotating)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{Matcher, Server};

    fn manager_json(id: u64, system_id: &str, status: &str, version: &str) -> String {
        format!(
            r#"{{"id":{},"system_id":"{}","created_at":"2024-01-15T10:30:00.000Z","contacted_at":"2024-01-20T14:22:00.000Z","ip_address":"10.0.1.1","status":"{}","version":"{}","revision":"abc123"}}"#,
            id, system_id, status, version
        )
    }

    fn runner_list_json(id: u64, status: &str) -> String {
        format!(
            r#"{{"id":{},"runner_type":"group_type","active":true,"paused":false,"description":"Runner {}","ip_address":"","is_shared":false,"status":"{}","name":null,"online":{},"tag_list":[]}}"#,
            id,
            id,
            status,
            status == "online"
        )
    }

    async fn setup_rotation_mocks(
        server: &mut Server,
        runners_with_managers: &[(u64, &str, Vec<(u64, &str, &str, &str)>)],
    ) -> Vec<mockito::Mock> {
        let mut mocks = Vec::new();

        // Build list response
        let list_bodies: Vec<String> = runners_with_managers
            .iter()
            .map(|(id, status, _)| runner_list_json(*id, status))
            .collect();
        let list_body = format!("[{}]", list_bodies.join(","));

        mocks.push(
            server
                .mock("GET", "/runners/all")
                .match_query(Matcher::AllOf(vec![
                    Matcher::UrlEncoded("per_page".into(), "100".into()),
                    Matcher::UrlEncoded("page".into(), "1".into()),
                ]))
                .with_status(200)
                .with_body(list_body)
                .create_async()
                .await,
        );

        // Manager endpoints per runner
        for (id, _, managers) in runners_with_managers {
            let mgr_bodies: Vec<String> = managers
                .iter()
                .map(|(mid, sys, status, ver)| manager_json(*mid, sys, status, ver))
                .collect();
            let mgr_body = format!("[{}]", mgr_bodies.join(","));

            mocks.push(
                server
                    .mock("GET", format!("/runners/{}/managers", id).as_str())
                    .with_status(200)
                    .with_body(mgr_body)
                    .create_async()
                    .await,
            );
        }

        mocks
    }

    #[tokio::test]
    async fn test_detect_rotating_runners_finds_multi_manager() {
        let mut server = Server::new_async().await;
        let _mocks = setup_rotation_mocks(
            &mut server,
            &[
                // Runner 1: two managers (rotation in progress)
                (
                    1,
                    "online",
                    vec![
                        (10, "old-host", "offline", "17.4.0"),
                        (11, "new-host", "online", "17.5.0"),
                    ],
                ),
                // Runner 2: single manager (no rotation)
                (2, "online", vec![(20, "stable-host", "online", "17.5.0")]),
            ],
        )
        .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new(client);

        let rotating = conductor
            .detect_rotating_runners(RunnerFilters::default())
            .await
            .unwrap();

        assert_eq!(rotating.len(), 1);
        assert_eq!(rotating[0].id, 1);
        assert_eq!(rotating[0].managers.len(), 2);
    }

    #[tokio::test]
    async fn test_detect_rotating_runners_empty_when_no_rotation() {
        let mut server = Server::new_async().await;
        let _mocks = setup_rotation_mocks(
            &mut server,
            &[
                (1, "online", vec![(10, "host-a", "online", "17.5.0")]),
                (2, "online", vec![(20, "host-b", "online", "17.5.0")]),
            ],
        )
        .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new(client);

        let rotating = conductor
            .detect_rotating_runners(RunnerFilters::default())
            .await
            .unwrap();

        assert!(rotating.is_empty());
    }

    #[tokio::test]
    async fn test_detect_rotating_runners_excludes_no_managers() {
        let mut server = Server::new_async().await;
        let _mocks = setup_rotation_mocks(
            &mut server,
            &[
                // Runner with no managers
                (1, "online", vec![]),
                // Runner with two managers (rotating)
                (
                    2,
                    "online",
                    vec![
                        (20, "old-host", "stale", "17.3.0"),
                        (21, "new-host", "online", "17.5.0"),
                    ],
                ),
            ],
        )
        .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new(client);

        let rotating = conductor
            .detect_rotating_runners(RunnerFilters::default())
            .await
            .unwrap();

        assert_eq!(rotating.len(), 1);
        assert_eq!(rotating[0].id, 2);
    }

    #[tokio::test]
    async fn test_detect_rotating_runners_three_managers() {
        let mut server = Server::new_async().await;
        let _mocks = setup_rotation_mocks(
            &mut server,
            &[(
                1,
                "online",
                vec![
                    (10, "host-v1", "offline", "17.3.0"),
                    (11, "host-v2", "stale", "17.4.0"),
                    (12, "host-v3", "online", "17.5.0"),
                ],
            )],
        )
        .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new(client);

        let rotating = conductor
            .detect_rotating_runners(RunnerFilters::default())
            .await
            .unwrap();

        assert_eq!(rotating.len(), 1);
        assert_eq!(rotating[0].managers.len(), 3);
    }
}
