# fetching-cli

Minimal self-contained CLI for Spotify via [librespot](https://github.com/librespot-org/librespot).

All business output (metadata JSON, audio data) goes to **stdout**.  
All logging goes to **stderr**.

## Commands

### `auth` — Initial OAuth flow

Opens the default browser for Spotify authorization and prints credentials to stdout.

```sh
fetching-cli auth > creds.json
```

### `reauth` — Refresh token

Refreshes an expired access token using the stored refresh token.

```sh
fetching-cli reauth --credentials creds.json > creds.json
```

### `fetch` — Metadata

Fetch metadata for any Spotify URI or URL. Supports tracks, albums, playlists, episodes, and shows.

```sh
# By Spotify URI
fetching-cli fetch --credentials creds.json spotify:album:7FwAtuhhWivxvK4aPgyyUD

# By URL
fetching-cli fetch --credentials creds.json 'https://open.spotify.com/album/7FwAtuhhWivxvK4aPgyyUD'
```

### `fetch` — Audio download

Download decrypted audio by providing a file ID (from metadata) and the owning track URI.

```sh
fetching-cli fetch \
  --credentials creds.json \
  --track-uri spotify:track:6rqhFgbbKwnb9MLmUQDhG6 \
  abc123def456... > track.ogg
```

## Credentials

The `--credentials` flag accepts:
- **Inline JSON**: `--credentials '{"access_token":"...","refresh_token":"...","expires_at":123}'`
- **File path**: `--credentials creds.json`
- **Stdin** (when `--credentials` is omitted)

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

Errors are emitted as JSON to stderr.

## Logging

Control log verbosity via the `RUST_LOG` environment variable:

```sh
RUST_LOG=debug fetching-cli auth
RUST_LOG=warn fetching-cli fetch --credentials creds.json spotify:track:...
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
