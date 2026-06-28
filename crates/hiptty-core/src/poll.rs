use serde::{Deserialize, Serialize};

/// Read-only poll data parsed from thread detail (no vote submission).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Poll {
    pub title: String,
    pub footer: Option<String>,
    pub max_answers: u32,
    pub options: Vec<PollOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollOption {
    pub id: String,
    pub label: String,
    pub votes: Option<u32>,
    pub percent: Option<String>,
}
