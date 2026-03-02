//! OAuth authentication: initial auth and token refresh.
//!
//! Uses `librespot-oauth` for the browser-based OAuth flow and token refresh.

use std::os::unix::io::AsRawFd;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tracing::{debug, info, warn};

use crate::credentials::Credentials;
use crate::error::{CliError, ExitCode};

// ── Constants ─────────────────────────────────────────────────────────────────

const CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";
const REDIRECT_URI: &str = "http://127.0.0.1:8898/login";
const SCOPES: &[&str] = &["streaming"];

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

// ── Auth: browser-based OAuth flow ────────────────────────────────────────────

/// Run the interactive browser-based OAuth flow.
///
/// Opens the user's default browser to the Spotify authorization page,
/// starts a local HTTP server to receive the callback, and returns
/// the resulting credentials.
pub fn auth() -> Result<Credentials, CliError> {
    info!("Starting browser-based OAuth flow");
    debug!("Client ID: {CLIENT_ID}, redirect: {REDIRECT_URI}, scopes: {SCOPES:?}");

    let client = librespot_oauth::OAuthClientBuilder::new(CLIENT_ID, REDIRECT_URI, SCOPES.to_vec())
        .open_in_browser()
        .build()
        .map_err(|e| {
            CliError::with_source(
                ExitCode::AuthError,
                format!("Failed to build OAuth client: {e}"),
                e.into(),
            )
        })?;

    // The librespot-oauth library prints "Browse to: <url>" to stdout via println!.
    // Temporarily redirect stdout → stderr so only our JSON ends up on stdout.
    let token = {
        let stdout_fd = std::io::stdout().as_raw_fd();
        let stderr_fd = std::io::stderr().as_raw_fd();
        let saved_stdout = unsafe { libc::dup(stdout_fd) };
        assert!(saved_stdout >= 0, "dup(stdout) failed");
        unsafe { libc::dup2(stderr_fd, stdout_fd) };

        let result = client.get_access_token();

        // Restore original stdout.
        unsafe { libc::dup2(saved_stdout, stdout_fd) };
        unsafe { libc::close(saved_stdout) };

        result
    }
    .map_err(|e| {
        CliError::with_source(
            ExitCode::AuthError,
            format!("OAuth flow failed: {e}"),
            e.into(),
        )
    })?;

    let seconds_until_expiry = token
        .expires_at
        .checked_duration_since(Instant::now())
        .map(|d| d.as_secs())
        .unwrap_or(3600);

    let creds = Credentials {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at: now_secs() + seconds_until_expiry,
    };

    info!(
        "OAuth flow successful, token expires at {} (in ~{}s)",
        creds.expires_at, seconds_until_expiry
    );

    Ok(creds)
}

// ── Reauth: refresh an existing token ─────────────────────────────────────────

/// Refresh credentials using the stored refresh token.
///
/// Returns new credentials with an updated access token and expiry.
pub async fn reauth(old: &Credentials) -> Result<Credentials, CliError> {
    info!("Refreshing access token using refresh_token");
    debug!("Old token expires_at: {}", old.expires_at);

    let client = librespot_oauth::OAuthClientBuilder::new(CLIENT_ID, REDIRECT_URI, SCOPES.to_vec())
        .build()
        .map_err(|e| {
            CliError::with_source(
                ExitCode::AuthError,
                format!("Failed to build OAuth client for refresh: {e}"),
                e.into(),
            )
        })?;

    let new_token = client
        .refresh_token_async(&old.refresh_token)
        .await
        .map_err(|e| {
            // Classify: if it looks like a network issue vs auth issue
            let msg = e.to_string();
            if msg.contains("timed out")
                || msg.contains("connection")
                || msg.contains("dns")
                || msg.contains("network")
            {
                warn!("Token refresh failed due to network issue: {e}");
                CliError::with_source(
                    ExitCode::NetworkError,
                    format!("Token refresh failed (network): {e}"),
                    e.into(),
                )
            } else {
                warn!("Token refresh failed: {e}");
                CliError::with_source(
                    ExitCode::AuthError,
                    format!("Token refresh failed: {e}"),
                    e.into(),
                )
            }
        })?;

    let seconds_until_expiry = new_token
        .expires_at
        .checked_duration_since(Instant::now())
        .map(|d| d.as_secs())
        .unwrap_or(3600);

    let creds = Credentials {
        access_token: new_token.access_token,
        refresh_token: new_token.refresh_token,
        expires_at: now_secs() + seconds_until_expiry,
    };

    info!(
        "Token refresh successful, new expiry at {} (in ~{}s)",
        creds.expires_at, seconds_until_expiry
    );

    Ok(creds)
}
