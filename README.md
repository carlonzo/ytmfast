# ytmfast

A small, fast, native YouTube Music client for Linux, written in Rust with
[egui](https://github.com/emilk/egui). Inspired by
[spotifast](https://github.com/crmne/spotifast) and styled after
music.youtube.com.

Unofficial: not affiliated with or endorsed by YouTube or Google.

## Features

- Home, Explore, search (songs, albums, artists, playlists), album, artist and
  playlist pages
- Playback with queue, shuffle, repeat and prefetch of the next track
- Media keys and desktop integration through MPRIS
- Optional sign-in for your library, liked music and private playlists

## Requirements

- Linux (x86_64 or arm64), Wayland or X11, ALSA/PipeWire audio
- [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) on `PATH`, kept up to date. It
  fetches the audio; recent versions also need a JavaScript runtime such as
  `deno`.

## Install

Download the archive for your architecture from the
[releases page](../../releases), unpack it and put `ytmfast` somewhere on your
`PATH`:

```sh
tar -xzf ytmfast-*.tar.gz
install -Dm755 ytmfast ~/.local/bin/ytmfast
```

## Build from source

Needs a recent stable Rust toolchain and, on Debian/Ubuntu:

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

| Path | Contents |
| --- | --- |
| `~/.config/ytmfast/` | Cookies (only when signed in) |
| `~/.cache/ytmfast/audio/` | Downloaded tracks |
| `~/.local/share/ytmfast/` | App state |

## Releasing

Push a tag starting with `v`; the release workflow builds both architectures
and attaches the binaries to a GitHub release:

```sh
git tag v0.1.0 && git push origin v0.1.0
```
