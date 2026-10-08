mod audio;
mod auth;
mod backend;
mod images;
mod lyrics;
#[cfg(target_os = "linux")]
mod tray;
mod ui;

use audio::{Player, Queue};
use backend::{
    ArtistPage, Backend, Cmd, Collection, Event, Home, Library, PageKind, SearchAll, SignInSource,
    Track,
};
use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use ui::{Action, Nav, View};
use lyrics::{Lyrics, LyricsState};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub volume: f32,
    pub sidebar_collapsed: bool,
    pub shuffle: bool,
    pub repeat: audio::Repeat,
    pub queue_open: bool,
    pub lyrics_open: bool,
    pub last_view: View,
    pub cache_max_mb: u32,
    #[serde(deserialize_with = "deserialize_prefetch_count")]
    pub prefetch_count: u8,
    pub mini: bool,
    pub mini_on_top: bool,
    /// Window size to go back to when leaving the mini player.
    pub full_size: [f32; 2],
    /// Last mini player size, reused the next time it opens.
    pub mini_size: [f32; 2],
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            volume: 1.0,
            sidebar_collapsed: false,
            shuffle: false,
            repeat: audio::Repeat::Off,
            queue_open: false,
            lyrics_open: false,
            last_view: View::Home,
            cache_max_mb: 2048,
            prefetch_count: 3,
            mini: false,
            mini_on_top: false,
            full_size: FULL_SIZE,
            mini_size: MINI_SIZE,
        }
    }
}

fn deserialize_prefetch_count<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u8, D::Error> {
    u8::deserialize(deserializer).map(|n| n.min(10))
}

impl Settings {
    /// Search holds a free query string; persist it as Home instead.
    fn sanitize_view(view: View) -> View {
        match view {
            View::Search(_) => View::Home,
            v => v,
        }
    }

    fn from_app(app: &App) -> Self {
        Self {
            volume: app.player.volume(),
            sidebar_collapsed: app.sidebar_collapsed,
            shuffle: app.queue.shuffle,
            repeat: app.queue.repeat,
            queue_open: app.queue_open,
            lyrics_open: app.lyrics_open,
            last_view: Self::sanitize_view(app.nav.current.clone()),
            cache_max_mb: app.cache_max_mb,
            prefetch_count: app.prefetch_count,
            mini: app.mini,
            mini_on_top: app.mini_on_top,
            full_size: app.full_size,
            mini_size: app.mini_size,
        }
    }

    fn apply(&self, app: &mut App) {
        app.player.set_volume(self.volume);
        app.sidebar_collapsed = self.sidebar_collapsed;
        app.queue.shuffle = self.shuffle;
        app.queue.repeat = self.repeat;
        app.queue_open = self.queue_open;
        app.lyrics_open = self.lyrics_open && !self.queue_open;
        app.cache_max_mb = self.cache_max_mb;
        app.sent_cache_max_mb = self.cache_max_mb;
        app.prefetch_count = self.prefetch_count.min(10);
        app.mini = self.mini;
        app.mini_on_top = self.mini_on_top;
        app.full_size = sane_size(self.full_size, FULL_SIZE, FULL_MIN);
        app.mini_size = sane_size(self.mini_size, MINI_SIZE, MINI_MIN);
        app.update_cache_policy();
        // Signed-in state comes from disk + rustypipe, not Settings: a
        // restored private view while signed out would show a broken page.
        // Album/Artist pages are public, so only Library and Playlist fall
        // back to Home when signed out.
        let mut view = Self::sanitize_view(self.last_view.clone());
        if is_private_view(&view) && !stored_cookie_present() {
            view = View::Home;
        }
        app.nav = Nav::new(view);
        app.ensure_view_loaded(&app.nav.current.clone());
    }
}

/// Default and minimum window sizes for the full app and the mini player.
const FULL_SIZE: [f32; 2] = [1200.0, 800.0];
const FULL_MIN: [f32; 2] = [800.0, 600.0];
const MINI_SIZE: [f32; 2] = [340.0, 560.0];
const MINI_MIN: [f32; 2] = [240.0, 96.0];

/// A stored window size, or `default` when it is missing, not finite, or
/// below `min` (a corrupt or hand-edited settings file).
fn sane_size(size: [f32; 2], default: [f32; 2], min: [f32; 2]) -> [f32; 2] {
    if size.iter().all(|v| v.is_finite()) && size[0] >= min[0] && size[1] >= min[1] {
        size
    } else {
        default
    }
}

/// Wakes whichever window is open. On Linux the window is closed in
/// background mode and a new one (with a new `egui::Context`) is opened
/// later, so threads hold this instead of a `Context`.
#[derive(Clone, Default)]
pub struct Repaint(Arc<Mutex<Option<egui::Context>>>);

impl Repaint {
    pub fn request_repaint(&self) {
        if let Some(ctx) = self.0.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            ctx.request_repaint();
        }
    }

    fn attach(&self, ctx: Option<&egui::Context>) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = ctx.cloned();
    }
}

/// Requests from outside the window: the tray icon and a second launch.
#[derive(Debug)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub enum Remote {
    Show,
    Quit,
    Action(Action),
}

/// True when a cookie jar file exists on disk, i.e. a sign-in (and the
/// private views it unlocks) is likely forthcoming via `Event::Auth`.
fn stored_cookie_present() -> bool {
    crate::auth::cookie_path().is_some_and(|p| {
        std::fs::symlink_metadata(&p).is_ok_and(|m| m.is_file())
    })
}

/// Views that require sign-in. Album/Artist pages are public; only Library
/// and Playlist fall back to Home when signed out (startup restore) or
/// navigate Home on sign-out.
fn is_private_view(view: &View) -> bool {
    matches!(view, View::Library | View::Playlist(_))
}
/// True when an event's generation matches the current auth generation:
/// the single guard behind every Collection/Artist/PageError/Library
/// drop-stale-result check. Pure so it is unit-testable.
fn generation_current(event_gen: u64, current: u64) -> bool {
    event_gen == current
}
/// Follow-up reload owed after an auth change. `#[must_use]`: an
/// `Event::Auth` branch that drops this without calling `reload` leaves
/// the open page stranded on a spinner, and the unused value warns.
/// Consuming it via `reload` issues exactly one `ensure_view_loaded`
/// re-request per auth event.
#[must_use]
struct AuthReload {
    generation: u64,
}
impl AuthReload {
    fn reload(self, app: &mut App) {
        let current = app.nav.current.clone();
        app.ensure_view_loaded(&current);
    }
}
/// Bookkeeping shared by both `Event::Auth` branches, so a sign-in (like a
/// sign-out) cannot strand an in-flight page on a spinner: bump the page
/// generation — results carrying the old generation are dropped by the
/// [`generation_current`] guards — and clear page loading sets/errors.
/// Returns the owed [`AuthReload`]: the caller MUST consume it via
/// `reload` so the open page is re-requested under the new generation.
/// Pure over its arguments so it is unit-testable without a GUI context.
fn auth_changed(
    generation: &mut u64,
    collections_loading: &mut HashSet<String>,
    collection_errors: &mut HashMap<String, String>,
    artists_loading: &mut HashSet<String>,
    artist_errors: &mut HashMap<String, String>,
) -> AuthReload {
    *generation = generation.wrapping_add(1);
    collections_loading.clear();
    collection_errors.clear();
    artists_loading.clear();
    artist_errors.clear();
    AuthReload { generation: *generation }
}

/// How long a radio failure notice stays in the player bar before expiring.
const RADIO_ERROR_TTL: Duration = Duration::from_secs(5);

/// Last track actually started via `start_track`, for the PlayList
/// double-click guard: one gesture starts playback once, so the second
/// PlayList of a double-click (same track, same list shape, < 500 ms after
/// the first) is a no-op.
#[derive(Debug, Clone)]
struct LastStart {
    track_id: String,
    list_len: usize,
    start: usize,
    at: Instant,
}

