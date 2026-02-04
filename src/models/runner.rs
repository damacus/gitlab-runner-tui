use super::manager::RunnerManager;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Runner {
    pub id: u64,
    pub runner_type: String,
    pub active: bool,
    pub paused: bool,
    pub description: Option<String>,
    pub created_at: Option<String>,
    pub ip_address: Option<String>,
    pub is_shared: bool,
    pub status: String,
    pub version: Option<String>,
    pub revision: Option<String>,
    pub tag_list: Vec<String>,
    #[serde(default)]
    pub managers: Vec<RunnerManager>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct RunnerFilters {
    pub tag_list: Option<Vec<String>>,
    pub status: Option<String>,
    pub version_prefix: Option<String>,
    pub runner_type: Option<String>,
    pub paused: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::manager::RunnerManager;

    #[test]
    fn test_runner_deserialization() {
        let json = r#"{
            "id": 12345,
            "runner_type": "group_type",
            "active": true,
            "paused": false,
            "description": "Production ALM Runner",
            "created_at": "2024-01-15T10:30:00.000Z",
            "ip_address": "10.0.1.50",
            "is_shared": false,
            "status": "online",
            "version": "17.5.0",
            "revision": "abc123def",
            "tag_list": ["alm", "production", "linux"],
            "managers": []
        }"#;

        let runner: Runner = serde_json::from_str(json).expect("Failed to deserialize runner");

        assert_eq!(runner.id, 12345);
        assert_eq!(runner.runner_type, "group_type");
        assert!(runner.active);
        assert_eq!(runner.status, "online");
        assert_eq!(runner.tag_list.len(), 3);
    }

    #[test]
    fn test_runner_with_managers() {
        let json = r#"{
            "id": 12345,
            "runner_type": "instance_type",
            "active": true,
            "paused": false,
            "description": null,
            "created_at": "2024-01-15T10:30:00.000Z",
            "ip_address": null,
            "is_shared": true,
            "status": "online",
            "version": "17.5.0",
            "revision": null,
            "tag_list": ["shared"],
            "managers": [{
                "id": 1,
                "system_id": "host-1",
                "created_at": "2024-01-15T10:30:00.000Z",
                "contacted_at": "2024-01-20T14:22:00.000Z",
                "ip_address": "10.0.1.1",
                "status": "online",
                "version": "17.5.0",
                "revision": "abc"
            }]
        }"#;

        let runner: Runner = serde_json::from_str(json).expect("Failed to deserialize runner");

        assert_eq!(runner.id, 12345);
        assert_eq!(runner.runner_type, "instance_type");
        assert!(runner.is_shared);
        assert_eq!(runner.managers.len(), 1);
        assert_eq!(runner.managers[0].system_id, "host-1");
    }

    #[test]
    fn test_runner_all_status_variants() {
        for status in &["online", "offline", "stale", "never_contacted"] {
            let json = format!(
                r#"{{
                    "id": 1,
                    "runner_type": "group_type",
                    "active": true,
                    "paused": false,
                    "description": null,
                    "created_at": "2024-01-15T10:30:00.000Z",
                    "ip_address": null,
                    "is_shared": false,
                    "status": "{}",
                    "version": null,
                    "revision": null,
                    "tag_list": [],
                    "managers": []
                }}"#,
                status
            );

            let runner: Runner = serde_json::from_str(&json).expect("Failed to deserialize");
            assert_eq!(runner.status, *status);
        }
    }

    #[test]
    fn test_runner_all_type_variants() {
        for runner_type in &["instance_type", "group_type", "project_type"] {
            let json = format!(
                r#"{{
                    "id": 1,
                    "runner_type": "{}",
                    "active": true,
                    "paused": false,
                    "description": null,
                    "created_at": "2024-01-15T10:30:00.000Z",
                    "ip_address": null,
                    "is_shared": false,
                    "status": "online",
                    "version": null,
                    "revision": null,
                    "tag_list": [],
                    "managers": []
                }}"#,
                runner_type
            );

            let runner: Runner = serde_json::from_str(&json).expect("Failed to deserialize");
            assert_eq!(runner.runner_type, *runner_type);
        }
    }

    #[test]
    fn test_runner_filters_default() {
        let filters = RunnerFilters::default();
        assert!(filters.tag_list.is_none());
        assert!(filters.status.is_none());
        assert!(filters.version_prefix.is_none());
        assert!(filters.runner_type.is_none());
        assert!(filters.paused.is_none());
    }

    #[test]
    fn test_runner_filters_with_tags() {
        let filters = RunnerFilters {
            tag_list: Some(vec!["alm".to_string(), "production".to_string()]),
            status: None,
            version_prefix: None,
            runner_type: None,
            paused: None,
        };

        let tags = filters.tag_list.unwrap();
        assert_eq!(tags.len(), 2);
        assert!(tags.contains(&"alm".to_string()));
    }

    fn create_test_runner(id: u64, status: &str, manager_status: Option<&str>) -> Runner {
        let managers = match manager_status {
            Some(s) => vec![RunnerManager {
                id: id * 10,
                system_id: format!("host-{}", id),
                created_at: "2024-01-15T10:30:00.000Z".to_string(),
                contacted_at: Some("2024-01-20T14:22:00.000Z".to_string()),
                ip_address: Some("10.0.1.1".to_string()),
                status: s.to_string(),
                version: Some("17.5.0".to_string()),
                revision: None,
            }],
            None => vec![],
        };

        Runner {
            id,
            runner_type: "group_type".to_string(),
            active: true,
            paused: false,
            description: None,
            created_at: Some("2024-01-15T10:30:00.000Z".to_string()),
            ip_address: None,
            is_shared: false,
            status: status.to_string(),
            version: Some("17.5.0".to_string()),
            revision: None,
            tag_list: vec!["alm".to_string()],
            managers,
        }
    }

    #[test]
    fn test_filter_runners_with_online_managers() {
        let runners = [
            create_test_runner(1, "online", Some("online")),
            create_test_runner(2, "online", Some("offline")),
            create_test_runner(3, "online", None),
        ];

        // Filter for runners with online managers
        let online: Vec<_> = runners
            .iter()
            .filter(|r| {
                r.managers
                    .first()
                    .map(|m| m.status == "online")
                    .unwrap_or(false)
            })
            .collect();

        assert_eq!(online.len(), 1);
        assert_eq!(online[0].id, 1);
    }

    #[test]
    fn test_filter_runners_without_managers() {
        let runners = [
            create_test_runner(1, "online", Some("online")),
            create_test_runner(2, "online", None),
            create_test_runner(3, "never_contacted", None),
        ];

        let empty: Vec<_> = runners.iter().filter(|r| r.managers.is_empty()).collect();

        assert_eq!(empty.len(), 2);
        assert!(empty.iter().any(|r| r.id == 2));
        assert!(empty.iter().any(|r| r.id == 3));
    }
}
