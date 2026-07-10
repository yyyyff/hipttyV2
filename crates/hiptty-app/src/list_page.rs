use hiptty_core::{ListItem, SearchQuery};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListPageKind {
    PmList,
    Notifications,
    Search,
    MyThreads,
    MyReplies,
    Favorites,
}

impl ListPageKind {
    pub fn breadcrumb(self) -> &'static str {
        match self {
            Self::PmList => "私信",
            Self::Notifications => "通知",
            Self::Search => "搜索",
            Self::MyThreads => "我的帖子",
            Self::MyReplies => "我的回复",
            Self::Favorites => "我的收藏",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ListPageState {
    pub kind: Option<ListPageKind>,
    pub items: Vec<ListItem>,
    pub selected: usize,
    pub scroll_lines: u16,
    pub page: u32,
    pub max_page: u32,
    pub search_id: Option<String>,
    pub search_query: String,
    pub loading: bool,
    pub error: Option<String>,
}

impl ListPageState {
    pub fn reset_for(&mut self, kind: ListPageKind) {
        self.kind = Some(kind);
        self.items.clear();
        self.selected = 0;
        self.scroll_lines = 0;
        self.page = 0;
        self.max_page = 0;
        self.search_id = None;
        self.loading = false;
        self.error = None;
    }

    pub fn search_query_for(&self, fid: u32) -> SearchQuery {
        let mut query = SearchQuery::new(self.search_query.clone());
        query.fid = Some(fid.to_string());
        query.page = self.page.max(1);
        query
    }
}

#[derive(Debug, Clone, Default)]
pub struct PmThreadState {
    pub peer_uid: String,
    pub peer_name: String,
    pub messages: Vec<ListItem>,
    pub selected: usize,
    pub scroll_lines: u16,
    pub loading: bool,
    pub error: Option<String>,
}

impl PmThreadState {
    pub fn reset(&mut self, uid: String, name: String) {
        self.peer_uid = uid;
        self.peer_name = name;
        self.messages.clear();
        self.selected = 0;
        self.scroll_lines = 0;
        self.loading = true;
        self.error = None;
    }
}
