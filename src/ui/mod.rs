pub mod pages;

use std::time::Duration;
use crate::audio::Repeat;
use crate::backend::{Card, CardKind, Track};
use crate::App;

pub use pages::*;

pub const COLOR_BG: egui::Color32 = egui::Color32::from_rgb(0x03, 0x03, 0x03);
pub const COLOR_SURFACE: egui::Color32 = egui::Color32::from_rgb(0x21, 0x21, 0x21);
pub const COLOR_SURFACE_HOVER: egui::Color32 = egui::Color32::from_rgb(0x2A, 0x2A, 0x2A);
pub const COLOR_DIVIDER: egui::Color32 = egui::Color32::from_rgb(0x1F, 0x1F, 0x1F);
pub const COLOR_TEXT_PRIMARY: egui::Color32 = egui::Color32::from_rgb(0xFF, 0xFF, 0xFF);
pub const COLOR_TEXT_SECONDARY: egui::Color32 = egui::Color32::from_rgb(0xAA, 0xAA, 0xAA);
pub const COLOR_ACCENT_RED: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x00, 0x00);

fastframe_icons::icons! {
    pub enum Icon {
        prefix: "ytm-icon-",
        directory: "../../assets/icons/",
        Menu => "menu",
        Home => "house",
        Explore => "compass",
        Library => "library",
        Search => lucide "search",
        Back => lucide "arrow-left",
        Play => lucide "play",
        Pause => lucide "pause",
        Prev => "skip-back",
        Next => "skip-forward",
        Shuffle => "shuffle",
        Repeat => "repeat",
        RepeatOne => "repeat-1",
        Queue => "list-music",
        Volume => lucide "volume-2",
        VolumeMute => lucide "volume-x",
        Plus => lucide "plus",
        User => lucide "user",
        Refresh => lucide "refresh-cw",
        Close => lucide "x",
        Alert => lucide "circle-alert",
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum View {
    Home,
    Explore,
    Library,
    Search(String),
    Album(String),
    Playlist(String),
    Artist(String),
}

#[derive(Debug, Clone)]
pub struct Nav {
    pub current: View,
    pub history: Vec<View>,
}

impl Nav {
    pub fn new(initial: View) -> Self {
        Self {
            current: initial,
            history: Vec::new(),
        }
    }

    pub fn go(&mut self, v: View) {
        if self.current == v {
            return;
        }
        self.history.push(self.current.clone());
        if self.history.len() > 50 {
            self.history.remove(0);
        }
        self.current = v;
    }

    pub fn back(&mut self) -> bool {
        if let Some(prev) = self.history.pop() {
            self.current = prev;
            true
        } else {
            false
        }
    }

    pub fn can_back(&self) -> bool {
        !self.history.is_empty()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TrackRowConfig {
    pub show_index: bool,
    pub show_thumb: bool,
    pub show_album: bool,
}

impl TrackRowConfig {
    pub const ALBUM: Self = Self {
        show_index: true,
        show_thumb: false,
        show_album: false,
    };
    pub const PLAYLIST: Self = Self {
        show_index: false,
        show_thumb: true,
        show_album: true,
    };
    pub const ARTIST_TOP: Self = Self {
        show_index: true,
        show_thumb: true,
        show_album: false,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopResult<'a> {
    Artist(&'a Card),
    Song(&'a Track),
}

pub fn pick_top_result<'a>(
    query: &str,
    songs: &'a [Track],
    artists: &'a [Card],
) -> Option<TopResult<'a>> {
    let q_clean = query.trim().to_lowercase();
    if !q_clean.is_empty() {
        let matched = artists
            .iter()
            .find(|a| a.title.trim().eq_ignore_ascii_case(q_clean.as_str()));
        if let Some(artist) = matched {
            return Some(TopResult::Artist(artist));
        }
    }
    songs.first().map(TopResult::Song)
}

#[derive(Debug, Clone)]
pub enum Action {
    PlayList {
        tracks: Vec<Track>,
        start: usize,
        shuffle: Option<bool>,
    },
    TogglePlayPause,
    NextTrack(bool),
    PrevTrack,
    Seek(Duration),
    SetVolume(f32),
    ToggleShuffle,
    CycleRepeat,
    SetShuffle(bool),
    SetRepeat(crate::audio::Repeat),
    Jump(usize),
    ToggleQueue,
    Resume,
    Pause,
    SeekRelative(i64),
    Raise,
    Quit,
    Navigate(View),
    Back,
    ToggleSidebar,
    Radio(String),
    Retry(View),
}

pub fn format_duration(d: Duration) -> String {
    let s = d.as_secs();
    format!("{}:{:02}", s / 60, s % 60)
}

pub fn format_secs(s: u32) -> String {
    format!("{}:{:02}", s / 60, s % 60)
}

pub fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = COLOR_BG;
    visuals.window_fill = COLOR_BG;
    visuals.extreme_bg_color = COLOR_SURFACE;
    visuals.widgets.inactive.bg_fill = COLOR_SURFACE;
    visuals.widgets.hovered.bg_fill = COLOR_SURFACE_HOVER;
    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(0x38, 0x38, 0x38);
    visuals.selection.bg_fill = COLOR_ACCENT_RED;
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);
    visuals.override_text_color = Some(COLOR_TEXT_PRIMARY);
    ctx.set_visuals(visuals);
    // Rows/cards allocate their click rect first and draw text inside it;
    // selectable labels would sense clicks over the text and swallow them.
    ctx.global_style_mut(|s| s.interaction.selectable_labels = false);
}

pub fn draw_top_bar(ui: &mut egui::Ui, app: &mut App, actions: &mut Vec<Action>) {
    egui::Panel::top("top_panel")
        .exact_size(64.0)
        .show_separator_line(false)
        .frame(
            egui::Frame::new()
                .fill(COLOR_BG)
                .inner_margin(egui::Margin::symmetric(16, 12)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                // Hamburger button
                if ui
                    .add(
                        egui::Button::image(Icon::Menu.image(COLOR_TEXT_PRIMARY, 20.0))
                            .frame(false),
                    )
                    .clicked()
                {
                    actions.push(Action::ToggleSidebar);
                }

                ui.add_space(12.0);

                // Red "Music" Logo (red circle with white play triangle + text)
                let logo_resp = ui
                    .horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::click());
                        ui.painter().circle_filled(rect.center(), 12.0, COLOR_ACCENT_RED);
                        // White triangle
                        let p1 = egui::pos2(rect.center().x - 3.5, rect.center().y - 5.5);
                        let p2 = egui::pos2(rect.center().x - 3.5, rect.center().y + 5.5);
                        let p3 = egui::pos2(rect.center().x + 5.5, rect.center().y);
                        ui.painter().add(egui::Shape::convex_polygon(
                            vec![p1, p2, p3],
                            COLOR_TEXT_PRIMARY,
                            egui::Stroke::NONE,
                        ));
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("Music")
                                .color(COLOR_TEXT_PRIMARY)
                                .strong()
                                .size(22.0),
                        );
                    })
                    .response;

                if logo_resp.clicked() {
                    actions.push(Action::Navigate(View::Home));
                }

                ui.add_space(20.0);

                // Back arrow button (enabled only if history non-empty)
                let can_back = app.nav.can_back();
                let back_color = if can_back {
                    COLOR_TEXT_PRIMARY
                } else {
                    egui::Color32::from_rgb(0x55, 0x55, 0x55)
                };
                let back_btn = ui.add_enabled(
                    can_back,
                    egui::Button::image(Icon::Back.image(back_color, 20.0)).frame(false),
                );
                if back_btn.clicked() {
                    actions.push(Action::Back);
                }

                ui.add_space(20.0);

                // Pill search box
                let search_frame = egui::Frame::new()
                    .fill(COLOR_SURFACE)
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(14, 8));

                let response = search_frame
                    .show(ui, |ui| {
                        ui.set_width(480.0);
                        ui.horizontal_centered(|ui| {
                            ui.add(Icon::Search.image(COLOR_TEXT_SECONDARY, 16.0));
                            ui.add_space(8.0);
                            let edit = egui::TextEdit::singleline(&mut app.search_input)
                                .hint_text("Search songs, albums, artists")
                                .frame(egui::Frame::NONE)
                                .desired_width(430.0);
                            ui.add(edit)
                        })
                        .inner
                    })
                    .inner;

                let enter_pressed =
                    response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let lost_focus_with_enter =
                    response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if enter_pressed || lost_focus_with_enter {
                    let q = app.search_input.trim().to_string();
                    if !q.is_empty() {
                        actions.push(Action::Navigate(View::Search(q)));
                    }
                }

                // If currently searching in search view
                if let View::Search(q) = &app.nav.current {
                    let is_loading = app.search_loading.contains(q);
                    if is_loading {
                        ui.add_space(12.0);
                        ui.spinner();
                    }
                }

                // Right aligned Sign in button placeholder
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    let sign_in_btn = egui::Button::image_and_text(
                        Icon::User.image(COLOR_TEXT_SECONDARY, 16.0),
                        egui::RichText::new("Sign in")
                            .color(COLOR_TEXT_SECONDARY)
                            .size(13.0),
                    )
                    .fill(COLOR_SURFACE)
                    .corner_radius(16);
                    let _ = ui.add_enabled(false, sign_in_btn);
                });
            });
        });
}

