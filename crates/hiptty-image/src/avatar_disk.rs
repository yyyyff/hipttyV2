use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

pub const AVATAR_PATH_MARKER: &str = "uc_server/data/avatar/";

const CACHE_TTL: Duration = Duration::from_secs(3 * 24 * 60 * 60);
const NOT_FOUND_TTL: Duration = Duration::from_secs(24 * 60 * 60);
/// Total size budget for on-disk avatar files (bytes + empty not-found markers).
const MAX_DISK_BYTES: u64 = 64 * 1024 * 1024;
/// Hard cap on number of avatar cache files.
const MAX_DISK_FILES: usize = 2_000;
/// Start cleanup when file count reaches this (high water).
const HIGH_WATER_FILES: usize = MAX_DISK_FILES * 9 / 10; // 1800
/// After cleanup, stop once at/under this count (low water / hysteresis).
const LOW_WATER_FILES: usize = MAX_DISK_FILES * 4 / 5; // 1600
/// Start cleanup when total bytes reach this.
const HIGH_WATER_BYTES: u64 = MAX_DISK_BYTES;
/// After cleanup, stop once at/under this byte total.
const LOW_WATER_BYTES: u64 = MAX_DISK_BYTES * 3 / 4; // 48 MiB
/// Opportunistic full enforce every N successful writes even under high water.
const ENFORCE_EVERY_N_WRITES: u32 = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvatarDiskEntry {
    Bytes(Vec<u8>),
    NotFound,
}

#[derive(Debug, Default)]
struct DiskStats {
    file_count: usize,
    total_bytes: u64,
    writes_since_enforce: u32,
}

/// On-disk avatar cache with in-memory size accounting (no full readdir on every save).
#[derive(Debug)]
pub struct AvatarDiskCache {
    dir: PathBuf,
    stats: Mutex<DiskStats>,
}

