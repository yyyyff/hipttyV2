use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub const AVATAR_PATH_MARKER: &str = "uc_server/data/avatar/";

const CACHE_TTL: Duration = Duration::from_secs(3 * 24 * 60 * 60);
const NOT_FOUND_TTL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvatarDiskEntry {
    Bytes(Vec<u8>),
    NotFound,
}

#[derive(Debug)]
pub struct AvatarDiskCache {
    dir: PathBuf,
}

impl AvatarDiskCache {
    pub fn new(config_dir: &Path) -> io::Result<Self> {
        let dir = config_dir.join("cache/avatars");
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    pub fn cache_file_name(url: &str) -> Option<String> {
        let idx = url.find(AVATAR_PATH_MARKER)?;
        let rest = &url[idx + AVATAR_PATH_MARKER.len()..];
        let end = rest.find('?').unwrap_or(rest.len());
        let path = &rest[..end];
        if path.is_empty() {
            return None;
        }
        Some(path.replace('/', "_"))
    }

    pub fn load(&self, url: &str) -> Option<AvatarDiskEntry> {
        let name = Self::cache_file_name(url)?;
        let path = self.dir.join(&name);
        if !path.is_file() {
            return None;
        }
        let modified = path.metadata().ok()?.modified().ok()?;
        let age = SystemTime::now().duration_since(modified).ok()?;
        let len = path.metadata().ok()?.len();
        if len == 0 {
            if age > NOT_FOUND_TTL {
                let _ = fs::remove_file(&path);
                return None;
            }
            return Some(AvatarDiskEntry::NotFound);
        }
        if age > CACHE_TTL {
            let _ = fs::remove_file(&path);
            return None;
        }
        let bytes = fs::read(&path).ok()?;
        if bytes.is_empty() {
            return Some(AvatarDiskEntry::NotFound);
        }
        Some(AvatarDiskEntry::Bytes(bytes))
    }

    pub fn save_bytes(&self, url: &str, bytes: &[u8]) -> io::Result<()> {
        let Some(name) = Self::cache_file_name(url) else {
            return Ok(());
        };
        let path = self.dir.join(name);
        fs::write(path, bytes)
    }

    pub fn save_not_found(&self, url: &str) -> io::Result<()> {
        let Some(name) = Self::cache_file_name(url) else {
            return Ok(());
        };
        let path = self.dir.join(name);
        fs::write(path, [])
    }

    pub fn clear(&self, url: &str) -> io::Result<()> {
        let Some(name) = Self::cache_file_name(url) else {
            return Ok(());
        };
        let path = self.dir.join(name);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_file_name_from_avatar_url() {
        let url =
            "https://img02.4d4y.com/uc_server/data/avatar/000/40/54/01/405451_avatar_middle.jpg";
        assert_eq!(
            AvatarDiskCache::cache_file_name(url),
            Some("000_40_54_01_405451_avatar_middle.jpg".into())
        );
    }

    #[test]
    fn round_trip_bytes_and_not_found() {
        let dir = std::env::temp_dir().join(format!("hiptty-avatar-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let cache = AvatarDiskCache::new(&dir).expect("cache");
        let url = "https://img02.4d4y.com/uc_server/data/avatar/000/00/00/00/1_avatar_middle.jpg";

        cache.save_bytes(url, b"jpeg").expect("save");
        assert!(matches!(
            cache.load(url),
            Some(AvatarDiskEntry::Bytes(b)) if b == b"jpeg"
        ));

        cache.save_not_found(url).expect("404");
        assert!(matches!(cache.load(url), Some(AvatarDiskEntry::NotFound)));

        let _ = fs::remove_dir_all(dir);
    }
}
