pub mod library;
pub mod lyrics;
pub mod mini;
pub mod pages;

use std::time::Duration;
use crate::audio::Repeat;
use crate::backend::{Card, CardKind, Track};
use crate::App;

pub use library::*;
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
        Lyrics => "mic-vocal",
        Volume => lucide "volume-2",
        VolumeMute => lucide "volume-x",
        Plus => lucide "plus",
        User => lucide "user",
        Settings => "settings",
        Refresh => lucide "refresh-cw",
        Close => lucide "x",
        Alert => lucide "circle-alert",
        Mini => lucide "minimize-2",
        Expand => lucide "maximize-2",
        Pin => lucide "pin",
        PinOff => lucide "pin-off",
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

/// Painted rects of one track row: the full row plus the title and duration
/// labels. Lets the headless layout test assert titles stay visible.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // read by the layout test; production callers ignore it
pub struct TrackRowRects {
    pub row: egui::Rect,
    pub title: egui::Rect,
    pub duration: egui::Rect,
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
    if !query.trim().is_empty() {
        let matched = artists
            .iter()
            .find(|a| eq_case_insensitive(a.title.trim(), query.trim()));
        if let Some(artist) = matched {
            return Some(TopResult::Artist(artist));
        }
    }
    songs.first().map(TopResult::Song)
}

/// Unicode-aware case-insensitive equality without per-frame allocation:
/// compare `to_lowercase` char streams directly ("BJÖRK" == "björk").
/// Not full case folding (no locale, no ß/ẞ handling) — right tradeoff for
/// a per-frame search predicate.
fn eq_case_insensitive(a: &str, b: &str) -> bool {
    a.chars()
        .flat_map(char::to_lowercase)
        .eq(b.chars().flat_map(char::to_lowercase))
}

#[derive(Debug, Clone)]
pub enum Action {
    PlayList {
        tracks: Vec<Track>,
        start: usize,
        shuffle: Option<bool>,
    },
    Prefetch(Track),
    ShowSettings,
    SetPrefetchCount(u8),
    SetCacheMaxMb(u32),
    ClearCache,
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
    ToggleLyrics,
    RetryLyrics,
    Resume,
    Pause,
    SeekRelative(i64),
    Raise,
    Quit,
    ToggleMini,
    ToggleOnTop,
    Navigate(View),
    Back,
    ToggleSidebar,
    Radio(String),
    Retry(View),
    ShowSignIn,
    HideSignIn,
    SignInBrowser(String),
    SignInFile(String),
    SignOut,
    ToggleUserMenu,
    SetLibraryChip(crate::LibraryChip),
}

pub fn format_duration(d: Duration) -> String {
    let s = d.as_secs();
    format!("{}:{:02}", s / 60, s % 60)
}

pub fn format_secs(s: u32) -> String {
    format!("{}:{:02}", s / 60, s % 60)
}

/// How much of the cover's colour the player bar takes.
const PLAYER_BAR_TINT: f32 = 0.16;
/// How long the cover-coloured tint takes to follow a new song.
const TINT_FADE_SECONDS: f32 = 0.45;

/// `base` tinted toward the cover's `accent` by `strength`, eased over
/// `TINT_FADE_SECONDS` so a song change fades instead of jumping. After
/// spotifast's player bar (MIT).
pub fn eased_tint(
    ctx: &egui::Context,
    salt: &'static str,
    base: egui::Color32,
    accent: Option<egui::Color32>,
    strength: f32,
) -> egui::Color32 {
    let target = accent.map_or(base, |accent| base.lerp_to_gamma(accent, strength));
    let [r, g, b, _] = target.to_array();
    let channel = |axis: &'static str, value: u8| {
        ctx.animate_value_with_time(egui::Id::new((salt, axis)), f32::from(value), TINT_FADE_SECONDS)
            .round() as u8
    };
    egui::Color32::from_rgb(channel("r", r), channel("g", g), channel("b", b))
}

