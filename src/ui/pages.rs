use super::*;

pub fn draw_home(ui: &mut egui::Ui, app: &App, actions: &mut Vec<Action>) {
    if app.home_loading {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            ui.spinner();
            ui.add_space(12.0);
            ui.label(egui::RichText::new("Loading Home…").color(COLOR_TEXT_SECONDARY));
        });
        return;
    }

    if let Some(err) = &app.home_error {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.label(
                egui::RichText::new(format!("Error loading Home: {err}"))
                    .color(egui::Color32::from_rgb(0xFF, 0x44, 0x44))
                    .size(16.0),
            );
            ui.add_space(12.0);
            if ui.button("Retry").clicked() {
                actions.push(Action::Retry(View::Home));
            }
        });
        return;
    }

    let Some(home) = &app.home else {
        return;
    };

    // 0. Signed-in shelves first: Listen again + Your playlists.
    if app.signed_in {
        draw_home_library_shelves(ui, app, actions);
    }

    // 1. Top songs: 4-row grid of compact song rows
    if !home.top_songs.is_empty() {
        ui.label(
            egui::RichText::new("Top songs")
                .color(COLOR_TEXT_PRIMARY)
                .strong()
                .size(24.0),
        );
        ui.add_space(12.0);

        let current_id = app.queue.current().map(|t| t.id.as_str());

        egui::ScrollArea::horizontal()
            .id_salt("home_top_songs")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for chunk in home.top_songs.chunks(4) {
                        ui.vertical(|ui| {
                            for track in chunk {
                                let (row_rect, row_resp) = ui.allocate_exact_size(
                                    egui::vec2(320.0, 52.0),
                                    egui::Sense::click(),
                                );
                                super::hover_prefetch(ui, &row_resp, track, actions);
                                let is_current = current_id == Some(track.id.as_str());
                                if row_resp.hovered() || is_current {
                                    ui.painter().rect_filled(
                                        row_rect,
                                        egui::CornerRadius::same(4),
                                        COLOR_SURFACE,
                                    );
                                }

                                // Double-click the row, or single-click the
                                // thumbnail (same affordance as draw_track_row).
                                let mut play_idx = None;
                                if row_resp.double_clicked() {
                                    play_idx = Some(
                                        home.top_songs
                                            .iter()
                                            .position(|t| t.id == track.id)
                                            .unwrap_or(0),
                                    );
                                }

                                ui.scope_builder(egui::UiBuilder::new().max_rect(row_rect), |ui| {
                                    ui.horizontal_centered(|ui| {
                                        ui.add_space(4.0);
                                        let (thumb_rect, thumb_resp) = ui.allocate_exact_size(
                                            egui::vec2(44.0, 44.0),
                                            egui::Sense::click(),
                                        );
                                        if thumb_resp.clicked() {
                                            play_idx = Some(
                                                home.top_songs
                                                    .iter()
                                                    .position(|t| t.id == track.id)
                                                    .unwrap_or(0),
                                            );
                                        }
                                        if let Some(url) = &track.thumb_url {
                                            ui.put(
                                                thumb_rect,
                                                egui::Image::from_uri(url)
                                                    .fit_to_exact_size(egui::vec2(44.0, 44.0))
                                                    .corner_radius(4),
                                            );
                                        } else {
                                            ui.painter().rect_filled(
                                                thumb_rect,
                                                egui::CornerRadius::same(4),
                                                egui::Color32::from_rgb(0x30, 0x30, 0x30),
                                            );
                                        }
                                        ui.add_space(8.0);
                                        ui.vertical(|ui| {
                                            let title_col = if is_current {
                                                COLOR_ACCENT_RED
                                            } else {
                                                COLOR_TEXT_PRIMARY
                                            };
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(&track.title)
                                                        .color(title_col)
                                                        .strong()
                                                        .size(13.0),
                                                )
                                                .truncate(),
                                            );
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(&track.artist)
                                                        .color(COLOR_TEXT_SECONDARY)
                                                        .size(11.0),
                                                )
                                                .truncate(),
                                            );
                                        });
                                    });
                                });
                                if let Some(idx) = play_idx {
                                    actions.push(Action::PlayList {
                                        tracks: home.top_songs.clone(),
                                        start: idx,
                                        shuffle: None,
                                    });
                                }
                                ui.add_space(4.0);
                            }
                        });
                        ui.add_space(16.0);
                    }
                });
            });
    }

    // 2. New releases
    draw_shelf(ui, "New releases", &home.new_releases, "home_new_releases", actions);

    // 3. Charts
    draw_shelf(ui, "Charts", &home.chart_playlists, "home_charts", actions);

    // 4. Top artists
    draw_shelf(ui, "Top artists", &home.chart_artists, "home_top_artists", actions);

    ui.add_space(40.0);
}

