use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct User {
    pub username: String,
    pub name: String,
    pub avatar_url: Option<String>,
    pub bot: bool,
    pub can_create_group: bool,
    pub can_create_project: bool,
    pub last_sign_in_at: Option<String>,
    pub locked: bool,
    pub state: String,
}
