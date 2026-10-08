# ytmfast v2: refinements (2026-10-08)

Three milestones, run in order, each on its own branch off master, each reviewed
before the next starts. Implementation is done by Codex (`gpt-6.1-sol`) through
`delegate .prompts/r<N>.txt codex gpt-6.1-sol`. Prompt files: `.prompts/r1.txt` to `r3.txt`.

## Findings

- **Icon**: there is no app icon. `main.rs` builds the `ViewportBuilder` with no
  `.with_icon`, so egui's default icon shows in the Dock and the title bar.
- **Double-click opens Terminal**: the release ships a bare Mach-O binary
  inside a tar.gz. Finder opens bare executables in Terminal.app. The fix is a
  real `ytmfast.app` bundle (Info.plist + .icns + ad-hoc codesign), the same as
  spotifast's `packaging/macos/bundle.sh`.
- **Hidden blocker found while checking the bundle**: an app started from
  Finder gets launchd's PATH (`/usr/bin:/bin:/usr/sbin:/sbin`). Homebrew's
  `/opt/homebrew/bin` is not on it, so `yt-dlp` and `deno` would not be found
  and nothing would play. On macOS the app has to add `/opt/homebrew/bin` and
  `/usr/local/bin` to PATH at startup.
- **Disk cache already exists**: `audio::fetch` writes
  `~/.cache/ytmfast/audio/<id>.m4a` and reuses it, so a song you have played
  before starts at once. It has **no size limit** and grows forever.
- **Prefetch**: only `peek_next()` (one track) is prefetched, and it is tracked
  in `App.prefetched: Option<String>`. Each `Cmd` runs as its own tokio task, so
  N prefetches would start N yt-dlp processes at once. They need a concurrency
  cap.
- **Cold start, measured 2026-10-08** (this machine): `yt-dlp -g` (URL
  extraction only: webpage, player API, JS challenge via deno) takes **~2.7 s**.
  A full `yt-dlp` download of a 3-4 MB track takes ~2.9 s, so the download
  itself is only **~0.3 s**. The file is fragmented MP4 (`ftyp dash`, `moov`
  up front, a `moof`+`mdat` pair every ~160 KB, about 10 s of audio), so it
  could be streamed. But a single GET without a range is throttled to about
  real time (106 s for 3.4 MB), so streaming would need 1 MB range requests
  like yt-dlp makes. **Decision: don't stream.** It would save ~0.3 s.
  Instead, hide the extraction behind hover prefetch (R2 item 8).
- **Lyrics in spotifast** (`src/lyrics.rs`, MIT): Spotify transcription first,
  then **LRCLIB** (`https://lrclib.net/api`: free, no key, LRC-synced lyrics).
  Matching uses `/get` (exact match on artist, title, album, duration), then a
  `/search` ranking where the track length must be within 30 s. We take the
  LRCLIB half and leave out Spotify. As a fallback for songs LRCLIB doesn't
  have, YouTube Music's own plain-text lyrics through rustypipe
  (`music_details(id).lyrics_id` → `music_lyrics(lyrics_id)`).
- `reqwest 0.12` is already in Cargo.lock through rustypipe, so a direct
  dependency adds no new crates.

## R1: icon + macOS app bundle

- `assets/app-icon.svg` (YouTube Music style: red disc, white ring, white
  play triangle), plus a committed 1024 px `assets/app-icon.png` rendered from
  it with `rsvg-convert`.
- Window/Dock icon: `.with_icon(eframe::icon_data::from_png_bytes(include_bytes!(…)))`
  and `.with_app_id("ytmfast")` (Wayland/X11 class).
- macOS PATH fix at the very top of `main()` (cfg target_os = "macos"):
  append `/opt/homebrew/bin` and `/usr/local/bin` when they're missing.
- `packaging/macos/Info.plist` + `packaging/macos/bundle.sh <binary> <out.app> <version>`
  (sips + iconutil → .icns, ad-hoc `codesign --force --sign -`).
