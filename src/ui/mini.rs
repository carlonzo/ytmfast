//! Mini player: the YouTube Music phone layout in a small window. Tall
//! windows stack cover, title, seek bar and controls; short ones (a strip on
//! a side screen) put the cover beside them.

use std::time::Duration;

use egui::{Align2, Color32, CornerRadius, Rect, RichText, Sense, Stroke, Vec2, pos2, vec2};

use super::{Action, COLOR_BG, Icon, eased_tint, format_duration, seek_interaction};
use crate::{App, audio::Repeat, images};

const PAD: f32 = 16.0;
const WHITE: Color32 = Color32::WHITE;
const DIM: Color32 = Color32::from_rgba_premultiplied(0x99, 0x99, 0x99, 0x99);
/// Height of the window buttons row plus its gap, above the cover.
const TOP: f32 = 40.0;
/// Height below the cover: gap, title, seek bar, controls, "up next".
const AFTER_ART: f32 = 212.0;
/// Space the tall layout needs besides the cover.
const TALL_CHROME: f32 = 2.0 * PAD + TOP + AFTER_ART;
/// Below this cover size the tall layout gives way to the strip.
const MIN_TALL_ART: f32 = 120.0;
/// How much of the cover's colour the background takes.
const BG_TINT: f32 = 0.38;

pub fn draw_mini_player(ui: &mut egui::Ui, app: &App, actions: &mut Vec<Action>) {
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(COLOR_BG))
        .show(ui, |ui| {
            let ctx = ui.ctx().clone();
            let rect = ui.max_rect();
            let track = app.queue.current();
            let accent = track
                .and_then(|t| t.thumb_url.as_deref())
                .and_then(|url| images::shared().accent(&ctx, url));
            let top = eased_tint(&ctx, "mini-tint", COLOR_BG, accent, BG_TINT);
            paint_gradient(ui.painter(), rect, top, top.lerp_to_gamma(COLOR_BG, 0.6));

            let art = (rect.width() - 2.0 * PAD).min(rect.height() - TALL_CHROME);
            if art >= MIN_TALL_ART {
                tall(ui, app, rect, art, actions);
            } else {
                strip(ui, app, rect, actions);
            }
        });
}

/// Phone layout, top to bottom.
fn tall(ui: &mut egui::Ui, app: &App, rect: Rect, art: f32, actions: &mut Vec<Action>) {
    let inner = rect.shrink(PAD);
    let mut y = inner.top();

    window_buttons(ui, app, Rect::from_min_size(inner.min, vec2(inner.width(), 28.0)), actions);
    y += TOP;

    // The cover sits in the space left over, centred.
    let free = inner.bottom() - AFTER_ART - y;
    let art_top = y + ((free - art) / 2.0).max(0.0);
    let art_rect = Rect::from_center_size(pos2(inner.center().x, art_top + art / 2.0), Vec2::splat(art));
    cover(ui, app, art_rect, 10.0);
    y = art_rect.bottom() + 20.0;

    titles(ui, app, Rect::from_min_size(pos2(inner.left(), y), vec2(inner.width(), 48.0)), 20.0);
    y += 60.0;

    seek_bar(ui, app, Rect::from_min_size(pos2(inner.left(), y), vec2(inner.width(), 34.0)), true, actions);
    y += 42.0;

    controls(ui, app, Rect::from_min_size(pos2(inner.left(), y), vec2(inner.width(), 64.0)), 64.0, true, actions);
    y += 72.0;

    if let Some(next) = app.queue.peek_next() {
        let line = Rect::from_min_max(pos2(inner.left(), y), pos2(inner.right(), inner.bottom()));
        label(ui, line, RichText::new(format!("Up next: {}", next.title)).size(12.0).color(DIM), egui::Align::Center);
    }
}

/// A short, wide window: cover on the left, everything else beside it.
fn strip(ui: &mut egui::Ui, app: &App, rect: Rect, actions: &mut Vec<Action>) {
    let pad = 10.0;
    let inner = rect.shrink(pad);
    let art = inner.height().min(160.0).min(inner.width() * 0.4);
    let art_rect = Rect::from_min_size(pos2(inner.left(), inner.center().y - art / 2.0), Vec2::splat(art));
    cover(ui, app, art_rect, 6.0);

    let left = art_rect.right() + 12.0;
    let col = Rect::from_min_max(pos2(left, inner.top()), inner.right_bottom());
    let buttons = Rect::from_min_max(pos2(col.right() - 56.0, col.top()), pos2(col.right(), col.top() + 24.0));
    window_buttons(ui, app, buttons, actions);

    // Title block, seek bar and controls share the column's height.
    let controls_h = (col.height() * 0.42).clamp(28.0, 48.0);
    let title_rect = Rect::from_min_max(col.min, pos2(buttons.left() - 4.0, col.top() + 36.0));
    titles(ui, app, title_rect, 14.0);
    let controls_rect = Rect::from_min_max(pos2(col.left(), col.bottom() - controls_h), col.right_bottom());
    let seek_rect = Rect::from_min_max(
        pos2(col.left(), title_rect.bottom()),
        pos2(col.right(), controls_rect.top()),
    );
    if seek_rect.height() >= 8.0 {
        seek_bar(ui, app, seek_rect, seek_rect.height() >= 26.0, actions);
    }
    let wide = col.width() >= 220.0;
    controls(ui, app, controls_rect, controls_h, wide, actions);
}

