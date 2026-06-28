use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::thread::ThreadDetail;

/// Write operations aligned with hipda `PostHelper` modes (excluding vote).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum PostAction {
    ReplyThread {
        tid: String,
    },
    ReplyPost {
        tid: String,
        pid: String,
    },
    QuotePost {
        tid: String,
        pid: String,
    },
    NewThread {
        fid: u32,
        type_id: Option<String>,
    },
    EditPost {
        tid: String,
        pid: String,
        fid: u32,
        page: u32,
    },
    QuickDelete {
        tid: String,
        pid: String,
        fid: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrePostInfo {
    pub formhash: String,
    pub uid: Option<String>,
    pub hash: Option<String>,
    pub subject: Option<String>,
    pub quote_text: Option<String>,
    pub type_id: Option<String>,
    pub type_values: BTreeMap<String, String>,
    pub deletable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostResult {
    pub success: bool,
    pub message: String,
    pub tid: Option<String>,
    pub floor: Option<u32>,
    pub detail: Option<ThreadDetail>,
}