/// Second half of a double-click? Same track id, same list shape, same (or
/// unspecified) shuffle, within the double-click window. Pure so it is
/// unit-testable.
fn is_double_click_repeat(
    last: Option<&LastStart>,
    track_id: &str,
    list_len: usize,
    start: usize,
    shuffle: Option<bool>,
    current_shuffle: bool,
    now: Instant,
) -> bool {
    match last {
        Some(l) => {
            l.track_id == track_id
                && l.list_len == list_len
                && l.start == start
                // Play (Some(false)) then Shuffle (Some(true)) on the same
                // list is a new request, not a double-click echo.
                && (shuffle.is_none() || shuffle == Some(current_shuffle))
                && now.duration_since(l.at) < Duration::from_millis(500)
        }
        None => false,
    }
}

fn cache_protection(queue: &Queue, prefetched: &HashSet<String>, n: usize, loading: bool) -> HashSet<String> {
    let mut protect = prefetched.clone();
    if let Some(track) = queue.current() {
        protect.insert(track.id.clone());
    }
    // Keep upcoming cached files through loading so a zero cap does not evict them on advance.
    if loading {
        protect.extend(queue.peek_ahead(n).into_iter().map(|track| track.id.clone()));
    }
    protect
}

/// MPRIS/media-keys command mapped onto an existing Action or direct state
/// change. Pure so it is unit-testable.
fn map_mpris_command(cmd: fastframe_now_playing::Command, current_id: Option<&str>) -> Option<Action> {
    use fastframe_now_playing::Command as C;
    match cmd {
        C::Play => Some(Action::Resume),
        C::Pause => Some(Action::Pause),
        C::PlayPause => Some(Action::TogglePlayPause),
        C::Stop => Some(Action::Pause),
        C::Next => Some(Action::NextTrack(true)),
        C::Previous => Some(Action::PrevTrack),
        C::SeekBy(ms) => Some(Action::SeekRelative(ms)),
        C::SetPosition { track_id, position } => {
            if Some(track_id.as_str()) == current_id {
                Some(Action::Seek(position))
            } else {
                None
            }
        }
        C::SetVolume(v) => Some(Action::SetVolume(v as f32)),
        C::SetShuffle(s) => Some(Action::SetShuffle(s)),
        C::SetRepeat(r) => Some(Action::SetRepeat(match r {
            fastframe_now_playing::Repeat::Off => audio::Repeat::Off,
            fastframe_now_playing::Repeat::Track => audio::Repeat::One,
            fastframe_now_playing::Repeat::Playlist => audio::Repeat::All,
        })),
        C::Raise => Some(Action::Raise),
        C::Quit => Some(Action::Quit),
        C::OpenUri(_) => None,
    }
}

pub struct App {
    pub backend: Backend,
    pub player: Player,
    pub queue: Queue,
    pub nav: Nav,
    pub sidebar_collapsed: bool,
    pub search_input: String,
    pub is_loading: bool,
    pub fetch_error: Option<String>,
    pub is_active_playback: bool,
    pub queue_open: bool,
    pub lyrics_open: bool,
    pub lyrics: Option<(String, LyricsState)>,
    lyrics_cache: HashMap<String, Option<Lyrics>>,
    pub prefetched: HashSet<String>,
    // ponytail: bounded by IDs hovered in this session; evict the set if sessions grow huge.
    hover_prefetched: HashSet<String>,
    pub cache_max_mb: u32,
    sent_cache_max_mb: u32,
    sent_cache_policy: Option<(u64, HashSet<String>)>,
    pub prefetch_count: u8,
    pub show_settings: bool,
    pub cache_usage: Option<(u64, usize)>,
    pub radio_pending: Option<String>,
    /// Transient radio failure notice with its timestamp: shown in the
    /// player bar but auto-expires after `RADIO_ERROR_TTL` so it never
    /// masquerades as the current track's error. Real fetch errors of the
    /// current track stay in `fetch_error` and do not expire.
    pub radio_error: Option<(String, Instant)>,
    /// Last track actually started, for the PlayList double-click guard.
    last_start: Option<LastStart>,
    pub now_playing: Option<fastframe_now_playing::NowPlaying>,
    pub repaint: Repaint,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    remote_tx: mpsc::Sender<Remote>,
    remote_rx: mpsc::Receiver<Remote>,
    /// Viewport commands from actions, sent by the next `logic` pass.
    viewport_cmds: Vec<egui::ViewportCommand>,
    /// The window was closed while music played (Linux): keep running.
    background: bool,
    /// Open the window again (Linux background mode).
    show_requested: bool,
    /// An explicit Quit: let the close through and exit.
    quit_requested: bool,
    /// Mini player state, and the mode the current window was last set up
    /// for (`None` for a window that has not been set up yet).
    pub mini: bool,
    pub mini_on_top: bool,
    pub full_size: [f32; 2],
    pub mini_size: [f32; 2],
    applied_mini: Option<bool>,
    mode_changed_at: Instant,

    // Page cache. Search/collection/artist maps are unbounded for the
    // session (ponytail: add eviction when it matters).
    pub home: Option<Home>,
    pub home_loading: bool,
    pub home_error: Option<String>,

    pub search_results: HashMap<String, SearchAll>,
    pub search_loading: HashSet<String>,
    pub search_errors: HashMap<String, String>,

    pub collections: HashMap<String, Collection>,
    pub collections_loading: HashSet<String>,
    pub collection_errors: HashMap<String, String>,

    pub artists: HashMap<String, ArtistPage>,
    pub artists_loading: HashSet<String>,
    pub artist_errors: HashMap<String, String>,

    // Auth + library (M3). Signed-in state comes from the cookie file on disk.
    pub signed_in: bool,
    pub auth_error: Option<String>,
    pub show_sign_in: bool,
    pub sign_in_busy: bool,
    pub sign_in_browser: String,
    pub sign_in_file: String,
    pub show_user_menu: bool,
    pub auth_generation: u64,
    pub library: Option<Library>,
    pub library_loading: bool,
    pub library_error: Option<String>,
    pub library_chip: LibraryChip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryChip {
    Playlists,
    Songs,
    Albums,
    Artists,
}

impl App {
    fn request_lyrics(&mut self) {
        if let Some(track) = self.queue.current().cloned() {
            self.lyrics = Some((track.id.clone(), LyricsState::Loading));
            self.backend.send(Cmd::Lyrics(track));
        }
    }

    fn maybe_lyrics(&mut self) {
        if !self.lyrics_open { return; }
        let Some(track) = self.queue.current() else {
            self.lyrics = None;
            return;
        };
        if self.lyrics.as_ref().is_some_and(|(id, _)| *id == track.id) { return; }
        if let Some(found) = self.lyrics_cache.get(&track.id) {
            self.lyrics = Some((track.id.clone(), LyricsState::Ready(found.clone())));
        } else {
            self.request_lyrics();
        }
    }

    pub fn new(repaint: Repaint) -> Self {
        let backend = Backend::new(repaint.clone());
        let (remote_tx, remote_rx) = mpsc::channel();
        let player = Player::new();
        let fetch_error = player.error.clone();
        backend.send(Cmd::LoadHome);

        Self {
            backend,
            player,
            queue: Queue::new(Vec::new(), 0),
            nav: Nav::new(View::Home),
            sidebar_collapsed: false,
            search_input: String::new(),
            is_loading: false,
            fetch_error,
            is_active_playback: false,
            queue_open: false,
            lyrics_open: false,
            lyrics: None,
            lyrics_cache: HashMap::new(),
            prefetched: HashSet::new(),
            hover_prefetched: HashSet::new(),
            cache_max_mb: 2048,
            sent_cache_max_mb: 2048,
            sent_cache_policy: None,
            prefetch_count: 3,
            show_settings: false,
            cache_usage: None,
            radio_pending: None,
            radio_error: None,
            last_start: None,
            now_playing: None,
            repaint,
            remote_tx,
            remote_rx,
            viewport_cmds: Vec::new(),
            background: false,
            show_requested: false,
            quit_requested: false,
            mini: false,
            mini_on_top: false,
            full_size: FULL_SIZE,
            mini_size: MINI_SIZE,
            applied_mini: None,
            mode_changed_at: Instant::now(),
            home: None,
            home_loading: true,
            home_error: None,
            search_results: HashMap::new(),
            search_loading: HashSet::new(),
            search_errors: HashMap::new(),
            collections: HashMap::new(),
            collections_loading: HashSet::new(),
            collection_errors: HashMap::new(),
            artists: HashMap::new(),
            artists_loading: HashSet::new(),
            artist_errors: HashMap::new(),
            signed_in: false,
            auth_error: None,
            show_sign_in: false,
            sign_in_busy: false,
            sign_in_browser: "firefox".to_string(),
            sign_in_file: String::new(),
            show_user_menu: false,
            auth_generation: 0,
            library: None,
            library_loading: false,
            library_error: None,
            library_chip: LibraryChip::Playlists,
        }
    }

