//! `fetching-cli` — minimal self-contained Spotify CLI via librespot.
//!
//! All business output (metadata JSON, audio data) goes to **stdout**.
//! All logging goes to **stderr**.
//!
//! # Usage
//!
//! ```text
//! fetching-cli
//!     Interactive: runs the browser OAuth flow and stores credentials to
//!     ~/.config/fetching-cli/credentials.json.
//!
//! fetching-cli <spotify-uri-or-url>
//!     Fetch metadata JSON. Credentials are loaded, refreshed, or acquired
//!     automatically.
//!
//! fetching-cli --track-uri <uri> <file-id>
//!     Download audio. Credentials are handled automatically.
//!
//! To re-authenticate, delete ~/.config/fetching-cli/credentials.json and
//! run without arguments.
//! ```

mod audio;
mod auth;
mod credentials;
mod error;
mod metadata;
mod session;

use clap::Parser;
use tracing::{debug, info};
use tracing_subscriber::{fmt, EnvFilter};

use error::{CliError, ExitCode};

// ── CLI structure ─────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "fetching-cli",
    about = "Minimal Spotify CLI via librespot",
    version
)]
struct Cli {
    /// For audio download: the owning track/episode URI
    /// (e.g. `spotify:track:6rqhFgbbKwnb9MLmUQDhG6`).
    /// When provided, <target> is treated as a file ID (hex).
    #[arg(long)]
    track_uri: Option<String>,

    /// Spotify URI/URL (for metadata) or file ID hex (for audio when --track-uri is set).
    target: Option<String>,
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    // Logging to stderr, controlled by RUST_LOG env var (default: info)
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();

    let cli = Cli::parse();

    let exit_code = match run(cli).await {
        Ok(()) => 0,
        Err(e) => e.emit_and_exit_code(),
    };

    std::process::exit(exit_code);
}

async fn run(cli: Cli) -> Result<(), CliError> {
    use std::io::IsTerminal;

    let target = match cli.target {
        Some(t) => t,
        None => {
            // No target given. In an interactive session, helpfully kick off
            // the auth flow (first-run UX). In non-interactive contexts, fail
            // fast — there's nothing useful we can do without a target.
            if std::io::stdin().is_terminal() {
                return cmd_auth();
            }
            return Err(CliError::new(
                ExitCode::InvalidInput,
                "No target specified. Pass a Spotify URI/URL, or use --auth to authenticate.",
            ));
        }
    };

    cmd_fetch(cli.track_uri.as_deref(), &target).await
}

// ── Command implementations ───────────────────────────────────────────────────

fn cmd_auth() -> Result<(), CliError> {
    info!("Running auth");
    let creds = auth::auth()?;
    credentials::save_stored(&creds)?;
    info!("Credentials stored to {}", credentials::credentials_path()?.display());
    Ok(())
}

async fn cmd_fetch(track_uri: Option<&str>, target: &str) -> Result<(), CliError> {
    let creds = ensure_credentials().await?;
    let session = session::create_session(&creds).await?;

    match track_uri {
        Some(uri) => {
            info!("Fetch audio mode: file_id={target}, track_uri={uri}");
            audio::fetch_audio(&session, target, uri).await
        }
        None => {
            info!("Fetch metadata mode: target={target}");
            let output = metadata::fetch_metadata(&session, target).await?;
            print_json(&output)
        }
    }
}

/// Load credentials from disk, refreshing if expired, running auth if absent.
///
/// On first run (no stored credentials) this triggers the browser OAuth flow
/// automatically — unless stdin is not a TTY, in which case it fails fast with
/// a helpful message directing the user to run `--auth` explicitly.
async fn ensure_credentials() -> Result<credentials::Credentials, CliError> {
    use std::io::IsTerminal;

    match credentials::load_stored()? {
        Some(creds) if !creds.is_expired() => {
            info!("Using stored credentials (valid until {})", creds.expires_at);
            Ok(creds)
        }
        Some(creds) => {
            info!("Stored credentials expired, refreshing");
            let new_creds = auth::reauth(&creds).await?;
            credentials::save_stored(&new_creds)?;
            Ok(new_creds)
        }
        None => {
            if !std::io::stdin().is_terminal() {
                return Err(CliError::new(
                    ExitCode::AuthError,
                    "No stored credentials found. Run `fetching-cli --auth` to authenticate.",
                ));
            }
            info!("No stored credentials found, starting OAuth flow");
            let creds = auth::auth()?;
            credentials::save_stored(&creds)?;
            Ok(creds)
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn print_json<T: serde::Serialize>(value: &T) -> Result<(), CliError> {
    debug!("Serializing output to JSON");
    let json = serde_json::to_string_pretty(value).map_err(|e| {
        CliError::with_source(
            ExitCode::SerializationError,
            format!("Failed to serialize output to JSON: {e}"),
            e.into(),
        )
    })?;
    println!("{json}");
    Ok(())
}