pub fn draw_explore(ui: &mut egui::Ui, app: &App, actions: &mut Vec<Action>) {
    if app.home_loading {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            ui.spinner();
            ui.add_space(12.0);
            ui.label(egui::RichText::new("Loading Explore…").color(COLOR_TEXT_SECONDARY));
        });
        return;
    }

    if let Some(err) = &app.home_error {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.label(
                egui::RichText::new(format!("Error loading Explore: {err}"))
                    .color(egui::Color32::from_rgb(0xFF, 0x44, 0x44))
                    .size(16.0),
            );
            ui.add_space(12.0);
            if ui.button("Retry").clicked() {
                actions.push(Action::Retry(View::Explore));
            }
        });
        return;
    }

    let Some(home) = &app.home else {
        return;
    };

    ui.add_space(4.0);

    // New albums shelf
    draw_shelf(ui, "New albums", &home.new_releases, "explore_new_albums", actions);

    // Top songs as full song list
    if !home.top_songs.is_empty() {
        ui.add_space(24.0);
        ui.label(
            egui::RichText::new("Top songs")
                .color(COLOR_TEXT_PRIMARY)
                .strong()
                .size(24.0),
        );
        ui.add_space(12.0);

        let current_id = app.queue.current().map(|t| t.id.as_str());
        for (idx, track) in home.top_songs.iter().enumerate().take(20) {
            draw_track_row(
                ui,
                idx,
                track,
                &home.top_songs,
                current_id,
                TrackRowConfig::PLAYLIST,
                actions,
            );
        }
    }

    // Chart playlists shelf
    draw_shelf(ui, "Charts", &home.chart_playlists, "explore_charts", actions);

    ui.add_space(40.0);
}

