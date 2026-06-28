pub mod content;
pub mod error;
pub mod forum;
pub mod list;
pub mod poll;
pub mod post;
pub mod search;
pub mod session;
pub mod thread;
pub mod user;

pub use content::{ContentNode, ContentSpan, Style, TextSpan};
pub use error::{AdapterError, AdapterResult, ErrorCode};
pub use forum::{
    Forum, COOKIE_DOMAIN, DEFAULT_FORUM_IDS, FORUMS, FORUM_BASE_PATH, FORUM_SERVER, IMAGE_HOST,
};
pub use list::{ListItem, SimpleList};
pub use poll::{Poll, PollOption};
pub use post::{PostAction, PostResult, PrePostInfo};
pub use search::SearchQuery;
pub use session::{Credentials, SessionInfo};
pub use thread::{Post, ThreadDetail, ThreadList, ThreadSummary};
pub use user::UserInfo;