    fn start_track(&mut self, track: Track) {
        self.player.stop();
        self.is_active_playback = true;
        self.is_loading = true;
        self.fetch_error = None;
        self.prefetched.clear();
        self.update_cache_policy();
        self.radio_error = None;
        self.last_start = Some(LastStart {
            track_id: track.id.clone(),
            list_len: self.queue.tracks.len(),
            // Track-space index (not order-space pos: shuffle reorders).
            start: self.queue.order.get(self.queue.pos).copied().unwrap_or(0),
            at: Instant::now(),
        });
        self.backend.send(Cmd::Fetch(track));
    }

    /// Error line for the player bar: a real fetch error of the current
    /// track wins; an unexpired radio failure shows only when there is none.
    pub fn player_error(&self) -> Option<&str> {
        if let Some(err) = &self.fetch_error {
            return Some(err);
        }
        self.radio_error.as_ref().and_then(|(msg, at)| {
            (at.elapsed() < RADIO_ERROR_TTL).then_some(msg.as_str())
        })
    }

    pub fn cache_protect(&self) -> HashSet<String> {
        cache_protection(&self.queue, &self.prefetched, self.prefetch_count as usize, self.is_loading)
    }

    fn update_cache_policy(&mut self) {
        let max_bytes = u64::from(self.sent_cache_max_mb) * 1024 * 1024;
        let protect = self.cache_protect();
        if self.sent_cache_policy.as_ref().is_some_and(|(sent_max, sent_protect)| {
            *sent_max == max_bytes && *sent_protect == protect
        }) {
            return;
        }
        self.backend.send(Cmd::SetCachePolicy {
            max_bytes,
            protect: protect.clone(),
        });
        self.sent_cache_policy = Some((max_bytes, protect));
    }

    fn maybe_prefetch(&mut self) {
        if !self.is_active_playback || self.is_loading {
            self.update_cache_policy();
            return;
        }
        let targets = audio::prefetch_targets(&self.queue, self.prefetch_count as usize, &self.prefetched);
        self.prefetched.extend(targets.iter().map(|t| t.id.clone()));
        self.update_cache_policy();
        for track in targets {
            self.backend.send(Cmd::Prefetch(track));
        }
    }

    fn auto_advance(&mut self) {
        if let Some(track) = self.queue.next(false).cloned() {
            self.start_track(track);
        } else {
            self.is_active_playback = false;
            self.is_loading = false;
            self.player.stop();
            self.update_cache_policy();
        }
    }

    pub fn navigate(&mut self, view: View) {
        self.nav.go(view.clone());
        self.ensure_view_loaded(&view);
    }

    pub fn ensure_view_loaded(&mut self, view: &View) {
        match view {
            View::Home | View::Explore => {
                if self.home.is_none() && !self.home_loading {
                    self.home_loading = true;
                    self.home_error = None;
                    self.backend.send(Cmd::LoadHome);
                }
            }
            View::Library => {
                if self.signed_in && self.library.is_none() && !self.library_loading {
                    self.library_loading = true;
                    self.library_error = None;
                    self.backend.send(Cmd::LoadLibrary(self.auth_generation));
                }
            }
            View::Search(q) => {
                if !self.search_results.contains_key(q) && !self.search_loading.contains(q) {
                    self.search_loading.insert(q.clone());
                    self.search_errors.remove(q);
                    self.backend.send(Cmd::Search(q.clone()));
                }
            }
            View::Album(id) => {
                if !self.collections.contains_key(id) && !self.collections_loading.contains(id) {
                    self.collections_loading.insert(id.clone());
                    self.collection_errors.remove(id);
                    self.backend.send(Cmd::OpenAlbum(id.clone(), self.auth_generation));
                }
            }
            View::Playlist(id) => {
                if !self.collections.contains_key(id) && !self.collections_loading.contains(id) {
                    self.collections_loading.insert(id.clone());
                    self.collection_errors.remove(id);
                    self.backend
                        .send(Cmd::OpenPlaylist(id.clone(), self.auth_generation));
                }
            }
            View::Artist(id) => {
                if !self.artists.contains_key(id) && !self.artists_loading.contains(id) {
                    self.artists_loading.insert(id.clone());
                    self.artist_errors.remove(id);
                    self.backend.send(Cmd::OpenArtist(id.clone(), self.auth_generation));
                }
            }
        }
    }