pub fn draw_search(ui: &mut egui::Ui, app: &App, query: &str, actions: &mut Vec<Action>) {
    if app.search_loading.contains(query) {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            ui.spinner();
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new(format!("Searching for \"{query}\"…"))
                    .color(COLOR_TEXT_SECONDARY),
            );
        });
        return;
    }

    if let Some(err) = app.search_errors.get(query) {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.label(
                egui::RichText::new(format!("Search error: {err}"))
                    .color(egui::Color32::from_rgb(0xFF, 0x44, 0x44))
                    .size(16.0),
            );
            ui.add_space(12.0);
            if ui.button("Retry").clicked() {
                actions.push(Action::Retry(View::Search(query.to_string())));
            }
        });
        return;
    }

    let Some(results) = app.search_results.get(query) else {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            ui.label(
                egui::RichText::new("Search songs, albums, or artists above")
                    .color(COLOR_TEXT_SECONDARY)
                    .size(16.0),
            );
        });
        return;
    };

    if results.songs.is_empty()
        && results.albums.is_empty()
        && results.artists.is_empty()
        && results.playlists.is_empty()
    {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            ui.label(
                egui::RichText::new(format!("No results found for \"{query}\""))
                    .color(COLOR_TEXT_SECONDARY)
                    .size(16.0),
            );
        });
        return;
    }

    // Top result card
    if let Some(top) = pick_top_result(query, &results.songs, &results.artists) {
        ui.label(
            egui::RichText::new("Top result")
                .color(COLOR_TEXT_PRIMARY)
                .strong()
                .size(24.0),
        );
        ui.add_space(12.0);

        let (card_rect, card_resp) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().min(500.0), 120.0),
            egui::Sense::click(),
        );

        if card_resp.hovered() {
            ui.painter().rect_filled(
                card_rect,
                egui::CornerRadius::same(8),
                COLOR_SURFACE,
            );
        }

        match &top {
            TopResult::Artist(artist_card) => {
                if card_resp.clicked() {
                    actions.push(Action::Navigate(View::Artist(artist_card.id.clone())));
                }

                ui.scope_builder(egui::UiBuilder::new().max_rect(card_rect), |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.add_space(12.0);
                        if let Some(url) = &artist_card.thumb_url {
                            ui.add(
                                egui::Image::from_uri(url)
                                    .fit_to_exact_size(egui::vec2(96.0, 96.0))
                                    .corner_radius(egui::CornerRadius::same(48)),
                            );
                        } else {
                            let (art_rect, _) = ui.allocate_exact_size(
                                egui::vec2(96.0, 96.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().circle_filled(
                                art_rect.center(),
                                48.0,
                                egui::Color32::from_rgb(0x30, 0x30, 0x30),
                            );
                        }
                        ui.add_space(16.0);
                        ui.vertical(|ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&artist_card.title)
                                        .color(COLOR_TEXT_PRIMARY)
                                        .strong()
                                        .size(22.0),
                                )
                                .truncate(),
                            );
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&artist_card.subtitle)
                                        .color(COLOR_TEXT_SECONDARY)
                                        .size(13.0),
                                )
                                .truncate(),
                            );
                        });
                    });
                });
            }
            TopResult::Song(track) => {
                super::hover_prefetch(ui, &card_resp, track, actions);
                ui.scope_builder(egui::UiBuilder::new().max_rect(card_rect), |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.add_space(12.0);
                        if let Some(url) = &track.thumb_url {
                            ui.add(
                                egui::Image::from_uri(url)
                                    .fit_to_exact_size(egui::vec2(96.0, 96.0))
                                    .corner_radius(4),
                            );
                        } else {
                            let (art_rect, _) = ui.allocate_exact_size(
                                egui::vec2(96.0, 96.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect_filled(
                                art_rect,
                                egui::CornerRadius::same(4),
                                egui::Color32::from_rgb(0x30, 0x30, 0x30),
                            );
                        }
                        ui.add_space(16.0);
                        ui.vertical(|ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&track.title)
                                        .color(COLOR_TEXT_PRIMARY)
                                        .strong()
                                        .size(20.0),
                                )
                                .truncate(),
                            );
                            let sub = if track.album.is_empty() {
                                format!("Song • {}", track.artist)
                            } else {
                                format!("Song • {} • {}", track.artist, track.album)
                            };
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(sub)
                                        .color(COLOR_TEXT_SECONDARY)
                                        .size(13.0),
                                )
                                .truncate(),
                            );
                            ui.add_space(8.0);
                            let play_pill = egui::Button::image_and_text(
                                Icon::Play.image(egui::Color32::BLACK, 14.0),
                                egui::RichText::new("Play")
                                    .color(egui::Color32::BLACK)
                                    .strong()
                                    .size(13.0),
                            )
                            .fill(egui::Color32::WHITE)
                            .corner_radius(16);
                            if ui.add(play_pill).clicked() {
                                actions.push(Action::PlayList {
                                    tracks: results.songs.clone(),
                                    start: 0,
                                    shuffle: None,
                                });
                            }
                        });
                    });
                });
            }
        }
        ui.add_space(24.0);
    }

    // Songs section
    if !results.songs.is_empty() {
        ui.label(
            egui::RichText::new("Songs")
                .color(COLOR_TEXT_PRIMARY)
                .strong()
                .size(24.0),
        );
        ui.add_space(8.0);

        let current_id = app.queue.current().map(|t| t.id.as_str());
        for (idx, track) in results.songs.iter().enumerate().take(6) {
            draw_track_row(
                ui,
                idx,
                track,
                &results.songs,
                current_id,
                TrackRowConfig::PLAYLIST,
                actions,
            );
        }
    }

    // Albums
    draw_shelf(ui, "Albums", &results.albums, "search_albums", actions);

    // Artists
    draw_shelf(ui, "Artists", &results.artists, "search_artists", actions);

    // Playlists
    draw_shelf(ui, "Playlists", &results.playlists, "search_playlists", actions);

    ui.add_space(40.0);
}

