use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cookie_store::serde::json;
use hiptty_core::AdapterError;
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};

/// Cross-platform config root: `~/.config/hiptty`.
pub fn config_dir(custom: Option<&Path>) -> Result<PathBuf, AdapterError> {
    if let Some(dir) = custom {
        return Ok(dir.to_path_buf());
    }
    home_dir()
        .map(|home| home.join(".config").join("hiptty"))
        .ok_or_else(|| AdapterError::InvalidInput("cannot resolve home directory".into()))
}

/// Validate profile names used as filename prefixes (no path separators / traversal).
pub fn validate_profile(profile: &str) -> Result<(), AdapterError> {
    if profile.is_empty() {
        return Err(AdapterError::InvalidInput("profile name is empty".into()));
    }
    if profile.len() > 64 {
        return Err(AdapterError::InvalidInput("profile name too long".into()));
    }
    let ok = profile
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.');
    if !ok || profile == "." || profile == ".." || profile.contains("..") {
        return Err(AdapterError::InvalidInput(format!(
            "invalid profile name {profile:?}: use only [A-Za-z0-9._-]"
        )));
    }
    if profile.contains('/') || profile.contains('\\') {
        return Err(AdapterError::InvalidInput(format!(
            "invalid profile name {profile:?}: path separators not allowed"
        )));
    }
    Ok(())
}

pub fn session_path(config_dir: &Path, profile: &str) -> PathBuf {
    debug_assert!(
        validate_profile(profile).is_ok(),
        "profile must be validated before session_path"
    );
    config_dir.join(format!("{profile}.session.json"))
}

/// macOS legacy path from `directories::ProjectDirs` before we unified on `~/.config/hiptty`.
fn legacy_config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        home_dir().map(|home| home.join("Library/Application Support/hiptty"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

fn session_file_usable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if meta.len() == 0 {
        return false;
    }
    let Ok(file) = File::open(path) else {
        return false;
    };
    json::load(BufReader::new(file)).is_ok()
}

/// Copy session file from legacy macOS location when the new path is missing or unusable.
pub fn migrate_legacy_session(config_dir: &Path, profile: &str) -> Result<(), AdapterError> {
    validate_profile(profile)?;
    let new_path = session_path(config_dir, profile);
    if new_path.exists() && session_file_usable(&new_path) {
        return Ok(());
    }
    let Some(legacy_dir) = legacy_config_dir() else {
        return Ok(());
    };
    let legacy_path = legacy_dir.join(format!("{profile}.session.json"));
    if !session_file_usable(&legacy_path) {
        return Ok(());
    }
    if let Some(parent) = new_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AdapterError::InvalidInput(format!("create config dir: {e}")))?;
    }
    std::fs::copy(&legacy_path, &new_path).map_err(|e| {
        AdapterError::InvalidInput(format!(
            "migrate session from {} to {}: {e}",
            legacy_path.display(),
            new_path.display()
        ))
    })?;
    // Preserve private permissions even if the legacy file was world-readable.
    restrict_permissions(&new_path)?;
    Ok(())
}

pub fn load_cookie_store(path: &Path) -> Result<Arc<CookieStoreMutex>, AdapterError> {
    let store = if path.exists() {
        let meta = std::fs::metadata(path)
            .map_err(|e| AdapterError::InvalidInput(format!("stat session: {e}")))?;
        if meta.len() == 0 {
            CookieStore::default()
        } else {
            let file = File::open(path)
                .map_err(|e| AdapterError::InvalidInput(format!("open session: {e}")))?;
            let reader = BufReader::new(file);
            json::load(reader).unwrap_or_else(|_| CookieStore::default())
        }
    } else {
        CookieStore::default()
    };
    Ok(Arc::new(CookieStoreMutex::new(store)))
}

pub fn save_cookie_store(store: &CookieStoreMutex, path: &Path) -> Result<(), AdapterError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AdapterError::InvalidInput(format!("create config dir: {e}")))?;
    }
    // Atomic write: temp in same dir then rename so readers never see a truncated file.
    let tmp = path.with_extension("session.json.tmp");
    {
        let file = create_private_file(&tmp)?;
        let mut writer = BufWriter::new(file);
        let inner = store
            .lock()
            .map_err(|e| AdapterError::InvalidInput(format!("lock cookie store: {e}")))?;
        json::save(&inner, &mut writer)
            .map_err(|e| AdapterError::InvalidInput(format!("save session: {e}")))?;
        writer
            .flush()
            .map_err(|e| AdapterError::InvalidInput(format!("flush session: {e}")))?;
    }
    // Session cookies are credentials: match credentials.json (0600).
    restrict_permissions(&tmp)?;
    std::fs::rename(&tmp, path)
        .map_err(|e| AdapterError::InvalidInput(format!("rename session file: {e}")))?;
    // Re-apply in case rename dropped mode bits on some platforms.
    restrict_permissions(path)
}

fn create_private_file(path: &Path) -> Result<File, AdapterError> {
    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| AdapterError::InvalidInput(format!("create session file: {e}")))
    }
    #[cfg(not(unix))]
    {
        File::create(path)
            .map_err(|e| AdapterError::InvalidInput(format!("create session file: {e}")))
    }
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<(), AdapterError> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    let perms = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, perms)
        .map_err(|e| AdapterError::InvalidInput(format!("chmod session file: {e}")))
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<(), AdapterError> {
    Ok(())
}

pub fn clear_cookie_store(store: &CookieStoreMutex) -> Result<(), AdapterError> {
    let mut inner = store
        .lock()
        .map_err(|e| AdapterError::InvalidInput(format!("lock cookie store: {e}")))?;
    inner.clear();
    Ok(())
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_dir_ends_with_dot_config_hiptty() {
        let dir = config_dir(None).expect("home dir");
        assert!(dir.ends_with(".config/hiptty") || dir.ends_with(".config\\hiptty"));
    }

    #[cfg(unix)]
    #[test]
    fn save_cookie_store_sets_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!("hiptty-session-perm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("default.session.json");
        let store = Arc::new(CookieStoreMutex::new(CookieStore::default()));
        save_cookie_store(&store, &path).expect("save");
        let mode = std::fs::metadata(&path).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "session file must be 0600, got {mode:o}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_profile_rejects_path_escape() {
        assert!(validate_profile("default").is_ok());
        assert!(validate_profile("user_1").is_ok());
        assert!(validate_profile("../evil").is_err());
        assert!(validate_profile("a/b").is_err());
        assert!(validate_profile("..").is_err());
        assert!(validate_profile("").is_err());
        assert!(validate_profile("has space").is_err());
    }
}