/// Leave the mini player, and pin it above other windows.
fn window_buttons(ui: &mut egui::Ui, app: &App, rect: Rect, actions: &mut Vec<Action>) {
    let size = Vec2::splat(rect.height());
    let expand = Rect::from_min_size(rect.min, size);
    let pin = Rect::from_min_size(pos2(rect.right() - size.x, rect.top()), size);
    let shortcut = if cfg!(target_os = "macos") { "Cmd+Shift+M" } else { "Ctrl+M" };
    if icon_button(ui, expand, Icon::Expand, WHITE, 16.0)
        .on_hover_text(format!("Full player ({shortcut})"))
        .clicked()
    {
        actions.push(Action::ToggleMini);
    }
    let (icon, color, tip) = if app.mini_on_top {
        (Icon::Pin, WHITE, "Stop keeping on top")
    } else {
        (Icon::PinOff, DIM, "Keep on top of other windows")
    };
    if icon_button(ui, pin, icon, color, 16.0).on_hover_text(tip).clicked() {
        actions.push(Action::ToggleOnTop);
    }
}

/// The cover, shrinking a little while paused (an eased "breath").
fn cover(ui: &mut egui::Ui, app: &App, rect: Rect, radius: f32) {
    let playing = app.is_active_playback && !app.is_loading && !app.player.is_paused();
    let t = ui.ctx().animate_bool_with_time(egui::Id::new("mini-cover-playing"), playing, 0.3);
    let rect = Rect::from_center_size(rect.center(), rect.size() * egui::lerp(0.9..=1.0, t));
    let corner = CornerRadius::same(radius as u8);
    ui.painter().rect_filled(rect, corner, Color32::from_white_alpha(14));
    let Some(url) = app.queue.current().and_then(|t| t.thumb_url.as_deref()) else {
        return;
    };
    // The small thumbnail is usually cached already; the large one paints
    // over it once loaded.
    paint_square(ui, url.to_string(), rect, corner);
    if rect.width() > 120.0 {
        let px = ((rect.width() * ui.ctx().pixels_per_point()) as u32)
            .clamp(226, 1200)
            .next_multiple_of(64);
        paint_square(ui, images::sized(url, px), rect, corner);
        // Fetch the next cover now so a skip shows it at once.
        if let Some(next) = app.queue.peek_next().and_then(|t| t.thumb_url.as_deref()) {
            images::shared().prefetch(ui.ctx(), &images::sized(next, px));
        }
    }
}

/// Paint `uri` into the square `rect`, cropping the middle of a wide video
/// thumbnail instead of squashing it.
fn paint_square(ui: &egui::Ui, uri: String, rect: Rect, corner: CornerRadius) {
    let image = egui::Image::from_uri(uri)
        .corner_radius(corner)
        .show_loading_spinner(false);
    let crop = match image.load_for_size(ui.ctx(), rect.size()) {
        Ok(egui::load::TexturePoll::Ready { texture }) => square_uv(texture.size),
        _ => Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
    };
    image.uv(crop).paint_at(ui, rect);
}

/// The centred square of an image of `size`, in texture coordinates.
fn square_uv(size: Vec2) -> Rect {
    let unit = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    if size.x <= 0.0 || size.y <= 0.0 {
        return unit;
    }
    if size.x > size.y {
        let m = (1.0 - size.y / size.x) / 2.0;
        Rect::from_min_max(pos2(m, 0.0), pos2(1.0 - m, 1.0))
    } else {
        let m = (1.0 - size.x / size.y) / 2.0;
        Rect::from_min_max(pos2(0.0, m), pos2(1.0, 1.0 - m))
    }
}