/// Click-or-drag seeking on `bar`. Returns the fraction to draw and the
/// position to show (the drag preview while dragging), and pushes a `Seek`
/// on release.
pub fn seek_interaction(
    ui: &mut egui::Ui,
    resp: &egui::Response,
    bar: egui::Rect,
    pos: Duration,
    total: Duration,
    actions: &mut Vec<Action>,
) -> (f32, Duration) {
    let drag_id = resp.id.with("drag_fraction");
    let mut drag_frac: Option<f32> = ui.data_mut(|d| d.get_temp(drag_id));
    let at = |p: egui::Pos2| ((p.x - bar.min.x) / bar.width()).clamp(0.0, 1.0);
    if resp.dragged() {
        if let Some(p) = resp.interact_pointer_pos() {
            drag_frac = Some(at(p));
            ui.data_mut(|d| d.insert_temp(drag_id, at(p)));
        }
    } else if resp.clicked() || resp.drag_stopped() {
        if let Some(frac) = resp.interact_pointer_pos().map(at).or(drag_frac) {
            actions.push(Action::Seek(Duration::from_secs_f32(frac * total.as_secs_f32())));
        }
        drag_frac = None;
        ui.data_mut(|d| d.remove_temp::<f32>(drag_id));
    } else if drag_frac.is_some() {
        drag_frac = None;
        ui.data_mut(|d| d.remove_temp::<f32>(drag_id));
    }
    let played = if total.as_secs_f32() > 0.0 {
        (pos.as_secs_f32() / total.as_secs_f32()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    match drag_frac {
        Some(frac) => (frac, Duration::from_secs_f32(frac * total.as_secs_f32())),
        None => (played, pos),
    }
}

const PLAYING_ID: &str = "ytmfast-now-playing";

/// Record once per frame whether music is audibly playing, for the
/// now-playing bars drawn deep inside rows.
pub fn set_playing(ctx: &egui::Context, playing: bool) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(PLAYING_ID), playing));
}

