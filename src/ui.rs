use std::time::Duration;
use crate::audio::Repeat;
use crate::App;

#[derive(Debug, Clone)]
pub enum Action {
    Search(String),
    PlayFrom(usize),
    TogglePlayPause,
    NextTrack(bool),
    PrevTrack,
    Seek(Duration),
    SetVolume(f32),
    ToggleShuffle,
    CycleRepeat,
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
    visuals.panel_fill = egui::Color32::from_rgb(0x03, 0x03, 0x03);
    visuals.window_fill = egui::Color32::from_rgb(0x03, 0x03, 0x03);
    visuals.extreme_bg_color = egui::Color32::from_rgb(0x21, 0x21, 0x21);
    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(0x21, 0x21, 0x21);
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(0x38, 0x38, 0x38);
    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(0x48, 0x48, 0x48);
    visuals.selection.bg_fill = egui::Color32::from_rgb(0xFF, 0x00, 0x00);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);
    visuals.override_text_color = Some(egui::Color32::WHITE);
    ctx.set_visuals(visuals);
}

pub fn draw_top_bar(ui: &mut egui::Ui, app: &mut App, actions: &mut Vec<Action>) {
    egui::Panel::top("top_panel")
        .exact_size(64.0)
        .show_separator_line(false)
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(0x03, 0x03, 0x03))
                .inner_margin(egui::Margin::symmetric(24, 14)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(
                    egui::RichText::new("Music")
                        .color(egui::Color32::from_rgb(0xFF, 0x00, 0x00))
                        .strong()
                        .size(24.0),
                );
                ui.add_space(32.0);

                let frame = egui::Frame::new()
                    .fill(egui::Color32::from_rgb(0x21, 0x21, 0x21))
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(14, 8));

                let response = frame
                    .show(ui, |ui| {
                        ui.set_width(480.0);
                        let edit = egui::TextEdit::singleline(&mut app.search_input)
                            .hint_text("Search songs")
                            .frame(egui::Frame::NONE)
                            .desired_width(460.0);
                        ui.add(edit)
                    })
                    .inner;

                app.search_focused = response.has_focus();

                let enter_pressed = response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let lost_focus_with_enter =
                    response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if enter_pressed || lost_focus_with_enter {
                    let q = app.search_input.trim().to_string();
                    if !q.is_empty() {
                        actions.push(Action::Search(q));
                    }
                }

                if app.is_searching {
                    ui.add_space(12.0);
                    ui.spinner();
                }
            });
        });
}

pub fn draw_central_panel(ui: &mut egui::Ui, app: &App, actions: &mut Vec<Action>) {
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(0x03, 0x03, 0x03))
                .inner_margin(egui::Margin::symmetric(24, 12)),
        )
        .show(ui, |ui| {
            if let Some(err) = &app.search_error {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Search error: {err}"))
                            .color(egui::Color32::from_rgb(0xFF, 0x44, 0x44))
                            .size(13.0),
                    );
                });
                ui.add_space(8.0);
            }

            if app.results.is_empty() {
                if !app.is_searching {
                    ui.vertical_centered(|ui| {
                        ui.add_space(100.0);
                        ui.label(
                            egui::RichText::new("Search songs, albums, or artists above")
                                .color(egui::Color32::from_rgb(0xAA, 0xAA, 0xAA))
                                .size(16.0),
                        );
                    });
                }
                return;
            }

            let current_id = app.queue.current().map(|t| t.id.as_str());

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show_rows(ui, 52.0, app.results.len(), |ui, row_range| {
                    for idx in row_range {
                        let track = &app.results[idx];
                        let is_current = current_id == Some(track.id.as_str());

                        let row_width = ui.available_width();
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(row_width, 48.0),
                            egui::Sense::click(),
                        );

                        if response.hovered() || is_current {
                            ui.painter().rect_filled(
                                rect,
                                egui::CornerRadius::same(4),
                                egui::Color32::from_rgb(0x21, 0x21, 0x21),
                            );
                        }

                        if response.clicked() {
                            actions.push(Action::PlayFrom(idx));
                        }

                        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.add_space(8.0);

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
                                    ui.add_space(4.0);
                                    let title_color = if is_current {
                                        egui::Color32::from_rgb(0xFF, 0x00, 0x00)
                                    } else {
                                        egui::Color32::WHITE
                                    };
                                    let title_prefix = if is_current { "▶ " } else { "" };
                                    ui.label(
                                        egui::RichText::new(format!("{title_prefix}{}", track.title))
                                            .color(title_color)
                                            .strong()
                                            .size(14.0),
                                    );

                                    let meta = if track.album.is_empty() {
                                        track.artist.clone()
                                    } else {
                                        format!("{} • {}", track.artist, track.album)
                                    };
                                    ui.label(
                                        egui::RichText::new(meta)
                                            .color(egui::Color32::from_rgb(0xAA, 0xAA, 0xAA))
                                            .size(12.0),
                                    );
                                });

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.add_space(16.0);
                                        let dur = format_secs(track.duration_secs);
                                        ui.label(
                                            egui::RichText::new(dur)
                                                .color(egui::Color32::from_rgb(0xAA, 0xAA, 0xAA))
                                                .size(13.0),
                                        );
                                    },
                                );
                            });
                        });
                        ui.add_space(4.0);
                    }
                });
        });
}

