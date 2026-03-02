//! Metadata fetching and JSON serialization.
//!
//! Since librespot's metadata types don't implement `serde::Serialize`, we
//! convert them to serializable DTO structs that faithfully represent all the
//! data from the librespot types.

use librespot_core::{Session, SpotifyUri};
use librespot_metadata::audio::AudioFiles;
use librespot_metadata::Metadata;
use serde::Serialize;
use tracing::{debug, info, warn};

use crate::error::{CliError, ExitCode};

// ── URI normalisation ─────────────────────────────────────────────────────────

/// Convert an `https://open.spotify.com/{type}/{id}` URL to a
/// `spotify:{type}:{id}` URI, stripping any query string or fragment.
///
/// Inputs that are already `spotify:` URIs are returned unchanged.
pub fn normalise_uri(input: &str) -> Result<String, CliError> {
    const PREFIX: &str = "https://open.spotify.com/";

    if input.starts_with("spotify:") {
        return Ok(input.to_owned());
    }

    if let Some(rest) = input.strip_prefix(PREFIX) {
        let path = rest.split(&['?', '#']).next().unwrap_or(rest);
        let mut parts = path.splitn(2, '/');
        let item_type = parts.next().unwrap_or("").trim();
        let item_id = parts.next().unwrap_or("").trim();

        if item_type.is_empty() || item_id.is_empty() {
            return Err(CliError::new(
                ExitCode::InvalidInput,
                format!(
                    "Cannot parse Spotify URL — expected https://open.spotify.com/{{type}}/{{id}}, got: {input}"
                ),
            ));
        }

        return Ok(format!("spotify:{item_type}:{item_id}"));
    }

    Err(CliError::new(
        ExitCode::InvalidInput,
        format!(
            "Unrecognised Spotify identifier '{input}' — \
             expected a spotify:… URI or https://open.spotify.com/… URL"
        ),
    ))
}

fn parse_uri(input: &str) -> Result<SpotifyUri, CliError> {
    let normalised = normalise_uri(input)?;
    SpotifyUri::from_uri(&normalised).map_err(|e| {
        CliError::with_source(
            ExitCode::InvalidInput,
            format!("Failed to parse Spotify URI '{normalised}': {e}"),
            e.into(),
        )
    })
}

// ── Serializable DTOs ─────────────────────────────────────────────────────────

/// Audio file entry: format → file ID.
#[derive(Debug, Serialize)]
pub struct AudioFileDto {
    pub format: String,
    pub file_id: String,
}

fn audio_files_to_dto(files: &AudioFiles) -> Vec<AudioFileDto> {
    files
        .0
        .iter()
        .map(|(fmt, id)| AudioFileDto {
            format: format!("{fmt:?}"),
            file_id: id.to_string(),
        })
        .collect()
}

/// Image/cover entry.
#[derive(Debug, Serialize)]
pub struct ImageDto {
    pub file_id: String,
    pub size: String,
    pub width: i32,
    pub height: i32,
}

/// External ID (e.g. ISRC, UPC).
#[derive(Debug, Serialize)]
pub struct ExternalIdDto {
    pub external_type: String,
    pub id: String,
}

/// Artist reference.
#[derive(Debug, Serialize)]
pub struct ArtistDto {
    pub uri: String,
    pub name: String,
}

/// Artist with role.
#[derive(Debug, Serialize)]
pub struct ArtistWithRoleDto {
    pub uri: String,
    pub name: String,
    pub role: String,
}

/// Copyright entry.
#[derive(Debug, Serialize)]
pub struct CopyrightDto {
    pub copyright_type: String,
    pub text: String,
}

/// Disc with its tracks.
#[derive(Debug, Serialize)]
pub struct DiscDto {
    pub number: i32,
    pub name: String,
    pub tracks: Vec<String>,
}

/// Sale period.
#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct SalePeriodDto {
    pub start: String,
    pub end: String,
}

/// Restriction entry.
#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct RestrictionDto {
    pub catalogue: String,
    pub countries_allowed: Option<String>,
    pub countries_forbidden: Option<String>,
}

