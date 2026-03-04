//! Audio data fetching: download encrypted audio, decrypt, strip OGG header,
//! and write raw audio bytes to stdout.
//!
//! The caller must supply:
//! - A valid `FileId` (hex string from metadata)
//! - The owning track/episode URI (needed for the audio-key request)

use std::io::{self, Read, Write};

use librespot_audio::{AudioDecrypt, AudioFile};
use librespot_core::{file_id::FileId, Session, SpotifyUri};
use librespot_metadata::audio::{AudioFileFormat, AudioFiles, AudioItem};
use tracing::{debug, info, warn};

use crate::error::{CliError, ExitCode};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Buffer hint for `AudioFile::open` — 320 kbps in bytes/sec.
const AUDIO_BUFFER_HINT: usize = 320 * 1024 / 8; // 40_960 bytes/sec

/// Length of the proprietary Spotify header on OGG Vorbis files.
const SPOTIFY_OGG_HEADER_LEN: usize = 0xa7; // 167 bytes

/// Copy buffer size.
const COPY_BUF_SIZE: usize = 64 * 1024;

// ── Format detection ──────────────────────────────────────────────────────────

/// All known Spotify audio formats ordered by descending quality.
#[allow(dead_code)]
const FORMAT_PREFERENCE: &[AudioFileFormat] = &[
    AudioFileFormat::FLAC_FLAC,
    AudioFileFormat::FLAC_FLAC_24BIT,
    AudioFileFormat::OGG_VORBIS_320,
    AudioFileFormat::AAC_320,
    AudioFileFormat::MP3_320,
    AudioFileFormat::OTHER5,
    AudioFileFormat::MP3_256,
    AudioFileFormat::OGG_VORBIS_160,
    AudioFileFormat::AAC_160,
    AudioFileFormat::MP3_160,
    AudioFileFormat::MP3_160_ENC,
    AudioFileFormat::MP4_128,
    AudioFileFormat::OGG_VORBIS_96,
    AudioFileFormat::MP3_96,
    AudioFileFormat::AAC_48,
    AudioFileFormat::AAC_24,
    AudioFileFormat::XHE_AAC_24,
    AudioFileFormat::XHE_AAC_16,
    AudioFileFormat::XHE_AAC_12,
];

