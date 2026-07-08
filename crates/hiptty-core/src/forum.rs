use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Forum {
    pub id: u32,
    pub name: &'static str,
}

/// Forum list from hipda `HiUtils.FORUMS`, synced with 4D4Y (2026-07).
pub const FORUMS: &[Forum] = &[
    Forum {
        id: 6,
        name: "Buy & Sell 交易服务区",
    },
    Forum {
        id: 63,
        name: "已完成交易",
    },
    Forum {
        id: 7,
        name: "Geek Talks · 奇客怪谈",
    },
    Forum {
        id: 62,
        name: "Joggler",
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
        id: 12,
        name: "PalmOS ，Treo",
    },
    Forum {
        id: 40,
        name: "Palm芝麻宝典",
    },
    Forum {
        id: 22,
        name: "麦客爱苹果",
    },
    Forum {
        id: 14,
        name: "Windows Mobile，PocketPC，HPC",
    },
    Forum {
        id: 50,
        name: "DC,NB,MP3,Gadgets...",
    },
    Forum {
        id: 2,
        name: "Discovery",
    },
    Forum {
        id: 70,
        name: "俄乌战争",
    },
    Forum {
        id: 64,
        name: "只讨论2.0",
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

/// Child forums shown under a parent in the forum picker (4D4Y hierarchy).
pub fn forum_children(parent_fid: u32) -> &'static [u32] {
    match parent_fid {
        6 => &[63][..],
        7 => &[62][..],
        12 => &[40][..],
        2 => &[70, 64][..],
        _ => &[],
    }
}

/// Selectable forums for the `f` picker: sub-forums of `current_fid` (if any), then the rest
/// excluding the three default tabs and any sub-forums already listed above.
pub fn forum_picker_fids(current_fid: u32, default_forums: &[u32; 3]) -> Vec<u32> {
    let subforums = forum_children(current_fid);
    let mut excluded: std::collections::HashSet<u32> = default_forums.iter().copied().collect();
    excluded.extend(subforums.iter().copied());

    let mut fids: Vec<u32> = subforums.iter().copied().collect();
    for forum in FORUMS {
        if !excluded.contains(&forum.id) {
            fids.push(forum.id);
        }
    }
    fids
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

    #[test]
    fn removed_forums_are_invalid() {
        for fid in [5, 57, 59, 65] {
            assert!(!is_valid_forum(fid), "fid {fid} should be removed");
        }
    }

    #[test]
    fn buy_and_sell_lists_completed_trades_subforum() {
        assert_eq!(forum_children(6), &[63]);
    }

    #[test]
    fn forum_picker_excludes_defaults_and_listed_subforums() {
        let defaults = [2, 6, 7];
        let fids = forum_picker_fids(6, &defaults);
        assert_eq!(fids.first().copied(), Some(63));
        assert!(!fids.contains(&6));
        assert!(!fids.contains(&2));
        assert!(!fids.contains(&7));
    }

    #[test]
    fn forum_picker_without_subforums_lists_all_non_defaults() {
        let defaults = [2, 6, 7];
        let fids = forum_picker_fids(24, &defaults);
        assert!(!fids.contains(&2));
        assert!(!fids.contains(&6));
        assert!(!fids.contains(&7));
        assert!(fids.contains(&24));
        assert!(fids.contains(&63));
    }
}