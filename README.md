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
- Mini player in the YouTube Music phone layout, for a small window on a
  side screen; it switches to a one-row strip when the window is short
- Keeps playing when you close the window (see below)
- Cover art cached on disk, so pages you have seen load instantly
- Media keys and desktop integration through MPRIS
- Optional sign-in for your library, liked music and private playlists
- A notice in the top bar when a newer release is out, checked at launch and
  every 6 hours

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

## Closing the window

Closing the window while music is paused or stopped quits ytmfast. Closing it
while music plays keeps the music going:

- **macOS**: the window minimizes to the Dock. Click the Dock icon to bring it
  back; `Cmd+Q` quits.
- **Linux**: the window closes and ytmfast keeps running with an icon in the
  system tray (Waybar's `tray` module on Omarchy, KDE, GNOME with the
  AppIndicator extension). Click the icon, choose **Show ytmfast** from its
  menu, or launch ytmfast again to get the window back. **Quit** in the tray
  menu exits. Media keys and MPRIS (`playerctl`, Waybar's `mpris` module)
  keep working while the window is closed.

## Shortcuts

| Key | Action |
| --- | --- |
| `Space` | Play / pause |
| `Ctrl` + `M` (`Cmd` + `Shift` + `M` on macOS) | Mini player / full player |
| `Alt` + `Left` | Back |
| `Enter` in the search box | Search |

## Files

On Linux (macOS uses the matching folders under `~/Library`):

| Path | Contents |
| --- | --- |
| `~/.config/ytmfast/` | Cookies (only when signed in) |
| `~/.cache/ytmfast/audio/` | Downloaded tracks; cache size limit in Settings |
| `~/.cache/ytmfast/images/` | Cover art; trimmed to 192 MB at startup once over 256 MB |
| `~/.local/share/ytmfast/` | App state |

## Releasing

The update notice compares the app's version, `version` in `Cargo.toml`, with the
latest GitHub release. Bump it before tagging:

1. Set `version` in `Cargo.toml`, then run `cargo check` so `Cargo.lock` follows.
2. Commit both files, then push a tag with the same version. The release
   workflow refuses a tag that does not match `Cargo.toml`.

```sh
git tag v0.1.1 && git push origin v0.1.1
```

The workflow builds Linux x86_64, Linux arm64 and macOS arm64 and attaches the
binaries to a GitHub release.
