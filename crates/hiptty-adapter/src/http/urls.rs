use hiptty_core::{SearchQuery, FORUM_BASE_PATH, FORUM_SERVER, IMAGE_HOST};

use crate::http::decode::gbk_urlencode;

pub const USER_AGENT: &str = "hiptty/0.1.0";
pub const AVATAR_SUFFIX: &str = "_avatar_middle.jpg";

pub struct ForumUrls {
    pub base: String,
    pub image_base: String,
}

impl ForumUrls {
    pub fn default_4d4y() -> Self {
        Self {
            base: format!("{FORUM_SERVER}{FORUM_BASE_PATH}"),
            image_base: format!("{IMAGE_HOST}{FORUM_BASE_PATH}"),
        }
    }

    pub fn thread_list(&self, fid: u32, page: u32) -> String {
        format!("{}forumdisplay.php?fid={fid}&page={page}", self.base)
    }

    pub fn thread_detail(&self, tid: &str, page: u32) -> String {
        format!("{}viewthread.php?tid={tid}&page={page}", self.base)
    }

    pub fn thread_last_page(&self, tid: &str) -> String {
        format!(
            "{}redirect.php?goto=lastpost&from=fastpost&tid={tid}",
            self.base
        )
    }

    pub fn thread_at_post(&self, tid: &str, pid: &str) -> String {
        format!(
            "{}redirect.php?goto=findpost&pid={pid}&ptid={tid}",
            self.base
        )
    }

    pub fn goto_post(&self, pid: &str) -> String {
        format!("{}gotopost.php?pid={pid}", self.base)
    }

    pub fn login_form(&self) -> String {
        format!("{}logging.php?action=login", self.base)
    }

    pub fn login_submit(&self) -> String {
        format!(
            "{}logging.php?action=login&loginsubmit=yes&inajax=1",
            self.base
        )
    }

    pub fn my_replies(&self, page: u32) -> String {
        let mut url = format!("{}my.php?item=posts", self.base);
        if page > 1 {
            url.push_str(&format!("&page={page}"));
        }
        url
    }

    pub fn my_threads(&self, page: u32) -> String {
        let mut url = format!("{}my.php?item=threads", self.base);
        if page > 1 {
            url.push_str(&format!("&page={page}"));
        }
        url
    }

    pub fn favorites(&self, item: &str, page: u32) -> String {
        let mut url = format!("{}my.php?item={item}&type=thread", self.base);
        if page > 1 {
            url.push_str(&format!("&page={page}"));
        }
        url
    }

    pub fn pm_list(&self) -> String {
        format!("{}pm.php?filter=privatepm", self.base)
    }

    pub fn pm_thread(&self, uid: &str) -> String {
        format!("{}pm.php?daterange=5&uid={uid}", self.base)
    }

    pub fn pm_new(&self) -> String {
        format!("{}pm.php?filter=newpm", self.base)
    }

    pub fn pm_check_new(&self) -> String {
        format!("{}pm.php?checknewpm", self.base)
    }

    pub fn pm_delete(&self, uid: &str) -> String {
        format!("{}pm.php?action=del&uid={uid}&filter=privatepm", self.base)
    }

    pub fn notifications(&self) -> String {
        format!("{}notice.php", self.base)
    }

    pub fn user_info(&self, uid: &str) -> String {
        format!("{}space.php?uid={uid}", self.base)
    }

    pub fn blacklist(&self) -> String {
        format!("{}pm.php?action=viewblack", self.base)
    }

    pub fn search(&self, query: &SearchQuery) -> String {
        let srchtype = if query.fulltext { "fulltext" } else { "title" };
        let srchtxt = gbk_urlencode(&query.query);
        let srchuname = query
            .author
            .as_ref()
            .map(|author| gbk_urlencode(author))
            .unwrap_or_default();
        let fid = query.fid.as_deref().unwrap_or("0");
        let mut url = format!(
            "{}search.php?srchtype={srchtype}&srchtxt={srchtxt}&searchsubmit=true&st=on&srchuname={srchuname}&srchfilter=all&srchfrom=0&before=&orderby=lastpost&ascdesc=desc&srchfid%5B0%5D={fid}",
            self.base
        );
        if query.page > 1 {
            url.push_str(&format!("&page={}", query.page));
        }
        url
    }

    pub fn new_posts(&self, search_id: Option<&str>, page: u32) -> String {
        let mut url = if let Some(id) = search_id {
            format!(
                "{}search.php?searchid={id}&orderby=lastpost&ascdesc=desc&searchsubmit=yes",
                self.base
            )
        } else {
            format!("{}search.php?srchfrom=86400&searchsubmit=yes", self.base)
        };
        if page > 1 {
            url.push_str(&format!("&page={page}"));
        }
        url
    }

    pub fn avatar_by_uid(&self, uid: &str) -> Option<String> {
        const AVATAR_BASE: &str = "000000000";
        if uid.is_empty()
            || !uid.chars().all(|c| c.is_ascii_digit())
            || uid.len() > AVATAR_BASE.len()
        {
            return None;
        }

        let full_uid = format!("{}{}", &AVATAR_BASE[..AVATAR_BASE.len() - uid.len()], uid);
        Some(format!(
            "{}uc_server/data/avatar/{}/{}/{}/{}{}",
            self.image_base,
            &full_uid[0..3],
            &full_uid[3..5],
            &full_uid[5..7],
            &full_uid[7..9],
            AVATAR_SUFFIX
        ))
    }
}
