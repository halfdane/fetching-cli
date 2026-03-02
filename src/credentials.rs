//! Credentials handling: parsing, loading, and serializing OAuth credentials.
//!
//! Credentials can be supplied as:
//! - An inline JSON string (starts with `{`)
//! - A path to a JSON file
//! - Via stdin (when `--credentials` is omitted)

use serde::{Deserialize, Serialize};
use std::io::Read;
use tracing::{debug, info};

use crate::error::{CliError, ExitCode};

/// OAuth credentials exchanged between CLI invocations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix timestamp (seconds) at which the access token expires.
    pub expires_at: u64,
}

/// Parse credentials from the `--credentials` argument value.
///
/// If the value starts with `{`, it is treated as inline JSON.
/// Otherwise it is treated as a file path.
pub fn parse_credentials(value: &str) -> Result<Credentials, CliError> {
    let trimmed = value.trim();
    if trimmed.starts_with('{') {
        debug!("Parsing credentials from inline JSON");
        serde_json::from_str(trimmed).map_err(|e| {
            CliError::with_source(
                ExitCode::InvalidInput,
                format!("Failed to parse inline credentials JSON: {e}"),
                e.into(),
            )
        })
    } else {
        info!("Loading credentials from file: {trimmed}");
        let content = std::fs::read_to_string(trimmed).map_err(|e| {
            CliError::with_source(
                ExitCode::InvalidInput,
                format!("Failed to read credentials file '{trimmed}': {e}"),
                e.into(),
            )
        })?;
        serde_json::from_str(&content).map_err(|e| {
            CliError::with_source(
                ExitCode::InvalidInput,
                format!("Failed to parse credentials file '{trimmed}': {e}"),
                e.into(),
            )
        })
    }
}

/// Read credentials from stdin (blocking).
pub fn read_credentials_from_stdin() -> Result<Credentials, CliError> {
    debug!("Reading credentials from stdin");
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf).map_err(|e| {
        CliError::with_source(
            ExitCode::InvalidInput,
            format!("Failed to read credentials from stdin: {e}"),
            e.into(),
        )
    })?;
    serde_json::from_str(&buf).map_err(|e| {
        CliError::with_source(
            ExitCode::InvalidInput,
            format!("Failed to parse credentials from stdin: {e}"),
            e.into(),
        )
    })
}

/// Resolve credentials from the optional `--credentials` flag or stdin fallback.
pub fn resolve_credentials(flag: Option<&str>) -> Result<Credentials, CliError> {
    match flag {
        Some(val) => parse_credentials(val),
        None => read_credentials_from_stdin(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn sample_json() -> String {
        r#"{"access_token":"acc","refresh_token":"ref","expires_at":9999999999}"#.to_string()
    }

    #[test]
    fn parse_inline_json() {
        let creds = parse_credentials(&sample_json()).unwrap();
        assert_eq!(creds.access_token, "acc");
        assert_eq!(creds.refresh_token, "ref");
        assert_eq!(creds.expires_at, 9999999999);
    }

    #[test]
    fn parse_from_file() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(sample_json().as_bytes()).unwrap();
        let creds = parse_credentials(f.path().to_str().unwrap()).unwrap();
        assert_eq!(creds.access_token, "acc");
    }

    #[test]
    fn parse_invalid_json_returns_error() {
        let result = parse_credentials("{broken");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ExitCode::InvalidInput);
    }

    #[test]
    fn parse_missing_file_returns_error() {
        let result = parse_credentials("/tmp/no_such_cred_file_xyz.json");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ExitCode::InvalidInput);
    }
}