// ── Track DTO ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct TrackDto {
    pub uri: String,
    pub name: String,
    pub album: TrackAlbumDto,
    pub artists: Vec<ArtistDto>,
    pub artists_with_role: Vec<ArtistWithRoleDto>,
    pub number: i32,
    pub disc_number: i32,
    pub duration_ms: i32,
    pub popularity: i32,
    pub is_explicit: bool,
    pub external_ids: Vec<ExternalIdDto>,
    pub files: Vec<AudioFileDto>,
    pub previews: Vec<AudioFileDto>,
    pub alternatives: Vec<String>,
    pub tags: Vec<String>,
    pub has_lyrics: bool,
    pub language_of_performance: Vec<String>,
    pub original_title: String,
    pub version_title: String,
}

/// Album info embedded within a track (subset of full Album).
#[derive(Debug, Serialize)]
pub struct TrackAlbumDto {
    pub uri: String,
    pub name: String,
    pub artists: Vec<ArtistDto>,
    pub covers: Vec<ImageDto>,
    pub date: String,
    pub label: String,
    pub external_ids: Vec<ExternalIdDto>,
}

fn track_to_dto(t: &librespot_metadata::track::Track) -> TrackDto {
    TrackDto {
        uri: t.id.to_string(),
        name: t.name.clone(),
        album: TrackAlbumDto {
            uri: t.album.id.to_string(),
            name: t.album.name.clone(),
            artists: t.album.artists.iter().map(|a| ArtistDto {
                uri: a.id.to_string(),
                name: a.name.clone(),
            }).collect(),
            covers: t.album.covers.iter().map(|img| ImageDto {
                file_id: img.id.to_string(),
                size: format!("{:?}", img.size),
                width: img.width,
                height: img.height,
            }).collect(),
            date: t.album.date.to_string(),
            label: t.album.label.clone(),
            external_ids: t.album.external_ids.iter().map(|e| ExternalIdDto {
                external_type: e.external_type.clone(),
                id: e.id.clone(),
            }).collect(),
        },
        artists: t.artists.iter().map(|a| ArtistDto {
            uri: a.id.to_string(),
            name: a.name.clone(),
        }).collect(),
        artists_with_role: t.artists_with_role.iter().map(|a| ArtistWithRoleDto {
            uri: a.id.to_string(),
            name: a.name.clone(),
            role: format!("{:?}", a.role),
        }).collect(),
        number: t.number,
        disc_number: t.disc_number,
        duration_ms: t.duration,
        popularity: t.popularity,
        is_explicit: t.is_explicit,
        external_ids: t.external_ids.iter().map(|e| ExternalIdDto {
            external_type: e.external_type.clone(),
            id: e.id.clone(),
        }).collect(),
        files: audio_files_to_dto(&t.files),
        previews: audio_files_to_dto(&t.previews),
        alternatives: t.alternatives.iter().map(|u| u.to_string()).collect(),
        tags: t.tags.clone(),
        has_lyrics: t.has_lyrics,
        language_of_performance: t.language_of_performance.clone(),
        original_title: t.original_title.clone(),
        version_title: t.version_title.clone(),
    }
}

// ── Album DTO ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AlbumDto {
    pub uri: String,
    pub name: String,
    pub artists: Vec<ArtistDto>,
    pub album_type: String,
    pub label: String,
    pub date: String,
    pub popularity: i32,
    pub covers: Vec<ImageDto>,
    pub external_ids: Vec<ExternalIdDto>,
    pub discs: Vec<DiscDto>,
    pub copyrights: Vec<CopyrightDto>,
    pub reviews: Vec<String>,
    pub original_title: String,
    pub version_title: String,
    pub type_str: String,
    pub tracks: Vec<TrackDto>,
}

