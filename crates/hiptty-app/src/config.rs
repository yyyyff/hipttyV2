use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use hiptty_core::{AdapterError, AppSettings, StoredCredentials};
use serde::{Deserialize, Serialize};

pub fn config_dir(custom: Option<&Path>) -> Result<PathBuf, AdapterError> {
    hiptty_adapter::session::config_dir(custom)
}

pub fn settings_path(config_dir: &Path) -> PathBuf {
    config_dir.join("settings.json")
}

pub fn credentials_path(config_dir: &Path, profile: &str) -> PathBuf {
    // Prefer validated names; fall back to "default" only for path construction after
    // DiscuzClient::new already rejected invalid profiles at process start.
    let profile = if hiptty_adapter::session::validate_profile(profile).is_ok() {
        profile
    } else {
        "default"
    };
    config_dir.join(format!("{profile}.credentials.json"))
}

pub fn load_settings(path: &Path) -> AppSettings {
    read_json(path).unwrap_or_default()
}

pub fn save_settings(path: &Path, settings: &AppSettings) -> Result<(), AdapterError> {
    write_json(path, settings)?;
    Ok(())
}

pub fn load_credentials(path: &Path) -> Option<StoredCredentials> {
    read_json(path).ok()
}

pub fn save_credentials(path: &Path, creds: &StoredCredentials) -> Result<(), AdapterError> {
    write_json(path, creds)?;
    restrict_permissions(path)?;
    Ok(())
}

pub fn clear_credentials(path: &Path) -> Result<(), AdapterError> {
    if path.exists() {
        fs::remove_file(path).map_err(|e| AdapterError::InvalidInput(e.to_string()))?;
    }
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, AdapterError> {
    let file = File::open(path).map_err(|e| AdapterError::InvalidInput(e.to_string()))?;
    serde_json::from_reader(BufReader::new(file)).map_err(|e| AdapterError::Parse(e.to_string()))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), AdapterError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| AdapterError::InvalidInput(e.to_string()))?;
    }
    // Atomic write into the same directory to avoid truncated credentials on crash.
    let tmp = path.with_extension("json.tmp");
    {
        let file = create_private_file(&tmp)?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, value)
            .map_err(|e| AdapterError::InvalidInput(e.to_string()))?;
        // Explicit flush: Drop of BufWriter can swallow the final write error.
        writer
            .flush()
            .map_err(|e| AdapterError::InvalidInput(e.to_string()))?;
    }
    restrict_permissions(&tmp)?;
    fs::rename(&tmp, path).map_err(|e| AdapterError::InvalidInput(e.to_string()))?;
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
            .map_err(|e| AdapterError::InvalidInput(e.to_string()))
    }
    #[cfg(not(unix))]
    {
        File::create(path).map_err(|e| AdapterError::InvalidInput(e.to_string()))
    }
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<(), AdapterError> {
    use std::os::unix::fs::PermissionsExt;
    let perms = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, perms).map_err(|e| AdapterError::InvalidInput(e.to_string()))
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<(), AdapterError> {
    Ok(())
}