/// Three bouncing bars marking the playing track; still while paused.
pub fn playing_bars(ui: &egui::Ui, rect: egui::Rect, color: egui::Color32) {
    let playing = ui.ctx().data(|d| d.get_temp::<bool>(egui::Id::new(PLAYING_ID))).unwrap_or(false);
    let time = ui.input(|i| i.time) as f32;
    let bar_w = rect.width() / 5.0;
    for (i, (speed, phase)) in [(7.0, 0.0), (9.5, 1.7), (6.0, 3.1)].into_iter().enumerate() {
        let level = if playing {
            0.35 + 0.65 * (0.5 + 0.5 * (time * speed + phase).sin())
        } else {
            [0.45, 0.8, 0.6][i]
        };
        let h = rect.height() * level;
        let x = rect.left() + bar_w * (2 * i) as f32;
        ui.painter().rect_filled(
            egui::Rect::from_min_max(egui::pos2(x, rect.bottom() - h), egui::pos2(x + bar_w, rect.bottom())),
            egui::CornerRadius::same(1),
            color,
        );
    }
    if playing {
        ui.ctx().request_repaint_after(Duration::from_millis(50));
    }
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

                // Red "Music" Logo (red circle with white play triangle + text).
                // `interact(Sense::click())`: the raw horizontal response only
                // senses hover, so upgrade it to a click target. The circle is
                // hover-only: a click sense here would eat the click meant for
                // the logo and it must navigate Home either way.
                let logo_resp = ui
                    .horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
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
                    .response
                    .interact(egui::Sense::click());

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

                // Right aligned Sign in / avatar button
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    if app.signed_in {
                        let avatar = egui::Button::image(
                            Icon::User.image(COLOR_TEXT_PRIMARY, 16.0),
                        )
                        .fill(COLOR_SURFACE)
                        .corner_radius(16);
                        let avatar_resp = ui.add(avatar);
                        let avatar_clicked = avatar_resp.clicked();
                        if avatar_clicked {
                            actions.push(Action::ToggleUserMenu);
                        }
                        if app.show_user_menu && !avatar_clicked {
                            let menu_resp = egui::Area::new(egui::Id::new("user_menu"))
                                .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-8.0, 56.0))
                                .show(ui.ctx(), |ui| {
                                    egui::Frame::new()
                                        .fill(COLOR_SURFACE)
                                        .corner_radius(8)
                                        .inner_margin(egui::Margin::same(8))
                                        .show(ui, |ui| {
                                            if ui.button("Sign out").clicked() {
                                                actions.push(Action::SignOut);
                                            }
                                        });
                                });
                            let press_origin =
                                ui.input(|i| i.pointer.press_origin().unwrap_or_default());
                            let clicked_outside = ui.input(|i| i.pointer.any_pressed())
                                && !menu_resp.response.rect.contains(press_origin)
                                && !avatar_resp.rect.contains(press_origin);
                            if ui.input(|i| i.key_pressed(egui::Key::Escape)) || clicked_outside
                            {
                                actions.push(Action::ToggleUserMenu);
                            }
                        }
                    } else {
                        let sign_in_btn = egui::Button::image_and_text(
                            Icon::User.image(COLOR_TEXT_SECONDARY, 16.0),
                            egui::RichText::new("Sign in")
                                .color(COLOR_TEXT_SECONDARY)
                                .size(13.0),
                        )
                        .fill(COLOR_SURFACE)
                        .corner_radius(16);
                        if ui.add(sign_in_btn).clicked() {
                            actions.push(Action::ShowSignIn);
                        }
                    }
                    if ui.add(egui::Button::image(Icon::Settings.image(COLOR_TEXT_SECONDARY, 20.0)))
                        .on_hover_text("Settings").clicked()
                    {
                        actions.push(Action::ShowSettings);
                    }
                    if let Some(newer) = &app.available_update {
                        let notice = egui::Button::new(
                            egui::RichText::new(format!("Update to v{}", newer.version))
                                .color(COLOR_TEXT_PRIMARY)
                                .size(13.0),
                        )
                        .fill(COLOR_ACCENT_RED)
                        .corner_radius(16);
                        if ui.add(notice).on_hover_text("Open the release page on GitHub").clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(&newer.url));
                        }
                    }
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

                ui.add_space(12.0);
                if app.signed_in {
                    if let Some(lib) = &app.library {
                        egui::ScrollArea::vertical()
                            .id_salt("sidebar_playlists")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                for pl in &lib.playlists {
                                    let is_active = matches!(&app.nav.current, View::Playlist(id) if *id == pl.id);
                                    let (rect, resp) = ui.allocate_exact_size(
                                        egui::vec2(width - 20.0, 32.0),
                                        egui::Sense::click(),
                                    );
                                    if resp.hovered() || is_active {
                                        ui.painter().rect_filled(
                                            rect,
                                            egui::CornerRadius::same(6),
                                            COLOR_SURFACE,
                                        );
                                    }
                                    ui.scope_builder(
                                        egui::UiBuilder::new().max_rect(rect),
                                        |ui| {
                                            ui.horizontal_centered(|ui| {
                                                ui.add_space(12.0);
                                                ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new(&pl.title)
                                                            .color(if is_active {
                                                                COLOR_TEXT_PRIMARY
                                                            } else {
                                                                COLOR_TEXT_SECONDARY
                                                            })
                                                            .size(13.0),
                                                    )
                                                    .truncate()
                                                    .selectable(false),
                                                );
                                            });
                                        },
                                    );
                                    if resp.clicked() {
                                        actions.push(Action::Navigate(View::Playlist(
                                            pl.id.clone(),
                                        )));
                                    }
                                }
                            });
                    } else if app.library_loading {
                        ui.spinner();
                    }
                } else {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("Sign in to see your playlists")
                            .color(egui::Color32::from_rgb(0x77, 0x77, 0x77))
                            .size(12.0),
                    );
                }
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
    let accent = app
        .queue
        .current()
        .and_then(|t| t.thumb_url.as_deref())
        .and_then(|url| crate::images::shared().accent(ui.ctx(), url));
    let fill = eased_tint(ui.ctx(), "player-bar-tint", COLOR_SURFACE, accent, PLAYER_BAR_TINT);
    egui::Panel::bottom("bottom_panel")
        .exact_size(72.0)
        .show_separator_line(false)
        .frame(
            egui::Frame::new()
                .fill(fill)
                .inner_margin(egui::Margin::symmetric(0, 0)),
        )
        .show(ui, |ui| {
            let cur_pos = app.player.position();
            let cur_track = app.queue.current();
            let total_dur = app.player.duration().unwrap_or_else(|| {
                Duration::from_secs(cur_track.map(|t| t.duration_secs as u64).unwrap_or(0))
            });

            let total_width = ui.available_width();
            let (bar_rect, bar_resp) = ui.allocate_exact_size(
                egui::vec2(total_width, 4.0),
                egui::Sense::click_and_drag(),
            );
            let (display_fraction, preview_pos) =
                seek_interaction(ui, &bar_resp, bar_rect, cur_pos, total_dur, actions);
            let dragging = bar_resp.dragged();

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
            if bar_resp.hovered() || dragging {
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
                let time_str =
                    format!("{} / {}", format_duration(preview_pos), format_duration(total_dur));
                ui.label(
                    egui::RichText::new(time_str)
                        .color(COLOR_TEXT_SECONDARY)
                        .size(12.0),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(16.0);

                    // Right-to-left order on screen: volume, repeat, shuffle, lyrics, queue, mini.
                    let mini_tip = if cfg!(target_os = "macos") {
                        "Mini player (Cmd+Shift+M)"
                    } else {
                        "Mini player (Ctrl+M)"
                    };
                    if ui
                        .add(egui::Button::image(Icon::Mini.image(COLOR_TEXT_SECONDARY, 18.0)).frame(false))
                        .on_hover_text(mini_tip)
                        .clicked()
                    {
                        actions.push(Action::ToggleMini);
                    }
                    ui.add_space(8.0);

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

                    let lyrics_color = if app.lyrics_open {
                        COLOR_ACCENT_RED
                    } else {
                        COLOR_TEXT_SECONDARY
                    };
                    if ui
                        .add(egui::Button::image(Icon::Lyrics.image(lyrics_color, 18.0)).frame(false))
                        .on_hover_text("Lyrics")
                        .clicked()
                    {
                        actions.push(Action::ToggleLyrics);
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
                                ui.set_max_width(ui.available_width().max(0.0));
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
                                if let Some(err) = app.player_error() {
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
                        } else if let Some(err) = app.player_error() {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(err)
                                        .color(egui::Color32::from_rgb(0xFF, 0x44, 0x44))
                                        .size(11.0),
                                )
                                .truncate(),
                            );
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
            // Scroll the current row into view only when the current track
            // changes, not every frame. Key on (pos, hashed track id): a NEW
            // queue starting at the same position must still scroll, and the
            // hash avoids a per-frame String clone. `show_rows` virtualizes
            // rows, so a far-away current row has no widget to `scroll_to_me`
            // — jump via an explicit scroll offset instead.
            let seen_key = (
                current_pos,
                app.queue.current().map(|t| {
                    use std::hash::{DefaultHasher, Hash, Hasher};
                    let mut h = DefaultHasher::new();
                    t.id.hash(&mut h);
                    h.finish()
                }),
            );
            let seen: Option<(usize, Option<u64>)> =
                ui.data_mut(|d| d.get_temp(ui.id().with("queue_seen")));
            let scroll_to_current = seen != Some(seen_key);
            if scroll_to_current {
                ui.data_mut(|d| {
                    d.insert_temp(ui.id().with("queue_seen"), seen_key);
                });
            }
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
                        hover_prefetch(ui, &row_resp, track, actions);
                        draw_queue_row_body(ui, row_rect, track, is_current);
                        if row_resp.clicked() {
                            actions.push(Action::Jump(pos));
                        }
                    }
                });
        });
}

