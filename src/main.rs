mod audio;
mod backend;
mod ui;

use audio::{Player, Queue};
use backend::{Backend, Cmd, Event, Track};
use std::time::Duration;
use ui::Action;

pub struct App {
    pub backend: Backend,
    pub player: Player,
    pub queue: Queue,
    pub results: Vec<Track>,
    pub search_input: String,
    pub current_search_query: String,
    pub is_searching: bool,
    pub is_loading: bool,
    pub search_error: Option<String>,
    pub fetch_error: Option<String>,
    pub search_focused: bool,
    pub is_active_playback: bool,
}

impl App {
    pub fn new(ctx: egui::Context) -> Self {
        let backend = Backend::new(ctx);
        let player = Player::new();
        let fetch_error = player.error.clone();
        Self {
            backend,
            player,
            queue: Queue::new(Vec::new(), 0),
            results: Vec::new(),
            search_input: String::new(),
            current_search_query: String::new(),
            is_searching: false,
            is_loading: false,
            search_error: None,
            fetch_error,
            search_focused: false,
            is_active_playback: false,
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

    fn apply_action(&mut self, action: Action) {
        match action {
            Action::Search(query) => {
                self.current_search_query = query.clone();
                self.is_searching = true;
                self.search_error = None;
                self.backend.send(Cmd::Search(query));
            }
            Action::PlayFrom(index) => {
                if self.is_loading
                    && self.queue.current().map(|t| &t.id) == self.results.get(index).map(|t| &t.id)
                {
                    return;
                }
                let prev_repeat = self.queue.repeat;
                let prev_shuffle = self.queue.shuffle;
                self.queue = Queue::new(self.results.clone(), index);
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
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Poll backend events
        while let Some(event) = self.backend.try_recv() {
            match event {
                Event::SearchResults { query, tracks } => {
                    if query == self.current_search_query {
                        self.results = tracks;
                        self.is_searching = false;
                        self.search_error = None;
                    }
                }
                Event::SearchError { query, error } => {
                    if query == self.current_search_query {
                        self.is_searching = false;
                        self.search_error = Some(error);
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

        // Keyboard: Space toggles play/pause only when no widget has keyboard focus
        if ctx.memory(|m| m.focused().is_none()) && ctx.input(|i| i.key_pressed(egui::Key::Space)) {
            actions.push(Action::TogglePlayPause);
        }

        // Draw views
        ui::draw_top_bar(ui, self, &mut actions);
        ui::draw_player_bar(ui, self, &mut actions);
        ui::draw_central_panel(ui, self, &mut actions);

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
            ui::apply_theme(&cc.egui_ctx);
            Ok(Box::new(App::new(cc.egui_ctx.clone())))
        }),
    )
}