- Release workflow: on macOS, run bundle.sh and ship
  `ytmfast-<tag>-aarch64-apple-darwin.zip` (made with `ditto -c -k --keepParent`)
  instead of the tar.gz. Linux keeps its tar.gz.
- README: macOS install = unzip, drag to /Applications,
  `xattr -dr com.apple.quarantine /Applications/ytmfast.app` (unsigned).

## R2: prefetch N + bounded LRU cache + Settings dialog

- `Queue::peek_ahead(n) -> Vec<&Track>`: the next n tracks auto-advance would
  play. It follows order, wraps with Repeat All, returns nothing with Repeat
  One, and skips the current id and duplicates. `peek_next` becomes
  `peek_ahead(1)`.
- `App.prefetched: HashSet<String>` (already requested). It is cleared when a
  new queue is built or shuffle/repeat changes. `maybe_prefetch` sends a
  `Prefetch` for each target that isn't in the set yet.
- Backend: a `tokio::sync::Semaphore` with 2 permits for **prefetch** downloads
  only. A user `Fetch` never waits on it. A Fetch for an id whose prefetch is
  already running joins it through the existing per-id `fetch_lock`.
- Cache limit: `audio::enforce_cache_limit(max_bytes, protect: &HashSet<String>)`.
  It lists `*.m4a` in the cache dir (skips `.dl.m4a` / `.part` temp files),
  sorts by mtime with the oldest first, and deletes until the total is at or
  under the cap. It never deletes protected ids (current track + prefetch
  set). "Last played" = mtime: touch the file (`File::set_modified(now)`) when
  playback starts. Runs on the backend after every successful fetch and when
  the cap changes. It uses `spawn_blocking` and only std::fs.
- Settings (persisted in the existing `Settings` struct with
  `#[serde(default)]`): `cache_max_mb: u32` (default 2048, 0 = keep only the
  current track and the prefetched ones) and `prefetch_count: u8` (default 3,
  range 0-10).
- Settings dialog: a gear button at the top right, next to the avatar, opens
  an `egui::Window`. It has a "Cache size limit (MB)" DragValue, a "Prefetch
  next N tracks" slider, a "Using X MB in N tracks" line (computed on the
  backend when the dialog opens), and a "Clear cache" button (clears
  everything except protected ids).
- Hover prefetch: resting the pointer on a song for 300 ms or more starts
  its download. A later click joins the download that is already running.

## R3: lyrics (synced)

- `src/lyrics.rs`: a port of the LRCLIB part of spotifast `src/lyrics.rs` (MIT;
  keep the attribution line in the module doc). It covers `Query`, `Line`,
  `Lyrics`, `active_line`, `parse_lrc`, `clean_title`/`clean_artist`,
  `score`/`pick` (30 s drift limit), `/get` then `/search`, and its unit tests.
  It sends `User-Agent: ytmfast/<version> (https://github.com/carlonzo/ytmfast)`
  as LRCLIB asks.
- Fallback: when LRCLIB has nothing, rustypipe `music_details` → `music_lyrics`
  (plain, unsynced).
- Cache: `~/.cache/ytmfast/lyrics/<videoId>.json`, keyed by video id. It
  stores "not found" too, kept for 7 days; found lyrics are kept forever.
  It's tiny and does not count toward the audio cap.
- Backend: `Cmd::Lyrics(Track)` → `Event::Lyrics { id, result: Option<Lyrics> }`.
  It's requested when the current track changes while the panel is open, or
  when the panel is opened.
- UI: a lyrics (mic) button in the player bar, next to the queue button,
  toggles a right side panel. Only one of queue and lyrics is open at a time.
  Synced lyrics: the active line is white and bold, other lines grey; the
  active line auto-scrolls to about a third from the top, and clicking a line
  seeks there. Plain lyrics: static text. States: "Loading…", "No lyrics
  found", "Instrumental". Source footer: "Lyrics: LRCLIB" / "Lyrics: YouTube Music".

## Later (not in this round)

- Streaming while downloading: rejected for now (see the cold start
  measurement). Revisit only if downloads turn out slow on the user's network
  (e.g. on a much longer track).
- A Linux `.desktop` file and icon install.
