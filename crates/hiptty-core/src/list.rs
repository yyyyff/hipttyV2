use serde::{Deserialize, Serialize};

/// Generic list item for PM, notifications, search results, favorites, etc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListItem {
    pub tid: Option<String>,
    pub pid: Option<String>,
    pub uid: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub avatar_url: Option<String>,
    pub forum: Option<String>,
    pub time: Option<String>,
    pub info: Option<String>,
    pub is_new: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimpleList {
    pub items: Vec<ListItem>,
    pub page: u32,
    pub max_page: u32,
    pub search_id: Option<String>,
}
