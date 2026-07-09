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
    /// Post time (datetime only; Discuz `发表于` prefix is stripped at parse).
    pub time: String,
    pub content: Vec<ContentNode>,
    pub poll: Option<Poll>,
    pub page: u32,
    pub warned: bool,
    /// User signature from `div.signatures` when present (reserved for future clients).
    pub signature: Option<String>,
    /// Last editor from Discuz `本帖最后由 … 编辑` notice (often the author).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited_by: Option<String>,
    /// Last edit time from the same notice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<String>,
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
