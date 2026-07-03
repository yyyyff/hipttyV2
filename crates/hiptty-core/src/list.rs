use serde::{Deserialize, Serialize};

use crate::thread::ThreadSummary;

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

/// Map a generic list row into [`ThreadSummary`] for reusing the thread list widget.
pub fn list_item_to_thread_summary(item: &ListItem) -> ThreadSummary {
    ThreadSummary {
        tid: item.tid.clone().unwrap_or_default(),
        title: item
            .title
            .clone()
            .filter(|t| !t.trim().is_empty())
            .or_else(|| item.info.clone())
            .unwrap_or_else(|| "(无标题)".into()),
        title_color: None,
        author: item.author.clone(),
        author_id: item.uid.clone(),
        avatar_url: item.avatar_url.clone(),
        last_post: None,
        reply_count: None,
        view_count: None,
        time_create: item.time.clone(),
        time_update: item.time.clone(),
        thread_type: item.forum.clone(),
        sticky: false,
        with_pic: false,
        is_new: item.is_new,
        is_poll: false,
        max_page: 1,
    }
}