pub fn draw_left_sidebar(ui: &mut egui::Ui, app: &App, actions: &mut Vec<Action>) {
    let width = if app.sidebar_collapsed { 72.0 } else { 240.0 };

    egui::Panel::left("left_sidebar")
        .exact_size(width)
        .show_separator_line(false)
        .frame(
            egui::Frame::new()
                .fill(COLOR_BG)
                .inner_margin(egui::Margin::symmetric(10, 16)),
        )
        .show(ui, |ui| {
            // 1px divider on right edge
            let right_x = ui.max_rect().right();
            ui.painter().line_segment(
                [
                    egui::pos2(right_x, ui.max_rect().top()),
                    egui::pos2(right_x, ui.max_rect().bottom()),
                ],
                egui::Stroke::new(1.0, COLOR_DIVIDER),
            );

            let is_collapsed = app.sidebar_collapsed;

            let items = [
                ("Home", Icon::Home, View::Home),
                ("Explore", Icon::Explore, View::Explore),
                ("Library", Icon::Library, View::Library),
            ];

            for (label, icon, view) in items {
                let is_active = app.nav.current == view;
                let bg_color = if is_active {
                    COLOR_SURFACE
                } else {
                    egui::Color32::TRANSPARENT
                };

                let item_size = if is_collapsed {
                    egui::vec2(width - 20.0, 56.0)
                } else {
                    egui::vec2(width - 20.0, 42.0)
                };

                let (rect, resp) = ui.allocate_exact_size(item_size, egui::Sense::click());

                let final_bg = if resp.hovered() && !is_active {
                    COLOR_SURFACE_HOVER
                } else {
                    bg_color
                };

                if final_bg != egui::Color32::TRANSPARENT {
                    ui.painter().rect_filled(rect, egui::CornerRadius::same(8), final_bg);
                }

                if is_collapsed {
                    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(8.0);
                            ui.add(icon.image(
                                if is_active {
                                    COLOR_TEXT_PRIMARY
                                } else {
                                    COLOR_TEXT_SECONDARY
                                },
                                16.0,
                            ));
                            ui.add_space(2.0);
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(label)
                                        .color(if is_active {
                                            COLOR_TEXT_PRIMARY
                                        } else {
                                            COLOR_TEXT_SECONDARY
                                        })
                                        .size(10.0),
                                )
                                .truncate(),
                            );
                        });
                    });
                } else {
                    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                        ui.horizontal_centered(|ui| {
                            ui.add_space(14.0);
                            ui.add(icon.image(
                                if is_active {
                                    COLOR_TEXT_PRIMARY
                                } else {
                                    COLOR_TEXT_SECONDARY
                                },
                                18.0,
                            ));
                            ui.add_space(14.0);
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(label)
                                        .color(if is_active {
                                            COLOR_TEXT_PRIMARY
                                        } else {
                                            COLOR_TEXT_SECONDARY
                                        })
                                        .strong()
                                        .size(14.0),
                                )
                                .truncate(),
                            );
                        });
                    });
                }

                if resp.clicked() {
                    actions.push(Action::Navigate(view));
                }

                ui.add_space(4.0);
            }

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(12.0);

            // "+ New playlist" pill (disabled)
            if !is_collapsed {
                let new_playlist_btn = egui::Button::image_and_text(
                    Icon::Plus.image(egui::Color32::from_rgb(0x77, 0x77, 0x77), 14.0),
                    egui::RichText::new("New playlist")
                        .color(egui::Color32::from_rgb(0x77, 0x77, 0x77))
                        .size(13.0),
                )
                .fill(COLOR_SURFACE)
                .corner_radius(16);
                let _ = ui.add_enabled(false, new_playlist_btn);

                ui.add_space(20.0);
                ui.label(
                    egui::RichText::new("Sign in to see your playlists")
                        .color(egui::Color32::from_rgb(0x77, 0x77, 0x77))
                        .size(12.0),
                );
            } else {
                let plus_btn = egui::Button::image(
                    Icon::Plus.image(egui::Color32::from_rgb(0x77, 0x77, 0x77), 16.0),
                )
                .fill(COLOR_SURFACE)
                .corner_radius(16);
                let _ = ui.add_enabled(false, plus_btn);
            }
        });
}