fn titles(ui: &mut egui::Ui, app: &App, rect: Rect, size: f32) {
    let Some(track) = app.queue.current() else {
        label(ui, rect, RichText::new("Nothing playing").size(size).strong().color(WHITE), egui::Align::Min);
        return;
    };
    let line = size + 6.0;
    let title = Rect::from_min_size(rect.min, vec2(rect.width(), line));
    label(ui, title, RichText::new(&track.title).size(size).strong().color(WHITE), egui::Align::Min);
    let sub = Rect::from_min_size(pos2(rect.left(), title.bottom()), vec2(rect.width(), line * 0.8));
    let (text, color) = match app.player_error() {
        Some(err) => (err.to_string(), Color32::from_rgb(0xFF, 0x66, 0x66)),
        None if app.is_loading => ("Loading…".to_string(), DIM),
        None => (track.artist.clone(), DIM),
    };
    label(ui, sub, RichText::new(text).size(size * 0.75).color(color), egui::Align::Min);
}

fn label(ui: &mut egui::Ui, rect: Rect, text: RichText, align: egui::Align) {
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(align)),
        |ui| ui.add(egui::Label::new(text).truncate()),
    );
}

/// Thin rounded bar, knob on hover, elapsed and total time underneath.
fn seek_bar(ui: &mut egui::Ui, app: &App, rect: Rect, with_times: bool, actions: &mut Vec<Action>) {
    let total = app.player.duration().unwrap_or_else(|| {
        Duration::from_secs(app.queue.current().map(|t| t.duration_secs as u64).unwrap_or(0))
    });
    let pos = app.player.position();
    let bar = Rect::from_min_size(rect.min + vec2(0.0, 4.0), vec2(rect.width(), 12.0));
    let resp = ui.interact(bar, ui.id().with("mini-seek"), Sense::click_and_drag());
    let (fraction, preview) = seek_interaction(ui, &resp, bar, pos, total, actions);

    let hot = ui.ctx().animate_bool_with_time(resp.id, resp.hovered() || resp.dragged(), 0.15);
    let track = Rect::from_center_size(bar.center(), vec2(bar.width(), egui::lerp(3.0..=5.0, hot)));
    let painter = ui.painter();
    painter.rect_filled(track, CornerRadius::same(3), Color32::from_white_alpha(60));
    let filled = Rect::from_min_size(track.min, vec2(track.width() * fraction, track.height()));
    painter.rect_filled(filled, CornerRadius::same(3), WHITE);
    if hot > 0.0 {
        painter.circle_filled(pos2(filled.right(), track.center().y), 6.0 * hot, WHITE);
    }
    if with_times {
        let font = egui::FontId::proportional(11.0);
        let y = bar.bottom() + 4.0;
        painter.text(pos2(rect.left(), y), Align2::LEFT_TOP, format_duration(preview), font.clone(), DIM);
        painter.text(pos2(rect.right(), y), Align2::RIGHT_TOP, format_duration(total), font, DIM);
    }
}

/// Shuffle, previous, play/pause on a white disc, next, repeat. `full`
/// drops shuffle and repeat when there is no room.
fn controls(ui: &mut egui::Ui, app: &App, rect: Rect, height: f32, full: bool, actions: &mut Vec<Action>) {
    let disc = height.min(rect.height());
    let icon = (disc * 0.4).clamp(14.0, 28.0);
    let slots: &[Option<Slot>] = if full {
        &[Some(Slot::Shuffle), Some(Slot::Prev), Some(Slot::Play), Some(Slot::Next), Some(Slot::Repeat)]
    } else {
        &[Some(Slot::Prev), Some(Slot::Play), Some(Slot::Next)]
    };
    let step = rect.width() / slots.len() as f32;
    for (i, slot) in slots.iter().flatten().enumerate() {
        let center = pos2(rect.left() + step * (i as f32 + 0.5), rect.center().y);
        let cell = Rect::from_center_size(center, Vec2::splat(disc));
        match slot {
            Slot::Play => play_button(ui, app, cell, icon, actions),
            Slot::Prev => {
                if icon_button(ui, cell, Icon::Prev, WHITE, icon).clicked() {
                    actions.push(Action::PrevTrack);
                }
            }
            Slot::Next => {
                if icon_button(ui, cell, Icon::Next, WHITE, icon).clicked() {
                    actions.push(Action::NextTrack(true));
                }
            }
            Slot::Shuffle => {
                let color = if app.queue.shuffle { WHITE } else { DIM };
                if icon_button(ui, cell, Icon::Shuffle, color, icon * 0.85).clicked() {
                    actions.push(Action::ToggleShuffle);
                }
            }
            Slot::Repeat => {
                let (glyph, color) = match app.queue.repeat {
                    Repeat::Off => (Icon::Repeat, DIM),
                    Repeat::All => (Icon::Repeat, WHITE),
                    Repeat::One => (Icon::RepeatOne, WHITE),
                };
                if icon_button(ui, cell, glyph, color, icon * 0.85).clicked() {
                    actions.push(Action::CycleRepeat);
                }
            }
        }
    }
}

