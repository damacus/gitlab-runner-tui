use crate::config::{RunnerTarget, RunnerTargetKind};
use crate::models::manager::RunnerManager;
use crate::models::runner::{Runner, RunnerFilters};
use anyhow::{Context, Result};
use reqwest::{
    header::{HeaderMap, HeaderValue, LINK},
    Client, Method, RequestBuilder, Url,
};
use std::net::IpAddr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pagination {
    Next(u32),
    Complete,
    Missing,
    Invalid,
}

#[derive(Debug)]
pub struct RunnerPage {
    pub runners: Vec<Runner>,
    pub pagination: Pagination,
}

impl RunnerPage {
    pub fn next_page(&self, current_page: u32, per_page: u32) -> Option<u32> {
        match self.pagination {
            Pagination::Next(page) => Some(page),
            Pagination::Complete | Pagination::Invalid => None,
            Pagination::Missing if self.runners.len() >= per_page as usize => {
                current_page.checked_add(1)
            }
            Pagination::Missing => None,
        }
    }
}

#[derive(Clone)]
pub struct GitLabClient {
    client: Client,
    host: String,
}

impl GitLabClient {
    pub fn new(host: String, token: String) -> Result<Self> {
        Self::new_with_insecure_loopback(host, token, cfg!(test))
    }

    /// Builds a client that may use plaintext HTTP only for loopback development endpoints.
    ///
    /// Production callers should use [`Self::new`]. This opt-in exists for local development
    /// servers and test fixtures that cannot provide HTTPS.
    pub fn new_with_insecure_loopback(
        host: String,
        token: String,
        allow_insecure_loopback: bool,
    ) -> Result<Self> {
        let host = normalize_host(&host, allow_insecure_loopback)?;

        let mut headers = reqwest::header::HeaderMap::new();
        let mut auth_value =
            reqwest::header::HeaderValue::from_str(&token).context("Invalid token format")?;
        auth_value.set_sensitive(true);
        headers.insert("private-token", auth_value);

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .default_headers(headers)
            .build()
            .context("Failed to build reqwest client")?;

        Ok(Self { client, host })
    }

    fn request(&self, method: Method, endpoint: &str) -> RequestBuilder {
        let url = format!(
            "{}/api/v4/{}",
            self.host.trim_end_matches('/'),
            endpoint.trim_start_matches('/')
        );
        self.client.request(method, &url)
    }

    pub async fn validate_token(&self) -> Result<()> {
        self.request(Method::GET, "user")
            .send()
            .await
            .context("Failed to send request")?
            .error_for_status()
            .context("GitLab token validation failed")?;

        Ok(())
    }

    async fn fetch_runners_from_endpoint(
        &self,
        endpoint: &str,
        filters: &RunnerFilters,
        page: u32,
        per_page: u32,
    ) -> Result<RunnerPage> {
        let mut request = self
            .request(Method::GET, endpoint)
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

        let response = request.send().await.context("Failed to send request")?;
        let response = response
            .error_for_status()
            .context("GitLab API request failed")?;
        let pagination = pagination_from_headers(response.headers(), response.url(), page);
        let runners = response
            .json::<Vec<Runner>>()
            .await
            .context("Failed to deserialize runners")?;

        Ok(RunnerPage {
            runners,
            pagination,
        })
    }

    pub async fn fetch_available_runners(
        &self,
        filters: &RunnerFilters,
        page: u32,
        per_page: u32,
    ) -> Result<RunnerPage> {
        self.fetch_runners_from_endpoint("runners", filters, page, per_page)
            .await
    }

    pub async fn fetch_all_runners(
        &self,
        filters: &RunnerFilters,
        page: u32,
        per_page: u32,
    ) -> Result<RunnerPage> {
        self.fetch_runners_from_endpoint("runners/all", filters, page, per_page)
            .await
    }

    pub async fn fetch_target_runners(
        &self,
        target: &RunnerTarget,
        filters: &RunnerFilters,
        page: u32,
        per_page: u32,
    ) -> Result<RunnerPage> {
        let endpoint = match target.kind {
            RunnerTargetKind::Group => format!("groups/{}/runners", encode_target_id(&target.id)),
            RunnerTargetKind::Project => {
                format!("projects/{}/runners", encode_target_id(&target.id))
            }
        };

        self.fetch_runners_from_endpoint(&endpoint, filters, page, per_page)
            .await
    }

    pub async fn fetch_group_runners(
        &self,
        group_id: &str,
        filters: &RunnerFilters,
        page: u32,
        per_page: u32,
    ) -> Result<RunnerPage> {
        let target = RunnerTarget {
            kind: RunnerTargetKind::Group,
            id: group_id.to_string(),
            label: None,
        };
        self.fetch_target_runners(&target, filters, page, per_page)
            .await
    }