pub fn draw_player_bar(ui: &mut egui::Ui, app: &App, actions: &mut Vec<Action>) {
    egui::Panel::bottom("bottom_panel")
        .exact_size(72.0)
        .show_separator_line(false)
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(0x21, 0x21, 0x21))
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
                egui::Color32::from_rgb(0xFF, 0x00, 0x00),
            );
            if bar_resp.hovered() || bar_resp.dragged() || drag_frac.is_some() {
                let circle_center = egui::pos2(
                    bar_rect.min.x + bar_rect.width() * display_fraction,
                    bar_rect.center().y,
                );
                ui.painter().circle_filled(
                    circle_center,
                    4.0,
                    egui::Color32::from_rgb(0xFF, 0x00, 0x00),
                );
            }

            ui.horizontal_centered(|ui| {
                ui.add_space(16.0);

                if ui.button(egui::RichText::new("⏮").size(16.0)).clicked() {
                    actions.push(Action::PrevTrack);
                }
                let is_paused = app.player.is_paused();
                let play_icon = if is_paused { "▶" } else { "⏸" };
                if ui.button(egui::RichText::new(play_icon).size(18.0)).clicked() {
                    actions.push(Action::TogglePlayPause);
                }
                if ui.button(egui::RichText::new("⏭").size(16.0)).clicked() {
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
                        .color(egui::Color32::from_rgb(0xAA, 0xAA, 0xAA))
                        .size(12.0),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(16.0);

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
                    ui.label(egui::RichText::new("🔊").size(14.0));
                    ui.add_space(8.0);

                    let shuf_color = if app.queue.shuffle {
                        egui::Color32::from_rgb(0xFF, 0x00, 0x00)
                    } else {
                        egui::Color32::from_rgb(0x88, 0x88, 0x88)
                    };
                    if ui.button(egui::RichText::new("🔀").color(shuf_color).size(15.0)).clicked() {
                        actions.push(Action::ToggleShuffle);
                    }

                    let (rep_icon, rep_color) = match app.queue.repeat {
                        Repeat::Off => ("🔁", egui::Color32::from_rgb(0x88, 0x88, 0x88)),
                        Repeat::All => ("🔁", egui::Color32::WHITE),
                        Repeat::One => ("🔂", egui::Color32::from_rgb(0xFF, 0x00, 0x00)),
                    };
                    if ui.button(egui::RichText::new(rep_icon).color(rep_color).size(15.0)).clicked() {
                        actions.push(Action::CycleRepeat);
                    }

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
                                    ui.label(
                                        egui::RichText::new(&track.title)
                                            .color(egui::Color32::WHITE)
                                            .strong()
                                            .size(13.0),
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
                                    ui.label(
                                        egui::RichText::new(err)
                                            .color(egui::Color32::from_rgb(0xFF, 0x44, 0x44))
                                            .size(11.0),
                                    );
                                } else {
                                    let meta = if track.album.is_empty() {
                                        track.artist.clone()
                                    } else {
                                        format!("{} • {}", track.artist, track.album)
                                    };
                                    ui.label(
                                        egui::RichText::new(meta)
                                            .color(egui::Color32::from_rgb(0xAA, 0xAA, 0xAA))
                                            .size(11.0),
                                    );
                                }
                            });
                        }
                    });
                });
            });
        });
}