async fn album_to_dto(
    a: &librespot_metadata::album::Album,
    session: &Session,
) -> AlbumDto {
    let mut tracks = Vec::new();
    for track_uri in a.tracks() {
        match librespot_metadata::track::Track::get(session, track_uri).await {
            Ok(t) => tracks.push(track_to_dto(&t)),
            Err(e) => warn!("Failed to fetch track {}: {e}", track_uri),
        }
    }

    AlbumDto {
        uri: a.id.to_string(),
        name: a.name.clone(),
        artists: a.artists.iter().map(|ar| ArtistDto {
            uri: ar.id.to_string(),
            name: ar.name.clone(),
        }).collect(),
        album_type: format!("{:?}", a.album_type),
        label: a.label.clone(),
        date: a.date.to_string(),
        popularity: a.popularity,
        covers: a.covers.iter().map(|img| ImageDto {
            file_id: img.id.to_string(),
            size: format!("{:?}", img.size),
            width: img.width,
            height: img.height,
        }).collect(),
        external_ids: a.external_ids.iter().map(|e| ExternalIdDto {
            external_type: e.external_type.clone(),
            id: e.id.clone(),
        }).collect(),
        discs: a.discs.iter().map(|d| DiscDto {
            number: d.number,
            name: d.name.clone(),
            tracks: d.tracks.iter().map(|u| u.to_string()).collect(),
        }).collect(),
        copyrights: a.copyrights.iter().map(|c| CopyrightDto {
            copyright_type: format!("{:?}", c.copyright_type),
            text: c.text.clone(),
        }).collect(),
        reviews: a.reviews.clone(),
        original_title: a.original_title.clone(),
        version_title: a.version_title.clone(),
        type_str: a.type_str.clone(),
        tracks,
    }
}

// ── Full Artist DTO ───────────────────────────────────────────────────────────

/// Biography entry.
#[derive(Debug, Serialize)]
pub struct BiographyDto {
    pub text: String,
    pub portraits: Vec<ImageDto>,
}

/// Activity period entry.
#[derive(Debug, Serialize)]
#[serde(tag = "kind")]
pub enum ActivityPeriodDto {
    #[serde(rename = "timespan")]
    Timespan {
        start_year: u16,
        end_year: Option<u16>,
    },
    #[serde(rename = "decade")]
    Decade { decade: u16 },
}

/// Top-tracks per country.
#[derive(Debug, Serialize)]
pub struct TopTracksDto {
    pub country: String,
    pub track_uris: Vec<String>,
}

/// Full artist metadata (as opposed to the lightweight `ArtistDto` reference).
#[derive(Debug, Serialize)]
pub struct FullArtistDto {
    pub uri: String,
    pub name: String,
    pub popularity: i32,
    pub top_tracks: Vec<TopTracksDto>,
    pub album_uris: Vec<String>,
    pub single_uris: Vec<String>,
    pub compilation_uris: Vec<String>,
    pub appears_on_album_uris: Vec<String>,
    pub external_ids: Vec<ExternalIdDto>,
    pub portraits: Vec<ImageDto>,
    pub biographies: Vec<BiographyDto>,
    pub activity_periods: Vec<ActivityPeriodDto>,
    pub related_artist_uris: Vec<String>,
    pub is_portrait_album_cover: bool,
}

fn artist_full_to_dto(a: &librespot_metadata::artist::Artist) -> FullArtistDto {
    FullArtistDto {
        uri: a.id.to_string(),
        name: a.name.clone(),
        popularity: a.popularity,
        top_tracks: a
            .top_tracks
            .iter()
            .map(|tt| TopTracksDto {
                country: tt.country.clone(),
                track_uris: tt.tracks.iter().map(|u| u.to_string()).collect(),
            })
            .collect(),
        album_uris: a.albums_current().map(|u| u.to_string()).collect(),
        single_uris: a.singles_current().map(|u| u.to_string()).collect(),
        compilation_uris: a.compilations_current().map(|u| u.to_string()).collect(),
        appears_on_album_uris: a
            .appears_on_albums_current()
            .map(|u| u.to_string())
            .collect(),
        external_ids: a
            .external_ids
            .iter()
            .map(|e| ExternalIdDto {
                external_type: e.external_type.clone(),
                id: e.id.clone(),
            })
            .collect(),
        portraits: a
            .portraits
            .iter()
            .map(|img| ImageDto {
                file_id: img.id.to_string(),
                size: format!("{:?}", img.size),
                width: img.width,
                height: img.height,
            })
            .collect(),
        biographies: a
            .biographies
            .iter()
            .map(|b| BiographyDto {
                text: b.text.clone(),
                portraits: b
                    .portraits
                    .iter()
                    .map(|img| ImageDto {
                        file_id: img.id.to_string(),
                        size: format!("{:?}", img.size),
                        width: img.width,
                        height: img.height,
                    })
                    .collect(),
            })
            .collect(),
        activity_periods: a
            .activity_periods
            .iter()
            .map(|p| match p {
                librespot_metadata::artist::ActivityPeriod::Timespan {
                    start_year,
                    end_year,
                } => ActivityPeriodDto::Timespan {
                    start_year: *start_year,
                    end_year: *end_year,
                },
                librespot_metadata::artist::ActivityPeriod::Decade(d) => {
                    ActivityPeriodDto::Decade { decade: *d }
                }
            })
            .collect(),
        related_artist_uris: a.related.iter().map(|r| r.id.to_string()).collect(),
        is_portrait_album_cover: a.is_portrait_album_cover,
    }
}