pub fn draw_collection(ui: &mut egui::Ui, app: &App, id: &str, actions: &mut Vec<Action>) {
    if app.collections_loading.contains(id) {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            ui.spinner();
            ui.add_space(12.0);
            ui.label(egui::RichText::new("Loading collection…").color(COLOR_TEXT_SECONDARY));
        });
        return;
    }

    if let Some(err) = app.collection_errors.get(id) {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.label(
                egui::RichText::new(format!("Error: {err}"))
                    .color(egui::Color32::from_rgb(0xFF, 0x44, 0x44))
                    .size(16.0),
            );
            ui.add_space(12.0);
            if ui.button("Retry").clicked() {
                let kind_view = if let View::Playlist(_) = app.nav.current {
                    View::Playlist(id.to_string())
                } else {
                    View::Album(id.to_string())
                };
                actions.push(Action::Retry(kind_view));
            }
        });
        return;
    }

    let Some(collection) = app.collections.get(id) else {
        return;
    };

    // Header: 264px art left + metadata right
    ui.horizontal(|ui| {
        let art_size = 264.0;
        if let Some(url) = &collection.thumb_url {
            ui.add(
                egui::Image::from_uri(url)
                    .fit_to_exact_size(egui::vec2(art_size, art_size))
                    .corner_radius(8),
            );
        } else {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(art_size, art_size), egui::Sense::hover());
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(8),
                egui::Color32::from_rgb(0x30, 0x30, 0x30),
            );
        }

        ui.add_space(32.0);

        ui.vertical(|ui| {
            ui.add_space(8.0);
            let kind_label = match collection.kind {
                CardKind::Album => "ALBUM",
                CardKind::Playlist => "PLAYLIST",
                CardKind::Artist => "ARTIST",
            };
            ui.label(
                egui::RichText::new(kind_label)
                    .color(COLOR_TEXT_SECONDARY)
                    .size(12.0)
                    .strong(),
            );
            ui.add_space(4.0);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&collection.title)
                        .color(COLOR_TEXT_PRIMARY)
                        .size(36.0)
                        .strong(),
                )
                .truncate(),
            );
            ui.add_space(8.0);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&collection.subtitle)
                        .color(COLOR_TEXT_SECONDARY)
                        .size(14.0),
                )
                .truncate(),
            );
            ui.add_space(20.0);

            // Play & Shuffle buttons
            ui.horizontal(|ui| {
                let play_pill = egui::Button::image_and_text(
                    Icon::Play.image(egui::Color32::BLACK, 16.0),
                    egui::RichText::new("Play")
                        .color(egui::Color32::BLACK)
                        .strong()
                        .size(14.0),
                )
                .fill(egui::Color32::WHITE)
                .corner_radius(20)
                .min_size(egui::vec2(100.0, 40.0));

                if ui.add(play_pill).clicked() {
                    actions.push(Action::PlayList {
                        tracks: collection.tracks.clone(),
                        start: 0,
                        shuffle: Some(false),
                    });
                }

                ui.add_space(12.0);

                let shuffle_btn = egui::Button::new(
                    egui::RichText::new("Shuffle")
                        .color(COLOR_TEXT_PRIMARY)
                        .strong()
                        .size(14.0),
                )
                .stroke(egui::Stroke::new(1.0, COLOR_TEXT_SECONDARY))
                .fill(egui::Color32::TRANSPARENT)
                .corner_radius(20)
                .min_size(egui::vec2(100.0, 40.0));

                if ui.add(shuffle_btn).clicked() {
                    actions.push(Action::PlayList {
                        tracks: collection.tracks.clone(),
                        start: 0,
                        shuffle: Some(true),
                    });
                }
            });
        });
    });

    ui.add_space(28.0);
    ui.separator();
    ui.add_space(16.0);

    let is_album = matches!(collection.kind, CardKind::Album);
    let row_config = if is_album {
        TrackRowConfig::ALBUM
    } else {
        TrackRowConfig::PLAYLIST
    };
    let current_id = app.queue.current().map(|t| t.id.as_str());

    for (idx, track) in collection.tracks.iter().enumerate() {
        draw_track_row(
            ui,
            idx,
            track,
            &collection.tracks,
            current_id,
            row_config,
            actions,
        );
    }

    ui.add_space(40.0);
}