pub fn draw_player_bar(ui: &mut egui::Ui, app: &App, actions: &mut Vec<Action>) {
    egui::Panel::bottom("bottom_panel")
        .exact_size(72.0)
        .show_separator_line(false)
        .frame(
            egui::Frame::new()
                .fill(COLOR_SURFACE)
                .inner_margin(egui::Margin::symmetric(0, 0)),
        )
        .show(ui, |ui| {
            let cur_pos = app.player.position();
            let cur_track = app.queue.current();
            let total_dur = app.player.duration().unwrap_or_else(|| {
                Duration::from_secs(cur_track.map(|t| t.duration_secs as u64).unwrap_or(0))
            });

            let fraction = if total_dur.as_secs_f32() > 0.0 {
                (cur_pos.as_secs_f32() / total_dur.as_secs_f32()).clamp(0.0, 1.0)
            } else {
                0.0
            };

            let drag_id = ui.id().with("progress_drag_fraction");
            let mut drag_frac: Option<f32> = ui.data_mut(|d| d.get_temp(drag_id));

            let total_width = ui.available_width();
            let (bar_rect, bar_resp) = ui.allocate_exact_size(
                egui::vec2(total_width, 4.0),
                egui::Sense::click_and_drag(),
            );

            if bar_resp.dragged() {
                if let Some(pos) = bar_resp.interact_pointer_pos() {
                    let frac = ((pos.x - bar_rect.min.x) / bar_rect.width()).clamp(0.0, 1.0);
                    drag_frac = Some(frac);
                    ui.data_mut(|d| d.insert_temp(drag_id, frac));
                }
            } else if bar_resp.clicked() || bar_resp.drag_stopped() {
                let final_frac = bar_resp
                    .interact_pointer_pos()
                    .map(|pos| ((pos.x - bar_rect.min.x) / bar_rect.width()).clamp(0.0, 1.0))
                    .or(drag_frac);

                if let Some(frac) = final_frac {
                    let target = Duration::from_secs_f32(frac * total_dur.as_secs_f32());
                    actions.push(Action::Seek(target));
                }
                drag_frac = None;
                ui.data_mut(|d| d.remove_temp::<f32>(drag_id));
            } else if drag_frac.is_some() {
                drag_frac = None;
                ui.data_mut(|d| d.remove_temp::<f32>(drag_id));
            }

            let display_fraction = drag_frac.unwrap_or(fraction);

            ui.painter().rect_filled(
                bar_rect,
                egui::CornerRadius::ZERO,
                egui::Color32::from_rgb(0x40, 0x40, 0x40),
            );
            let filled_rect = egui::Rect::from_min_size(
                bar_rect.min,
                egui::vec2(bar_rect.width() * display_fraction, bar_rect.height()),
            );
            ui.painter().rect_filled(
                filled_rect,
                egui::CornerRadius::ZERO,
                COLOR_ACCENT_RED,
            );
            if bar_resp.hovered() || bar_resp.dragged() || drag_frac.is_some() {
                let circle_center = egui::pos2(
                    bar_rect.min.x + bar_rect.width() * display_fraction,
                    bar_rect.center().y,
                );
                ui.painter().circle_filled(
                    circle_center,
                    4.0,
                    COLOR_ACCENT_RED,
                );
            }

            ui.horizontal_centered(|ui| {
                ui.add_space(16.0);

                if ui
                    .add(egui::Button::image(Icon::Prev.image(COLOR_TEXT_PRIMARY, 20.0)).frame(false))
                    .clicked()
                {
                    actions.push(Action::PrevTrack);
                }

                let is_paused = app.player.is_paused();
                let play_icon = if is_paused {
                    Icon::Play.image(COLOR_TEXT_PRIMARY, 20.0)
                } else {
                    Icon::Pause.image(COLOR_TEXT_PRIMARY, 20.0)
                };
                if ui
                    .add_sized([36.0, 36.0], egui::Button::image(play_icon).frame(false))
                    .clicked()
                {
                    actions.push(Action::TogglePlayPause);
                }

                if ui
                    .add(egui::Button::image(Icon::Next.image(COLOR_TEXT_PRIMARY, 20.0)).frame(false))
                    .clicked()
                {
                    actions.push(Action::NextTrack(true));
                }

                ui.add_space(8.0);
                let preview_pos = drag_frac
                    .map(|f| Duration::from_secs_f32(f * total_dur.as_secs_f32()))
                    .unwrap_or(cur_pos);
                let time_str =
                    format!("{} / {}", format_duration(preview_pos), format_duration(total_dur));
                ui.label(
                    egui::RichText::new(time_str)
                        .color(COLOR_TEXT_SECONDARY)
                        .size(12.0),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(16.0);

                    // Right-to-left order on screen: volume, repeat, shuffle, queue.
                    let queue_color = if app.queue_open {
                        COLOR_ACCENT_RED
                    } else {
                        COLOR_TEXT_SECONDARY
                    };
                    if ui
                        .add(egui::Button::image(Icon::Queue.image(queue_color, 18.0)).frame(false))
                        .on_hover_text(if app.queue_open {
                            "Close queue"
                        } else {
                            "Open queue"
                        })
                        .clicked()
                    {
                        actions.push(Action::ToggleQueue);
                    }
                    ui.add_space(8.0);

                    let shuf_color = if app.queue.shuffle {
                        COLOR_ACCENT_RED
                    } else {
                        egui::Color32::from_rgb(0x88, 0x88, 0x88)
                    };
                    let shuf_tip = if app.queue.shuffle { "Shuffle on" } else { "Shuffle off" };
                    if ui
                        .add(egui::Button::image(Icon::Shuffle.image(shuf_color, 16.0)).frame(false))
                        .on_hover_text(shuf_tip)
                        .clicked()
                    {
                        actions.push(Action::ToggleShuffle);
                    }
                    ui.add_space(8.0);

                    let (rep_icon, rep_color, rep_tip) = match app.queue.repeat {
                        Repeat::Off => (
                            Icon::Repeat,
                            egui::Color32::from_rgb(0x88, 0x88, 0x88),
                            "Repeat off",
                        ),
                        Repeat::All => (Icon::Repeat, COLOR_TEXT_PRIMARY, "Repeat all"),
                        Repeat::One => (Icon::RepeatOne, COLOR_ACCENT_RED, "Repeat one"),
                    };
                    if ui
                        .add(egui::Button::image(rep_icon.image(rep_color, 16.0)).frame(false))
                        .on_hover_text(rep_tip)
                        .clicked()
                    {
                        actions.push(Action::CycleRepeat);
                    }
                    ui.add_space(8.0);

                    let mut vol = app.player.volume();
                    if ui
                        .add_sized(
                            [70.0, 16.0],
                            egui::Slider::new(&mut vol, 0.0..=1.0).show_value(false),
                        )
                        .changed()
                    {
                        actions.push(Action::SetVolume(vol));
                    }

                    let vol_icon = if vol > 0.0 {
                        Icon::Volume.image(COLOR_TEXT_SECONDARY, 16.0)
                    } else {
                        Icon::VolumeMute.image(COLOR_TEXT_SECONDARY, 16.0)
                    };
                    ui.add(vol_icon);
                    ui.add_space(16.0);

                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        if let Some(track) = cur_track {
                            if let Some(url) = &track.thumb_url {
                                ui.add(
                                    egui::Image::from_uri(url)
                                        .fit_to_exact_size(egui::vec2(40.0, 40.0))
                                        .corner_radius(4),
                                );
                            } else {
                                let (thumb_rect, _) = ui.allocate_exact_size(
                                    egui::vec2(40.0, 40.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(
                                    thumb_rect,
                                    egui::CornerRadius::same(4),
                                    egui::Color32::from_rgb(0x30, 0x30, 0x30),
                                );
                            }
                            ui.add_space(8.0);
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&track.title)
                                                .color(COLOR_TEXT_PRIMARY)
                                                .strong()
                                                .size(13.0),
                                        )
                                        .truncate(),
                                    );
                                    if app.is_loading {
                                        ui.label(
                                            egui::RichText::new("Loading…")
                                                .color(egui::Color32::from_rgb(0xFF, 0x88, 0x88))
                                                .size(11.0),
                                        );
                                    }
                                });
                                if let Some(err) = &app.fetch_error {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(err)
                                                .color(egui::Color32::from_rgb(0xFF, 0x44, 0x44))
                                                .size(11.0),
                                        )
                                        .truncate(),
                                    );
                                } else {
                                    let meta = if track.album.is_empty() {
                                        track.artist.clone()
                                    } else {
                                        format!("{} • {}", track.artist, track.album)
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(meta)
                                                .color(COLOR_TEXT_SECONDARY)
                                                .size(11.0),
                                        )
                                        .truncate(),
                                    );
                                }
                            });
                        }
                    });
                });
            });
        });
}

