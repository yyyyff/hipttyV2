use crate::app::Page;

#[derive(Debug, Default)]
pub struct NavStack {
    pages: Vec<Page>,
}

impl NavStack {
    pub fn push(&mut self, page: Page) {
        self.pages.push(page);
    }

    pub fn pop(&mut self) -> Option<Page> {
        self.pages.pop()
    }

    pub fn clear(&mut self) {
        self.pages.clear();
    }
}

pub fn navigate_to(app: &mut crate::app::App, page: Page) {
    let from = app.page;
    if from != page && !matches!(from, Page::Startup | Page::Login) {
        app.nav_stack.push(from);
    }
    app.page = page;
}

pub fn navigate_back(app: &mut crate::app::App) -> bool {
    if let Some(page) = app.nav_stack.pop() {
        app.page = page;
        return true;
    }
    if !matches!(app.page, Page::Startup | Page::Login | Page::ThreadFeed) {
        app.page = Page::ThreadFeed;
        return true;
    }
    false
}