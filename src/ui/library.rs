use super::{Action, COLOR_SURFACE, COLOR_TEXT_PRIMARY, COLOR_TEXT_SECONDARY, Icon};
use crate::LibraryChip;
use crate::auth::ALLOWED_BROWSERS;
use crate::backend::Card;
use crate::App;

fn selected_chip(ui: &mut egui::Ui, label: &str, selected: bool, actions: &mut Vec<Action>, chip: LibraryChip) {
    let btn = egui::Button::new(
        egui::RichText::new(label)
            .color(if selected { egui::Color32::BLACK } else { COLOR_TEXT_PRIMARY })
            .size(13.0),
    )
    .fill(if selected {
        COLOR_TEXT_PRIMARY
    } else {
        COLOR_SURFACE
    })
    .corner_radius(16);
    if ui.add(btn).clicked() {
        actions.push(Action::SetLibraryChip(chip));
    }
}

fn card_grid(ui: &mut egui::Ui, cards: &[Card], actions: &mut Vec<Action>) {
    // Outer central-panel scroll handles scrolling; just wrap.
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(16.0, 16.0);
        for card in cards {
            super::draw_card(ui, card, actions);
        }
    });
}

pub fn draw_sign_in_dialog(ui: &mut egui::Ui, app: &mut App, actions: &mut Vec<Action>) {
    if !app.show_sign_in {
        return;
    }
    let mut open = true;
    egui::Window::new("Sign in")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(0x21, 0x21, 0x21))
                .corner_radius(8)
                .inner_margin(egui::Margin::same(20)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(380.0);
            ui.label(
                egui::RichText::new("Import cookies from a browser, or point at a cookies.txt file.")
                    .color(COLOR_TEXT_SECONDARY)
                    .size(13.0),
            );
            ui.add_space(12.0);

            ui.label(egui::RichText::new("Browser").color(COLOR_TEXT_PRIMARY).size(13.0));
            egui::ComboBox::from_id_salt("signin_browser")
                .selected_text(app.sign_in_browser.as_str())
                .show_ui(ui, |ui| {
                    for b in ALLOWED_BROWSERS {
                        ui.selectable_value(&mut app.sign_in_browser, (*b).to_string(), *b);
                    }
                });
            ui.add_space(8.0);
            let import_btn = ui.add_enabled(
                !app.sign_in_busy,
                egui::Button::new("Import from browser"),
            );
            if import_btn.clicked() {
                actions.push(Action::SignInBrowser(app.sign_in_browser.clone()));
            }

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(12.0);

            ui.label(
                egui::RichText::new("cookies.txt file path")
                    .color(COLOR_TEXT_PRIMARY)
                    .size(13.0),
            );
            ui.add_enabled(
                !app.sign_in_busy,
                egui::TextEdit::singleline(&mut app.sign_in_file)
                    .hint_text("~/.config/ytmfast/cookies.txt")
                    .desired_width(360.0),
            );
            ui.add_space(8.0);
            let use_file = ui.add_enabled(
                !app.sign_in_busy && !app.sign_in_file.trim().is_empty(),
                egui::Button::new("Use file"),
            );
            if use_file.clicked() {
                actions.push(Action::SignInFile(app.sign_in_file.trim().to_string()));
            }

            ui.add_space(12.0);
            ui.label(
                egui::RichText::new(
                    "Cookies are stored locally in ~/.config/ytmfast/cookies.txt (0600).",
                )
                .color(COLOR_TEXT_SECONDARY)
                .size(12.0),
            );

            if app.sign_in_busy {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(
                        egui::RichText::new("Signing in…")
                            .color(COLOR_TEXT_SECONDARY)
                            .size(13.0),
                    );
                });
            }
            if let Some(err) = &app.auth_error {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(err)
                        .color(egui::Color32::from_rgb(0xFF, 0x44, 0x44))
                        .size(13.0),
                );
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    actions.push(Action::HideSignIn);
                }
            });
        });
    if !open {
        actions.push(Action::HideSignIn);
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        actions.push(Action::HideSignIn);
    }
}