/// Right "Up next" panel: queue in play order, current highlighted, click
/// jumps. `show_rows` keeps long queues cheap; auto-scrolls only when the
/// current track changes.
pub fn draw_queue_panel(ui: &mut egui::Ui, app: &App, actions: &mut Vec<Action>) {
    let order = &app.queue.order;
    let tracks = &app.queue.tracks;
    let current_pos = app.queue.pos;
    let order_len = order.len();

    egui::Panel::right("queue_panel")
        .exact_size(360.0)
        .show_separator_line(false)
        .frame(
            egui::Frame::new()
                .fill(COLOR_BG)
                .inner_margin(egui::Margin::same(12)),
        )
        .show(ui, |ui| {
            // 1px divider on left edge
            let left_x = ui.max_rect().left();
            ui.painter().line_segment(
                [
                    egui::pos2(left_x, ui.max_rect().top()),
                    egui::pos2(left_x, ui.max_rect().bottom()),
                ],
                egui::Stroke::new(1.0, COLOR_DIVIDER),
            );

            ui.add(
                egui::Label::new(
                    egui::RichText::new("Up next")
                        .color(COLOR_TEXT_PRIMARY)
                        .strong()
                        .size(16.0),
                )
                .truncate(),
            );
            ui.add_space(8.0);

            let row_h = 56.0;
            // Scroll the current row into view only when the position
            // changed, not every frame. `show_rows` virtualizes rows, so a
            // far-away current row has no widget to `scroll_to_me` — jump
            // via an explicit scroll offset instead.
            let seen_pos = ui.data_mut(|d| d.get_temp::<usize>(ui.id().with("queue_seen_pos")));
            let scroll_to_current = seen_pos != Some(current_pos);
            ui.data_mut(|d| {
                d.insert_temp(ui.id().with("queue_seen_pos"), current_pos);
            });
            let spacing_y = ui.spacing().item_spacing.y;
            let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
            if scroll_to_current {
                scroll = scroll.vertical_scroll_offset(current_pos as f32 * (row_h + spacing_y));
            }
            scroll.show_rows(ui, row_h, order_len, |ui, range| {
                    for pos in range {
                        let Some(&track_idx) = order.get(pos) else {
                            continue;
                        };
                        let Some(track) = tracks.get(track_idx) else {
                            continue;
                        };
                        let is_current = pos == current_pos;
                        let (row_rect, row_resp) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), row_h),
                            egui::Sense::click(),
                        );
                        if is_current {
                            ui.painter().rect_filled(
                                row_rect,
                                egui::CornerRadius::same(4),
                                COLOR_SURFACE,
                            );
                        }
                        ui.scope_builder(egui::UiBuilder::new().max_rect(row_rect), |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.add_space(4.0);
                                if let Some(url) = &track.thumb_url {
                                    ui.add(
                                        egui::Image::from_uri(url)
                                            .fit_to_exact_size(egui::vec2(40.0, 40.0))
                                            .corner_radius(4),
                                    );
                                } else {
                                    let (r, _) = ui.allocate_exact_size(
                                        egui::vec2(40.0, 40.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().rect_filled(
                                        r,
                                        egui::CornerRadius::same(4),
                                        egui::Color32::from_rgb(0x30, 0x30, 0x30),
                                    );
                                }
                                ui.add_space(8.0);
                                ui.vertical(|ui| {
                                    ui.add_space(6.0);
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&track.title)
                                                .color(if is_current {
                                                    COLOR_ACCENT_RED
                                                } else {
                                                    COLOR_TEXT_PRIMARY
                                                })
                                                .strong()
                                                .size(13.0),
                                        )
                                        .truncate(),
                                    );
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&track.artist)
                                                .color(COLOR_TEXT_SECONDARY)
                                                .size(12.0),
                                        )
                                        .truncate(),
                                    );
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.add_space(8.0);
                                        ui.label(
                                            egui::RichText::new(format_secs(track.duration_secs))
                                                .color(COLOR_TEXT_SECONDARY)
                                                .size(12.0),
                                        );
                                    },
                                );
                            });
                        });
                        if row_resp.clicked() {
                            actions.push(Action::Jump(pos));
                        }
                    }
                });
        });
}

