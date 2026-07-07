use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "assets/avatar/"]
struct AvatarAssets;

pub fn noavatar_bytes() -> Option<Vec<u8>> {
    AvatarAssets::get("noavatar.jpg").map(|file| file.data.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_noavatar_exists() {
        let bytes = noavatar_bytes().expect("noavatar.jpg");
        assert!(bytes.len() > 100);
    }
}