    fn apply_action(&mut self, action: Action) {
        match action {
            Action::Prefetch(track) => {
                if !self.prefetched.contains(&track.id) && self.hover_prefetched.insert(track.id.clone()) {
                    self.backend.send(Cmd::Prefetch(track));
                }
            }
            Action::ShowSettings => {
                self.show_settings = true;
                self.cache_usage = None;
                self.backend.send(Cmd::CacheUsage);
            }
            Action::SetPrefetchCount(count) => {
                if self.prefetch_count != count {
                    self.prefetch_count = count.min(10);
                    let ahead: HashSet<_> = self.queue.peek_ahead(self.prefetch_count as usize)
                        .into_iter().map(|track| track.id.clone()).collect();
                    self.prefetched.retain(|id| ahead.contains(id));
                    self.maybe_prefetch();
                }
            }
            Action::SetCacheMaxMb(mb) => {
                self.cache_max_mb = mb;
                if self.sent_cache_max_mb != mb {
                    self.sent_cache_max_mb = mb;
                    let max_bytes = u64::from(mb) * 1024 * 1024;
                    let protect = self.cache_protect();
                    self.backend.send(Cmd::EnforceCache {
                        max_bytes,
                        protect: protect.clone(),
                    });
                    self.sent_cache_policy = Some((max_bytes, protect));
                }
            }
            Action::ClearCache => {
                self.cache_usage = None;
                self.hover_prefetched.clear();
                self.backend.send(Cmd::ClearCache { protect: self.cache_protect() });
            }
            Action::PlayList {
                tracks,
                start,
                shuffle,
            } => {
                // One gesture starts playback once: swallow only the second
                // half of a double-click (same track, same list shape, same
                // or unspecified shuffle, fast).
                let is_dbl = match tracks.get(start) {
                    Some(t) => is_double_click_repeat(
                        self.last_start.as_ref(),
                        &t.id,
                        tracks.len(),
                        start,
                        shuffle,
                        self.queue.shuffle,
                        Instant::now(),
                    ),
                    None => false,
                };
                if is_dbl {
                    return;
                }
                // A fresh explicit Play/Shuffle/Jump wins over a late radio
                // result: it must never replace the list the user started.
                self.radio_pending = None;
                let prev_repeat = self.queue.repeat;
                let prev_shuffle = match shuffle {
                    Some(s) => s,
                    None => self.queue.shuffle,
                };
                self.prefetched.clear();
                self.queue = Queue::new(tracks, start);
                self.queue.repeat = prev_repeat;
                self.queue.set_shuffle(prev_shuffle);
                if let Some(track) = self.queue.current().cloned() {
                    self.start_track(track);
                } else {
                    self.update_cache_policy();
                }
            }
            Action::TogglePlayPause => {
                self.player.toggle_pause();
            }
            Action::NextTrack(user) => {
                if let Some(track) = self.queue.next(user).cloned() {
                    self.start_track(track);
                } else {
                    self.is_active_playback = false;
                    self.is_loading = false;
                    self.player.stop();
                    self.update_cache_policy();
                }
            }
            Action::ToggleShuffle => {
                let new_shuf = !self.queue.shuffle;
                self.queue.set_shuffle(new_shuf);
                self.prefetched.clear();
                self.maybe_prefetch();
            }
            Action::CycleRepeat => {
                self.queue.cycle_repeat();
                self.prefetched.clear();
                self.maybe_prefetch();
            }
            Action::Jump(pos) => {
                if self.is_loading && pos == self.queue.pos {
                    return;
                }
                self.radio_pending = None;
                if let Some(track) = self.queue.jump(pos).cloned() {
                    self.start_track(track);
                }
            }
            Action::ToggleLyrics => {
                self.lyrics_open = !self.lyrics_open;
                if self.lyrics_open { self.queue_open = false; }
                self.maybe_lyrics();
            }
            Action::RetryLyrics => self.request_lyrics(),
            Action::ToggleQueue => {
                self.queue_open = !self.queue_open;
                if self.queue_open { self.lyrics_open = false; }
            }
            Action::SetShuffle(s) => {
                if self.queue.shuffle == s { return; }
                self.queue.set_shuffle(s);
                self.prefetched.clear();
                self.maybe_prefetch();
            }
            Action::SetRepeat(r) => {
                if self.queue.repeat == r { return; }
                self.queue.repeat = r;
                self.prefetched.clear();
                self.maybe_prefetch();
            }
            Action::Resume => {
                if self.player.is_paused() {
                    self.player.toggle_pause();
                }
            }
            Action::Pause => {
                if !self.player.is_paused() {
                    self.player.toggle_pause();
                }
            }
            Action::SeekRelative(ms) => {
                let Some(dur) = self.player.duration() else {
                    return;
                };
                let cur = self.player.position();
                let target = if ms >= 0 {
                    cur + Duration::from_millis(ms as u64)
                } else {
                    cur.saturating_sub(Duration::from_millis(ms.unsigned_abs()))
                };
                let target = target.min(dur);
                self.player.seek(target);
                if let Some(np) = &self.now_playing {
                    np.seeked(target);
                }
            }
            Action::Raise => self.raise(),
            Action::Quit => {
                self.quit_requested = true;
                self.viewport_cmds.push(egui::ViewportCommand::Close);
            }
            Action::ToggleMini => self.mini = !self.mini,
            Action::ToggleOnTop => {
                self.mini_on_top = !self.mini_on_top;
                self.applied_mini = None;
            }
            Action::PrevTrack => {
                if self.player.position() > Duration::from_secs(3) {
                    self.player.seek(Duration::ZERO);
                    if let Some(np) = &self.now_playing {
                        np.seeked(Duration::ZERO);
                    }
                } else if let Some(track) = self.queue.prev().cloned() {
                    self.start_track(track);
                }
            }
            Action::Seek(pos) => {
                let Some(dur) = self.player.duration() else {
                    return;
                };
                let pos = pos.min(dur);
                self.player.seek(pos);
                if let Some(np) = &self.now_playing {
                    np.seeked(pos);
                }
            }
            Action::SetVolume(vol) => {
                self.player.set_volume(vol);
            }
            Action::Navigate(view) => {
                self.navigate(view);
            }
            Action::Back => {
                if self.nav.back() {
                    let cur = self.nav.current.clone();
                    self.ensure_view_loaded(&cur);
                }
            }
            Action::ToggleSidebar => {
                self.sidebar_collapsed = !self.sidebar_collapsed;
            }
            Action::ShowSignIn => {
                self.show_sign_in = true;
                self.show_user_menu = false;
            }
            Action::HideSignIn => {
                // A sign-in may still be running on the backend; keep
                // `sign_in_busy` so the buttons stay disabled and no second
                // import can start. Only `Event::Auth` clears it.
                self.show_sign_in = false;
                if !self.sign_in_busy {
                    self.auth_error = None;
                }
            }
            Action::SignInBrowser(browser) => {
                self.sign_in_busy = true;
                self.auth_error = None;
                self.backend.send(Cmd::SignIn(SignInSource::Browser(browser)));
            }
            Action::SignInFile(path) => {
                self.sign_in_busy = true;
                self.auth_error = None;
                self.backend.send(Cmd::SignIn(SignInSource::File(path.into())));
            }
            Action::SignOut => {
                self.show_user_menu = false;
                self.auth_generation = self.auth_generation.wrapping_add(1);
                // Only private views navigate Home: Album/Artist are public
                // and stay open (reloaded on sign-out in `Event::Auth`).
                if is_private_view(&self.nav.current) {
                    self.nav.go(View::Home);
                }
                self.backend.send(Cmd::SignOut);
            }
            Action::ToggleUserMenu => {
                self.show_user_menu = !self.show_user_menu;
            }
            Action::SetLibraryChip(chip) => {
                self.library_chip = chip;
            }
            Action::Radio(video_id) => {
                if self.radio_pending.as_deref() == Some(video_id.as_str()) {
                    return;
                }
                self.radio_pending = Some(video_id.clone());
                self.radio_error = None;
                self.backend.send(Cmd::Radio(video_id));
            }
            Action::Retry(view) => match view {
                View::Home | View::Explore => {
                    self.home_loading = true;
                    self.home_error = None;
                    self.backend.send(Cmd::LoadHome);
                }
                View::Search(q) => {
                    self.search_errors.remove(&q);
                    self.search_loading.insert(q.clone());
                    self.backend.send(Cmd::Search(q));
                }
                View::Album(id) => {
                    self.collection_errors.remove(&id);
                    self.collections_loading.insert(id.clone());
                    self.backend.send(Cmd::OpenAlbum(id, self.auth_generation));
                }
                View::Playlist(id) => {
                    self.collection_errors.remove(&id);
                    self.collections_loading.insert(id.clone());
                    self.backend.send(Cmd::OpenPlaylist(id, self.auth_generation));
                }
                View::Artist(id) => {
                    self.artist_errors.remove(&id);
                    self.artists_loading.insert(id.clone());
                    self.backend.send(Cmd::OpenArtist(id, self.auth_generation));
                }
                View::Library => {
                    self.library_error = None;
                    self.library_loading = true;
                    self.backend.send(Cmd::LoadLibrary(self.auth_generation));
                }
            },
        }
    }
}

impl App {
    /// Everything that must keep running without a visible window: backend
    /// events, auto-advance, media keys, the tray. `ctx` is `None` in Linux
    /// background mode, where the caller polls on a timer instead.
    fn tick(&mut self, ctx: Option<&egui::Context>) {
        // Poll backend events
        while let Some(event) = self.backend.try_recv() {
            match event {
                Event::Lyrics { id, result } => {
                    if self.queue.current().is_some_and(|track| track.id == id) {
                        let state = match result {
                            Ok(found) => {
                                // ponytail: at most 64 session results; disk cache retains evicted lyrics.
                                if self.lyrics_cache.len() >= 64 && !self.lyrics_cache.contains_key(&id) {
                                    self.lyrics_cache.clear();
                                }
                                self.lyrics_cache.insert(id.clone(), found.clone());
                                LyricsState::Ready(found)
                            }
                            Err(error) => LyricsState::Error(error),
                        };
                        self.lyrics = Some((id, state));
                    }
                }
                Event::Home(home) => {
                    self.home_loading = false;
                    self.home_error = None;
                    self.home = Some(home);
                }
                Event::HomeError(err) => {
                    self.home_loading = false;
                    self.home_error = Some(err);
                }
                Event::SearchResults(res) => {
                    self.search_loading.remove(&res.query);
                    self.search_errors.remove(&res.query);
                    self.search_results.insert(res.query.clone(), res);
                }
                Event::SearchError { query, error } => {
                    self.search_loading.remove(&query);
                    self.search_errors.insert(query, error);
                }
                Event::Collection { generation, collection } => {
                    if !generation_current(generation, self.auth_generation) {
                        continue;
                    }
                    self.collections_loading.remove(&collection.id);
                    self.collection_errors.remove(&collection.id);
                    self.collections.insert(collection.id.clone(), collection);
                }
                Event::Artist { generation, artist } => {
                    if !generation_current(generation, self.auth_generation) {
                        continue;
                    }
                    self.artists_loading.remove(&artist.id);
                    self.artist_errors.remove(&artist.id);
                    self.artists.insert(artist.id.clone(), artist);
                }
                Event::Radio { id, tracks } => {
                    if self.radio_pending.as_deref() != Some(id.as_str()) {
                        continue;
                    }
                    self.radio_pending = None;
                    if tracks.is_empty() {
                        self.radio_error =
                            Some(("Radio returned no tracks".to_string(), Instant::now()));
                    } else {
                        let prev_repeat = self.queue.repeat;
                        let prev_shuffle = self.queue.shuffle;
                        self.prefetched.clear();
                        self.queue = Queue::new(tracks, 0);
                        self.queue.repeat = prev_repeat;
                        self.queue.set_shuffle(prev_shuffle);
                        if let Some(track) = self.queue.current().cloned() {
                            self.start_track(track);
                        }
                    }
                }
                Event::RadioError { id, error } => {
                    if self.radio_pending.as_deref() != Some(id.as_str()) {
                        continue;
                    }
                    self.radio_pending = None;
                    self.radio_error = Some((format!("Radio failed: {error}"), Instant::now()));
                }
                Event::PageError { id, kind, error, generation } => {
                    if !generation_current(generation, self.auth_generation) {
                        continue;
                    }
                    match kind {
                        PageKind::Collection => {
                            self.collections_loading.remove(&id);
                            self.collection_errors.insert(id, error);
                        }
                        PageKind::Artist => {
                            self.artists_loading.remove(&id);
                            self.artist_errors.insert(id, error);
                        }
                    }
                }
                Event::CacheUsage { bytes, files } => {
                    self.cache_usage = Some((bytes, files));
                }
                Event::Ready { id, path } => {
                    if self.is_loading
                        && self.queue.current().map(|t| t.id.as_str()) == Some(&id)
                    {
                        self.is_loading = false;
                        let _ = std::fs::File::options().write(true).open(&path)
                            .and_then(|file| file.set_modified(std::time::SystemTime::now()));
                        if let Err(e) = self.player.play_file(&path) {
                            self.fetch_error = Some(e);
                        } else {
                            self.fetch_error = None;
                        }
                        self.maybe_prefetch();
                    }
                }
                Event::FetchError { id, msg } => {
                    if self.is_loading
                        && self.queue.current().map(|t| t.id.as_str()) == Some(&id)
                    {
                        self.is_loading = false;
                        self.fetch_error = Some(msg);
                        self.maybe_prefetch();
                    }
                }
                Event::Auth { signed_in, error } => {
                    self.signed_in = signed_in;
                    self.sign_in_busy = false;
                    if signed_in {
                        let reload = auth_changed(
                            &mut self.auth_generation,
                            &mut self.collections_loading,
                            &mut self.collection_errors,
                            &mut self.artists_loading,
                            &mut self.artist_errors,
                        );
                        self.auth_error = None;
                        self.show_sign_in = false;
                        self.library_loading = true;
                        self.library_error = None;
                        self.backend.send(Cmd::LoadLibrary(reload.generation));
                        reload.reload(self);
                    } else if error.is_none() {
                        // Clean sign-out (not a failed sign-in): drop library
                        // and any cached private pages, then reload the open
                        // public page (if any) instead of leaving it blank.
                        let reload = auth_changed(
                            &mut self.auth_generation,
                            &mut self.collections_loading,
                            &mut self.collection_errors,
                            &mut self.artists_loading,
                            &mut self.artist_errors,
                        );
                        self.auth_error = None;
                        self.library = None;
                        self.library_loading = false;
                        self.library_error = None;
                        self.collections.clear();
                        self.search_errors.clear();
                        reload.reload(self);
                    } else {
                        self.auth_error = error;
                    }
                }
                Event::Library { generation, library } => {
                    if !self.signed_in || !generation_current(generation, self.auth_generation) {
                        continue;
                    }
                    self.library_loading = false;
                    self.library_error = None;
                    self.library = Some(library);
                }
                Event::LibraryError { generation, error } => {
                    if !self.signed_in || !generation_current(generation, self.auth_generation) {
                        continue;
                    }
                    self.library_loading = false;
                    self.library_error = Some(error);
                }
            }
        }

        // Radio failure notices are transient: expire them so the player
        // bar falls back to the current track's artist/album line. Fetch
        // errors of the current track (`fetch_error`) never expire.
        if let Some((_, at)) = &self.radio_error {
            let age = at.elapsed();
            if age >= RADIO_ERROR_TTL {
                self.radio_error = None;
            } else if let Some(ctx) = ctx {
                ctx.request_repaint_after(RADIO_ERROR_TTL - age);
            }
        }

        // Repaint and auto-advance
        if let Some(ctx) = ctx
            && (self.is_loading || (self.is_active_playback && !self.player.is_paused()))
        {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        self.player.recover_output();
        if self.is_active_playback
            && !self.is_loading
            && !self.player.is_paused()
            && self.player.is_finished()
        {
            self.auto_advance();
        }
        self.maybe_lyrics();
        let mut actions = Vec::new();

        // MPRIS / media keys: map desktop commands onto Actions.
        if let Some(np) = &self.now_playing {
            let current_id = self.queue.current().map(|t| t.id.as_str());
            for cmd in np.commands() {
                if let Some(action) = map_mpris_command(cmd, current_id) {
                    actions.push(action);
                }
            }
        }
        while let Ok(remote) = self.remote_rx.try_recv() {
            actions.push(match remote {
                Remote::Show => Action::Raise,
                Remote::Quit => Action::Quit,
                Remote::Action(action) => action,
            });
        }
        for action in actions {
            self.apply_action(action);
        }
        self.push_mpris_state();
    }

    /// Music is playing or about to: closing the window keeps the app.
    fn keeps_playing(&self) -> bool {
        self.is_active_playback && (self.is_loading || !self.player.is_paused())
    }

    fn raise(&mut self) {
        if self.background {
            self.show_requested = true;
            return;
        }
        self.viewport_cmds.extend([
            egui::ViewportCommand::Minimized(false),
            egui::ViewportCommand::Visible(true),
            egui::ViewportCommand::Focus,
        ]);
    }

    /// Close button: quit when nothing plays, otherwise keep the music going.
    /// macOS minimizes to the Dock; Linux closes the window and keeps
    /// running behind the tray icon (see `main`).
    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested())
            || self.quit_requested
            || !self.keeps_playing()
        {
            return;
        }
        if cfg!(target_os = "linux") {
            self.background = true;
        } else if cfg!(target_os = "macos") {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
    }

