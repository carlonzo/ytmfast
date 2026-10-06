use std::path::{Path, PathBuf};

use crate::backend::{Card, CardKind, Track};

/// Browsers accepted for `--cookies-from-browser`. Kept small on purpose:
/// passed as an argv element, never through a shell.
pub const ALLOWED_BROWSERS: &[&str] = &["brave", "firefox", "chromium", "chrome"];

/// Max history tracks kept on the Listen again shelf.
pub const HISTORY_CAP: usize = 20;

/// `<config_dir>/ytmfast/cookies.txt`, e.g. `~/.config/ytmfast/cookies.txt`.
pub fn cookie_path() -> PathBuf {
    directories::BaseDirs::new()
        .map(|b| b.config_dir().join("ytmfast").join("cookies.txt"))
        .unwrap_or_else(|| PathBuf::from("/tmp/ytmfast-cookies.txt"))
}

/// True iff the browser name is in the fixed allowlist (exact match).
pub fn is_allowed_browser(browser: &str) -> bool {
    ALLOWED_BROWSERS.contains(&browser)
}

/// argv for the browser cookie import:
/// `yt-dlp --cookies-from-browser <b> --cookies <out> --skip-download -q --no-warnings https://music.youtube.com`
/// Returns `None` for browsers outside the allowlist.
pub fn import_args(browser: &str, out: &Path) -> Option<Vec<String>> {
    if !is_allowed_browser(browser) {
        return None;
    }
    Some(vec![
        "--cookies-from-browser".to_string(),
        browser.to_string(),
        "--cookies".to_string(),
        out.to_string_lossy().into_owned(),
        "--skip-download".to_string(),
        "-q".to_string(),
        "--no-warnings".to_string(),
        "https://music.youtube.com".to_string(),
    ])
}

/// Dedupe history tracks by id (keep first occurrence order), cap at
/// [`HISTORY_CAP`].
pub fn dedupe_history(tracks: Vec<Track>) -> Vec<Track> {
    let mut seen = std::collections::HashSet::new();
    tracks
        .into_iter()
        .filter(|t| seen.insert(t.id.clone()))
        .take(HISTORY_CAP)
        .collect()
}

/// Liked Music card placed first in the playlist list. The id is the real
/// playlist id returned by rustypipe so it opens as a normal playlist page.
pub fn liked_card(liked_playlist_id: &str) -> Card {
    Card {
        kind: CardKind::Playlist,
        id: liked_playlist_id.to_string(),
        title: "Liked Music".to_string(),
        subtitle: "Playlist • You".to_string(),
        thumb_url: None,
    }
}

/// Insert the Liked Music card at the front of the playlist list.
pub fn with_liked_first(mut playlists: Vec<Card>, liked_playlist_id: &str) -> Vec<Card> {
    playlists.insert(0, liked_card(liked_playlist_id));
    playlists
}

/// Last line of yt-dlp stderr (contains no cookie values), truncated to
/// ~200 chars for display. Never pass cookie contents here.
pub fn short_error(stderr: &str) -> String {
    let line = stderr.lines().last().unwrap_or("").trim();
    let line = if line.is_empty() {
        "unknown error"
    } else {
        line
    };
    let mut s: String = line.chars().take(200).collect();
    if line.chars().count() > 200 {
        s.push('…');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: &str) -> Track {
        Track {
            id: id.to_string(),
            title: format!("title {id}"),
            artist: "artist".to_string(),
            album: "album".to_string(),
            duration_secs: 180,
            thumb_url: None,
            artist_id: None,
            album_id: None,
        }
    }

    #[test]
    fn test_import_args_exact_argv() {
        let out = Path::new("/home/u/.config/ytmfast/cookies.txt");
        assert_eq!(
            import_args("firefox", out),
            Some(vec![
                "--cookies-from-browser".to_string(),
                "firefox".to_string(),
                "--cookies".to_string(),
                "/home/u/.config/ytmfast/cookies.txt".to_string(),
                "--skip-download".to_string(),
                "-q".to_string(),
                "--no-warnings".to_string(),
                "https://music.youtube.com".to_string(),
            ])
        );
        for b in ALLOWED_BROWSERS {
            assert!(import_args(b, out).is_some(), "{b} must be allowed");
        }
    }

    #[test]
    fn test_import_args_rejects_non_allowlisted() {
        let out = Path::new("/tmp/cookies.txt");
        assert_eq!(import_args("firefox; rm -rf", out), None);
        assert_eq!(import_args("safari", out), None);
        assert_eq!(import_args("", out), None);
        assert_eq!(import_args("Firefox", out), None);
        assert_eq!(import_args(" firefox", out), None);
    }

    #[test]
    fn test_cookie_path_ends_with_your_config() {
        let p = cookie_path();
        assert_eq!(
            p.file_name().and_then(|n| n.to_str()),
            Some("cookies.txt")
        );
        assert!(p.parent().is_some_and(|d| d.ends_with("ytmfast")));
    }

    #[test]
    fn test_dedupe_history_keeps_order_and_caps() {
        let tracks = vec![track("a"), track("b"), track("a"), track("c"), track("b")];
        let ids: Vec<_> = dedupe_history(tracks).iter().map(|t| t.id.clone()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);

        let many: Vec<_> = (0..50).map(|i| track(&format!("t{i}"))).collect();
        let deduped = dedupe_history(many);
        assert_eq!(deduped.len(), HISTORY_CAP);
        assert_eq!(deduped[0].id, "t0");
        assert_eq!(deduped[HISTORY_CAP - 1].id, format!("t{}", HISTORY_CAP - 1));

        assert!(dedupe_history(Vec::new()).is_empty());
    }

    #[test]
    fn test_liked_music_placed_first() {
        let mine = Card {
            kind: CardKind::Playlist,
            id: "PLmine".to_string(),
            title: "Mine".to_string(),
            subtitle: "Playlist".to_string(),
            thumb_url: None,
        };
        let out = with_liked_first(vec![mine.clone()], "LM");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].kind, CardKind::Playlist);
        assert_eq!(out[0].id, "LM");
        assert_eq!(out[0].title, "Liked Music");
        assert_eq!(out[1], mine);

        let empty = with_liked_first(Vec::new(), "LM");
        assert_eq!(empty.len(), 1);
        assert_eq!(empty[0].id, "LM");
    }

    #[test]
    fn test_short_error_last_line_truncated() {
        assert_eq!(short_error(""), "unknown error");
        assert_eq!(
            short_error("first line\nsecond line  "),
            "second line"
        );
        let long = format!("x{}\n{}", "y".repeat(300), "z".repeat(300));
        let s = short_error(&long);
        assert_eq!(s.chars().count(), 201);
        assert!(s.ends_with('…'));
    }
}
