use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserInfo {
    pub uid: String,
    pub username: String,
    pub avatar_url: Option<String>,
    pub online: bool,
    pub detail: String,
}