    /// Size limits, size and window level for the current mode, sent when
    /// the mode changes and once for every new window.
    fn sync_window_mode(&mut self, ctx: &egui::Context) {
        let settled = self.mode_changed_at.elapsed() > Duration::from_secs(1);
        if self.applied_mini == Some(self.mini)
            && settled
            && let Some(size) = ctx.input(|i| i.viewport().inner_rect.map(|r| r.size()))
        {
            let size = [size.x, size.y];
            if self.mini {
                self.mini_size = sane_size(size, self.mini_size, MINI_MIN);
            } else {
                self.full_size = sane_size(size, self.full_size, FULL_MIN);
            }
        }
        if self.applied_mini == Some(self.mini) {
            return;
        }
        let switching = self.applied_mini.is_some_and(|mini| mini != self.mini);
        let (min, size) = if self.mini {
            (MINI_MIN, self.mini_size)
        } else {
            (FULL_MIN, self.full_size)
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(min.into()));
        // A new window restores its last geometry; only the mini player is
        // also resized then, in case that geometry was the full window's.
        if switching || self.mini {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size.into()));
            self.mode_changed_at = Instant::now();
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
            if self.mini && self.mini_on_top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            },
        ));
        self.applied_mini = Some(self.mini);
    }

    fn flush_viewport_cmds(&mut self, ctx: &egui::Context) {
        for cmd in self.viewport_cmds.drain(..) {
            ctx.send_viewport_cmd(cmd);
        }
    }
}