/// Body of one queue row inside its click rect: thumb + title/artist against
/// the duration. Returns title/duration rects like `draw_track_row` for the
/// headless layout test.
fn draw_queue_row_body(
    ui: &mut egui::Ui,
    row_rect: egui::Rect,
    track: &Track,
    is_current: bool,
) -> TrackRowRects {
    let mut title_rect = egui::Rect::NOTHING;
    let mut duration_rect = egui::Rect::NOTHING;
    ui.scope_builder(egui::UiBuilder::new().max_rect(row_rect), |ui| {
        ui.horizontal_centered(|ui| {
            ui.add_space(4.0);
            let thumb = if let Some(url) = &track.thumb_url {
                ui.add(
                    egui::Image::from_uri(url)
                        .fit_to_exact_size(egui::vec2(40.0, 40.0))
                        .corner_radius(4),
                )
                .rect
            } else {
                let (r, _) =
                    ui.allocate_exact_size(egui::vec2(40.0, 40.0), egui::Sense::hover());
                ui.painter().rect_filled(
                    r,
                    egui::CornerRadius::same(4),
                    egui::Color32::from_rgb(0x30, 0x30, 0x30),
                );
                r
            };
            if is_current {
                ui.painter().rect_filled(thumb, egui::CornerRadius::same(4), egui::Color32::from_black_alpha(150));
                playing_bars(
                    ui,
                    egui::Rect::from_center_size(thumb.center(), egui::vec2(16.0, 16.0)),
                    COLOR_TEXT_PRIMARY,
                );
            }
            ui.add_space(8.0);
            // Duration first so the title truncates against it
            // instead of under it (360 px panel, long titles).
            // The text column lives INSIDE the right-to-left
            // closure: a bare `with_layout(right_to_left)`
            // moves the parent cursor past the right edge,
            // so a following sibling column is laid out
            // outside the row and the title goes invisible.
            ui.with_layout(
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.add_space(8.0);
                    duration_rect = ui
                        .label(
                            egui::RichText::new(format_secs(track.duration_secs))
                                .color(COLOR_TEXT_SECONDARY)
                                .size(12.0),
                        )
                        .rect;
                    ui.with_layout(
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.add_space(6.0);
                            title_rect = ui
                                .add(
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
                                )
                                .rect;
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&track.artist)
                                        .color(COLOR_TEXT_SECONDARY)
                                        .size(12.0),
                                )
                                .truncate(),
                            );
                        },
                    );
                },
            );
        });
    });
    TrackRowRects {
        row: row_rect,
        title: title_rect,
        duration: duration_rect,
    }
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

