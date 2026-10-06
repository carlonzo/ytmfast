mod audio;
mod backend;
mod ui;

use audio::{Player, Queue};
use backend::{ArtistPage, Backend, Cmd, Collection, Event, Home, PageKind, SearchAll, Track};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use ui::{Action, Nav, View};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub volume: f32,
    pub sidebar_collapsed: bool,
    pub shuffle: bool,
    pub repeat: audio::Repeat,
    pub queue_open: bool,
    pub last_view: View,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            volume: 1.0,
            sidebar_collapsed: false,
            shuffle: false,
            repeat: audio::Repeat::Off,
            queue_open: false,
            last_view: View::Home,
        }
    }
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
            last_view: Self::sanitize_view(app.nav.current.clone()),
        }
    }

    fn apply(&self, app: &mut App) {
        app.player.set_volume(self.volume);
        app.sidebar_collapsed = self.sidebar_collapsed;
        app.queue.shuffle = self.shuffle;
        app.queue.repeat = self.repeat;
        app.queue_open = self.queue_open;
        app.nav = Nav::new(Self::sanitize_view(self.last_view.clone()));
        app.ensure_view_loaded(&app.nav.current.clone());
    }
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
    pub prefetched: Option<String>,
    pub radio_pending: Option<String>,
    /// Transient radio failure notice with its timestamp: shown in the
    /// player bar but auto-expires after `RADIO_ERROR_TTL` so it never
    /// masquerades as the current track's error. Real fetch errors of the
    /// current track stay in `fetch_error` and do not expire.
    pub radio_error: Option<(String, Instant)>,
    /// Last track actually started, for the PlayList double-click guard.
    last_start: Option<LastStart>,
    pub now_playing: Option<fastframe_now_playing::NowPlaying>,

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
}

