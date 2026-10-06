mod audio;
mod auth;
mod backend;
mod ui;

use audio::{Player, Queue};
use backend::{
    ArtistPage, Backend, Cmd, Collection, Event, Home, Library, SearchAll, SignInSource, Track,
};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use ui::{Action, Nav, View};

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

    // Page cache
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
        self.backend.send(Cmd::Fetch(track));
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
            View::Library => {
                if self.signed_in && self.library.is_none() && !self.library_loading {
                    self.library_loading = true;
                    self.library_error = None;
                    self.backend.send(Cmd::LoadLibrary);
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
                let target = tracks.get(start);
                if self.is_loading
                    && self.queue.current().map(|t| &t.id) == target.map(|t| &t.id)
                {
                    return;
                }
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
            Action::PrevTrack => {
                if self.player.position() > Duration::from_secs(3) {
                    self.player.seek(Duration::ZERO);
                } else if let Some(track) = self.queue.prev().cloned() {
                    self.start_track(track);
                }
            }
            Action::Seek(pos) => {
                self.player.seek(pos);
            }
            Action::SetVolume(vol) => {
                self.player.set_volume(vol);
            }
            Action::ToggleShuffle => {
                let new_shuf = !self.queue.shuffle;
                self.queue.set_shuffle(new_shuf);
            }
            Action::CycleRepeat => {
                self.queue.cycle_repeat();
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
                self.auth_error = None;
                self.show_user_menu = false;
            }
            Action::HideSignIn => {
                // A sign-in may still be running on the backend; keep
                // `sign_in_busy` so the buttons stay disabled and no second
                // import can start. Only `Event::Auth` clears it.
                self.show_sign_in = false;
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
                // Leave private pages so the view is not blank after sign-out.
                match &self.nav.current {
                    View::Playlist(_) | View::Album(_) | View::Artist(_) | View::Library => {
                        self.nav.go(View::Home);
                        self.ensure_view_loaded(&View::Home);
                    }
                    _ => {}
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
                View::Library => {
                    self.library_error = None;
                    self.library_loading = true;
                    self.backend.send(Cmd::LoadLibrary);
                }
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
                Event::Radio { id: _, tracks } => {
                    if !tracks.is_empty() {
                        let prev_repeat = self.queue.repeat;
                        self.queue = Queue::new(tracks, 0);
                        self.queue.repeat = prev_repeat;
                        if let Some(track) = self.queue.current().cloned() {
                            self.start_track(track);
                        }
                    }
                }
                Event::PageError { id, error } => {
                    self.collections_loading.remove(&id);
                    self.collection_errors.insert(id.clone(), error.clone());
                    self.artists_loading.remove(&id);
                    self.artist_errors.insert(id, error);
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
                Event::Auth { signed_in, error } => {
                    self.signed_in = signed_in;
                    self.sign_in_busy = false;
                    if signed_in {
                        self.auth_error = None;
                        self.show_sign_in = false;
                        self.library_loading = true;
                        self.library_error = None;
                        self.backend.send(Cmd::LoadLibrary);
                    } else if error.is_none() {
                        // Clean sign-out (not a failed sign-in): drop library
                        // and any cached private pages.
                        self.auth_error = None;
                        self.library = None;
                        self.library_loading = false;
                        self.library_error = None;
                        self.collections.clear();
                        self.collection_errors.clear();
                    } else {
                        self.auth_error = error;
                    }
                }
                Event::Library(lib) => {
                    if !self.signed_in {
                        continue;
                    }
                    self.library_loading = false;
                    self.library_error = None;
                    self.library = Some(lib);
                }
                Event::LibraryError(err) => {
                    if !self.signed_in {
                        continue;
                    }
                    self.library_loading = false;
                    self.library_error = Some(err);
                }
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
        ui::draw_central_panel(ui, self, &mut actions);
        ui::draw_sign_in_dialog(ui, self, &mut actions);

        // Apply actions
        for action in actions {
            self.apply_action(action);
        }
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
            Ok(Box::new(App::new(cc.egui_ctx.clone())))
        }),
    )
}