pub fn hover_prefetch(ui: &mut egui::Ui, response: &egui::Response, track: &Track, actions: &mut Vec<Action>) {
    let id = response.id.with(("hover_prefetch", &track.id));
    let frame = ui.ctx().cumulative_frame_nr();
    let now = std::time::Instant::now();
    if !response.contains_pointer() {
        ui.data_mut(|data| data.remove::<(std::time::Instant, u64, bool)>(id));
        return;
    }
    let (start, last_frame, sent) = ui.data_mut(|data| {
        data.get_temp::<(std::time::Instant, u64, bool)>(id).unwrap_or((now, frame, false))
    });
    let (start, sent) = if frame.saturating_sub(last_frame) > 1 { (now, false) } else { (start, sent) };
    let elapsed = now.duration_since(start);
    let delay = Duration::from_millis(300);
    let ready = elapsed >= delay;
    if ready && !sent {
        actions.push(Action::Prefetch(track.clone()));
    } else if !ready {
        ui.ctx().request_repaint_after(delay - elapsed);
    }
    ui.data_mut(|data| data.insert_temp(id, (start, frame, sent || ready)));
}

pub fn draw_settings_dialog(ui: &mut egui::Ui, app: &mut App, actions: &mut Vec<Action>) {
    if !app.show_settings { return; }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.show_settings = false;
        return;
    }
    egui::Window::new("Settings")
        .open(&mut app.show_settings)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .collapsible(false)
        .resizable(false)
        .frame(egui::Frame::window(ui.style()).fill(COLOR_SURFACE))
        .show(ui.ctx(), |ui| {
            ui.heading("Playback");
            let mut count = app.prefetch_count;
            if ui.add(egui::Slider::new(&mut count, 0..=10).text("Prefetch next tracks")).changed() {
                actions.push(Action::SetPrefetchCount(count));
            }
            ui.add_space(12.0);
            ui.heading("Cache");
            ui.horizontal(|ui| {
                ui.label("Cache size limit");
                let mut mb = app.cache_max_mb;
                let r = ui.add(egui::DragValue::new(&mut mb).range(0..=100000).suffix(" MB").speed(16).update_while_editing(false));
                app.cache_max_mb = mb;
                if r.drag_stopped() || (r.changed() && !r.dragged()) {
                    actions.push(Action::SetCacheMaxMb(mb));
                }
            });
            ui.label(egui::RichText::new("0 keeps only the playing and upcoming tracks").small().color(COLOR_TEXT_SECONDARY));
            if let Some((bytes, files)) = app.cache_usage {
                ui.label(format!("Using {:.1} MB in {files} tracks", bytes as f64 / (1024.0 * 1024.0)));
            } else {
                ui.label("Using …");
            }
            if ui.button("Clear cache").clicked() {
                actions.push(Action::ClearCache);
            }
            ui.add_space(12.0);
            ui.heading("About");
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
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
) -> TrackRowRects {
    let row_height = 56.0;
    let row_width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(row_width, row_height), egui::Sense::click());

    hover_prefetch(ui, &resp, track, actions);
    let is_current = current_id == Some(track.id.as_str());

    if resp.hovered() || is_current {
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(4),
            COLOR_SURFACE,
        );
    }

    let mut play_triggered = false;
    let mut title_rect = egui::Rect::NOTHING;
    let mut duration_rect = egui::Rect::NOTHING;
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
                    playing_bars(
                        ui,
                        egui::Rect::from_center_size(btn_rect.center(), egui::vec2(14.0, 14.0)),
                        COLOR_ACCENT_RED,
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

                if is_current && !(resp.hovered() || thumb_resp.hovered()) {
                    ui.painter().rect_filled(
                        thumb_rect,
                        egui::CornerRadius::same(4),
                        egui::Color32::from_black_alpha(150),
                    );
                    playing_bars(
                        ui,
                        egui::Rect::from_center_size(thumb_rect.center(), egui::vec2(16.0, 16.0)),
                        COLOR_TEXT_PRIMARY,
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

            // Right edge first: duration (and album) claim their width before
            // the text column. The text column lives INSIDE the right-to-left
            // closure: a bare `with_layout(right_to_left)` moves the parent
            // cursor past the right edge, so a following sibling column is
            // laid out outside the row and the title goes invisible.
            let show_album_col =
                config.show_album && row_width > 600.0 && !track.album.is_empty();
            let title_color = if is_current {
                COLOR_ACCENT_RED
            } else {
                COLOR_TEXT_PRIMARY
            };
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(16.0);
                let dur = format_secs(track.duration_secs);
                duration_rect = ui
                    .label(
                        egui::RichText::new(dur)
                            .color(COLOR_TEXT_SECONDARY)
                            .size(13.0),
                    )
                    .rect;
                if show_album_col {
                    ui.add_space(32.0);
                    ui.add_sized(
                        [180.0, 20.0],
                        egui::Label::new(
                            egui::RichText::new(&track.album)
                                .color(COLOR_TEXT_SECONDARY)
                                .size(13.0),
                        )
                        .truncate(),
                    );
                }
                // Title & artist take the remaining width, truncating against
                // the duration/album instead of under them.
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.add_space(6.0);
                    title_rect = ui
                        .add(
                            egui::Label::new(
                                egui::RichText::new(&track.title)
                                    .color(title_color)
                                    .strong()
                                    .size(14.0),
                            )
                            .truncate(),
                        )
                        .rect;
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&track.artist)
                                .color(COLOR_TEXT_SECONDARY)
                                .size(12.0),
                        )
                        .truncate(),
                    );
                });
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
    TrackRowRects {
        row: rect,
        title: title_rect,
        duration: duration_rect,
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

    /// Draw one row headlessly at `width` and return its painted rects.
    /// Two `ctx.run_ui` passes: the first warms the font atlas so the
    /// second measures real glyph widths.
    fn row_rects_at(width: f32) -> TrackRowRects {
        let ctx = egui::Context::default();
        let track = Track {
            id: "t1".to_string(),
            title: "A very long track title that must stay visible and truncate instead of sliding out of the row"
                .to_string(),
            artist: "Some Artist".to_string(),
            album: "Some Album".to_string(),
            duration_secs: 245,
            thumb_url: None,
            artist_id: None,
            album_id: None,
        };
        let tracks = vec![track.clone()];
        let mut out = TrackRowRects {
            row: egui::Rect::NOTHING,
            title: egui::Rect::NOTHING,
            duration: egui::Rect::NOTHING,
        };
        for _ in 0..2 {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::pos2(0.0, 0.0),
                        egui::vec2(width, 200.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let mut actions = Vec::new();
                        out = draw_track_row(
                            ui,
                            0,
                            &track,
                            &tracks,
                            None,
                            TrackRowConfig::PLAYLIST,
                            &mut actions,
                        );
                    });
                },
            )
            .drop_without_applying_deltas();
        }
        out
    }

    /// Draw one queue row body headlessly at `width` (long title, no thumb).
    fn queue_row_rects_at(width: f32) -> TrackRowRects {
        let ctx = egui::Context::default();
        let track = Track {
            id: "t1".to_string(),
            title: "A very long track title that must stay visible and truncate instead of sliding out of the row"
                .to_string(),
            artist: "Some Artist".to_string(),
            album: "Some Album".to_string(),
            duration_secs: 245,
            thumb_url: None,
            artist_id: None,
            album_id: None,
        };
        let mut out = TrackRowRects {
            row: egui::Rect::NOTHING,
            title: egui::Rect::NOTHING,
            duration: egui::Rect::NOTHING,
        };
        for _ in 0..2 {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::pos2(0.0, 0.0),
                        egui::vec2(width, 200.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let row_rect = egui::Rect::from_min_size(
                            ui.available_rect_before_wrap().min,
                            egui::vec2(ui.available_width(), 56.0),
                        );
                        out = draw_queue_row_body(ui, row_rect, &track, false);
                    });
                },
            )
            .drop_without_applying_deltas();
        }
        out
    }
    fn assert_title_visible(r: TrackRowRects, width: f32, what: &str) {
        assert!(r.row.width() > 0.0, "{what} row has no width at {width}");
        // Title sits left of the duration, is wide, stays inside the
        // row, and never overlaps the duration label.
        assert!(
            r.title.min.x < r.duration.min.x,
            "{what} title not left of duration at {width}: {:?} vs {:?}",
            r.title,
            r.duration
        );
        assert!(
            r.title.width() > 100.0,
            "{what} title too narrow at {width}: {:?}",
            r.title
        );
        assert!(
            r.row.contains_rect(r.title),
            "{what} title outside row at {width}: {:?} vs {:?}",
            r.title,
            r.row
        );
        assert!(
            !r.title.intersects(r.duration),
            "{what} title overlaps duration at {width}: {:?} vs {:?}",
            r.title,
            r.duration
        );
    }

    #[test]
    fn test_track_row_title_stays_visible() {
        for width in [360.0, 1000.0] {
            assert_title_visible(row_rects_at(width), width, "track");
        }
        // Queue panel rows share the same geometry contract at panel width.
        assert_title_visible(queue_row_rects_at(360.0), 360.0, "queue");
    }

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

    #[test]
    fn test_pick_top_result_non_ascii_case() {
        let songs: Vec<Track> = vec![];
        let artists = vec![Card {
            kind: CardKind::Artist,
            id: "artist_bjork".to_string(),
            title: "BJÖRK".to_string(),
            subtitle: "Artist".to_string(),
            thumb_url: None,
        }];
        // eq_ignore_ascii_case would fail on Ö/ö; the char-stream compare
        // must still match.
        assert_eq!(
            pick_top_result("björk", &songs, &artists),
            Some(TopResult::Artist(&artists[0]))
        );
        assert_eq!(
            pick_top_result("  BJÖRK ", &songs, &artists),
            Some(TopResult::Artist(&artists[0]))
        );
        assert!(eq_case_insensitive("BJÖRK", "björk"));
        assert!(!eq_case_insensitive("björk", "bjork"));
    }
}
