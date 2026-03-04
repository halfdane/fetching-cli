//! `fetching-cli` — minimal self-contained Spotify CLI via librespot.
//!
//! All business output (metadata JSON, audio data) goes to **stdout** or a file (`-o`).
//! All logging goes to **stderr**.
//!
//! # Usage
//!
//! ```text
//! fetching-cli
//!     Interactive: runs the browser OAuth flow and stores credentials to
//!     ~/.config/fetching-cli/credentials.json. Subsequent runs reuse and
//!     auto-refresh stored credentials.
//!
//! fetching-cli <spotify-uri-or-url>
//!     Fetch metadata JSON.
//!
//! fetching-cli <spotify-uri-or-url> <file-id> [-o <path>]
//! fetching-cli <file-id> <spotify-uri-or-url> [-o <path>]
//!     Download audio. Argument order doesn't matter.
//!     Writes to stdout by default; use -o to write to a file.
//!
//! To re-authenticate, delete ~/.config/fetching-cli/credentials.json
//! and run without arguments.
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
    /// Write audio output to this file instead of stdout.
    /// Only used when downloading audio (two positional arguments).
    #[arg(long, short, value_name = "PATH")]
    output: Option<std::path::PathBuf>,

    #[arg(
        num_args = 0..=2,
        value_name = "TARGET",
        help = "What to fetch:\n  \
                (none)                       interactive auth flow or validate credentials\n  \
                <spotify-uri-or-url>         fetch metadata\n  \
                <spotify-uri-or-url> <id>    download audio\n  \
                <id> <spotify-uri-or-url>    download audio (order doesn't matter)"
    )]
    targets: Vec<String>,
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

    match cli.targets.as_slice() {
        [] => {
            if std::io::stdin().is_terminal() {
                // Ensure credentials (auth flow only happens if none are stored).
                ensure_credentials().await?;
                eprintln!("Authenticated. Pass a Spotify URI/URL to fetch metadata.");
                Ok(())
            } else {
                Err(CliError::new(
                    ExitCode::InvalidInput,
                    "No target specified. Pass a Spotify URI/URL to fetch metadata, \
                     or a Spotify URI/URL and a file ID to download audio.",
                ))
            }
        }
        [a] => match classify(a)? {
            ArgKind::SpotifyTarget(uri) => cmd_fetch_metadata(&uri).await,
            ArgKind::FileId(_) => Err(CliError::new(
                ExitCode::InvalidInput,
                format!("'{a}' looks like a file ID but no Spotify URI/URL was given."),
            )),
        },
        [a, b] => match (classify(a)?, classify(b)?) {
            (ArgKind::SpotifyTarget(uri), ArgKind::FileId(file_id))
            | (ArgKind::FileId(file_id), ArgKind::SpotifyTarget(uri)) => {
                cmd_fetch_audio(&uri, &file_id, cli.output.as_deref()).await
            }
            (ArgKind::SpotifyTarget(_), ArgKind::SpotifyTarget(_)) => Err(CliError::new(
                ExitCode::InvalidInput,
                "Two Spotify URIs/URLs given — expected a Spotify URI/URL and a file ID.",
            )),
            (ArgKind::FileId(_), ArgKind::FileId(_)) => Err(CliError::new(
                ExitCode::InvalidInput,
                "Two file IDs given — expected a Spotify URI/URL and a file ID.",
            )),
        },
        _ => unreachable!("clap enforces num_args = 0..=2"),
    }
}

// ── Argument classification ───────────────────────────────────────────────────

enum ArgKind {
    SpotifyTarget(String),
    FileId(String),
}

fn classify(arg: &str) -> Result<ArgKind, CliError> {
    if arg.starts_with("spotify:") || arg.starts_with("https://") || arg.starts_with("http://") {
        Ok(ArgKind::SpotifyTarget(arg.to_owned()))
    } else if !arg.is_empty() && arg.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(ArgKind::FileId(arg.to_owned()))
    } else {
        Err(CliError::new(
            ExitCode::InvalidInput,
            format!("Unrecognised argument '{arg}': expected a Spotify URI/URL or a hex file ID."),
        ))
    }
}

// ── Command implementations ───────────────────────────────────────────────────

async fn cmd_fetch_metadata(uri: &str) -> Result<(), CliError> {
    info!("Fetch metadata mode: target={uri}");
    let creds = ensure_credentials().await?;
    let session = session::create_session(&creds).await?;
    let output = metadata::fetch_metadata(&session, uri).await?;
    print_json(&output)
}

async fn cmd_fetch_audio(
    uri: &str,
    file_id: &str,
    output: Option<&std::path::Path>,
) -> Result<(), CliError> {
    info!("Fetch audio mode: file_id={file_id}, track_uri={uri}");
    let creds = ensure_credentials().await?;
    let session = session::create_session(&creds).await?;
    audio::fetch_audio(&session, file_id, uri, output).await
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
            info!(
                "Using stored credentials (valid until {})",
                creds.expires_at
            );
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
                    "No stored credentials found. Run fetching-cli interactively to authenticate.",
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
