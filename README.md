# ytmfast

A small, fast, native YouTube Music client for Linux and macOS, written in Rust with
[egui](https://github.com/emilk/egui). Inspired by
[spotifast](https://github.com/crmne/spotifast) and styled after
music.youtube.com.

Unofficial: not affiliated with or endorsed by YouTube or Google.

## Features

- Synced lyrics from LRCLIB (fallback: YouTube Music)
- Home, Explore, search (songs, albums, artists, playlists), album, artist and
  playlist pages
- Playback with queue, shuffle, repeat and prefetch of the next track
- Media keys and desktop integration through MPRIS
- Optional sign-in for your library, liked music and private playlists

## Requirements

- Linux (x86_64 or arm64), Wayland or X11, ALSA/PipeWire audio; or macOS on
  Apple Silicon (builds and passes tests in CI, otherwise untested)
- [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) on `PATH`, kept up to date. It
  fetches the audio; recent versions also need a JavaScript runtime such as
  `deno`.

## Install

### Linux

Download the archive for your architecture from the
[releases page](../../releases), unpack it and put `ytmfast` somewhere on your
`PATH`:

```sh
tar -xzf ytmfast-*.tar.gz
install -Dm755 ytmfast ~/.local/bin/ytmfast
```

### macOS (Apple Silicon)

Download the macOS `.zip` from the [releases page](../../releases), unzip it,
and drag `ytmfast.app` to `/Applications`. Clear the quarantine flag once for
this unsigned app, then open it normally from Finder:

```sh
xattr -dr com.apple.quarantine /Applications/ytmfast.app
```

Install the playback dependencies with Homebrew:

```sh
brew install yt-dlp deno
```

Homebrew's `yt-dlp` and `deno` are found automatically, including when the app
is opened from Finder.

## Build from source

Needs a recent stable Rust toolchain. On Debian/Ubuntu install the system
packages first; macOS needs none:

```sh
sudo apt install pkg-config libasound2-dev libssl-dev libxkbcommon-dev libwayland-dev
cargo build --release
./target/release/ytmfast
```

## Signing in

Sign-in is optional. The dialog either imports cookies from a browser where
you are already signed in to YouTube Music (via `yt-dlp
--cookies-from-browser`) or reads a `cookies.txt` file you point it at.

Only `youtube.com` and `google.com` cookies are kept. They are stored in
`~/.config/ytmfast/cookies.txt` with mode `0600` and removed when you sign
out. These cookies give full access to your Google session for YouTube, so
treat that file like a password.

## Shortcuts

| Key | Action |
| --- | --- |
| `Space` | Play / pause |
| `Alt` + `Left` | Back |
| `Enter` in the search box | Search |

## Files

On Linux (macOS uses the matching folders under `~/Library`):

| Path | Contents |
| --- | --- |
| `~/.config/ytmfast/` | Cookies (only when signed in) |
| `~/.cache/ytmfast/audio/` | Downloaded tracks; cache size limit in Settings |
| `~/.local/share/ytmfast/` | App state |
| `~/.local/share/applications/ytmfast.desktop`, `~/.local/share/icons/hicolor/scalable/apps/ytmfast.svg` | Launcher entry and icon (Linux), written at startup when missing or when the binary moved |

## Releasing

Push a tag starting with `v`; the release workflow builds Linux x86_64, Linux arm64
and macOS arm64 and attaches the binaries to a GitHub release:

```sh
git tag v0.1.0 && git push origin v0.1.0
```
