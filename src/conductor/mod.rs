use crate::client::GitLabClient;
use crate::models::runner::{Runner, RunnerFilters};
use anyhow::Result;
use chrono::{DateTime, Utc};
use futures::stream::{self, StreamExt};

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
            let runners = self.client.fetch_runners(&filters, page, per_page).await?;
            if runners.is_empty() {
                break;
            }

            let count = runners.len();

            // Enrich each runner with detail (tags, version) and managers
            // Use buffer_unordered to limit concurrent API requests
            let enriched: Vec<Runner> = stream::iter(runners.into_iter().map(|r| {
                let client = self.client.clone();
                async move {
                    let mut detail = match client.fetch_runner_detail(r.id).await {
                        Ok(d) => d,
                        Err(e) => {
                            tracing::warn!(runner_id = r.id, error = %e, "Failed to fetch runner detail, using list data");
                            r
                        }
                    };
                    match client.fetch_runner_managers(detail.id).await {
                        Ok(managers) => detail.managers = managers,
                        Err(e) => {
                            tracing::warn!(runner_id = detail.id, error = %e, "Failed to fetch runner managers");
                        }
                    }
                    detail
                }
            }))
            .buffer_unordered(10)
            .collect()
            .await;
            all_runners.extend(enriched);

            if count < per_page as usize {
                break;
            }
            page += 1;
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
}

#[cfg(test)]
mod tests {
    use super::*;
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

    async fn setup_runner_mocks(
        server: &mut Server,
        runners: &[(u64, &str, &[&str], Option<(u64, &str)>)],
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
                .mock("GET", "/api/v4/runners/all")
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
        for (id, status, tags, manager) in runners {
            mocks.push(
                server
                    .mock("GET", format!("/api/v4/runners/{}", id).as_str())
                    .with_status(200)
                    .with_body(detail_response_body(*id, status, tags))
                    .create_async()
                    .await,
            );

            let managers_body = match manager {
                Some((mid, mstatus)) => {
                    format!("[{}]", manager_response_body(*mid, *id, mstatus))
                }
                None => "[]".to_string(),
            };
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

    #[tokio::test]
    async fn test_enrichment_adds_tags_from_detail() {
        let mut server = Server::new_async().await;
        let mocks = setup_runner_mocks(
            &mut server,
            &[(1, "online", &["alm", "production"], Some((10, "online")))],
        )
        .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new(client);

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
            .mock("GET", "/api/v4/runners/all")
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
        let conductor = Conductor::new(client);

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
                (1, "online", &["prod"], Some((10, "online"))),
                (2, "offline", &["staging"], Some((20, "offline"))),
                (3, "online", &["dev"], None),
            ],
        )
        .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new(client);

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
    async fn test_list_runners_without_managers() {
        let mut server = Server::new_async().await;
        let mocks = setup_runner_mocks(
            &mut server,
            &[
                (1, "online", &["prod"], Some((10, "online"))),
                (2, "online", &["staging"], None),
            ],
        )
        .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let conductor = Conductor::new(client);

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
}
