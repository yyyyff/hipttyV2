/// Discuz security questions (hipda `pref_login_question_list_*`).
pub const SECURITY_QUESTIONS: &[(&str, &str)] = &[
    ("0", "无安全提问"),
    ("1", "母亲的名字"),
    ("2", "爷爷的名字"),
    ("3", "父亲出生的城市"),
    ("4", "您其中一位老师的名字"),
    ("5", "您个人计算机的型号"),
    ("6", "您最喜欢的餐馆名称"),
    ("7", "驾驶执照的最后四位数字"),
];

pub fn security_question_label(id: &str) -> &str {
    SECURITY_QUESTIONS
        .iter()
        .find(|(qid, _)| *qid == id)
        .map(|(_, label)| *label)
        .unwrap_or("无安全提问")
}
