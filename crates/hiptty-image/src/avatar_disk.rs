use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub const AVATAR_PATH_MARKER: &str = "uc_server/data/avatar/";

const CACHE_TTL: Duration = Duration::from_secs(3 * 24 * 60 * 60);
const NOT_FOUND_TTL: Duration = Duration::from_secs(24 * 60 * 60);
/// Total size budget for on-disk avatar files (bytes + empty not-found markers).
const MAX_DISK_BYTES: u64 = 64 * 1024 * 1024;
/// Hard cap on number of avatar cache files.
const MAX_DISK_FILES: usize = 2_000;

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
        let cache = Self { dir };
        let _ = cache.purge();
        Ok(cache)
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
        fs::write(path, bytes)?;
        let _ = self.enforce_budget();
        Ok(())
    }

    pub fn save_not_found(&self, url: &str) -> io::Result<()> {
        let Some(name) = Self::cache_file_name(url) else {
            return Ok(());
        };
        let path = self.dir.join(name);
        fs::write(path, [])?;
        let _ = self.enforce_budget();
        Ok(())
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

    /// Remove TTL-expired files, then shrink to budget by oldest mtime.
    pub fn purge(&self) -> io::Result<()> {
        self.purge_expired()?;
        self.enforce_budget()
    }

    fn purge_expired(&self) -> io::Result<()> {
        let now = SystemTime::now();
        let Ok(rd) = fs::read_dir(&self.dir) else {
            return Ok(());
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Ok(meta) = path.metadata() else {
                continue;
            };
            let Ok(modified) = meta.modified() else {
                continue;
            };
            let Ok(age) = now.duration_since(modified) else {
                continue;
            };
            let ttl = if meta.len() == 0 {
                NOT_FOUND_TTL
            } else {
                CACHE_TTL
            };
            if age > ttl {
                let _ = fs::remove_file(path);
            }
        }
        Ok(())
    }

    fn enforce_budget(&self) -> io::Result<()> {
        let Ok(rd) = fs::read_dir(&self.dir) else {
            return Ok(());
        };
        let mut files: Vec<(PathBuf, SystemTime, u64)> = Vec::new();
        for entry in rd.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Ok(meta) = path.metadata() else {
                continue;
            };
            let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            files.push((path, modified, meta.len()));
        }
        if files.is_empty() {
            return Ok(());
        }
        // Oldest first.
        files.sort_by_key(|(_, mtime, _)| *mtime);
        let mut total: u64 = files.iter().map(|(_, _, len)| *len).sum();
        let mut count = files.len();
        let mut i = 0;
        while (count > MAX_DISK_FILES || total > MAX_DISK_BYTES) && i < files.len() {
            let (path, _, len) = &files[i];
            if fs::remove_file(path).is_ok() {
                total = total.saturating_sub(*len);
                count = count.saturating_sub(1);
            }
            i += 1;
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

    #[test]
    fn purge_keeps_fresh_entries() {
        let dir = std::env::temp_dir().join(format!("hiptty-avatar-purge-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let cache = AvatarDiskCache::new(&dir).expect("cache");
        let url = "https://img02.4d4y.com/uc_server/data/avatar/000/00/00/00/2_avatar_middle.jpg";
        cache.save_bytes(url, b"fresh").expect("save");
        cache.purge().expect("purge");
        assert!(matches!(
            cache.load(url),
            Some(AvatarDiskEntry::Bytes(b)) if b == b"fresh"
        ));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn enforce_budget_caps_file_count() {
        let dir = std::env::temp_dir().join(format!("hiptty-avatar-budget-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let cache = AvatarDiskCache::new(&dir).expect("cache");
        // Write more than MAX_DISK_FILES with direct paths to avoid URL mapping limits.
        let avatar_dir = dir.join("cache/avatars");
        for i in 0..(MAX_DISK_FILES + 5) {
            fs::write(avatar_dir.join(format!("pad_{i}.jpg")), b"x").expect("write");
        }
        cache.enforce_budget().expect("budget");
        let count = fs::read_dir(&avatar_dir)
            .expect("rd")
            .filter(|e| e.as_ref().is_ok_and(|e| e.path().is_file()))
            .count();
        assert!(count <= MAX_DISK_FILES, "count={count}");
        let _ = fs::remove_dir_all(dir);
    }
}