pub fn draw_card(ui: &mut egui::Ui, card: &Card, actions: &mut Vec<Action>) {
    let card_width = 160.0;
    let card_height = 224.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(card_width, card_height), egui::Sense::click());

    if resp.hovered() {
        ui.painter().rect_filled(
            rect.expand(4.0),
            egui::CornerRadius::same(8),
            COLOR_SURFACE,
        );
    }

    if resp.clicked() {
        match card.kind {
            CardKind::Album => actions.push(Action::Navigate(View::Album(card.id.clone()))),
            CardKind::Playlist => actions.push(Action::Navigate(View::Playlist(card.id.clone()))),
            CardKind::Artist => actions.push(Action::Navigate(View::Artist(card.id.clone()))),
        }
    }

    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        ui.vertical(|ui| {
            let art_rect = egui::Rect::from_min_size(rect.min, egui::vec2(160.0, 160.0));
            let corner = if card.kind == CardKind::Artist {
                egui::CornerRadius::same(80)
            } else {
                egui::CornerRadius::same(4)
            };

            if let Some(url) = &card.thumb_url {
                ui.put(
                    art_rect,
                    egui::Image::from_uri(url)
                        .fit_to_exact_size(egui::vec2(160.0, 160.0))
                        .corner_radius(corner),
                );
            } else {
                ui.allocate_rect(art_rect, egui::Sense::hover());
                ui.painter().rect_filled(art_rect, corner, egui::Color32::from_rgb(0x30, 0x30, 0x30));
            }

            ui.add_space(8.0);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&card.title)
                        .color(COLOR_TEXT_PRIMARY)
                        .strong()
                        .size(14.0),
                )
                .truncate(),
            );
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&card.subtitle)
                        .color(COLOR_TEXT_SECONDARY)
                        .size(12.0),
                )
                .truncate(),
            );
        });
    });
}