// ── Playlist DTO ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct PlaylistDto {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub length: i32,
    pub track_uris: Vec<String>,
    pub is_collaborative: bool,
}

fn playlist_to_dto(p: &librespot_metadata::playlist::Playlist) -> PlaylistDto {
    PlaylistDto {
        uri: p.id.to_string(),
        name: p.attributes.name.clone(),
        description: p.attributes.description.clone(),
        length: p.length,
        track_uris: p.tracks().map(|u| u.to_string()).collect(),
        is_collaborative: p.attributes.is_collaborative,
    }
}

// ── Episode DTO ───────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct EpisodeDto {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub duration_ms: i32,
    pub number: i32,
    pub publish_time: String,
    pub show_name: String,
    pub language: String,
    pub is_explicit: bool,
    pub covers: Vec<ImageDto>,
    pub audio_files: Vec<AudioFileDto>,
    pub audio_previews: Vec<AudioFileDto>,
    pub keywords: Vec<String>,
    pub external_url: String,
    pub episode_type: String,
    pub has_music_and_talk: bool,
    pub is_audiobook_chapter: bool,
}

fn episode_to_dto(e: &librespot_metadata::episode::Episode) -> EpisodeDto {
    EpisodeDto {
        uri: e.id.to_string(),
        name: e.name.clone(),
        description: e.description.clone(),
        duration_ms: e.duration,
        number: e.number,
        publish_time: e.publish_time.to_string(),
        show_name: e.show_name.clone(),
        language: e.language.clone(),
        is_explicit: e.is_explicit,
        covers: e.covers.iter().map(|img| ImageDto {
            file_id: img.id.to_string(),
            size: format!("{:?}", img.size),
            width: img.width,
            height: img.height,
        }).collect(),
        audio_files: audio_files_to_dto(&e.audio),
        audio_previews: audio_files_to_dto(&e.audio_previews),
        keywords: e.keywords.clone(),
        external_url: e.external_url.clone(),
        episode_type: format!("{:?}", e.episode_type),
        has_music_and_talk: e.has_music_and_talk,
        is_audiobook_chapter: e.is_audiobook_chapter,
    }
}

// ── Show DTO ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ShowDto {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub publisher: String,
    pub language: String,
    pub is_explicit: bool,
    pub covers: Vec<ImageDto>,
    pub episode_uris: Vec<String>,
    pub copyrights: Vec<CopyrightDto>,
    pub keywords: Vec<String>,
    pub media_type: String,
    pub is_audiobook: bool,
    pub has_music_and_talk: bool,
}

fn show_to_dto(s: &librespot_metadata::show::Show) -> ShowDto {
    ShowDto {
        uri: s.id.to_string(),
        name: s.name.clone(),
        description: s.description.clone(),
        publisher: s.publisher.clone(),
        language: s.language.clone(),
        is_explicit: s.is_explicit,
        covers: s.covers.iter().map(|img| ImageDto {
            file_id: img.id.to_string(),
            size: format!("{:?}", img.size),
            width: img.width,
            height: img.height,
        }).collect(),
        episode_uris: s.episodes.iter().map(|u| u.to_string()).collect(),
        copyrights: s.copyrights.iter().map(|c| CopyrightDto {
            copyright_type: format!("{:?}", c.copyright_type),
            text: c.text.clone(),
        }).collect(),
        keywords: s.keywords.clone(),
        media_type: format!("{:?}", s.media_type),
        is_audiobook: s.is_audiobook,
        has_music_and_talk: s.has_music_and_talk,
    }
}