/// Find the format for a given file ID by checking all audio files maps.
///
/// When fetching by raw file ID we don't know the format upfront. We resolve
/// the track/episode's `AudioItem` and look up the format from its file maps.
fn find_format_for_file_id(file_id: &FileId, files: &AudioFiles) -> Option<AudioFileFormat> {
    for (fmt, id) in &files.0 {
        if id == file_id {
            return Some(*fmt);
        }
    }
    None
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Download audio for a specific file ID and write raw decrypted bytes to stdout.
///
/// # Arguments
/// - `session` — authenticated librespot session
/// - `file_id_hex` — hex-encoded file ID (from metadata output)
/// - `track_uri_str` — the owning track/episode URI (needed for audio key)
pub async fn fetch_audio(
    session: &Session,
    file_id_hex: &str,
    track_uri_str: &str,
    output: Option<&std::path::Path>,
) -> Result<(), CliError> {
    info!("Fetching audio for file_id={file_id_hex}, track_uri={track_uri_str}");

    // Parse the file ID from hex
    let file_id = parse_file_id(file_id_hex)?;

    // Parse the track URI
    let track_uri = SpotifyUri::from_uri(track_uri_str).map_err(|e| {
        CliError::with_source(
            ExitCode::InvalidInput,
            format!("Invalid track URI '{track_uri_str}': {e}"),
            e.into(),
        )
    })?;

    // Resolve the SpotifyId for the audio key request
    let spotify_id = match &track_uri {
        SpotifyUri::Track { id } => *id,
        SpotifyUri::Episode { id } => *id,
        _ => {
            return Err(CliError::new(
                ExitCode::InvalidInput,
                format!("URI must be a track or episode for audio download, got: {track_uri}"),
            ));
        }
    };

    // Resolve format by fetching AudioItem metadata
    let audio_item = AudioItem::get_file(session, track_uri.clone())
        .await
        .map_err(|e| {
            CliError::with_source(
                ExitCode::ApiError,
                format!("Failed to load AudioItem for {track_uri_str}: {e}"),
                e.into(),
            )
        })?;

    let format = match find_format_for_file_id(&file_id, &audio_item.files) {
        Some(fmt) => fmt,
        None => {
            return Err(CliError::new(
                ExitCode::InvalidInput,
                format!(
                    "File ID {file_id_hex} was not found in the track's audio files. \
                     Note: preview file IDs are not supported for audio download."
                ),
            ));
        }
    };

    info!("Resolved format: {format:?} for file_id {file_id_hex}");

    // AAC and MP4 variants use a proprietary Spotify container that is not
    // compatible with standard media players (VLC, ffmpeg, Navidrome, etc.).
    // OGG Vorbis and MP3 formats produce standard-compatible output.
    let is_aac_or_mp4 = matches!(
        format,
        AudioFileFormat::AAC_24
            | AudioFileFormat::AAC_48
            | AudioFileFormat::AAC_160
            | AudioFileFormat::AAC_320
            | AudioFileFormat::XHE_AAC_24
            | AudioFileFormat::XHE_AAC_16
            | AudioFileFormat::XHE_AAC_12
            | AudioFileFormat::MP4_128
    );
    if is_aac_or_mp4 {
        warn!(
            "Format {format:?} uses a proprietary Spotify container and will not be playable \
             by standard media players. Choose an OGG_VORBIS_* or MP3_* file ID instead."
        );
    }

    // Open the encrypted audio stream
    debug!("Opening audio file stream");
    let audio_file = AudioFile::open(session, file_id, AUDIO_BUFFER_HINT)
        .await
        .map_err(|e| {
            CliError::with_source(
                ExitCode::AudioDownloadError,
                format!("Failed to open audio file: {e}"),
                e.into(),
            )
        })?;

    // Request the decryption key
    debug!("Requesting audio key");
    let key = session
        .audio_key()
        .request(spotify_id, file_id)
        .await
        .map_err(|e| {
            CliError::with_source(
                ExitCode::AudioKeyError,
                format!("Audio key request failed for {track_uri_str}: {e}"),
                e.into(),
            )
        })?;
    debug!("Audio key obtained");

    // Set up decryption
    let raw_reader: Box<dyn Read> = match audio_file {
        AudioFile::Streaming(stream) => Box::new(stream),
        AudioFile::Cached(file) => Box::new(file),
    };
    let mut decrypted = AudioDecrypt::new(Some(key), raw_reader);

    // Strip OGG Vorbis header if applicable
    let is_ogg = AudioFiles::is_ogg_vorbis(format);
    if is_ogg {
        debug!("Stripping {SPOTIFY_OGG_HEADER_LEN}-byte Spotify OGG header");
        let mut header = [0u8; SPOTIFY_OGG_HEADER_LEN];
        decrypted.read_exact(&mut header).map_err(|e| {
            CliError::with_source(
                ExitCode::AudioDownloadError,
                format!("Failed to read OGG header: {e}"),
                e.into(),
            )
        })?;
    }

    // Stream decrypted audio to output destination
    let bytes_written = match output {
        Some(path) => {
            info!("Writing audio to {} ({format:?})", path.display());
            let mut file = std::fs::File::create(path).map_err(|e| {
                CliError::with_source(
                    ExitCode::AudioDownloadError,
                    format!("Failed to create output file '{}': {e}", path.display()),
                    e.into(),
                )
            })?;
            copy_to_writer(&mut decrypted, &mut file)?
        }
        None => {
            info!("Streaming audio to stdout ({format:?})");
            copy_to_writer(&mut decrypted, &mut io::stdout().lock())?
        }
    };
    info!("Wrote {bytes_written} bytes");

    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_file_id(hex_str: &str) -> Result<FileId, CliError> {
    let bytes = hex::decode(hex_str).map_err(|e| {
        CliError::with_source(
            ExitCode::InvalidInput,
            format!("Invalid file ID hex '{hex_str}': {e}"),
            e.into(),
        )
    })?;
    if bytes.len() != 20 {
        return Err(CliError::new(
            ExitCode::InvalidInput,
            format!(
                "File ID must be 20 bytes (40 hex chars), got {} bytes from '{hex_str}'",
                bytes.len()
            ),
        ));
    }
    let mut arr = [0u8; 20];
    arr.copy_from_slice(&bytes);
    Ok(FileId(arr))
}

fn copy_to_writer(reader: &mut dyn Read, writer: &mut dyn Write) -> Result<u64, CliError> {
    let mut buf = [0u8; COPY_BUF_SIZE];
    let mut total: u64 = 0;

    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => {
                return Err(CliError::with_source(
                    ExitCode::AudioDownloadError,
                    format!("Read error during audio streaming: {e}"),
                    e.into(),
                ));
            }
        };
        writer.write_all(&buf[..n]).map_err(|e| {
            CliError::with_source(
                ExitCode::AudioDownloadError,
                format!("Write error: {e}"),
                e.into(),
            )
        })?;
        total += n as u64;
    }

    writer.flush().map_err(|e| {
        CliError::with_source(
            ExitCode::AudioDownloadError,
            format!("Failed to flush output: {e}"),
            e.into(),
        )
    })?;

    Ok(total)
}
