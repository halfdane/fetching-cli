//! `fetching-cli` — minimal self-contained Spotify CLI via librespot.
//!
//! All business output (metadata JSON, audio data) goes to **stdout**.
//! All logging goes to **stderr**.
//!
//! # Commands
//!
//! ```text
//! fetching-cli auth
//!     Open browser OAuth flow, print credentials JSON to stdout.
//!
//! fetching-cli reauth --credentials <json-or-file>
//!     Refresh token, print new credentials JSON to stdout.
//!
//! fetching-cli fetch --credentials <json-or-file> <spotify-uri-or-url>
//!     Fetch metadata, print JSON to stdout.
//!
//! fetching-cli fetch --credentials <json-or-file> --track-uri <uri> <file-id>
//!     Download audio, write raw bytes to stdout.
//! ```

mod audio;
mod auth;
mod credentials;
mod error;
mod metadata;
mod session;

use clap::{Parser, Subcommand};
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
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the browser-based OAuth flow and print credentials to stdout.
    Auth,

    /// Refresh an existing access token and print new credentials to stdout.
    Reauth {
        /// Credentials: inline JSON string or path to a JSON file.
        /// If omitted, reads from stdin.
        #[arg(long)]
        credentials: Option<String>,
    },

    /// Fetch metadata or audio from Spotify.
    Fetch {
        /// Credentials: inline JSON string or path to a JSON file.
        /// If omitted, reads from stdin.
        #[arg(long)]
        credentials: Option<String>,

        /// For audio download: the owning track/episode URI
        /// (e.g. `spotify:track:6rqhFgbbKwnb9MLmUQDhG6`).
        /// When provided, the positional argument is treated as a file ID (hex).
        #[arg(long)]
        track_uri: Option<String>,

        /// Spotify URI/URL (for metadata) or file ID hex (for audio when --track-uri is set).
        target: String,
    },
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
    match cli.command {
        Command::Auth => cmd_auth(),
        Command::Reauth { credentials } => cmd_reauth(credentials.as_deref()).await,
        Command::Fetch {
            credentials,
            track_uri,
            target,
        } => cmd_fetch(credentials.as_deref(), track_uri.as_deref(), &target).await,
    }
}

// ── Command implementations ───────────────────────────────────────────────────

fn cmd_auth() -> Result<(), CliError> {
    info!("Running auth command");
    let creds = auth::auth()?;
    print_json(&creds)
}

async fn cmd_reauth(credentials_flag: Option<&str>) -> Result<(), CliError> {
    info!("Running reauth command");
    let old_creds = credentials::resolve_credentials(credentials_flag)?;
    let new_creds = auth::reauth(&old_creds).await?;
    print_json(&new_creds)
}

async fn cmd_fetch(
    credentials_flag: Option<&str>,
    track_uri: Option<&str>,
    target: &str,
) -> Result<(), CliError> {
    let creds = credentials::resolve_credentials(credentials_flag)?;
    let session = session::create_session(&creds).await?;

    match track_uri {
        Some(uri) => {
            // Audio download mode: target is a file ID hex, --track-uri is the owning URI
            info!("Fetch audio mode: file_id={target}, track_uri={uri}");
            audio::fetch_audio(&session, target, uri).await
        }
        None => {
            // Metadata mode: target is a Spotify URI/URL
            info!("Fetch metadata mode: target={target}");
            let output = metadata::fetch_metadata(&session, target).await?;
            print_json(&output)
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