impl AvatarDiskCache {
    pub fn new(config_dir: &Path) -> io::Result<Self> {
        let dir = config_dir.join("cache/avatars");
        fs::create_dir_all(&dir)?;
        let cache = Self {
            dir,
            stats: Mutex::new(DiskStats::default()),
        };
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
        let meta = path.metadata().ok()?;
        let modified = meta.modified().ok()?;
        let age = SystemTime::now().duration_since(modified).ok()?;
        let len = meta.len();
        if len == 0 {
            if age > NOT_FOUND_TTL {
                self.remove_path(&path, len);
                return None;
            }
            return Some(AvatarDiskEntry::NotFound);
        }
        if age > CACHE_TTL {
            self.remove_path(&path, len);
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
        self.write_file(&path, bytes)
    }

    pub fn save_not_found(&self, url: &str) -> io::Result<()> {
        let Some(name) = Self::cache_file_name(url) else {
            return Ok(());
        };
        let path = self.dir.join(name);
        self.write_file(&path, &[])
    }

    fn write_file(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let old_len = path.metadata().ok().map(|m| m.len());
        fs::write(path, bytes)?;
        let new_len = bytes.len() as u64;
        {
            let mut stats = self.stats.lock().expect("avatar disk stats");
            match old_len {
                Some(old) => {
                    stats.total_bytes = stats
                        .total_bytes
                        .saturating_sub(old)
                        .saturating_add(new_len);
                }
                None => {
                    stats.file_count = stats.file_count.saturating_add(1);
                    stats.total_bytes = stats.total_bytes.saturating_add(new_len);
                }
            }
            stats.writes_since_enforce = stats.writes_since_enforce.saturating_add(1);
            let over_files = stats.file_count >= HIGH_WATER_FILES;
            let over_bytes = stats.total_bytes >= HIGH_WATER_BYTES;
            let periodic = stats.writes_since_enforce >= ENFORCE_EVERY_N_WRITES
                && (stats.file_count > LOW_WATER_FILES || stats.total_bytes > LOW_WATER_BYTES);
            if over_files || over_bytes || periodic {
                stats.writes_since_enforce = 0;
                drop(stats);
                let _ = self.enforce_budget_to_low_water();
            }
        }
        Ok(())
    }

    pub fn clear(&self, url: &str) -> io::Result<()> {
        let Some(name) = Self::cache_file_name(url) else {
            return Ok(());
        };
        let path = self.dir.join(name);
        if path.exists() {
            let len = path.metadata().map(|m| m.len()).unwrap_or(0);
            self.remove_path(&path, len);
        }
        Ok(())
    }

    fn remove_path(&self, path: &Path, known_len: u64) {
        if fs::remove_file(path).is_ok() {
            if let Ok(mut stats) = self.stats.lock() {
                stats.file_count = stats.file_count.saturating_sub(1);
                stats.total_bytes = stats.total_bytes.saturating_sub(known_len);
            }
        }
    }

    /// Remove TTL-expired files, then shrink to high-water hard caps.
    pub fn purge(&self) -> io::Result<()> {
        self.purge_expired()?;
        self.enforce_budget_to_low_water()?;
        self.recount_stats()?;
        Ok(())
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
                self.remove_path(&path, meta.len());
            }
        }
        Ok(())
    }

    /// Full directory scan used by tests and after purge to resync counters.
    pub fn enforce_budget(&self) -> io::Result<()> {
        self.enforce_budget_to_low_water()
    }

    fn enforce_budget_to_low_water(&self) -> io::Result<()> {
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
            if let Ok(mut stats) = self.stats.lock() {
                *stats = DiskStats::default();
            }
            return Ok(());
        }
        // Oldest first.
        files.sort_by_key(|(_, mtime, _)| *mtime);
        let mut total: u64 = files.iter().map(|(_, _, len)| *len).sum();
        let mut count = files.len();
        let mut i = 0;
        // Evict until under both low-water targets (hysteresis), never above hard caps.
        while (count > LOW_WATER_FILES || total > LOW_WATER_BYTES) && i < files.len() {
            // Always respect hard max even if somehow under low water after partial deletes.
            if count <= LOW_WATER_FILES
                && total <= LOW_WATER_BYTES
                && count <= MAX_DISK_FILES
                && total <= MAX_DISK_BYTES
            {
                break;
            }
            let (path, _, len) = &files[i];
            if fs::remove_file(path).is_ok() {
                total = total.saturating_sub(*len);
                count = count.saturating_sub(1);
            }
            i += 1;
        }
        // Safety: if still over hard cap (shouldn't), keep going.
        while (count > MAX_DISK_FILES || total > MAX_DISK_BYTES) && i < files.len() {
            let (path, _, len) = &files[i];
            if fs::remove_file(path).is_ok() {
                total = total.saturating_sub(*len);
                count = count.saturating_sub(1);
            }
            i += 1;
        }
        if let Ok(mut stats) = self.stats.lock() {
            stats.file_count = count;
            stats.total_bytes = total;
            stats.writes_since_enforce = 0;
        }
        Ok(())
    }

    fn recount_stats(&self) -> io::Result<()> {
        let Ok(rd) = fs::read_dir(&self.dir) else {
            return Ok(());
        };
        let mut count = 0usize;
        let mut total = 0u64;
        for entry in rd.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Ok(meta) = path.metadata() else {
                continue;
            };
            count += 1;
            total = total.saturating_add(meta.len());
        }
        if let Ok(mut stats) = self.stats.lock() {
            stats.file_count = count;
            stats.total_bytes = total;
            stats.writes_since_enforce = 0;
        }
        Ok(())
    }

    #[cfg(test)]
    fn stats_snapshot(&self) -> (usize, u64) {
        let s = self.stats.lock().expect("stats");
        (s.file_count, s.total_bytes)
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
        let (count, bytes) = cache.stats_snapshot();
        assert_eq!(count, 1);
        assert_eq!(bytes, 4);

        cache.save_not_found(url).expect("404");
        assert!(matches!(cache.load(url), Some(AvatarDiskEntry::NotFound)));
        let (count, bytes) = cache.stats_snapshot();
        assert_eq!(count, 1);
        assert_eq!(bytes, 0);

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
        // Direct writes bypass stats — enforce rescans and applies low water.
        cache.enforce_budget().expect("budget");
        let count = fs::read_dir(&avatar_dir)
            .expect("rd")
            .filter(|e| e.as_ref().is_ok_and(|e| e.path().is_file()))
            .count();
        assert!(
            count <= LOW_WATER_FILES,
            "count={count} should be <= low water {LOW_WATER_FILES}"
        );
        let (stat_count, _) = cache.stats_snapshot();
        assert_eq!(stat_count, count);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn save_tracks_bytes_without_full_scan() {
        let dir = std::env::temp_dir().join(format!("hiptty-avatar-stats-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let cache = AvatarDiskCache::new(&dir).expect("cache");
        for i in 0..10 {
            let url = format!(
                "https://img02.4d4y.com/uc_server/data/avatar/000/00/00/{i:02}/{i}_avatar_middle.jpg"
            );
            cache.save_bytes(&url, &vec![b'a'; i + 1]).expect("save");
        }
        let (count, bytes) = cache.stats_snapshot();
        assert_eq!(count, 10);
        assert_eq!(bytes, (1..=10).sum::<usize>() as u64);
        let _ = fs::remove_dir_all(dir);
    }
}
