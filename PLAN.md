# ytmfast: native YouTube Music desktop client (Rust + egui)

Inspired by crmne/spotifast (same stack: eframe/egui 0.36 + fastframe crates),
styled after music.youtube.com. Not a fork: spotifast is ~100k lines bound to
librespot/Spotify Connect.

## Verified facts (2026-10-06, probed on this machine)

- `rustypipe` 0.11.4 (crates.io, features `rustls-tls-native-roots,userdata`)
  works for **metadata**: `music_search_tracks/albums/artists/playlists`,
  `music_playlist`, `music_album`, `music_artist`, `music_charts`,
  `music_new_albums`, `music_radio_track`, and with auth: `music_liked_tracks`,
  `music_saved_playlists/albums/artists`, `music_history`.
  Auth: `user_auth_set_cookie_txt(&str)` (cookies.txt contents), stored in
  `storage_dir`. Use `.authenticated()` on the query for user data.
- rustypipe **cannot stream** anymore: deobfuscation fails on every client
  except iOS, and iOS URLs return 403 after the first 1 MiB (no PO token).
  Upstream has had no deobf fix since 2025-04. Do not use `rp.player()`.
- **`yt-dlp`** (`/usr/bin/yt-dlp` 2026.08.19, deno present) downloads full
  audio fine: `yt-dlp -q --no-warnings -f 'bestaudio[ext=m4a]' -o <file>
  https://music.youtube.com/watch?v=<id>` takes ~4 s for a 6 MB track.
- `rodio` 0.22 with features `playback,symphonia-aac,symphonia-isomp4`
  decodes that m4a (`rodio::Decoder::try_from(File)`), 44.1 kHz stereo, with
  correct total_duration. Opus/webm is NOT decodable by symphonia: always m4a.

## Architecture (keep it small: ~8 files)

```
Cargo.toml
src/main.rs      eframe setup, fonts, icons, image loaders, app struct, Action apply
src/backend.rs   tokio runtime thread. Cmd enum in (std mpsc), Event enum out
                 (std mpsc + ctx.request_repaint()). Wraps rustypipe. Maps
                 rustypipe models to small own structs (Track, Album, Playlist,
                 Artist, Shelf) so UI never touches rustypipe types.
src/audio.rs     yt-dlp fetch into cache dir (~/.cache/ytmfast/audio/<id>.m4a,
                 reuse if exists, write to .part then rename), rodio player,
                 queue (Vec<Track> + index, shuffle, repeat), prefetch next track.
src/ui.rs        theme + sidebar + top bar + player bar + views (split into
                 src/ui/*.rs only if a file passes ~1500 lines)
```

- Deps: eframe 0.36 (default-features=false: glow, wayland, x11,
  default_fonts, persistence, accesskit), egui_extras 0.36 (features
  `http`, `image`, `svg`) + image (jpeg, png, webp) so thumbnails load straight
  from URLs via `egui::Image::from_uri`; rustypipe; tokio; serde; directories;
  rodio; `fastframe-icons` + `fastframe-fonts` + `fastframe-now-playing` from
  `git = "https://github.com/crmne/fastframe", tag = "v0.4.1"` (stock egui
  0.36.1 is fine; no egui fork). If a fastframe crate fights the build, drop it
  and use egui defaults: do not spend more than one attempt.
- UI never blocks: every network/yt-dlp/IO call runs on the backend runtime.
  Views emit `Action`s; apply after drawing (spotifast rule).
- Optimistic UI: clicking a track shows it as playing immediately with a
  "loading" state while yt-dlp fetches.

## Look (YouTube Music web, dark)

- Colors: bg `#030303`, sidebar `#030303` with 1px `#1f1f1f` divider, cards/
  hover `#212121`, player bar `#212121`, primary text `#FFFFFF`, secondary
  `#AAAAAA`, accent red `#FF0000` (progress bar, like icon, logo).
- Top bar (64px): hamburger + red "Music" logo left; pill-shaped search box
  (`#212121`, rounded 8, magnifier icon, ~480px) left-aligned after the
  sidebar; avatar/sign-in button right.
- Left sidebar (240px, collapsible to 72px icon rail): Home, Explore, Library;
  divider; "+ New playlist" (disabled, v2); list of user playlists
  (Liked Music first).
- Home: vertical scroll of shelves. Each shelf = title (bold 24px) + horizontal
  row of square cards (160px art, rounded 4, title 1 line, subtitle grey).
  Shelves: "Listen again" (history, logged-in), "Your playlists", "Top songs"
  (charts, as a 4-row grid of song rows), "New releases" (new albums).
- Explore: charts + new albums.
- Search: results grouped: Top result card + Songs list, then Albums, Artists,
  Playlists shelves. Enter in search box triggers it.
- Album/playlist page: large art (264px) left, title (bold 36), meta line
  (artist, year, track count, duration), Play (white pill) + Shuffle buttons;
  then track rows: index/hover play icon, thumb 40px, title, artist, album,
  duration right. Double-click row plays from that track (queue = list).
- Artist page: name header, Play/Radio button, Top songs list, Albums shelf,
  Singles shelf.
- Library: chips (Playlists, Songs = liked, Albums, Artists) with grid/list.
- Player bar (72px, bottom): thin red progress line across the full top edge
  (draggable seek), left: prev / play-pause (big) / next + time "1:23 / 3:45";
  center: thumb 40px + title + "artist • album"; right: volume slider,
  repeat, shuffle, queue toggle.
- Queue: right side panel (360px) "Up next" with current track highlighted,
  click to jump.
- Icons: lucide via fastframe-icons.

## Auth

Sign-in dialog: dropdown of browsers (brave, firefox, chromium, chrome) plus
"cookies.txt file path" field. Browser import runs
`yt-dlp --cookies-from-browser <b> --cookies <cfg>/cookies.txt --skip-download
-q https://music.youtube.com` then feeds the file to
`user_auth_set_cookie_txt`. Cookie file saved 0600 in config dir; pass
`--cookies <file>` to every yt-dlp download when signed in. Never log cookie
contents. Sign out deletes the file + `user_auth_remove_cookie`.
(OAuth device login is skipped: Google blocks it for YouTube API clients.)

## Milestones

1. **Core playback**: Cargo skeleton, backend thread, search tracks, click to
   play via yt-dlp + rodio, player bar (play/pause/seek/next/prev/volume),
   queue logic. Done when: `cargo build` clean, `cargo test` passes, searching
   "daft punk" and playing a result produces audio for its full length.
2. **YT Music shell**: theme, top bar, sidebar, Home/Explore shelves,
   album/playlist/artist pages, search grouped results, queue panel,
   prefetch, MPRIS via fastframe-now-playing, shuffle/repeat, window state
   persistence (eframe persistence: volume, sidebar collapsed, last view).
3. **Account + library**: sign-in dialog, liked music, saved playlists/albums/
   artists, history ("Listen again"), sidebar playlists, sign-out.

## Tests (smallest useful)

- Queue logic unit tests in audio.rs (next/prev/shuffle/repeat/jump/end of list).
- yt-dlp argument builder test (with and without cookies).
- `#[ignore]` network smoke test: search returns tracks; yt-dlp fetch of one
  id yields a decodable file with duration > 60 s.

## Out of scope (v1)

Playlist editing, like/unlike (rustypipe has no rating API), lyrics, video,
downloads UI, equalizer, visualisers, i18n, self-update, packaging.
Cache dir has no size cap (ponytail: add LRU eviction when it matters).
