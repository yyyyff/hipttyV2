use serde::{Deserialize, Serialize};

use crate::content::ContentNode;
use crate::poll::Poll;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadSummary {
    pub tid: String,
    pub title: String,
    pub title_color: Option<String>,
    pub author: Option<String>,
    pub author_id: Option<String>,
    pub avatar_url: Option<String>,
    pub last_post: Option<String>,
    pub reply_count: Option<String>,
    pub view_count: Option<String>,
    pub time_create: Option<String>,
    pub time_update: Option<String>,
    pub thread_type: Option<String>,
    pub sticky: bool,
    pub with_pic: bool,
    pub is_new: bool,
    pub is_poll: bool,
    pub max_page: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadList {
    pub threads: Vec<ThreadSummary>,
    pub page: u32,
    pub max_page: u32,
    pub uid_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Post {
    pub pid: String,
    pub floor: u32,
    pub author: String,
    pub uid: Option<String>,
    pub avatar_url: Option<String>,
    pub time: String,
    pub content: Vec<ContentNode>,
    pub poll: Option<Poll>,
    pub page: u32,
    pub warned: bool,
    /// User signature from `div.signatures` when present (reserved for future clients).
    pub signature: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadDetail {
    pub tid: String,
    pub fid: Option<u32>,
    pub title: String,
    pub posts: Vec<Post>,
    pub page: u32,
    pub last_page: u32,
}
