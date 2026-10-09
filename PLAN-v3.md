# ytmfast v3: image cache, mini player, background playback (2026-10-08)

## Done in this round

### Image cache
- `src/images.rs`: a disk-backed egui `BytesLoader` adapted from spotifast's
  `ArtLoader` (MIT). It replaces egui_extras' `http` loader, which cached in
  memory only, so every launch fetched every cover again.
- Files: `~/.cache/ytmfast/images/<fnv64>.img`, written atomically
  (`.part` then rename). Reads touch mtime; at startup the folder is trimmed
  to 192 MB, oldest first, once it is over 256 MB.
- Up to 48 MB of encoded bytes are kept in memory; beyond that the oldest are
  dropped and reloaded from disk if egui asks again.
- It runs on its own small tokio runtime, so it outlives the window (see
  Linux background mode).
- `images::sized(url, px)` asks Google's image servers for a bigger cover
  (`=w544-h544-…`) for the mini player. The next track's cover is prefetched.
- `images::accent_color` (from spotifast) picks the cover's main colour for
  the tinted backgrounds.

### Mini player
- `Ctrl+M` (`Cmd+Shift+M` on macOS) or the new button at the right end of the
  player bar. Tall windows get the phone layout: cover, title/artist, seek
  bar with times, shuffle/prev/play/next/repeat, "Up next". A short window
  (cover would be under 120 px) switches to a one-row strip.
- A pin button keeps the window above others (`WindowLevel::AlwaysOnTop`;
  tiling compositors may ignore it).
- Full and mini window sizes are remembered separately in Settings.

### Close keeps playing
- The per-frame work (backend events, auto-advance, MPRIS, tray) moved from
  `ui()` to eframe's `logic()`, which eframe keeps calling while the window
  is minimized or hidden.
- macOS: closing while playing cancels the close and minimizes to the Dock.
- Linux: hiding a window is not possible on Wayland, so the window really
  closes. `main` keeps the `App` and ticks it every 50 ms, then opens a new
  window (eframe reuses its event loop) when asked. A tray icon (`ksni`,
  StatusNotifierItem) offers Show, Play/Pause, Next, Previous and Quit. A
  socket at `$XDG_RUNTIME_DIR/ytmfast.sock` makes a second launch show the
  running window instead of starting another copy.
- Closing while paused or stopped still quits.

### Animations (from spotifast)
- Player bar and mini player background tinted with the cover colour,
  fading over 0.45 s when the song changes.
- Lyrics: the sung line lights up and the previous one fades over 0.22 s.
- Mini player: cover shrinks slightly while paused, seek bar thickens and
  shows a knob on hover, play disc grows on hover.
- Animated "now playing" bars on the current row in track lists and the
  queue (still while paused).

## spotifast features worth porting next

Roughly in order of value for the effort. Paths are in crmne/spotifast (MIT).

1. **Spectrum / waveform visualiser in the player bar** (`src/vis.rs`,
   `src/ui/player_bar.rs` `spectrum`/`waveform`). Needs an audio tap: a
   rodio `Source` wrapper that copies samples into a ring buffer, plus a
   small FFT. Bars in the cover colours, with peak caps and a bass glow.
2. **Omarchy theme following** (`src/app.rs` custom themes,
   `follows_omarchy`). Read `~/.config/omarchy/current/theme` and take its
   colours, live as the theme changes.
3. **Blurred cover backdrop for lyrics** (`src/images.rs` `LYRICS_BLUR`):
   full-panel softened art behind the lyrics, like the phone app.
4. **Local play history** (`src/history.rs`): a track counts after enough
   listening time; feeds a "Recently played" shelf on Home.
5. **Command-line control over the single-instance socket**
   (`src/single_instance.rs`): `ytmfast play-pause`, `next`, `now-playing`
   for Waybar scripts and keybindings. Our socket already exists; this
   adds verbs and replies.
6. **Keyboard shortcuts** (`src/ui/keys.rs`): Ctrl+arrows for prev/next,
   volume keys, Ctrl+L to search, Ctrl+Q to quit.
7. **Light/dark following the desktop** (`src/appearance.rs`, portal
   `org.freedesktop.appearance color-scheme`).
8. **Colour emoji** (`src/emoji.rs`, `fastframe-emoji`): song titles with
   emoji currently show tofu or monochrome glyphs.
9. **Ten-band equalizer + limiter** (`src/eq.rs`, `src/limiter.rs`).
10. **Winamp skin mini player** (`src/winamp.rs`, `src/ui/winamp/`) with
    marquee title. Large; our phone-style mini player covers the use case.
11. **MilkDrop visualiser** (`src/milkdrop/`). Very large; separate process.
12. **macOS notch / Touch Bar / Windows thumbbar** (`src/mac_notch.rs`,
    `src/thumbbar.rs`). Platform extras.

## Delegation (v3 round 2)

Split by file ownership so the two run in parallel, each on its own branch:

| Agent | Branch | Prompt | Items |
| --- | --- | --- | --- |
| codex | `v3-audio` | `.prompts/v3-codex.txt` | 1 visualiser, 9 equalizer + limiter, 4 play history, 5 CLI control |
| agy | `v3-ui` | `.prompts/v3-agy.txt` | palette, 2 Omarchy theme, 7 light/dark, 3 lyrics backdrop, 6 shortcuts, 8 colour emoji |

codex owns `audio.rs`, `backend.rs`, `tray.rs` and the player bar; agy owns
colours/theme, `lyrics.rs` and shortcuts. Merge `v3-audio` first; agy then
merges it into `v3-ui` (its palette refactor touches the player bar).
Items 10-12 (Winamp skins, MilkDrop, notch/Touch Bar/thumbbar) stay out.

```sh
delegate .prompts/v3-codex.txt codex gpt-6.1-sol
delegate .prompts/v3-agy.txt agy <model>
```

## Not covered / follow-ups
- macOS was not built locally (no Apple toolchain in this environment); the
  minimize-on-close path uses only egui viewport commands.
- Linux background mode was checked under Xvfb (close, tick, reopen with
  covers and fonts intact); the tray needs a session bus and an SNI host to
  test for real.
