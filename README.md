# fetching-cli

Minimal self-contained CLI for Spotify via [librespot](https://github.com/librespot-org/librespot).

All business output (metadata JSON, audio data) goes to **stdout** (or a file via `-o`).
All logging goes to **stderr**.

## Usage

```sh
fetching-cli [OPTIONS] [TARGET]...
```

### Authentication

On first run without arguments (in an interactive terminal), the browser OAuth flow starts
automatically and stores credentials to `~/.config/fetching-cli/credentials.json`.
Subsequent invocations reuse stored credentials, refreshing silently when expired.

```sh
# First run — opens browser, stores credentials
fetching-cli
```

To re-authenticate (e.g. to switch accounts), delete the credentials file and run again:

```sh
rm ~/.config/fetching-cli/credentials.json
fetching-cli
```

### Metadata

Fetch metadata for any Spotify URI or URL. Returns JSON.
Supports tracks, albums, playlists, episodes, shows, and artists.

```sh
# By Spotify URI
fetching-cli spotify:album:7FwAtuhhWivxvK4aPgyyUD

# By URL
fetching-cli 'https://open.spotify.com/album/7FwAtuhhWivxvK4aPgyyUD'
```

See [example JSON outputs](docs/) for each metadata type.

### Audio download

Download decrypted audio by providing both a Spotify URI and a file ID (from the metadata
`files` array). Argument order does not matter.

```sh
# Stream to stdout
fetching-cli spotify:track:6rqhFgbbKwnb9MLmUQDhG6 abc123def456... > track.ogg

# Write directly to a file
fetching-cli spotify:track:6rqhFgbbKwnb9MLmUQDhG6 abc123def456... -o track.ogg

# Either argument order works
fetching-cli abc123def456... spotify:track:6rqhFgbbKwnb9MLmUQDhG6 -o track.ogg
```

> **Note:** Only `OGG_VORBIS_*` and `MP3_*` file IDs produce output compatible with
> standard media players (VLC, ffmpeg, Navidrome, etc.). `AAC_*` and `MP4_*` formats
> use a proprietary Spotify container and will trigger a warning.

## Options

| Flag | Description |
|------|-------------|
| `-o, --output <PATH>` | Write audio to a file instead of stdout |
| `-h, --help` | Print help |
| `-V, --version` | Print version |

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Authentication error |
| 2 | Invalid input |
| 3 | Network error |
| 4 | API error |
| 5 | Audio key error |
| 6 | Audio download error |
| 7 | Serialization error |

Errors are emitted as JSON to stderr:

```json
{"error":{"code":2,"message":"..."}}
```

## Logging

Control log verbosity via `RUST_LOG` (default: `info`):

```sh
RUST_LOG=debug fetching-cli spotify:track:...
RUST_LOG=warn  fetching-cli spotify:track:... abc123... -o track.ogg
```

## Development

```sh
# Enter dev shell (requires Nix + direnv)
direnv allow

# Build
cargo build

# Test
cargo test

# Release
./bump_and_tag.sh        # auto-bump patch
./bump_and_tag.sh 0.2.0  # explicit version
```