/// The eframe side of one window. The `App` outlives it: on Linux the
/// window can close while music keeps playing, and a later window picks
/// the same `App` back up.
struct Window<'a> {
    app: &'a mut App,
}

impl eframe::App for Window<'_> {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let app = &mut *self.app;
        app.tick(Some(ctx));
        app.handle_close_request(ctx);
        app.sync_window_mode(ctx);
        app.flush_viewport_cmds(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let app = &mut *self.app;
        let ctx = ui.ctx().clone();
        let mut actions = Vec::new();
        ui::set_playing(&ctx, app.keeps_playing() && !app.is_loading);

        // Keyboard shortcuts:
        // Space toggles play/pause only when no widget has keyboard focus
        if ctx.memory(|m| m.focused().is_none()) && ctx.input(|i| i.key_pressed(egui::Key::Space)) {
            actions.push(Action::TogglePlayPause);
        }
        // Mini player: Ctrl+M, or Cmd+Shift+M on macOS (Cmd+M minimizes).
        let mini_keys = if cfg!(target_os = "macos") {
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT
        } else {
            egui::Modifiers::COMMAND
        };
        if ctx.input_mut(|i| i.consume_key(mini_keys, egui::Key::M)) {
            actions.push(Action::ToggleMini);
        }

        if app.mini {
            ui::mini::draw_mini_player(ui, app, &mut actions);
        } else {
            // Back shortcuts: Mouse extra1 and Alt+Left
            let mouse_back = ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Extra1));
            let alt_left = ctx.input(|i| i.modifiers.alt && i.key_pressed(egui::Key::ArrowLeft));
            if (mouse_back || alt_left) && app.nav.can_back() {
                actions.push(Action::Back);
            }

            ui::draw_top_bar(ui, app, &mut actions);
            ui::draw_left_sidebar(ui, app, &mut actions);
            ui::draw_player_bar(ui, app, &mut actions);
            if app.queue_open {
                ui::draw_queue_panel(ui, app, &mut actions);
            } else if app.lyrics_open {
                ui::lyrics::draw_lyrics_panel(ui, app, &mut actions);
            }
            ui::draw_central_panel(ui, app, &mut actions);
            ui::draw_sign_in_dialog(ui, app, &mut actions);
            ui::draw_settings_dialog(ui, app, &mut actions);
        }

        for action in actions {
            app.apply_action(action);
        }
        app.flush_viewport_cmds(&ctx);
        app.push_mpris_state();
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &Settings::from_app(self.app));
    }
}

impl App {
    fn push_mpris_state(&mut self) {
        let Some(np) = self.now_playing.as_mut() else {
            return;
        };
        let current = self.queue.current().cloned();
        let track = current.as_ref().map(|t| fastframe_now_playing::Track {
            id: t.id.clone(),
            title: t.title.clone(),
            artists: vec![t.artist.clone()],
            album: t.album.clone(),
            duration: Some(Duration::from_secs(t.duration_secs as u64)),
            // ponytail: Track carries only the small thumb_url; prefer a large
            // thumbnail here once the backend provides one.
            art_url: t.thumb_url.clone(),
            ..Default::default()
        });
        let playback = if current.is_none() {
            fastframe_now_playing::Playback::Stopped
        } else if self.is_loading || self.player.is_paused() {
            fastframe_now_playing::Playback::Paused
        } else {
            fastframe_now_playing::Playback::Playing
        };
        np.update(fastframe_now_playing::State {
            playback,
            track,
            position: self.player.position(),
            volume: Some(self.player.volume() as f64),
            shuffle: Some(self.queue.shuffle),
            repeat: Some(match self.queue.repeat {
                audio::Repeat::Off => fastframe_now_playing::Repeat::Off,
                audio::Repeat::One => fastframe_now_playing::Repeat::Track,
                audio::Repeat::All => fastframe_now_playing::Repeat::Playlist,
            }),
            controls: fastframe_now_playing::Controls::default(),
        });
    }
}
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn with_extra_paths(path: &OsStr, extra: &[&str]) -> OsString {
    let mut paths: Vec<_> = if path.is_empty() {
        Vec::new()
    } else {
        std::env::split_paths(path).collect()
    };
    for entry in extra {
        let entry = std::path::PathBuf::from(entry);
        if !paths.contains(&entry) {
            paths.push(entry);
        }
    }
    std::env::join_paths(paths).unwrap_or_else(|_| path.to_os_string())
}

#[cfg(target_os = "macos")]
fn add_homebrew_paths() {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let path = with_extra_paths(&path, &["/opt/homebrew/bin", "/usr/local/bin"]);
    // SAFETY: called first in main, before any other thread exists.
    unsafe { std::env::set_var("PATH", path) };
}

/// Contents of the launcher entry pointing at `exe`. `Exec` is quoted per
/// the Desktop Entry spec so paths with spaces or `$` survive.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn desktop_entry(exe: &std::path::Path) -> String {
    let mut quoted = String::from("\"");
    for c in exe.display().to_string().chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    format!(
        "[Desktop Entry]\nType=Application\nName=ytmfast\nGenericName=Music Player\n\
         Comment=YouTube Music client\nExec={quoted}\nIcon=ytmfast\nTerminal=false\n\
         Categories=AudioVideo;Audio;Player;\nStartupWMClass=ytmfast\n"
    )
}

/// Install (or refresh, when the binary moved) the launcher entry and icon
/// under `~/.local/share`, so ytmfast shows up in the app launcher. Writes
/// only when the content differs; failures are logged and ignored.
#[cfg(target_os = "linux")]
fn install_desktop_entry() {
    let (Some(dirs), Ok(exe)) = (directories::BaseDirs::new(), std::env::current_exe()) else {
        return;
    };
    let data = dirs.data_dir();
    let files: [(std::path::PathBuf, Vec<u8>); 2] = [
        (
            data.join("icons/hicolor/scalable/apps/ytmfast.svg"),
            include_bytes!("../assets/app-icon.svg").to_vec(),
        ),
        (data.join("applications/ytmfast.desktop"), desktop_entry(&exe).into_bytes()),
    ];
    for (path, content) in files {
        if std::fs::read(&path).is_ok_and(|old| old == content) {
            continue;
        }
        let res = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|_| std::fs::write(&path, &content));
        if let Err(e) = res {
            eprintln!("Could not install {}: {e}", path.display());
        }
    }
}

fn main() -> eframe::Result {
    #[cfg(target_os = "macos")]
    add_homebrew_paths();
    #[cfg(target_os = "linux")]
    install_desktop_entry();

    // A second launch shows the running window instead of starting again.
    #[cfg(target_os = "linux")]
    let _instance = match tray::SingleInstance::claim() {
        tray::Claim::Primary(instance) => Some(instance),
        tray::Claim::Forwarded => return Ok(()),
        tray::Claim::Unavailable => None,
    };

    let mut slot: Option<App> = None;
    loop {
        run_window(&mut slot)?;
        let Some(app) = slot.as_mut() else {
            return Ok(());
        };
        if !app.background || app.quit_requested {
            return Ok(());
        }
        // Linux background mode: the window is gone, the music plays on.
        // Tick on a timer until the tray, MPRIS or a second launch asks for
        // the window again, or for Quit.
        app.repaint.attach(None);
        loop {
            app.tick(None);
            app.viewport_cmds.clear();
            if app.quit_requested {
                return Ok(());
            }
            if app.show_requested {
                app.show_requested = false;
                app.background = false;
                app.applied_mini = None;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// Open the window and run it until it closes. The first window creates the
/// `App` in `slot` (settings come from eframe's storage); later windows
/// reuse it.
fn run_window(slot: &mut Option<App>) -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("ytmfast")
        .with_app_id("ytmfast")
        .with_inner_size(FULL_SIZE)
        // The real minimum follows the mode (`sync_window_mode`); start low
        // so a restored mini player window is not forced to full size.
        .with_min_inner_size(MINI_MIN);
    match eframe::icon_data::from_png_bytes(include_bytes!("../assets/app-icon.png")) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(error) => eprintln!("Could not load app icon: {error}"),
    }
    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "ytmfast",
        native_options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            images::install(&cc.egui_ctx);
            fastframe_icons::install::<ui::Icon>(&cc.egui_ctx);
            ui::apply_theme(&cc.egui_ctx);
            let app = match slot {
                Some(app) => app,
                None => slot.insert(create_app(cc)),
            };
            app.repaint.attach(Some(&cc.egui_ctx));
            Ok(Box::new(Window { app }))
        }),
    )
}

