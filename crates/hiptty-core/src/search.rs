use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    pub author: Option<String>,
    pub fid: Option<String>,
    pub fulltext: bool,
    pub page: u32,
}

impl SearchQuery {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            author: None,
            fid: None,
            fulltext: false,
            page: 1,
        }
    }
}
