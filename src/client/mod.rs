use crate::models::manager::RunnerManager;
use crate::models::runner::{Runner, RunnerFilters};
use anyhow::{Context, Result};
use reqwest::{Client, Method, RequestBuilder};

#[derive(Clone)]
pub struct GitLabClient {
    client: Client,
    host: String,
    token: String,
}

impl GitLabClient {
    pub fn new(host: String, token: String) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .context("Failed to build reqwest client")?;

        Ok(Self {
            client,
            host,
            token,
        })
    }

    fn request(&self, method: Method, endpoint: &str) -> RequestBuilder {
        let url = format!(
            "{}/api/v4/{}",
            self.host.trim_end_matches('/'),
            endpoint.trim_start_matches('/')
        );
        self.client
            .request(method, &url)
            .header("PRIVATE-TOKEN", &self.token)
    }

    pub async fn fetch_runners(
        &self,
        filters: &RunnerFilters,
        page: u32,
        per_page: u32,
    ) -> Result<Vec<Runner>> {
        let mut request = self
            .request(Method::GET, "runners/all")
            .query(&[("per_page", per_page), ("page", page)]);

        if let Some(status) = &filters.status {
            request = request.query(&[("status", status)]);
        }
        if let Some(runner_type) = &filters.runner_type {
            request = request.query(&[("type", runner_type)]);
        }
        if let Some(paused) = filters.paused {
            request = request.query(&[("paused", paused.to_string())]);
        }
        if let Some(tags) = &filters.tag_list {
            for tag in tags {
                request = request.query(&[("tag_list[]", tag)]);
            }
        }
        if let Some(prefix) = &filters.version_prefix {
            request = request.query(&[("version_prefix", prefix)]);
        }

        let response = request.send().await.context("Failed to send request")?;
        let response = response
            .error_for_status()
            .context("GitLab API request failed")?;
        let runners = response
            .json::<Vec<Runner>>()
            .await
            .context("Failed to deserialize runners")?;

        Ok(runners)
    }

    pub async fn fetch_runner_detail(&self, runner_id: u64) -> Result<Runner> {
        let endpoint = format!("runners/{}", runner_id);
        let response = self
            .request(Method::GET, &endpoint)
            .send()
            .await
            .context("Failed to send request")?;
        let response = response
            .error_for_status()
            .context("Failed to fetch runner detail")?;
        let runner = response
            .json::<Runner>()
            .await
            .context("Failed to deserialize runner detail")?;
        Ok(runner)
    }

    pub async fn fetch_runner_managers(&self, runner_id: u64) -> Result<Vec<RunnerManager>> {
        let endpoint = format!("runners/{}/managers", runner_id);
        let response = self
            .request(Method::GET, &endpoint)
            .send()
            .await
            .context("Failed to send request")?;

        // Handle 404 (no managers) as empty list
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }

        let managers = response
            .json::<Vec<RunnerManager>>()
            .await
            .context("Failed to deserialize managers")?;
        Ok(managers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{Matcher, Server};

    #[test]
    fn test_client_creation() {
        let client = GitLabClient::new(
            "https://gitlab.example.com".to_string(),
            "token".to_string(),
        );
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_creation_with_trailing_slash() {
        let client = GitLabClient::new(
            "https://gitlab.example.com/".to_string(),
            "token".to_string(),
        );
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_fetch_runners_success() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/api/v4/runners/all")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .match_header("PRIVATE-TOKEN", "test-token")
            .with_status(200)
            .with_body(
                r#"[{
                    "id": 12345,
                    "runner_type": "group_type",
                    "active": true,
                    "paused": false,
                    "description": "Test Runner",
                    "created_at": "2024-01-15T10:30:00.000Z",
                    "ip_address": "10.0.1.50",
                    "is_shared": false,
                    "status": "online",
                    "version": "17.5.0",
                    "revision": "abc123",
                    "tag_list": ["alm", "production"],
                    "managers": []
                }]"#,
            )
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let filters = RunnerFilters::default();

        let runners = client.fetch_runners(&filters, 1, 100).await.unwrap();

        mock.assert_async().await;
        assert_eq!(runners.len(), 1);
        assert_eq!(runners[0].id, 12345);
        assert_eq!(runners[0].status, "online");
        assert_eq!(runners[0].tag_list, vec!["alm", "production"]);
    }

    #[tokio::test]
    async fn test_fetch_runners_with_status_filter() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/api/v4/runners/all")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
                Matcher::UrlEncoded("status".into(), "online".into()),
            ]))
            .match_header("PRIVATE-TOKEN", "test-token")
            .with_status(200)
            .with_body("[]")
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let filters = RunnerFilters {
            status: Some("online".to_string()),
            ..Default::default()
        };

        let runners = client.fetch_runners(&filters, 1, 100).await.unwrap();

        mock.assert_async().await;
        assert!(runners.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_runners_with_type_filter() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/api/v4/runners/all")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
                Matcher::UrlEncoded("type".into(), "group_type".into()),
            ]))
            .match_header("PRIVATE-TOKEN", "test-token")
            .with_status(200)
            .with_body("[]")
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let filters = RunnerFilters {
            runner_type: Some("group_type".to_string()),
            ..Default::default()
        };

        let _ = client.fetch_runners(&filters, 1, 100).await.unwrap();
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_runner_managers_success() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/api/v4/runners/12345/managers")
            .match_header("PRIVATE-TOKEN", "test-token")
            .with_status(200)
            .with_body(
                r#"[{
                    "id": 67890,
                    "system_id": "runner-host-01",
                    "created_at": "2024-01-15T10:30:00.000Z",
                    "contacted_at": "2024-01-20T14:22:00.000Z",
                    "ip_address": "10.0.1.50",
                    "status": "online",
                    "version": "17.5.0",
                    "revision": "abc123def"
                }]"#,
            )
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();

        let managers = client.fetch_runner_managers(12345).await.unwrap();

        mock.assert_async().await;
        assert_eq!(managers.len(), 1);
        assert_eq!(managers[0].id, 67890);
        assert_eq!(managers[0].system_id, "runner-host-01");
        assert_eq!(managers[0].status, "online");
    }

    #[tokio::test]
    async fn test_fetch_runner_managers_not_found_returns_empty() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/api/v4/runners/99999/managers")
            .match_header("PRIVATE-TOKEN", "test-token")
            .with_status(404)
            .with_body(r#"{"message":"404 Runner Not Found"}"#)
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();

        let managers = client.fetch_runner_managers(99999).await.unwrap();

        mock.assert_async().await;
        assert!(managers.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_runners_empty_response() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/api/v4/runners/all")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .match_header("PRIVATE-TOKEN", "test-token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("[]")
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let filters = RunnerFilters::default();

        let runners = client.fetch_runners(&filters, 1, 100).await.unwrap();

        mock.assert_async().await;
        assert!(runners.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_runners_returns_error_on_401() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/api/v4/runners/all")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .match_header("PRIVATE-TOKEN", "bad-token")
            .with_status(401)
            .with_body(r#"{"message":"401 Unauthorized"}"#)
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "bad-token".to_string()).unwrap();
        let filters = RunnerFilters::default();

        let result = client.fetch_runners(&filters, 1, 100).await;

        mock.assert_async().await;
        assert!(result.is_err());
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(
            err_msg.contains("401"),
            "Error should mention 401, got: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_fetch_runners_returns_error_on_500() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/api/v4/runners/all")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .match_header("PRIVATE-TOKEN", "test-token")
            .with_status(500)
            .with_body(r#"{"message":"500 Internal Server Error"}"#)
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let filters = RunnerFilters::default();

        let result = client.fetch_runners(&filters, 1, 100).await;

        mock.assert_async().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fetch_runners_with_tag_filter() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/api/v4/runners/all")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
                Matcher::UrlEncoded("tag_list[]".into(), "alm".into()),
            ]))
            .match_header("PRIVATE-TOKEN", "test-token")
            .with_status(200)
            .with_body("[]")
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let filters = RunnerFilters {
            tag_list: Some(vec!["alm".to_string()]),
            ..Default::default()
        };

        let runners = client.fetch_runners(&filters, 1, 100).await.unwrap();

        mock.assert_async().await;
        assert!(runners.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_runner_detail_success() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/api/v4/runners/12345")
            .match_header("PRIVATE-TOKEN", "test-token")
            .with_status(200)
            .with_body(
                r#"{
                    "id": 12345,
                    "runner_type": "group_type",
                    "active": true,
                    "paused": false,
                    "description": "Test Runner",
                    "created_at": "2024-01-15T10:30:00.000Z",
                    "ip_address": "10.0.1.50",
                    "is_shared": false,
                    "status": "online",
                    "version": "17.5.0",
                    "revision": "abc123",
                    "tag_list": ["alm", "production"]
                }"#,
            )
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();

        let runner = client.fetch_runner_detail(12345).await.unwrap();

        mock.assert_async().await;
        assert_eq!(runner.id, 12345);
        assert_eq!(runner.tag_list, vec!["alm", "production"]);
        assert_eq!(runner.version, Some("17.5.0".to_string()));
    }

    #[tokio::test]
    async fn test_runner_deserialization_without_tag_list() {
        let json = r#"{
            "id": 1,
            "runner_type": "instance_type",
            "active": true,
            "paused": false,
            "description": "Shared",
            "is_shared": true,
            "status": "online"
        }"#;

        let runner: Runner =
            serde_json::from_str(json).expect("Should deserialize without tag_list");
        assert_eq!(runner.id, 1);
        assert!(runner.tag_list.is_empty());
    }
}