impl App {
    pub fn new(ctx: egui::Context) -> Self {
        let backend = Backend::new(ctx);
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
            prefetched: None,
            radio_pending: None,
            radio_error: None,
            last_start: None,
            now_playing: None,
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
        }
    }

    fn start_track(&mut self, track: Track) {
        self.player.stop();
        self.is_active_playback = true;
        self.is_loading = true;
        self.fetch_error = None;
        self.prefetched = None;
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

    /// Send Prefetch for the likely-next track, at most once per id.
    fn maybe_prefetch(&mut self) {
        if !self.is_active_playback || self.is_loading {
            return;
        }
        if let Some(next) = audio::prefetch_target(&self.queue, self.prefetched.as_deref()) {
            self.prefetched = Some(next.id.clone());
            self.backend.send(Cmd::Prefetch(next));
        }
    }

    fn auto_advance(&mut self) {
        if let Some(track) = self.queue.next(false).cloned() {
            self.start_track(track);
        } else {
            self.is_active_playback = false;
            self.is_loading = false;
            self.player.stop();
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
            View::Library => {}
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
                    self.backend.send(Cmd::OpenAlbum(id.clone()));
                }
            }
            View::Playlist(id) => {
                if !self.collections.contains_key(id) && !self.collections_loading.contains(id) {
                    self.collections_loading.insert(id.clone());
                    self.collection_errors.remove(id);
                    self.backend.send(Cmd::OpenPlaylist(id.clone()));
                }
            }
            View::Artist(id) => {
                if !self.artists.contains_key(id) && !self.artists_loading.contains(id) {
                    self.artists_loading.insert(id.clone());
                    self.artist_errors.remove(id);
                    self.backend.send(Cmd::OpenArtist(id.clone()));
                }
            }
        }
    }

    fn apply_action(&mut self, action: Action) {
        match action {
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
                self.queue = Queue::new(tracks, start);
                self.queue.repeat = prev_repeat;
                self.queue.set_shuffle(prev_shuffle);
                if let Some(track) = self.queue.current().cloned() {
                    self.start_track(track);
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
                }
            }
            Action::ToggleShuffle => {
                let new_shuf = !self.queue.shuffle;
                self.queue.set_shuffle(new_shuf);
                self.maybe_prefetch();
            }
            Action::CycleRepeat => {
                self.queue.cycle_repeat();
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
            Action::ToggleQueue => {
                self.queue_open = !self.queue_open;
            }
            Action::SetShuffle(s) => {
                self.queue.set_shuffle(s);
                self.maybe_prefetch();
            }
            Action::SetRepeat(r) => {
                self.queue.repeat = r;
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
            Action::Raise => { /* handled in ui() via viewport cmd */ }
            Action::Quit => { /* handled in ui() via viewport cmd */ }
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
                    self.backend.send(Cmd::OpenAlbum(id));
                }
                View::Playlist(id) => {
                    self.collection_errors.remove(&id);
                    self.collections_loading.insert(id.clone());
                    self.backend.send(Cmd::OpenPlaylist(id));
                }
                View::Artist(id) => {
                    self.artist_errors.remove(&id);
                    self.artists_loading.insert(id.clone());
                    self.backend.send(Cmd::OpenArtist(id));
                }
                View::Library => {}
            },
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Poll backend events
        while let Some(event) = self.backend.try_recv() {
            match event {
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
                Event::Collection(collection) => {
                    self.collections_loading.remove(&collection.id);
                    self.collection_errors.remove(&collection.id);
                    self.collections.insert(collection.id.clone(), collection);
                }
                Event::Artist(artist) => {
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
                Event::PageError { id, kind, error } => {
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
                Event::Ready { id, path } => {
                    if self.is_loading
                        && self.queue.current().map(|t| t.id.as_str()) == Some(&id)
                    {
                        self.is_loading = false;
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
                    }
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
            } else {
                ctx.request_repaint_after(RADIO_ERROR_TTL - age);
            }
        }

        // Repaint and auto-advance
        if self.is_loading || (self.is_active_playback && !self.player.is_paused()) {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        if self.is_active_playback
            && !self.is_loading
            && !self.player.is_paused()
            && self.player.is_finished()
        {
            self.auto_advance();
        }
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

        // Keyboard shortcuts:
        // Space toggles play/pause only when no widget has keyboard focus
        if ctx.memory(|m| m.focused().is_none()) && ctx.input(|i| i.key_pressed(egui::Key::Space)) {
            actions.push(Action::TogglePlayPause);
        }

        // Back shortcuts: Mouse extra1 and Alt+Left
        let mouse_back = ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Extra1));
        let alt_left = ctx.input(|i| i.modifiers.alt && i.key_pressed(egui::Key::ArrowLeft));
        if (mouse_back || alt_left) && self.nav.can_back() {
            actions.push(Action::Back);
        }

        // Draw layout
        ui::draw_top_bar(ui, self, &mut actions);
        ui::draw_left_sidebar(ui, self, &mut actions);
        ui::draw_player_bar(ui, self, &mut actions);
        if self.queue_open {
            ui::draw_queue_panel(ui, self, &mut actions);
        }
        ui::draw_central_panel(ui, self, &mut actions);

        // Apply actions (Raise/Quit need the viewport, so handle inline)
        let mut viewport_cmds = Vec::new();
        let mut rest = Vec::new();
        for action in actions {
            match action {
                Action::Raise => viewport_cmds.push(egui::ViewportCommand::Focus),
                Action::Quit => viewport_cmds.push(egui::ViewportCommand::Close),
                a => rest.push(a),
            }
        }
        for action in rest {
            self.apply_action(action);
        }
        for cmd in viewport_cmds {
            ctx.send_viewport_cmd(cmd);
        }

        self.push_mpris_state();
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &Settings::from_app(self));
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
fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ytmfast")
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "ytmfast",
        native_options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            fastframe_icons::install::<ui::Icon>(&cc.egui_ctx);
            ui::apply_theme(&cc.egui_ctx);
            let settings: Settings = cc
                .storage
                .and_then(|s| eframe::get_value::<Settings>(s, eframe::APP_KEY))
                .unwrap_or_default();
            let mut app = App::new(cc.egui_ctx.clone());
            settings.apply(&mut app);
            let repaint_ctx = cc.egui_ctx.clone();
            app.now_playing = Some(fastframe_now_playing::NowPlaying::start(
                fastframe_now_playing::App::new("ytmfast", "ytmfast"),
                move || repaint_ctx.request_repaint(),
            ));
            Ok(Box::new(app))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_default_volume() {
        let s = Settings::default();
        assert_eq!(s.volume, 1.0);
        assert!(!s.sidebar_collapsed);
        assert!(!s.shuffle);
        assert_eq!(s.repeat, audio::Repeat::Off);
        assert!(!s.queue_open);
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
            last_view: View::Artist("abc123".to_string()),
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.volume, 0.5);
        assert!(back.sidebar_collapsed);
        assert!(back.shuffle);
        assert_eq!(back.repeat, audio::Repeat::One);
        assert!(back.queue_open);
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
        assert!(!partial.sidebar_collapsed);
        assert!(!partial.shuffle);
        assert_eq!(partial.repeat, audio::Repeat::Off);
        assert!(!partial.queue_open);
        assert_eq!(partial.last_view, View::Home);
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