    pub async fn fetch_project_runners(
        &self,
        project_id: &str,
        filters: &RunnerFilters,
        page: u32,
        per_page: u32,
    ) -> Result<RunnerPage> {
        let target = RunnerTarget {
            kind: RunnerTargetKind::Project,
            id: project_id.to_string(),
            label: None,
        };
        self.fetch_target_runners(&target, filters, page, per_page)
            .await
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

        let response = response
            .error_for_status()
            .context("GitLab API request failed for runner managers")?;

        let managers = response
            .json::<Vec<RunnerManager>>()
            .await
            .context("Failed to deserialize managers")?;
        Ok(managers)
    }
}

fn pagination_from_headers(
    headers: &HeaderMap,
    response_url: &Url,
    current_page: u32,
) -> Pagination {
    if let Some(next_page) = headers.get("x-next-page") {
        let pagination = pagination_from_x_next_page(next_page, current_page);
        if pagination != Pagination::Invalid {
            return pagination;
        }

        return headers
            .get(LINK)
            .map(|link| pagination_from_link(link, response_url, current_page))
            .unwrap_or(Pagination::Invalid);
    }

    headers
        .get(LINK)
        .map(|link| pagination_from_link(link, response_url, current_page))
        .unwrap_or(Pagination::Missing)
}

fn pagination_from_x_next_page(value: &HeaderValue, current_page: u32) -> Pagination {
    let Ok(value) = value.to_str() else {
        return Pagination::Invalid;
    };
    let value = value.trim();
    if value.is_empty() {
        return Pagination::Complete;
    }

    parse_advancing_page(value, current_page)
}

fn pagination_from_link(value: &HeaderValue, response_url: &Url, current_page: u32) -> Pagination {
    let Ok(value) = value.to_str() else {
        return Pagination::Invalid;
    };
    let mut saw_valid_link = false;

    for entry in value.split(',') {
        let Some((target, parameters)) = entry.trim().split_once('>') else {
            continue;
        };
        let Some(target) = target.trim().strip_prefix('<') else {
            continue;
        };
        saw_valid_link = true;

        if !has_next_relation(parameters) {
            continue;
        }

        let Ok(url) = Url::parse(target).or_else(|_| response_url.join(target)) else {
            return Pagination::Invalid;
        };
        let Some(page) = url
            .query_pairs()
            .find_map(|(name, value)| (name == "page").then_some(value))
        else {
            return Pagination::Invalid;
        };

        return parse_advancing_page(&page, current_page);
    }

    if saw_valid_link {
        Pagination::Complete
    } else {
        Pagination::Invalid
    }
}

fn has_next_relation(parameters: &str) -> bool {
    parameters.split(';').any(|parameter| {
        let parameter = parameter.trim();
        let Some(value) = parameter.strip_prefix("rel=") else {
            return false;
        };
        value
            .trim_matches('"')
            .split_ascii_whitespace()
            .any(|relation| relation.eq_ignore_ascii_case("next"))
    })
}

fn parse_advancing_page(value: &str, current_page: u32) -> Pagination {
    match value.parse::<u32>() {
        Ok(page) if page > current_page => Pagination::Next(page),
        Ok(_) | Err(_) => Pagination::Invalid,
    }
}

fn encode_target_id(id: &str) -> String {
    let mut encoded = String::with_capacity(id.len());
    for byte in id.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{:02X}", byte)),
        }
    }
    encoded
}

fn normalize_host(host: &str, allow_insecure_loopback: bool) -> Result<String> {
    let trimmed = host.trim().trim_end_matches('/');
    let normalized = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let url = Url::parse(&normalized).context("GitLab host must be a valid URL")?;

    if url.host_str().is_none() {
        anyhow::bail!("GitLab host must include a hostname");
    }
    if !url.username().is_empty() || url.password().is_some() {
        anyhow::bail!("GitLab host must not include credentials");
    }
    if url.query().is_some() || url.fragment().is_some() {
        anyhow::bail!("GitLab host must not include a query string or fragment");
    }

    match url.scheme() {
        "https" => {}
        "http" if allow_insecure_loopback && is_loopback_host(&url) => {}
        "http" => {
            anyhow::bail!(
                "GitLab host must use HTTPS; HTTP is allowed only for explicitly enabled loopback development"
            )
        }
        scheme => anyhow::bail!("Unsupported GitLab host scheme: {scheme}"),
    }

    Ok(url.as_str().trim_end_matches('/').to_string())
}

