use std::fs::File;
use std::io::{BufReader, BufWriter};
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

pub fn session_path(config_dir: &Path, profile: &str) -> PathBuf {
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
    let file = File::create(path)
        .map_err(|e| AdapterError::InvalidInput(format!("create session file: {e}")))?;
    let mut writer = BufWriter::new(file);
    let inner = store
        .lock()
        .map_err(|e| AdapterError::InvalidInput(format!("lock cookie store: {e}")))?;
    json::save(&inner, &mut writer)
        .map_err(|e| AdapterError::InvalidInput(format!("save session: {e}")))
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
}