pub fn draw_shelf(
    ui: &mut egui::Ui,
    title: &str,
    cards: &[Card],
    id_salt: &str,
    actions: &mut Vec<Action>,
) {
    if cards.is_empty() {
        return;
    }
    ui.add_space(16.0);
    ui.label(
        egui::RichText::new(title)
            .color(COLOR_TEXT_PRIMARY)
            .strong()
            .size(24.0),
    );
    ui.add_space(12.0);

    egui::ScrollArea::horizontal()
        .id_salt(id_salt)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for card in cards {
                    draw_card(ui, card, actions);
                    ui.add_space(16.0);
                }
            });
        });
}

pub fn draw_track_row(
    ui: &mut egui::Ui,
    idx: usize,
    track: &Track,
    all_tracks: &[Track],
    current_id: Option<&str>,
    config: TrackRowConfig,
    actions: &mut Vec<Action>,
) {
    let row_height = 56.0;
    let row_width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(row_width, row_height), egui::Sense::click());

    let is_current = current_id == Some(track.id.as_str());

    if resp.hovered() || is_current {
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(4),
            COLOR_SURFACE,
        );
    }

    let mut play_triggered = false;
    if resp.double_clicked() {
        play_triggered = true;
    }

    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        ui.horizontal_centered(|ui| {
            ui.add_space(8.0);

            // Left area: index or thumbnail with play icon hover
            if config.show_index {
                let (btn_rect, btn_resp) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
                if btn_resp.clicked() {
                    play_triggered = true;
                }
                if resp.hovered() || btn_resp.hovered() {
                    ui.put(
                        btn_rect,
                        egui::Image::new(Icon::Play.uri())
                            .tint(COLOR_TEXT_PRIMARY)
                            .fit_to_exact_size(egui::vec2(16.0, 16.0)),
                    );
                } else if is_current {
                    ui.put(
                        btn_rect,
                        egui::Image::new(Icon::Play.uri())
                            .tint(COLOR_ACCENT_RED)
                            .fit_to_exact_size(egui::vec2(14.0, 14.0)),
                    );
                } else {
                    ui.painter().text(
                        btn_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        format!("{}", idx + 1),
                        egui::FontId::proportional(13.0),
                        COLOR_TEXT_SECONDARY,
                    );
                }
                ui.add_space(8.0);
            }

            if config.show_thumb {
                let (thumb_rect, thumb_resp) = ui.allocate_exact_size(egui::vec2(40.0, 40.0), egui::Sense::click());
                if thumb_resp.clicked() {
                    play_triggered = true;
                }
                if let Some(url) = &track.thumb_url {
                    ui.put(
                        thumb_rect,
                        egui::Image::from_uri(url)
                            .fit_to_exact_size(egui::vec2(40.0, 40.0))
                            .corner_radius(4),
                    );
                } else {
                    ui.painter().rect_filled(
                        thumb_rect,
                        egui::CornerRadius::same(4),
                        egui::Color32::from_rgb(0x30, 0x30, 0x30),
                    );
                }

                if resp.hovered() || thumb_resp.hovered() {
                    ui.painter().rect_filled(
                        thumb_rect,
                        egui::CornerRadius::same(4),
                        egui::Color32::from_black_alpha(150),
                    );
                    ui.put(
                        thumb_rect,
                        egui::Image::new(Icon::Play.uri())
                            .tint(COLOR_TEXT_PRIMARY)
                            .fit_to_exact_size(egui::vec2(16.0, 16.0)),
                    );
                }
                ui.add_space(8.0);
            }

            // Title & Artist
            let title_color = if is_current {
                COLOR_ACCENT_RED
            } else {
                COLOR_TEXT_PRIMARY
            };

            ui.vertical(|ui| {
                ui.add_space(6.0);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&track.title)
                            .color(title_color)
                            .strong()
                            .size(14.0),
                    )
                    .truncate(),
                );
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&track.artist)
                            .color(COLOR_TEXT_SECONDARY)
                            .size(12.0),
                    )
                    .truncate(),
                );
            });

            // Album column (if broad enough and requested)
            if config.show_album && row_width > 600.0 && !track.album.is_empty() {
                ui.add_space(32.0);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&track.album)
                            .color(COLOR_TEXT_SECONDARY)
                            .size(13.0),
                    )
                    .truncate(),
                );
            }

            // Duration right aligned
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(16.0);
                let dur = format_secs(track.duration_secs);
                ui.label(
                    egui::RichText::new(dur)
                        .color(COLOR_TEXT_SECONDARY)
                        .size(13.0),
                );
            });
        });
    });

    if play_triggered {
        actions.push(Action::PlayList {
            tracks: all_tracks.to_vec(),
            start: idx,
            shuffle: None,
        });
    }
}

