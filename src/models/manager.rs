use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct RunnerManager {
    pub id: u64,
    pub system_id: String,
    pub created_at: String, // Keeping as String for now, will parse to DateTime later or verify serde_json handles it if we use chrono
    pub contacted_at: Option<String>,
    pub ip_address: Option<String>,
    pub status: String,
    pub version: Option<String>,
    pub revision: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manager_deserialization() {
        let json = r#"{
            "id": 67890,
            "system_id": "runner-host-01",
            "created_at": "2024-01-15T10:30:00.000Z",
            "contacted_at": "2024-01-20T14:22:00.000Z",
            "ip_address": "10.0.1.50",
            "status": "online",
            "version": "17.5.0",
            "revision": "abc123def"
        }"#;

        let manager: RunnerManager =
            serde_json::from_str(json).expect("Failed to deserialize manager");

        assert_eq!(manager.id, 67890);
        assert_eq!(manager.system_id, "runner-host-01");
        assert_eq!(manager.status, "online");
        assert_eq!(manager.version, Some("17.5.0".to_string()));
        assert_eq!(manager.ip_address, Some("10.0.1.50".to_string()));
    }

    #[test]
    fn test_manager_with_null_optional_fields() {
        let json = r#"{
            "id": 12345,
            "system_id": "test-host",
            "created_at": "2024-01-15T10:30:00.000Z",
            "contacted_at": null,
            "ip_address": null,
            "status": "never_contacted",
            "version": null,
            "revision": null
        }"#;

        let manager: RunnerManager =
            serde_json::from_str(json).expect("Failed to deserialize manager");

        assert_eq!(manager.id, 12345);
        assert_eq!(manager.status, "never_contacted");
        assert!(manager.contacted_at.is_none());
        assert!(manager.ip_address.is_none());
        assert!(manager.version.is_none());
    }
}