// ── Unified metadata output ───────────────────────────────────────────────────

/// Wrapper enum so the JSON output has a `type` discriminator.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum MetadataOutput {
    #[serde(rename = "track")]
    Track(TrackDto),
    #[serde(rename = "album")]
    Album(AlbumDto),
    #[serde(rename = "playlist")]
    Playlist(PlaylistDto),
    #[serde(rename = "episode")]
    Episode(EpisodeDto),
    #[serde(rename = "show")]
    Show(ShowDto),
    #[serde(rename = "artist")]
    Artist(FullArtistDto),
}

// ── Public fetch entry point ──────────────────────────────────────────────────

/// Fetch metadata for any Spotify URI/URL and return it as a
/// [`MetadataOutput`] ready for JSON serialization.
pub async fn fetch_metadata(
    session: &Session,
    input: &str,
) -> Result<MetadataOutput, CliError> {
    let uri = parse_uri(input)?;
    info!("Fetching metadata for {uri}");

    let item_type = uri.item_type().to_lowercase();
    debug!("Item type: {item_type}");

    match item_type.as_str() {
        "track" => {
            let track = librespot_metadata::track::Track::get(session, &uri)
                .await
                .map_err(|e| {
                    CliError::with_source(
                        ExitCode::ApiError,
                        format!("Failed to fetch track metadata: {e}"),
                        e.into(),
                    )
                })?;
            info!("Fetched track: {}", track.name);
            Ok(MetadataOutput::Track(track_to_dto(&track)))
        }
        "album" => {
            let album = librespot_metadata::album::Album::get(session, &uri)
                .await
                .map_err(|e| {
                    CliError::with_source(
                        ExitCode::ApiError,
                        format!("Failed to fetch album metadata: {e}"),
                        e.into(),
                    )
                })?;
            info!("Fetched album: {} ({} discs)", album.name, album.discs.len());
            Ok(MetadataOutput::Album(album_to_dto(&album, session).await))
        }
        "playlist" => {
            let playlist = librespot_metadata::playlist::Playlist::get(session, &uri)
                .await
                .map_err(|e| {
                    CliError::with_source(
                        ExitCode::ApiError,
                        format!("Failed to fetch playlist metadata: {e}"),
                        e.into(),
                    )
                })?;
            info!("Fetched playlist: {}", playlist.attributes.name);
            Ok(MetadataOutput::Playlist(playlist_to_dto(&playlist)))
        }
        "episode" => {
            let episode = librespot_metadata::episode::Episode::get(session, &uri)
                .await
                .map_err(|e| {
                    CliError::with_source(
                        ExitCode::ApiError,
                        format!("Failed to fetch episode metadata: {e}"),
                        e.into(),
                    )
                })?;
            info!("Fetched episode: {}", episode.name);
            Ok(MetadataOutput::Episode(episode_to_dto(&episode)))
        }
        "show" => {
            let show = librespot_metadata::show::Show::get(session, &uri)
                .await
                .map_err(|e| {
                    CliError::with_source(
                        ExitCode::ApiError,
                        format!("Failed to fetch show metadata: {e}"),
                        e.into(),
                    )
                })?;
            info!("Fetched show: {}", show.name);
            Ok(MetadataOutput::Show(show_to_dto(&show)))
        }
        "artist" => {
            let artist = librespot_metadata::artist::Artist::get(session, &uri)
                .await
                .map_err(|e| {
                    CliError::with_source(
                        ExitCode::ApiError,
                        format!("Failed to fetch artist metadata: {e}"),
                        e.into(),
                    )
                })?;
            info!("Fetched artist: {}", artist.name);
            Ok(MetadataOutput::Artist(artist_full_to_dto(&artist)))
        }
        other => Err(CliError::new(
            ExitCode::InvalidInput,
            format!("Unsupported Spotify item type: '{other}'"),
        )),
    }
}
