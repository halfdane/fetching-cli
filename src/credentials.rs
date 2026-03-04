//! Credentials storage: persist and load OAuth credentials from disk.
//!
//! The credentials file lives at `$XDG_CONFIG_HOME/fetching-cli/credentials.json`
//! (falling back to `~/.config/fetching-cli/credentials.json`).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, info};

use crate::error::{CliError, ExitCode};

/// OAuth credentials persisted between CLI invocations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix timestamp (seconds) at which the access token expires.
    pub expires_at: u64,
}

impl Credentials {
    /// Returns `true` if the access token has expired (with a 60-second buffer).
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        now + 60 >= self.expires_at
    }
}

/// Returns the path to the stored credentials file.
///
/// Respects `XDG_CONFIG_HOME`; falls back to `~/.config/fetching-cli/credentials.json`.
pub fn credentials_path() -> Result<PathBuf, CliError> {
    let config_base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .ok_or_else(|| {
            CliError::new(
                ExitCode::InvalidInput,
                "Cannot determine config directory (HOME not set)",
            )
        })?;
    Ok(config_base.join("fetching-cli").join("credentials.json"))
}

/// Load credentials from an explicit path. Returns `None` if the file does not exist.
pub fn load_from_path(path: &Path) -> Result<Option<Credentials>, CliError> {
    debug!("Looking for credentials at {}", path.display());
    if !path.exists() {
        debug!("No credentials file found");
        return Ok(None);
    }
    info!("Loading credentials from {}", path.display());
    let content = std::fs::read_to_string(path).map_err(|e| {
        CliError::with_source(
            ExitCode::InvalidInput,
            format!("Failed to read credentials file '{}': {e}", path.display()),
            e.into(),
        )
    })?;
    let creds = serde_json::from_str(&content).map_err(|e| {
        CliError::with_source(
            ExitCode::InvalidInput,
            format!("Failed to parse credentials file '{}': {e}", path.display()),
            e.into(),
        )
    })?;
    Ok(Some(creds))
}

/// Persist credentials to an explicit path, creating parent directories as needed.
pub fn save_to_path(path: &Path, creds: &Credentials) -> Result<(), CliError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            CliError::with_source(
                ExitCode::InvalidInput,
                format!(
                    "Failed to create config directory '{}': {e}",
                    parent.display()
                ),
                e.into(),
            )
        })?;
    }
    let json = serde_json::to_string_pretty(creds).map_err(|e| {
        CliError::with_source(
            ExitCode::SerializationError,
            format!("Failed to serialize credentials: {e}"),
            e.into(),
        )
    })?;
    std::fs::write(path, json).map_err(|e| {
        CliError::with_source(
            ExitCode::InvalidInput,
            format!("Failed to write credentials to '{}': {e}", path.display()),
            e.into(),
        )
    })?;
    info!("Credentials saved to {}", path.display());
    Ok(())
}

/// Load stored credentials from the default config path.
///
/// Returns `None` on first run when no credentials file exists yet.
pub fn load_stored() -> Result<Option<Credentials>, CliError> {
    load_from_path(&credentials_path()?)
}

/// Persist credentials to the default config path.
pub fn save_stored(creds: &Credentials) -> Result<(), CliError> {
    save_to_path(&credentials_path()?, creds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_creds() -> Credentials {
        Credentials {
            access_token: "acc".into(),
            refresh_token: "ref".into(),
            expires_at: 9999999999,
        }
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("credentials.json");
        save_to_path(&path, &sample_creds()).unwrap();
        let loaded = load_from_path(&path).unwrap().unwrap();
        assert_eq!(loaded.access_token, "acc");
        assert_eq!(loaded.refresh_token, "ref");
        assert_eq!(loaded.expires_at, 9999999999);
    }

    #[test]
    fn load_returns_none_when_missing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("credentials.json");
        let result = load_from_path(&path).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn save_creates_parent_directories() {
        let dir = TempDir::new().unwrap();
        let path = dir
            .path()
            .join("nested")
            .join("deep")
            .join("credentials.json");
        save_to_path(&path, &sample_creds()).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn is_expired_true_for_past() {
        let creds = Credentials {
            access_token: "a".into(),
            refresh_token: "r".into(),
            expires_at: 1,
        };
        assert!(creds.is_expired());
    }

    #[test]
    fn is_expired_false_for_future() {
        assert!(!sample_creds().is_expired());
    }
}