pub fn draw_central_panel(ui: &mut egui::Ui, app: &App, actions: &mut Vec<Action>) {
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(COLOR_BG)
                .inner_margin(egui::Margin::symmetric(24, 16)),
        )
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(&app.nav.current)
                .auto_shrink([false, false])
                .show(ui, |ui| match &app.nav.current {
                    View::Home => draw_home(ui, app, actions),
                    View::Explore => draw_explore(ui, app, actions),
                    View::Library => draw_library(ui, app, actions),
                    View::Search(q) => draw_search(ui, app, q, actions),
                    View::Album(id) => draw_collection(ui, app, id, actions),
                    View::Playlist(id) => draw_collection(ui, app, id, actions),
                    View::Artist(id) => draw_artist(ui, app, id, actions),
                });
        });
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nav_history_and_capping() {
        let mut nav = Nav::new(View::Home);
        assert_eq!(nav.current, View::Home);
        assert!(!nav.can_back());

        // go pushes history
        nav.go(View::Explore);
        assert_eq!(nav.current, View::Explore);
        assert_eq!(nav.history, vec![View::Home]);
        assert!(nav.can_back());

        // go to same view is a no-op
        nav.go(View::Explore);
        assert_eq!(nav.current, View::Explore);
        assert_eq!(nav.history.len(), 1);

        nav.go(View::Search("daft punk".to_string()));
        assert_eq!(nav.current, View::Search("daft punk".to_string()));
        assert_eq!(nav.history.len(), 2);

        // back pops history
        assert!(nav.back());
        assert_eq!(nav.current, View::Explore);
        assert_eq!(nav.history.len(), 1);

        assert!(nav.back());
        assert_eq!(nav.current, View::Home);
        assert_eq!(nav.history.len(), 0);

        // back on empty returns false
        assert!(!nav.back());
        assert_eq!(nav.current, View::Home);

        // history cap at 50
        for i in 0..60 {
            nav.go(View::Album(format!("album_{i}")));
        }
        assert_eq!(nav.history.len(), 50);
    }

    #[test]
    fn test_pick_top_result() {
        let songs = vec![
            Track {
                id: "song_1".to_string(),
                title: "Around the World".to_string(),
                artist: "Daft Punk".to_string(),
                album: "Homework".to_string(),
                duration_secs: 240,
                thumb_url: None,
                artist_id: None,
                album_id: None,
            },
        ];

        let artists = vec![
            Card {
                kind: CardKind::Artist,
                id: "artist_1".to_string(),
                title: "Daft Punk".to_string(),
                subtitle: "Artist • 5M subscribers".to_string(),
                thumb_url: None,
            },
            Card {
                kind: CardKind::Artist,
                id: "artist_2".to_string(),
                title: "Justice".to_string(),
                subtitle: "Artist".to_string(),
                thumb_url: None,
            },
        ];

        // Artist exact match (case-insensitive) -> chooses artist
        assert_eq!(
            pick_top_result("daft punk", &songs, &artists),
            Some(TopResult::Artist(&artists[0]))
        );

        assert_eq!(
            pick_top_result("DAFT PUNK ", &songs, &artists),
            Some(TopResult::Artist(&artists[0]))
        );

        // No artist match -> chooses first song
        assert_eq!(
            pick_top_result("around the world", &songs, &artists),
            Some(TopResult::Song(&songs[0]))
        );

        // No artists present -> chooses first song
        assert_eq!(
            pick_top_result("daft punk", &songs, &[]),
            Some(TopResult::Song(&songs[0]))
        );

        // Both empty -> None
        assert_eq!(pick_top_result("test", &[], &[]), None);
    }
}