fn create_app(cc: &eframe::CreationContext<'_>) -> App {
    let settings: Settings = cc
        .storage
        .and_then(|s| eframe::get_value::<Settings>(s, eframe::APP_KEY))
        .unwrap_or_default();
    let repaint = Repaint::default();
    repaint.attach(Some(&cc.egui_ctx));
    let mut app = App::new(repaint.clone());
    settings.apply(&mut app);
    let np_repaint = repaint.clone();
    app.now_playing = Some(fastframe_now_playing::NowPlaying::start(
        fastframe_now_playing::App::new("ytmfast", "ytmfast"),
        move || np_repaint.request_repaint(),
    ));
    #[cfg(target_os = "linux")]
    {
        tray::spawn(app.remote_tx.clone(), repaint.clone());
        tray::listen(app.remote_tx.clone(), repaint);
    }
    app
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_entry_quotes_exec() {
        let entry = desktop_entry(std::path::Path::new("/home/a b/$x/ytmfast"));
        assert!(entry.contains("Exec=\"/home/a b/\\$x/ytmfast\"\n"));
        assert!(entry.contains("Icon=ytmfast\n"));
        assert!(entry.contains("StartupWMClass=ytmfast\n"));
    }

    #[test]
    fn extra_paths_preserve_order_and_add_only_missing_entries() {
        let path = OsStr::new("/usr/bin:/opt/homebrew/bin:/bin");
        let extra = ["/opt/homebrew/bin", "/usr/local/bin", "/usr/local/bin"];
        let extended = with_extra_paths(path, &extra);
        assert_eq!(extended, "/usr/bin:/opt/homebrew/bin:/bin:/usr/local/bin");
        assert_eq!(with_extra_paths(&extended, &extra), extended);
        assert_eq!(with_extra_paths(path, &[]), path);
    }

    #[test]
    fn extra_paths_handle_empty_path() {
        assert_eq!(
            with_extra_paths(OsStr::new(""), &["/opt/homebrew/bin", "/usr/local/bin"]),
            "/opt/homebrew/bin:/usr/local/bin"
        );
        assert_eq!(with_extra_paths(OsStr::new(""), &[]), "");
    }

    #[test]
    fn test_cache_protection_survives_loading_transition() {
        let tracks = (0..5).map(|i| Track {
            id: format!("test_id_{i:04}"),
            title: String::new(), artist: String::new(), album: String::new(),
            duration_secs: 0, thumb_url: None, artist_id: None, album_id: None,
        }).collect();
        let mut queue = Queue::new(tracks, 1);
        let empty = HashSet::new();
        // Advance cleared requests: retain the current and next two files until Ready.
        assert_eq!(cache_protection(&queue, &empty, 2, true),
            HashSet::from(["test_id_0001".to_owned(), "test_id_0002".to_owned(), "test_id_0003".to_owned()]));
        let current = HashSet::from(["test_id_0001".to_owned()]);
        assert_eq!(cache_protection(&queue, &empty, 0, true), current);
        assert_eq!(cache_protection(&queue, &empty, 2, false), current);
        let prefetched = HashSet::from(["test_id_0002".to_owned()]);
        assert_eq!(cache_protection(&queue, &prefetched, 2, false),
            HashSet::from(["test_id_0001".to_owned(), "test_id_0002".to_owned()]));
        queue.order = vec![1, 4, 0, 3, 2];
        queue.pos = 0;
        assert_eq!(cache_protection(&queue, &empty, 2, true),
            HashSet::from(["test_id_0001".to_owned(), "test_id_0004".to_owned(), "test_id_0000".to_owned()]));
        queue.repeat = audio::Repeat::One;
        assert_eq!(cache_protection(&queue, &empty, 2, true), current);
        assert!(cache_protection(&Queue::new(Vec::new(), 0), &empty, 2, true).is_empty());
    }

    #[test]
    fn test_settings_default_volume() {
        let s = Settings::default();
        assert_eq!(s.volume, 1.0);
        assert_eq!(s.cache_max_mb, 2048);
        assert_eq!(s.prefetch_count, 3);
        assert!(!s.sidebar_collapsed);
        assert!(!s.shuffle);
        assert_eq!(s.repeat, audio::Repeat::Off);
        assert!(!s.queue_open);
        assert!(!s.lyrics_open);
        assert_eq!(s.last_view, View::Home);
    }

    #[test]
    fn test_settings_serde_round_trip() {
        let s = Settings {
            volume: 0.5,
            sidebar_collapsed: true,
            shuffle: true,
            repeat: audio::Repeat::One,
            queue_open: true,
            lyrics_open: true,
            last_view: View::Artist("abc123".to_string()),
            cache_max_mb: 512,
            prefetch_count: 7,
            mini: true,
            mini_on_top: true,
            full_size: [1000.0, 700.0],
            mini_size: [300.0, 120.0],
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.volume, 0.5);
        assert_eq!(back.cache_max_mb, 512);
        assert_eq!(back.prefetch_count, 7);
        assert!(back.mini);
        assert!(back.mini_on_top);
        assert_eq!(back.full_size, [1000.0, 700.0]);
        assert_eq!(back.mini_size, [300.0, 120.0]);
        assert!(back.sidebar_collapsed);
        assert!(back.shuffle);
        assert_eq!(back.repeat, audio::Repeat::One);
        assert!(back.queue_open);
        assert!(back.lyrics_open);
        assert_eq!(back.last_view, View::Artist("abc123".to_string()));

        // Every View variant round-trips
        for view in [
            View::Home,
            View::Explore,
            View::Library,
            View::Search("q".to_string()),
            View::Album("a".to_string()),
            View::Playlist("p".to_string()),
            View::Artist("r".to_string()),
        ] {
            let v: View =
                serde_json::from_str(&serde_json::to_string(&view).unwrap()).unwrap();
            assert_eq!(v, view);
        }
        // Search maps to Home on sanitize
        assert_eq!(
            Settings::sanitize_view(View::Search("x".to_string())),
            View::Home
        );
    }

    #[test]
    fn test_private_view_only_library_and_playlist() {
        // Signed-out fallback (startup restore, sign-out nav) covers only
        // Library and Playlist: Album/Artist pages are public and stay open.
        assert!(is_private_view(&View::Library));
        assert!(is_private_view(&View::Playlist("p".to_string())));
        assert!(!is_private_view(&View::Home));
        assert!(!is_private_view(&View::Explore));
        assert!(!is_private_view(&View::Search("q".to_string())));
        assert!(!is_private_view(&View::Album("a".to_string())));
        assert!(!is_private_view(&View::Artist("r".to_string())));
    }
    #[test]
    fn test_auth_changed_drops_old_generation_and_reloads() {
        // Regression: an auth change in either branch clears in-flight
        // page state, so the open page is re-requested under the new
        // generation and a stale result is dropped by the guard instead
        // of spinning forever.
        let mut generation = 0u64;
        let old_generation = generation;
        let mut collections_loading = HashSet::from(["album1".to_string()]);
        let mut collection_errors = HashMap::from([("old".to_string(), "e".to_string())]);
        let mut artists_loading = HashSet::from(["artist1".to_string()]);
        let mut artist_errors = HashMap::from([("old".to_string(), "e".to_string())]);
        let reload = auth_changed(
            &mut generation,
            &mut collections_loading,
            &mut collection_errors,
            &mut artists_loading,
            &mut artist_errors,
        );
        assert_eq!(reload.generation, old_generation.wrapping_add(1));
        assert!(collections_loading.is_empty());
        assert!(collection_errors.is_empty());
        assert!(artists_loading.is_empty());
        assert!(artist_errors.is_empty());
        // Exercise the same guard the Collection/Artist/PageError/Library
        // handlers run: the in-flight result (old generation) is
        // rejected, the re-requested one (new generation) is accepted.
        assert!(!generation_current(old_generation, generation));
        assert!(generation_current(reload.generation, generation));
        assert!(generation_current(generation, generation));
    }


    #[test]
    fn test_mpris_command_mapping() {
        use fastframe_now_playing::Command as C;
        let id = Some("track1");
        assert!(matches!(
            map_mpris_command(C::Play, id),
            Some(Action::Resume)
        ));
        assert!(matches!(
            map_mpris_command(C::Pause, id),
            Some(Action::Pause)
        ));
        assert!(matches!(
            map_mpris_command(C::PlayPause, id),
            Some(Action::TogglePlayPause)
        ));
        assert!(matches!(
            map_mpris_command(C::Next, id),
            Some(Action::NextTrack(true))
        ));
        assert!(matches!(
            map_mpris_command(C::Previous, id),
            Some(Action::PrevTrack)
        ));
        assert!(matches!(
            map_mpris_command(C::SeekBy(5000), id),
            Some(Action::SeekRelative(5000))
        ));
        // SetPosition only for the current track
        assert!(matches!(
            map_mpris_command(
                C::SetPosition {
                    track_id: "track1".to_string(),
                    position: Duration::from_secs(10),
                },
                id,
            ),
            Some(Action::Seek(_))
        ));
        assert!(
            map_mpris_command(
                C::SetPosition {
                    track_id: "other".to_string(),
                    position: Duration::from_secs(10),
                },
                id,
            )
            .is_none()
        );
        assert!(matches!(
            map_mpris_command(C::SetVolume(0.5), id),
            Some(Action::SetVolume(v)) if v == 0.5_f32
        ));
        assert!(matches!(
            map_mpris_command(C::SetVolume(0.0), id),
            Some(Action::SetVolume(v)) if v == 0.0_f32
        ));
        assert!(matches!(
            map_mpris_command(C::SetShuffle(true), id),
            Some(Action::SetShuffle(true))
        ));
        assert!(matches!(
            map_mpris_command(C::SetShuffle(false), id),
            Some(Action::SetShuffle(false))
        ));
        assert!(matches!(
            map_mpris_command(
                C::SetRepeat(fastframe_now_playing::Repeat::Track),
                id,
            ),
            Some(Action::SetRepeat(audio::Repeat::One))
        ));
        assert!(matches!(
            map_mpris_command(
                C::SetRepeat(fastframe_now_playing::Repeat::Playlist),
                id,
            ),
            Some(Action::SetRepeat(audio::Repeat::All))
        ));
        assert!(matches!(
            map_mpris_command(
                C::SetRepeat(fastframe_now_playing::Repeat::Off),
                id,
            ),
            Some(Action::SetRepeat(audio::Repeat::Off))
        ));
        assert!(matches!(
            map_mpris_command(C::Raise, id),
            Some(Action::Raise)
        ));
        assert!(matches!(map_mpris_command(C::Quit, id), Some(Action::Quit)));
        assert!(map_mpris_command(C::OpenUri("x".to_string()), id).is_none());
    }

    #[test]
    fn test_settings_missing_fields_use_defaults() {
        // `{}` (and partial objects from older versions) must not reset
        // saved settings: every missing field falls back to Default.
        let empty: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, Settings::default());

        let partial: Settings = serde_json::from_str(r#"{"volume": 0.25}"#).unwrap();
        assert_eq!(partial.volume, 0.25);
        assert_eq!(partial.cache_max_mb, 2048);
        assert_eq!(partial.prefetch_count, 3);
        let clamped: Settings = serde_json::from_str(r#"{"prefetch_count":255}"#).unwrap();
        assert_eq!(clamped.prefetch_count, 10);
        assert!(!partial.sidebar_collapsed);
        assert!(!partial.shuffle);
        assert_eq!(partial.repeat, audio::Repeat::Off);
        assert!(!partial.queue_open);
        assert!(!partial.lyrics_open);
        assert_eq!(partial.last_view, View::Home);
    }

    #[test]
    fn test_sane_size_rejects_tiny_and_non_finite() {
        assert_eq!(sane_size([300.0, 120.0], MINI_SIZE, MINI_MIN), [300.0, 120.0]);
        assert_eq!(sane_size([100.0, 120.0], MINI_SIZE, MINI_MIN), MINI_SIZE);
        assert_eq!(sane_size([f32::NAN, 900.0], FULL_SIZE, FULL_MIN), FULL_SIZE);
        assert_eq!(sane_size([700.0, 900.0], FULL_SIZE, FULL_MIN), FULL_SIZE);
        let partial: Settings = serde_json::from_str(r#"{"mini": true}"#).unwrap();
        assert!(partial.mini);
        assert_eq!(partial.mini_size, MINI_SIZE);
        assert_eq!(partial.full_size, FULL_SIZE);
    }

    #[test]
    fn test_double_click_repeat_guard() {
        let t0 = Instant::now();
        let last = LastStart {
            track_id: "t1".to_string(),
            list_len: 10,
            start: 3,
            at: t0,
        };
        // Second click of a double-click: same track, same list, fast.
        // `shuffle: None` (row double-click) echoes the current mode.
        assert!(is_double_click_repeat(
            Some(&last),
            "t1",
            10,
            3,
            None,
            false,
            t0 + Duration::from_millis(100),
        ));
        // Replay after the queue ended (repeat off): seconds later, not
        // within the double-click window — must go through.
        assert!(!is_double_click_repeat(
            Some(&last),
            "t1",
            10,
            3,
            None,
            false,
            t0 + Duration::from_secs(30),
        ));
        // Retry after FetchError / play_file error: same track, but the
        // error surfaces well after the window — must go through.
        assert!(!is_double_click_repeat(
            Some(&last),
            "t1",
            10,
            3,
            None,
            false,
            t0 + Duration::from_secs(5),
        ));
        // Different list whose start track equals the current one:
        // list length differs — must go through.
        assert!(!is_double_click_repeat(
            Some(&last),
            "t1",
            4,
            0,
            None,
            false,
            t0 + Duration::from_millis(100),
        ));
        // Same length but different start index — must go through.
        assert!(!is_double_click_repeat(
            Some(&last),
            "t1",
            10,
            5,
            None,
            false,
            t0 + Duration::from_millis(100),
        ));
        // Different track id — must go through.
        assert!(!is_double_click_repeat(
            Some(&last),
            "t2",
            10,
            3,
            None,
            false,
            t0 + Duration::from_millis(100),
        ));
        // No previous start — must go through.
        assert!(!is_double_click_repeat(
            None, "t1", 10, 3, None, false, t0
        ));
        // Collection Play (shuffle off) then Shuffle on the same list:
        // a new request, not a double-click echo — must go through.
        let play_last = LastStart {
            track_id: "t1".to_string(),
            list_len: 4,
            start: 0,
            at: t0,
        };
        assert!(!is_double_click_repeat(
            Some(&play_last),
            "t1",
            4,
            0,
            Some(true),
            false,
            t0 + Duration::from_millis(100),
        ));
        // And vice versa: Shuffle then Play (shuffle off) goes through.
        assert!(!is_double_click_repeat(
            Some(&play_last),
            "t1",
            4,
            0,
            Some(false),
            true,
            t0 + Duration::from_millis(100),
        ));
        // Same shuffle mode re-requested fast is still a double-click echo.
        assert!(is_double_click_repeat(
            Some(&play_last),
            "t1",
            4,
            0,
            Some(false),
            false,
            t0 + Duration::from_millis(100),
        ));
        // ponytail: a different list with the SAME length and SAME start
        // index and same first track id inside 500 ms is indistinguishable
        // from a double-click and is swallowed. Cheap to accept: it needs
        // two different Play/Shuffle gestures on identical-shape lists
        // within half a second.
    }
}
