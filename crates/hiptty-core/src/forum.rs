use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Forum {
    pub id: u32,
    pub name: &'static str,
}

/// Forum list from hipda `HiUtils.FORUMS`.
pub const FORUMS: &[Forum] = &[
    Forum {
        id: 2,
        name: "Discovery",
    },
    Forum {
        id: 6,
        name: "Buy & Sell",
    },
    Forum {
        id: 7,
        name: "Geek Talks",
    },
    Forum {
        id: 59,
        name: "E-INK",
    },
    Forum {
        id: 12,
        name: "PalmOS",
    },
    Forum {
        id: 57,
        name: "疑似机器人",
    },
    Forum {
        id: 63,
        name: "已完成交易",
    },
    Forum {
        id: 62,
        name: "Joggler",
    },
    Forum {
        id: 5,
        name: "站务与公告",
    },
    Forum {
        id: 9,
        name: "Smartphone",
    },
    Forum {
        id: 56,
        name: "iPhone, iPod Touch，iPad",
    },
    Forum {
        id: 60,
        name: "Android, Chrome, & Google",
    },
    Forum {
        id: 14,
        name: "Windows Mobile，PocketPC，HPC",
    },
    Forum {
        id: 22,
        name: "麦客爱苹果",
    },
    Forum {
        id: 50,
        name: "DC,NB,MP3,Gadgets",
    },
    Forum {
        id: 24,
        name: "意欲蔓延",
    },
    Forum {
        id: 23,
        name: "随笔与个人文集",
    },
    Forum {
        id: 25,
        name: "吃喝玩乐",
    },
    Forum {
        id: 51,
        name: "La Femme",
    },
    Forum {
        id: 65,
        name: "改版建议",
    },
    Forum {
        id: 64,
        name: "只讨论2.0",
    },
];

pub const DEFAULT_FORUM_IDS: &[u32] = &[2, 6, 7];

pub const FORUM_SERVER: &str = "https://www.4d4y.com";
pub const FORUM_BASE_PATH: &str = "/forum/";
pub const IMAGE_HOST: &str = "https://img02.4d4y.com";
pub const COOKIE_DOMAIN: &str = "4d4y.com";

pub fn forum_name(fid: u32) -> Option<&'static str> {
    FORUMS.iter().find(|f| f.id == fid).map(|f| f.name)
}

pub fn is_valid_forum(fid: u32) -> bool {
    FORUMS.iter().any(|f| f.id == fid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_forums_exist() {
        for &fid in DEFAULT_FORUM_IDS {
            assert!(is_valid_forum(fid), "fid {fid} should be valid");
        }
    }

    #[test]
    fn discovery_forum_name() {
        assert_eq!(forum_name(2), Some("Discovery"));
    }
}
