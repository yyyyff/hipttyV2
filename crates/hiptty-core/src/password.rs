use md5::{Digest, Md5};

/// Discuz login password: MD5 of escaped plaintext, or passthrough if already 32-char hex.
pub fn processed_password(password: &str) -> String {
    if password.len() == 32 && password.chars().all(|c| c.is_ascii_hexdigit()) {
        return password.to_string();
    }

    let escaped = password
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('"', "\\\"");

    let digest = Md5::digest(escaped.as_bytes());
    format!("{digest:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_password_plaintext() {
        assert_eq!(processed_password("hello").len(), 32);
    }

    #[test]
    fn md5_password_passthrough() {
        let md5 = "d41d8cd98f00b204e9800998ecf8427e";
        assert_eq!(processed_password(md5), md5);
    }
}
