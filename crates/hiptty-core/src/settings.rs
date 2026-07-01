use serde::{Deserialize, Serialize};

use crate::forum::DEFAULT_FORUM_IDS;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub theme: Theme,
    #[serde(default = "default_forums")]
    pub default_forums: [u32; 3],
}

fn default_forums() -> [u32; 3] {
    [
        DEFAULT_FORUM_IDS[0],
        DEFAULT_FORUM_IDS[1],
        DEFAULT_FORUM_IDS[2],
    ]
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            default_forums: default_forums(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCredentials {
    pub username: String,
    pub password_md5: String,
    #[serde(default)]
    pub security_question: String,
    #[serde(default)]
    pub security_answer: String,
}

impl StoredCredentials {
    pub fn to_login_credentials(&self) -> crate::Credentials {
        crate::Credentials {
            username: self.username.clone(),
            password: self.password_md5.clone(),
            security_question: if self.security_question.is_empty() {
                Some("0".into())
            } else {
                Some(self.security_question.clone())
            },
            security_answer: Some(self.security_answer.clone()),
        }
    }

    pub fn from_login(
        username: String,
        password_plain: &str,
        question: &str,
        answer: &str,
    ) -> Self {
        Self {
            username,
            password_md5: crate::password::processed_password(password_plain),
            security_question: question.to_string(),
            security_answer: answer.to_string(),
        }
    }
}
