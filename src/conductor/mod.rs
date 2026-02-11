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
            let runners = self.client.fetch_runners(&filters, page, per_page).await?;
            if runners.is_empty() {
                break;
            }

            let count = runners.len();

            // Enrich each runner with detail (tags, version) and managers
            let futures: Vec<_> = runners
                .into_iter()
                .map(|r| {
                    let client = self.client.clone();
                    async move {
                        let mut detail = client.fetch_runner_detail(r.id).await.unwrap_or(r);
                        let managers = client
                            .fetch_runner_managers(detail.id)
                            .await
                            .unwrap_or_default();
                        detail.managers = managers;
                        detail
                    }
                })
                .collect();

            let enriched = futures::future::join_all(futures).await;
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