pub fn draw_artist(ui: &mut egui::Ui, app: &App, id: &str, actions: &mut Vec<Action>) {
    if app.artists_loading.contains(id) {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            ui.spinner();
            ui.add_space(12.0);
            ui.label(egui::RichText::new("Loading artist…").color(COLOR_TEXT_SECONDARY));
        });
        return;
    }

    if let Some(err) = app.artist_errors.get(id) {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.label(
                egui::RichText::new(format!("Error: {err}"))
                    .color(egui::Color32::from_rgb(0xFF, 0x44, 0x44))
                    .size(16.0),
            );
            ui.add_space(12.0);
            if ui.button("Retry").clicked() {
                actions.push(Action::Retry(View::Artist(id.to_string())));
            }
        });
        return;
    }

    let Some(artist) = app.artists.get(id) else {
        return;
    };

    // Artist header: wide banner or 160px round avatar
    let banner_width = ui.available_width();
    let banner_height = 240.0;

    let (header_rect, _) = ui.allocate_exact_size(
        egui::vec2(banner_width, banner_height),
        egui::Sense::hover(),
    );

    if let Some(url) = &artist.thumb_url {
        ui.put(
            header_rect,
            egui::Image::from_uri(url)
                .fit_to_exact_size(egui::vec2(banner_width, banner_height))
                .corner_radius(8),
        );
        // Gradient overlay
        ui.painter().rect_filled(
            header_rect,
            egui::CornerRadius::same(8),
            egui::Color32::from_black_alpha(120),
        );
    } else {
        ui.painter().rect_filled(
            header_rect,
            egui::CornerRadius::same(8),
            COLOR_SURFACE,
        );
    }

    ui.scope_builder(egui::UiBuilder::new().max_rect(header_rect), |ui| {
        ui.vertical(|ui| {
            ui.add_space(80.0);
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.vertical(|ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&artist.name)
                                .color(COLOR_TEXT_PRIMARY)
                                .size(48.0)
                                .strong(),
                        )
                        .truncate(),
                    );
                    if let Some(sub) = &artist.subscribers {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(sub)
                                    .color(COLOR_TEXT_SECONDARY)
                                    .size(14.0),
                            )
                            .truncate(),
                        );
                    }
                    ui.add_space(16.0);

                    // Play & Radio buttons
                    ui.horizontal(|ui| {
                        if !artist.top_songs.is_empty() {
                            let play_pill = egui::Button::image_and_text(
                                Icon::Play.image(egui::Color32::BLACK, 16.0),
                                egui::RichText::new("Play")
                                    .color(egui::Color32::BLACK)
                                    .strong()
                                    .size(14.0),
                            )
                            .fill(egui::Color32::WHITE)
                            .corner_radius(20)
                            .min_size(egui::vec2(90.0, 36.0));

                            if ui.add(play_pill).clicked() {
                                actions.push(Action::PlayList {
                                    tracks: artist.top_songs.clone(),
                                    start: 0,
                                    shuffle: Some(false),
                                });
                            }

                            ui.add_space(12.0);

                            if let Some(first) = artist.top_songs.first() {
                                let radio_btn = egui::Button::new(
                                    egui::RichText::new("Radio")
                                        .color(COLOR_TEXT_PRIMARY)
                                        .strong()
                                        .size(14.0),
                                )
                                .stroke(egui::Stroke::new(1.0, COLOR_TEXT_SECONDARY))
                                .fill(egui::Color32::TRANSPARENT)
                                .corner_radius(20)
                                .min_size(egui::vec2(90.0, 36.0));

                                if ui.add(radio_btn).clicked() {
                                    actions.push(Action::Radio(first.id.clone()));
                                }
                            }
                        }
                    });
                });
            });
        });
    });

    ui.add_space(24.0);

    // Top songs
    if !artist.top_songs.is_empty() {
        ui.label(
            egui::RichText::new("Top songs")
                .color(COLOR_TEXT_PRIMARY)
                .strong()
                .size(24.0),
        );
        ui.add_space(12.0);

        let current_id = app.queue.current().map(|t| t.id.as_str());
        for (idx, track) in artist.top_songs.iter().enumerate().take(5) {
            draw_track_row(
                ui,
                idx,
                track,
                &artist.top_songs,
                current_id,
                TrackRowConfig::ARTIST_TOP,
                actions,
            );
        }
    }

    // Albums
    draw_shelf(ui, "Albums", &artist.albums, "artist_albums", actions);

    // Singles
    draw_shelf(ui, "Singles", &artist.singles, "artist_singles", actions);

    ui.add_space(40.0);
}

pub fn draw_library(ui: &mut egui::Ui, app: &App, actions: &mut Vec<Action>) {
    draw_library_page(ui, app, actions);
}