enum Slot {
    Shuffle,
    Prev,
    Play,
    Next,
    Repeat,
}

fn play_button(ui: &mut egui::Ui, app: &App, rect: Rect, icon: f32, actions: &mut Vec<Action>) {
    let resp = ui.interact(rect, ui.id().with("mini-play"), Sense::click());
    let grow = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    let radius = rect.width() / 2.0 * egui::lerp(0.94..=1.0, grow);
    ui.painter().circle(rect.center(), radius, WHITE, Stroke::NONE);
    if app.is_loading {
        egui::Spinner::new()
            .size(icon)
            .color(Color32::BLACK)
            .paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(icon)));
    } else {
        let glyph = if app.player.is_paused() { Icon::Play } else { Icon::Pause };
        // The play triangle reads centred a touch right of the middle.
        let nudge = if app.player.is_paused() { icon * 0.06 } else { 0.0 };
        glyph
            .image(Color32::BLACK, icon)
            .paint_at(ui, Rect::from_center_size(rect.center() + vec2(nudge, 0.0), Vec2::splat(icon)));
    }
    if resp.clicked() {
        actions.push(Action::TogglePlayPause);
    }
}

fn icon_button(ui: &mut egui::Ui, rect: Rect, icon: Icon, color: Color32, size: f32) -> egui::Response {
    let resp = ui.interact(rect, ui.id().with(("mini-btn", rect.min.x as i32, rect.min.y as i32)), Sense::click());
    let lift = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    if lift > 0.0 {
        ui.painter().circle_filled(rect.center(), rect.width().min(rect.height()) / 2.0, Color32::from_white_alpha((24.0 * lift) as u8));
    }
    icon.image(color, size).paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(size)));
    resp
}

/// Top-to-bottom gradient over `rect`.
fn paint_gradient(painter: &egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lay the mini player out headlessly at `size` and return the actions
    /// a click at `click` produced.
    fn click_at(size: Vec2, click: egui::Pos2) -> Vec<Action> {
        let ctx = egui::Context::default();
        let mut app = App::new(crate::Repaint::default());
        let track = crate::backend::Track {
            id: "abcdefghijk".into(),
            title: "In Bloom".into(),
            artist: "Neck Deep".into(),
            album: String::new(),
            duration_secs: 218,
            thumb_url: None,
            artist_id: None,
            album_id: None,
        };
        app.queue = crate::audio::Queue::new(vec![track], 0);
        let mut actions = Vec::new();
        let screen = Rect::from_min_size(pos2(0.0, 0.0), size);
        let events = [
            vec![egui::Event::PointerMoved(click)],
            vec![egui::Event::PointerButton {
                pos: click,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
            vec![egui::Event::PointerButton {
                pos: click,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        ];
        for frame in events {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events: frame,
                    ..Default::default()
                },
                |ui| draw_mini_player(ui, &app, &mut actions),
            )
            .drop_without_applying_deltas();
        }
        actions
    }

    #[test]
    fn square_uv_crops_the_middle() {
        let uv = square_uv(vec2(480.0, 360.0));
        assert!((uv.min.x - 0.125).abs() < 1e-6 && (uv.max.x - 0.875).abs() < 1e-6);
        assert_eq!((uv.min.y, uv.max.y), (0.0, 1.0));
        let tall = square_uv(vec2(100.0, 200.0));
        assert_eq!((tall.min.y, tall.max.y), (0.25, 0.75));
        assert_eq!(square_uv(vec2(64.0, 64.0)), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)));
    }

    #[test]
    fn tall_layout_play_button_sits_low_and_centred() {
        // Phone-like window: the white disc is centred near the bottom.
        let size = vec2(340.0, 560.0);
        let actions = click_at(size, pos2(170.0, 483.0));
        assert!(
            actions.iter().any(|a| matches!(a, Action::TogglePlayPause)),
            "no play/pause at the disc: {actions:?}"
        );
    }

    #[test]
    fn strip_layout_expand_button_returns_to_full_player() {
        // A short strip on a side screen: the expand button is top right.
        let size = vec2(420.0, 110.0);
        let actions = click_at(size, pos2(420.0 - 10.0 - 56.0 + 12.0, 22.0));
        assert!(
            actions.iter().any(|a| matches!(a, Action::ToggleMini)),
            "no ToggleMini at the expand button: {actions:?}"
        );
    }
}
