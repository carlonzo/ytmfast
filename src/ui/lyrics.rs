//! The playing track's lyrics, with timed highlights and click-to-seek.

use super::{Action, COLOR_BG, COLOR_DIVIDER, COLOR_TEXT_PRIMARY, COLOR_TEXT_SECONDARY};
use crate::{
    App,
    lyrics::{LyricsState, Source},
};
use std::time::Duration;

pub fn draw_lyrics_panel(ui: &mut egui::Ui, app: &App, actions: &mut Vec<Action>) {
    egui::Panel::right("lyrics_panel")
        .exact_size(360.0)
        .show_separator_line(false)
        .frame(
            egui::Frame::new()
                .fill(COLOR_BG)
                .inner_margin(egui::Margin::same(12)),
        )
        .show(ui, |ui| {
            let rect = ui.max_rect();
            ui.painter().line_segment(
                [rect.left_top(), rect.left_bottom()],
                egui::Stroke::new(1.0, COLOR_DIVIDER),
            );
            ui.label(
                egui::RichText::new("Lyrics")
                    .color(COLOR_TEXT_PRIMARY)
                    .strong()
                    .size(16.0),
            );
            let Some(track) = app.queue.current() else {
                ui.label("Play a song to see its lyrics");
                return;
            };
            ui.add(
                egui::Label::new(egui::RichText::new(&track.title).color(COLOR_TEXT_SECONDARY))
                    .truncate(),
            );
            ui.add(
                egui::Label::new(egui::RichText::new(&track.artist).color(COLOR_TEXT_SECONDARY))
                    .truncate(),
            );
            ui.add_space(12.0);
            let state = app
                .lyrics
                .as_ref()
                .filter(|(id, _)| *id == track.id)
                .map(|(_, state)| state);
            let lyrics = match state {
                None | Some(LyricsState::Loading) => {
                    ui.spinner();
                    return;
                }
                Some(LyricsState::Error(error)) => {
                    ui.label(egui::RichText::new(error).color(COLOR_TEXT_SECONDARY));
                    if ui.button("Retry").clicked() {
                        actions.push(Action::RetryLyrics);
                    }
                    return;
                }
                Some(LyricsState::Ready(None)) => {
                    ui.label("No lyrics found");
                    return;
                }
                Some(LyricsState::Ready(Some(lyrics))) => lyrics,
            };
            let source = match lyrics.source {
                Source::Lrclib => "Lyrics: LRCLIB",
                Source::YouTubeMusic => "Lyrics: YouTube Music",
            };
            if lyrics.instrumental {
                ui.label("Instrumental");
                ui.label(
                    egui::RichText::new(source)
                        .small()
                        .color(COLOR_TEXT_SECONDARY),
                );
                return;
            }
            if !lyrics.synced {
                ui.label(
                    egui::RichText::new("Not synced")
                        .small()
                        .color(COLOR_TEXT_SECONDARY),
                );
            }
            let active =
                lyrics.active_line(app.player.position().as_millis().min(u32::MAX as u128) as u32);
            let seen_id = ui.id().with("lyrics_seen");
            let current = {
                use std::hash::{DefaultHasher, Hash, Hasher};
                let mut h = DefaultHasher::new();
                track.id.hash(&mut h);
                (h.finish(), active)
            };
            let seen: Option<(u64, Option<usize>)> = ui.data_mut(|d| d.get_temp(seen_id));
            let follow = seen != Some(current);
            ui.data_mut(|d| d.insert_temp(seen_id, current));
            // Reserve space so attribution remains visible even for long songs.
            egui::ScrollArea::vertical()
                .id_salt(("lyrics", &track.id))
                .auto_shrink([false, false])
                .max_height((ui.available_height() - 28.0).max(0.0))
                .show(ui, |ui| {
                    for (index, line) in lyrics.lines.iter().enumerate() {
                        let color = if !lyrics.synced || active == Some(index) {
                            COLOR_TEXT_PRIMARY
                        } else if active.is_some_and(|active| index < active) {
                            COLOR_TEXT_SECONDARY
                        } else {
                            egui::Color32::from_gray(0x77)
                        };
                        let mut text = egui::RichText::new(if line.text.is_empty() {
                            " "
                        } else {
                            &line.text
                        })
                        .size(20.0)
                        .color(color);
                        if lyrics.synced && active == Some(index) {
                            text = text.strong();
                        }
                        let response = ui.add(egui::Label::new(text).wrap().sense(
                            if line.at_ms.is_some() {
                                egui::Sense::click()
                            } else {
                                egui::Sense::hover()
                            },
                        ));
                        if response.clicked()
                            && let Some(at) = line.at_ms
                        {
                            actions.push(Action::Seek(Duration::from_millis(u64::from(at))));
                        }
                        if follow && active == Some(index) {
                            response.scroll_to_me(Some(egui::Align::Center));
                        }
                        ui.add_space(10.0);
                    }
                });
            ui.label(
                egui::RichText::new(source)
                    .small()
                    .color(COLOR_TEXT_SECONDARY),
            );
        });
}
