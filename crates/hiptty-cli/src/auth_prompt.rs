use std::io::IsTerminal;

use dialoguer::{Input, Password, Select};
use hiptty_core::{AdapterError, AdapterResult, Credentials};

/// Discuz security questions (hipda `pref_login_question_list_*`).
const SECURITY_QUESTIONS: &[(&str, &str)] = &[
    ("0", "无安全提问"),
    ("1", "母亲的名字"),
    ("2", "爷爷的名字"),
    ("3", "父亲出生的城市"),
    ("4", "您其中一位老师的名字"),
    ("5", "您个人计算机的型号"),
    ("6", "您最喜欢的餐馆名称"),
    ("7", "驾驶执照的最后四位数字"),
];

pub fn gather_credentials(
    username: Option<String>,
    password: Option<String>,
    question_id: Option<String>,
    answer: Option<String>,
) -> AdapterResult<Credentials> {
    let interactive = std::io::stdin().is_terminal();

    let username = match username {
        Some(u) if !u.is_empty() => u,
        _ if interactive => Input::new()
            .with_prompt("用户名")
            .interact_text()
            .map_err(prompt_error)?,
        _ => {
            return Err(AdapterError::InvalidInput(
                "username required (omit flag to prompt interactively)".into(),
            ));
        }
    };

    let password = match password {
        Some(p) if !p.is_empty() => p,
        _ if interactive => Password::new()
            .with_prompt("密码")
            .interact()
            .map_err(prompt_error)?,
        _ => {
            return Err(AdapterError::InvalidInput(
                "password required (prompt interactively or use --password / HIPTTY_PASSWORD)"
                    .into(),
            ));
        }
    };

    let (security_question, security_answer) = resolve_security(question_id, answer, interactive)?;

    Ok(Credentials {
        username,
        password,
        security_question,
        security_answer,
    })
}

fn resolve_security(
    question_id: Option<String>,
    answer: Option<String>,
    interactive: bool,
) -> AdapterResult<(Option<String>, Option<String>)> {
    let question_id = match question_id {
        Some(id) if !id.is_empty() => id,
        _ if interactive => {
            let labels: Vec<&str> = SECURITY_QUESTIONS.iter().map(|(_, label)| *label).collect();
            let idx = Select::new()
                .with_prompt("安全提问")
                .items(&labels)
                .default(0)
                .interact()
                .map_err(prompt_error)?;
            SECURITY_QUESTIONS[idx].0.to_string()
        }
        _ => "0".to_string(),
    };

    if question_id == "0" {
        return Ok((Some("0".into()), Some(String::new())));
    }

    let security_answer = match answer {
        Some(a) if !a.is_empty() => a,
        _ if interactive => Input::new()
            .with_prompt("安全问题答案")
            .interact_text()
            .map_err(prompt_error)?,
        _ => {
            return Err(AdapterError::InvalidInput(
                "security answer required when question-id > 0 (use --answer or prompt)".into(),
            ));
        }
    };

    Ok((Some(question_id), Some(security_answer)))
}

fn prompt_error(err: dialoguer::Error) -> AdapterError {
    AdapterError::InvalidInput(format!("prompt cancelled: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_interactive_requires_username() {
        let err = gather_credentials(None, Some("pw".into()), None, None).unwrap_err();
        assert!(matches!(err, AdapterError::InvalidInput(_)));
    }

    #[test]
    fn explicit_credentials_skip_prompt() {
        let creds = gather_credentials(
            Some("alice".into()),
            Some("secret".into()),
            Some("0".into()),
            None,
        )
        .expect("credentials");

        assert_eq!(creds.username, "alice");
        assert_eq!(creds.security_question.as_deref(), Some("0"));
    }

    #[test]
    fn security_question_requires_answer_when_non_interactive() {
        let err = gather_credentials(
            Some("alice".into()),
            Some("secret".into()),
            Some("1".into()),
            None,
        )
        .unwrap_err();
        assert!(matches!(err, AdapterError::InvalidInput(_)));
    }
}