/// "Listen again" (history) + "Your playlists" shelves, drawn BEFORE Top songs.
pub fn draw_home_library_shelves(ui: &mut egui::Ui, app: &App, actions: &mut Vec<Action>) {
    let Some(lib) = &app.library else { return };
    if lib.history.is_empty() && lib.playlists.is_empty() {
        return;
    }
    if !lib.history.is_empty() {
        ui.label(
            egui::RichText::new("Listen again")
                .color(COLOR_TEXT_PRIMARY)
                .strong()
                .size(24.0),
        );
        ui.add_space(12.0);
        let current_id = app.queue.current().map(|t| t.id.as_str());
        egui::ScrollArea::horizontal()
            .id_salt("home_listen_again")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for track in lib.history.iter().take(20) {
                        let (rect, resp) = ui.allocate_exact_size(
                            egui::vec2(160.0, 210.0),
                            egui::Sense::click(),
                        );
                        if resp.hovered() {
                            ui.painter().rect_filled(
                                rect.expand(4.0),
                                egui::CornerRadius::same(8),
                                COLOR_SURFACE,
                            );
                        }
                        if resp.clicked() || resp.double_clicked() {
                            let idx = lib
                                .history
                                .iter()
                                .position(|t| t.id == track.id)
                                .unwrap_or(0);
                            actions.push(Action::PlayList {
                                tracks: lib.history.clone(),
                                start: idx,
                                shuffle: None,
                            });
                        }
                        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                            ui.vertical(|ui| {
                                let art = egui::Rect::from_min_size(
                                    rect.min,
                                    egui::vec2(160.0, 160.0),
                                );
                                if let Some(url) = &track.thumb_url {
                                    ui.put(
                                        art,
                                        egui::Image::from_uri(url)
                                            .fit_to_exact_size(egui::vec2(160.0, 160.0))
                                            .corner_radius(4),
                                    );
                                } else {
                                    ui.painter().rect_filled(
                                        art,
                                        egui::CornerRadius::same(4),
                                        egui::Color32::from_rgb(0x30, 0x30, 0x30),
                                    );
                                }
                                ui.add_space(8.0);
                                let title_col = if current_id == Some(track.id.as_str()) {
                                    super::COLOR_ACCENT_RED
                                } else {
                                    COLOR_TEXT_PRIMARY
                                };
                                ui.label(
                                    egui::RichText::new(&track.title)
                                        .color(title_col)
                                        .strong()
                                        .size(14.0),
                                );
                                ui.label(
                                    egui::RichText::new(&track.artist)
                                        .color(COLOR_TEXT_SECONDARY)
                                        .size(12.0),
                                );
                            });
                        });
                        ui.add_space(16.0);
                    }
                });
            });
        ui.add_space(16.0);
    }
    if !lib.playlists.is_empty() {
        super::draw_shelf(ui, "Your playlists", &lib.playlists, "home_your_playlists", actions);
    }
}

pub fn draw_library_page(ui: &mut egui::Ui, app: &App, actions: &mut Vec<Action>) {
    if !app.signed_in {
        ui.vertical_centered(|ui| {
            ui.add_space(140.0);
            ui.label(egui::RichText::new("📚").size(48.0));
            ui.add_space(16.0);
            ui.label(
                egui::RichText::new("Sign in to see your library")
                    .color(COLOR_TEXT_SECONDARY)
                    .size(18.0),
            );
            ui.add_space(16.0);
            if ui.button("Sign in").clicked() {
                actions.push(Action::ShowSignIn);
            }
        });
        return;
    }
    if app.library_loading {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            ui.spinner();
            ui.add_space(12.0);
            ui.label(egui::RichText::new("Loading library…").color(COLOR_TEXT_SECONDARY));
        });
        return;
    }
    if let Some(err) = &app.library_error {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.label(
                egui::RichText::new(format!("Error loading library: {err}"))
                    .color(egui::Color32::from_rgb(0xFF, 0x44, 0x44))
                    .size(16.0),
            );
            ui.add_space(12.0);
            if ui.button("Retry").clicked() {
                actions.push(Action::Retry(super::View::Library));
            }
        });
        return;
    }
    let Some(lib) = &app.library else {
        return;
    };
    ui.horizontal(|ui| {
        ui.add(Icon::User.image(COLOR_TEXT_SECONDARY, 16.0));
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Library")
                .color(COLOR_TEXT_PRIMARY)
                .strong()
                .size(24.0),
        );
    });
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        selected_chip(ui, "Playlists", app.library_chip == LibraryChip::Playlists, actions, LibraryChip::Playlists);
        selected_chip(ui, "Songs", app.library_chip == LibraryChip::Songs, actions, LibraryChip::Songs);
        selected_chip(ui, "Albums", app.library_chip == LibraryChip::Albums, actions, LibraryChip::Albums);
        selected_chip(ui, "Artists", app.library_chip == LibraryChip::Artists, actions, LibraryChip::Artists);
    });
    ui.add_space(16.0);
    match app.library_chip {
        LibraryChip::Playlists => {
            card_grid(ui, &lib.playlists, actions);
        }
        LibraryChip::Albums => {
            card_grid(ui, &lib.albums, actions);
        }
        LibraryChip::Artists => {
            card_grid(ui, &lib.artists, actions);
        }
        LibraryChip::Songs => {
            // ponytail: grid/list toggle; grid only for now (list = liked tracks).
            let current_id = app.queue.current().map(|t| t.id.as_str());
            for (idx, track) in lib.liked.iter().enumerate() {
                super::draw_track_row(
                    ui,
                    idx,
                    track,
                    &lib.liked,
                    current_id,
                    super::TrackRowConfig::DEFAULT,
                    actions,
                );
            }
        }
    }
    ui.add_space(40.0);
}
