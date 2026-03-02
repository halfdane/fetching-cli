//! Librespot session creation from CLI credentials.
//!
//! Creates a [`librespot_core::Session`] using an access token, suitable for
//! metadata lookups and audio downloads.

use librespot_core::{
    authentication::Credentials as LibrespotCredentials, config::SessionConfig, Session,
};
use tracing::{debug, info, warn};

use crate::credentials::Credentials;
use crate::error::{CliError, ExitCode};

/// Create an authenticated librespot session from CLI credentials.
///
/// No caching or background refresh — each CLI invocation is short-lived.
pub async fn create_session(creds: &Credentials) -> Result<Session, CliError> {
    info!("Creating librespot session");
    debug!("Token expires_at: {}", creds.expires_at);

    let librespot_creds = LibrespotCredentials::with_access_token(creds.access_token.trim());

    let session = Session::new(SessionConfig::default(), None);

    session.connect(librespot_creds, false).await.map_err(|e| {
        let msg = e.to_string();
        if msg.contains("Bad credentials") || msg.contains("auth") {
            warn!("Session connection failed (auth): {e}");
            CliError::with_source(
                ExitCode::AuthError,
                format!("Failed to connect session (bad credentials): {e}"),
                e.into(),
            )
        } else if msg.contains("timed out") || msg.contains("connection") || msg.contains("resolve")
        {
            warn!("Session connection failed (network): {e}");
            CliError::with_source(
                ExitCode::NetworkError,
                format!("Failed to connect session (network): {e}"),
                e.into(),
            )
        } else {
            warn!("Session connection failed: {e}");
            CliError::with_source(
                ExitCode::ApiError,
                format!("Failed to connect session: {e}"),
                e.into(),
            )
        }
    })?;

    info!("Session connected successfully");
    Ok(session)
}