fn is_loopback_host(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }

    host.trim_matches(['[', ']'])
        .parse::<IpAddr>()
        .is_ok_and(|address| address.is_loopback())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{Matcher, Server};

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

    #[test]
    fn pagination_headers_distinguish_next_complete_missing_and_invalid() {
        let response_url = Url::parse("https://gitlab.example.com/api/v4/runners?page=1").unwrap();

        let mut headers = HeaderMap::new();
        headers.insert("x-next-page", HeaderValue::from_static("2"));
        assert_eq!(
            pagination_from_headers(&headers, &response_url, 1),
            Pagination::Next(2)
        );

        headers.insert("x-next-page", HeaderValue::from_static(""));
        assert_eq!(
            pagination_from_headers(&headers, &response_url, 1),
            Pagination::Complete
        );

        headers.clear();
        assert_eq!(
            pagination_from_headers(&headers, &response_url, 1),
            Pagination::Missing
        );

        headers.insert("x-next-page", HeaderValue::from_static("not-a-page"));
        assert_eq!(
            pagination_from_headers(&headers, &response_url, 1),
            Pagination::Invalid
        );

        headers.insert("x-next-page", HeaderValue::from_static("1"));
        assert_eq!(
            pagination_from_headers(&headers, &response_url, 1),
            Pagination::Invalid
        );
    }

    #[test]
    fn pagination_uses_link_next_when_x_next_page_is_absent() {
        let response_url = Url::parse("https://gitlab.example.com/api/v4/runners?page=1").unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            LINK,
            HeaderValue::from_static(
                "<https://gitlab.example.com/api/v4/runners?page=2&per_page=100>; rel=\"next\"",
            ),
        );

        assert_eq!(
            pagination_from_headers(&headers, &response_url, 1),
            Pagination::Next(2)
        );
    }

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

    #[test]
    fn test_client_creation_without_scheme() {
        let client = GitLabClient::new("gitlab.example.com".to_string(), "token".to_string());
        assert!(client.is_ok());
    }

    #[test]
    fn strict_client_accepts_https_hosts() {
        let client = GitLabClient::new_with_insecure_loopback(
            "https://gitlab.example.com".to_string(),
            "token".to_string(),
            false,
        );

        assert!(client.is_ok());
    }

    #[test]
    fn strict_client_rejects_http_hosts_before_building_requests() {
        for host in [
            "http://gitlab.example.com",
            "http://10.0.0.7",
            "http://192.168.1.7",
            "http://127.0.0.1:8080",
        ] {
            let error = GitLabClient::new_with_insecure_loopback(
                host.to_string(),
                "token".to_string(),
                false,
            )
            .err()
            .expect("plaintext HTTP should be rejected");

            assert!(
                error.to_string().contains("must use HTTPS"),
                "{host}: {error}"
            );
        }
    }

    #[test]
    fn explicit_development_option_accepts_only_loopback_http() {
        for host in [
            "http://localhost:8080",
            "http://127.0.0.1:8080",
            "http://[::1]:8080",
        ] {
            let client = GitLabClient::new_with_insecure_loopback(
                host.to_string(),
                "token".to_string(),
                true,
            );
            assert!(client.is_ok(), "{host} should be accepted");
        }

        for host in ["http://gitlab.example.com", "http://10.0.0.7"] {
            let client = GitLabClient::new_with_insecure_loopback(
                host.to_string(),
                "token".to_string(),
                true,
            );
            assert!(client.is_err(), "{host} must remain rejected");
        }
    }

    #[test]
    fn strict_client_rejects_unsupported_or_ambiguous_host_urls() {
        for host in [
            "ftp://gitlab.example.com",
            "file:///tmp/gitlab",
            "https://user:password@gitlab.example.com",
            "https://gitlab.example.com?redirect=http://example.com",
            "https://gitlab.example.com/#fragment",
        ] {
            let client = GitLabClient::new_with_insecure_loopback(
                host.to_string(),
                "token".to_string(),
                false,
            );
            assert!(client.is_err(), "{host} must be rejected");
        }
    }

    #[test]
    fn test_request_url_construction() {
        let client = GitLabClient::new(
            "https://gitlab.example.com".to_string(),
            "token".to_string(),
        )
        .unwrap();

        let req = client.request(Method::GET, "runners").build().unwrap();
        assert_eq!(
            req.url().as_str(),
            "https://gitlab.example.com/api/v4/runners"
        );

        let req = client.request(Method::GET, "/runners").build().unwrap();
        assert_eq!(
            req.url().as_str(),
            "https://gitlab.example.com/api/v4/runners"
        );

        let client_with_slash = GitLabClient::new(
            "https://gitlab.example.com/".to_string(),
            "token".to_string(),
        )
        .unwrap();

        let req = client_with_slash
            .request(Method::GET, "runners")
            .build()
            .unwrap();
        assert_eq!(
            req.url().as_str(),
            "https://gitlab.example.com/api/v4/runners"
        );

        let req = client_with_slash
            .request(Method::GET, "/runners")
            .build()
            .unwrap();
        assert_eq!(
            req.url().as_str(),
            "https://gitlab.example.com/api/v4/runners"
        );

        let no_scheme_client =
            GitLabClient::new("gitlab.example.com".to_string(), "token".to_string()).unwrap();

        let req = no_scheme_client
            .request(Method::GET, "runners")
            .build()
            .unwrap();
        assert_eq!(
            req.url().as_str(),
            "https://gitlab.example.com/api/v4/runners"
        );
    }

    #[tokio::test]
    async fn test_validate_token_success() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/api/v4/user")
            .match_header("PRIVATE-TOKEN", "test-token")
            .with_status(200)
            .with_body(r#"{"id":1,"username":"alice"}"#)
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();

        client.validate_token().await.unwrap();

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_validate_token_returns_error_on_401() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/api/v4/user")
            .match_header("PRIVATE-TOKEN", "bad-token")
            .with_status(401)
            .with_body(r#"{"message":"401 Unauthorized"}"#)
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "bad-token".to_string()).unwrap();
        let error = client.validate_token().await.unwrap_err();

        mock.assert_async().await;
        assert!(format!("{:#}", error).contains("401"));
    }

    #[tokio::test]
    async fn test_fetch_group_runners_success() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/api/v4/groups/my-org%2Fplatform/runners")
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
                    "ip_address": "10.0.1.50",
                    "is_shared": false,
                    "status": "online",
                    "name": null,
                    "online": true
                }]"#,
            )
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let filters = RunnerFilters::default();
        let target = group_target("my-org/platform");

        let runners = client
            .fetch_target_runners(&target, &filters, 1, 100)
            .await
            .unwrap();

        mock.assert_async().await;
        assert_eq!(runners.runners.len(), 1);
        assert_eq!(runners.runners[0].id, 12345);
        assert_eq!(runners.runners[0].status, "online");
        assert!(runners.runners[0].tag_list.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_available_runners_success() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/api/v4/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .match_header("PRIVATE-TOKEN", "test-token")
            .with_status(200)
            .with_body("[]")
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let runners = client
            .fetch_available_runners(&RunnerFilters::default(), 1, 100)
            .await
            .unwrap();

        mock.assert_async().await;
        assert!(runners.runners.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_group_runners_with_status_filter() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/api/v4/groups/123/runners")
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
        let target = group_target("123");

        let runners = client
            .fetch_target_runners(&target, &filters, 1, 100)
            .await
            .unwrap();

        mock.assert_async().await;
        assert!(runners.runners.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_group_runners_does_not_send_version_filter() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/api/v4/groups/123/runners")
            .match_query(Matcher::Exact("per_page=100&page=1".to_string()))
            .match_header("PRIVATE-TOKEN", "test-token")
            .with_status(200)
            .with_body("[]")
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();
        let filters = RunnerFilters {
            version_prefix: Some("16.11".to_string()),
            ..Default::default()
        };
        let target = group_target("123");

        let runners = client
            .fetch_target_runners(&target, &filters, 1, 100)
            .await
            .unwrap();

        mock.assert_async().await;
        assert!(runners.runners.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_project_runners_with_type_filter() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/api/v4/projects/my-org%2Fapp/runners")
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
        let target = project_target("my-org/app");

        let _ = client
            .fetch_target_runners(&target, &filters, 1, 100)
            .await
            .unwrap();
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
    async fn test_fetch_runner_managers_returns_error_on_500() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/api/v4/runners/12345/managers")
            .match_header("PRIVATE-TOKEN", "test-token")
            .with_status(500)
            .with_body(r#"{"message":"500 Internal Server Error"}"#)
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "test-token".to_string()).unwrap();

        let result = client.fetch_runner_managers(12345).await;

        mock.assert_async().await;
        assert!(result.is_err());
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(
            err_msg.contains("500"),
            "Error should mention 500, got: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_fetch_target_runners_empty_response() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/api/v4/projects/123/runners")
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
        let target = project_target("123");

        let runners = client
            .fetch_target_runners(&target, &filters, 1, 100)
            .await
            .unwrap();

        mock.assert_async().await;
        assert!(runners.runners.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_target_runners_returns_error_on_401() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/api/v4/groups/123/runners")
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
        let target = group_target("123");

        let result = client.fetch_target_runners(&target, &filters, 1, 100).await;

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
    async fn test_fetch_target_runners_returns_error_on_500() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/api/v4/groups/123/runners")
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
        let target = group_target("123");

        let result = client.fetch_target_runners(&target, &filters, 1, 100).await;

        mock.assert_async().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fetch_target_runners_with_tag_filter() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/api/v4/projects/99/runners")
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
        let target = project_target("99");

        let runners = client
            .fetch_target_runners(&target, &filters, 1, 100)
            .await
            .unwrap();

        mock.assert_async().await;
        assert!(runners.runners.is_empty());
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
