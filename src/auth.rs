use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::backend::{Card, CardKind, Track};

/// Browsers accepted for `--cookies-from-browser`. Kept small on purpose:
/// passed as an argv element, never through a shell.
#[cfg(not(target_os = "macos"))]
pub const ALLOWED_BROWSERS: &[&str] = &["brave", "firefox", "chromium", "chrome"];
#[cfg(target_os = "macos")]
pub const ALLOWED_BROWSERS: &[&str] = &["brave", "firefox", "chromium", "chrome", "safari"];

/// One browser profile found on disk. `profile` is the absolute profile dir
/// (`None` for Safari, which has no profiles in yt-dlp).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserProfile {
    pub browser: &'static str,
    pub profile: Option<PathBuf>,
    pub label: String,
}

/// Outcome of trying one profile during a scan.
#[derive(Debug, Clone)]
pub struct ScanResult {
    pub profile: BrowserProfile,
    /// `Ok` when the profile holds a Google login cookie, else why not.
    pub status: Result<(), String>,
}

/// Chromium-family user-data dirs, mirroring yt-dlp's
/// `_get_chromium_based_browser_settings` (relative to the config dir:
/// `~/.config` on Linux, `~/Library/Application Support` on macOS).
#[cfg(not(target_os = "macos"))]
const CHROMIUM_DIRS: &[(&str, &str, &str)] = &[
    ("chrome", "Chrome", "google-chrome"),
    ("chromium", "Chromium", "chromium"),
    ("brave", "Brave", "BraveSoftware/Brave-Browser"),
];
#[cfg(target_os = "macos")]
const CHROMIUM_DIRS: &[(&str, &str, &str)] = &[
    ("chrome", "Chrome", "Google/Chrome"),
    ("chromium", "Chromium", "Chromium"),
    ("brave", "Brave", "BraveSoftware/Brave-Browser"),
];

/// Firefox profile roots, mirroring yt-dlp's `_firefox_browser_dirs`.
fn firefox_roots(home: &Path, config: &Path) -> Vec<PathBuf> {
    if cfg!(target_os = "macos") {
        vec![config.join("Firefox/Profiles")]
    } else {
        vec![
            config.join("mozilla/firefox"),
            home.join(".mozilla/firefox"),
            home.join(".var/app/org.mozilla.firefox/config/mozilla/firefox"),
            home.join(".var/app/org.mozilla.firefox/.mozilla/firefox"),
            home.join("snap/firefox/common/.mozilla/firefox"),
        ]
    }
}

/// Sorted subdirs of `root`; empty when it is missing or unreadable.
fn subdirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs
}

/// Profiles of one Chromium-family browser: every subdir holding a cookie DB
/// (`Cookies` or `Network/Cookies`), labelled from `Local State`
/// (`profile.info_cache.<dir>.name` and `.user_name`, the Google account).
fn chromium_profiles(browser: &'static str, display: &str, root: &Path) -> Vec<BrowserProfile> {
    let info: serde_json::Value = std::fs::read_to_string(root.join("Local State"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let cache = &info["profile"]["info_cache"];
    subdirs(root)
        .into_iter()
        .filter(|p| p.join("Cookies").is_file() || p.join("Network/Cookies").is_file())
        .filter_map(|p| {
            let dir = p.file_name()?.to_str()?.to_string();
            if dir == "System Profile" || dir == "Guest Profile" {
                return None;
            }
            let entry = &cache[dir.as_str()];
            let name = entry["name"].as_str().filter(|s| !s.is_empty()).unwrap_or(&dir);
            let label = match entry["user_name"].as_str().filter(|s| !s.is_empty()) {
                Some(email) => format!("{display} · {name} ({email})"),
                None => format!("{display} · {name}"),
            };
            Some(BrowserProfile { browser, profile: Some(p), label })
        })
        .collect()
}

/// Firefox profiles: subdirs of each root holding `cookies.sqlite`. The
/// label drops the random `xxxxxxxx.` prefix of the dir name.
fn firefox_profiles(root: &Path) -> Vec<BrowserProfile> {
    subdirs(root)
        .into_iter()
        .filter(|p| p.join("cookies.sqlite").is_file())
        .filter_map(|p| {
            let dir = p.file_name()?.to_str()?;
            let name = dir.split_once('.').map_or(dir, |(_, n)| n).to_string();
            Some(BrowserProfile { browser: "firefox", profile: Some(p), label: format!("Firefox · {name}") })
        })
        .collect()
}

/// `found`, or, when the browser dir exists but no profile could be listed
/// (e.g. macOS privacy blocks reading it), one profile-less entry so yt-dlp
/// still tries it and its error reaches the user instead of the browser
/// silently vanishing from the scan.
fn or_whole_browser(
    found: Vec<BrowserProfile>,
    root: &Path,
    browser: &'static str,
    display: &str,
) -> Vec<BrowserProfile> {
    if found.is_empty() && std::fs::symlink_metadata(root).is_ok() {
        vec![BrowserProfile { browser, profile: None, label: display.to_string() }]
    } else {
        found
    }
}

/// Every installed browser profile that could hold YouTube cookies. Cheap:
/// directory listings plus one small JSON read per Chromium browser. Browsers
/// that are not installed have no dir and simply do not show up.
pub fn scan_profiles() -> Vec<BrowserProfile> {
    let Some(base) = directories::BaseDirs::new() else {
        return Vec::new();
    };
    let (home, config) = (base.home_dir(), base.config_dir());
    let mut out = Vec::new();
    for (b, display, rel) in CHROMIUM_DIRS {
        let root = config.join(rel);
        out.extend(or_whole_browser(chromium_profiles(b, display, &root), &root, b, display));
    }
    for root in firefox_roots(home, config) {
        out.extend(or_whole_browser(firefox_profiles(&root), &root, "firefox", "Firefox"));
    }
    // Safari's jar sits in a sandboxed container we cannot even stat without
    // Full Disk Access; always offer it and let yt-dlp report the failure.
    if cfg!(target_os = "macos") {
        out.push(BrowserProfile { browser: "safari", profile: None, label: "Safari".to_string() });
    }
    out
}

/// Reject user-typed cookie files larger than this (checked via metadata first).
pub const MAX_COOKIE_FILE_BYTES: u64 = 1024 * 1024;
/// Placeholder written 0600 before a browser import lands: yt-dlp refuses
/// a pre-existing EMPTY `--cookies` file ("does not look like a Netscape
/// format cookies file") but accepts a header-only file.
pub const COOKIE_FILE_HEADER: &str = "# Netscape HTTP Cookie File\n";

/// Max history tracks kept on the Listen again shelf.
pub const HISTORY_CAP: usize = 20;

/// `<config_dir>/ytmfast/cookies.txt`, e.g. `~/.config/ytmfast/cookies.txt`.
/// `None` when there is no config dir (never fall back to a world-writable dir).
pub fn cookie_path() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| b.config_dir().join("ytmfast").join("cookies.txt"))
}

/// Private per-download cookie copy next to the canonical jar:
/// `<dir>/cookies.<pid>.<counter>.tmp`. yt-dlp rewrites the `--cookies`
/// file when it exits, so downloads NEVER get the canonical path: each
/// Fetch/Prefetch copies the jar here (0600) and deletes it afterwards.
pub fn download_cookie_path(canonical: &Path, pid: u32, counter: u64) -> PathBuf {
    canonical
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("cookies.{pid}.{counter}.tmp"))
}

/// Private per-import dir holding a copy of one profile's cookie DB, named
/// `cookie-db.<pid>.<counter>`. Sweeps alongside download copies.
pub fn staged_cookie_db_dir(canonical: &Path, pid: u32, counter: u64) -> PathBuf {
    canonical
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("cookie-db.{pid}.{counter}"))
}

/// True only for `cookies.<digits>.<digits>.tmp` and `cookie-db.<digits>.<digits>`
/// leftovers from a crash; never `cookies.txt` or anything else.
pub fn is_download_cookie_leftover(file_name: &str) -> bool {
    let rest = file_name
        .strip_prefix("cookies.")
        .and_then(|s| s.strip_suffix(".tmp"))
        .or_else(|| file_name.strip_prefix("cookie-db."));
    let Some(rest) = rest else {
        return false;
    };
    let mut parts = rest.split('.');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(a), Some(b), None) => {
            !a.is_empty()
                && !b.is_empty()
                && a.bytes().all(|c| c.is_ascii_digit())
                && b.bytes().all(|c| c.is_ascii_digit())
        }
        _ => false,
    }
}

/// True iff the browser name is in the fixed allowlist (exact match).
pub fn is_allowed_browser(browser: &str) -> bool {
    ALLOWED_BROWSERS.contains(&browser)
}

/// yt-dlp browser spec `browser[+gnomekeyring][:profile]`. The profile is an
/// absolute path, which yt-dlp uses as-is (any value containing `/` is a path).
pub fn browser_spec(browser: &str, profile: Option<&Path>, gnome_keyring: bool) -> String {
    let mut spec = browser.to_string();
    if gnome_keyring {
        spec.push_str("+gnomekeyring");
    }
    if let Some(p) = profile {
        spec.push(':');
        spec.push_str(&p.to_string_lossy());
    }
    spec
}

/// argv for the browser cookie import:
/// `yt-dlp -v --cookies-from-browser <spec> --cookies <out> --skip-download https://music.youtube.com`
/// `-v` so a failure leaves yt-dlp's decrypt/keyring lines in stderr for
/// [`import_error`]. Returns `None` for browsers outside the allowlist.
pub fn import_args(
    browser: &str,
    profile: Option<&Path>,
    gnome_keyring: bool,
    out: &Path,
) -> Option<Vec<String>> {
    if !is_allowed_browser(browser) {
        return None;
    }
    Some(vec![
        "-v".to_string(),
        "--cookies-from-browser".to_string(),
        browser_spec(browser, profile, gnome_keyring),
        "--cookies".to_string(),
        out.to_string_lossy().into_owned(),
        "--skip-download".to_string(),
        "https://music.youtube.com".to_string(),
    ])
}

/// Why an import yielded no login cookie, from yt-dlp's verbose stderr: the
/// first `WARNING:`/`ERROR:` line or "N could not be decrypted" summary,
/// minus the expected noise from the probe URL. yt-dlp never prints cookie
/// values. Falls back to "not signed in to YouTube".
pub fn import_error(stderr: &str) -> String {
    stderr
        .lines()
        .map(str::trim)
        .find(|l| {
            (l.starts_with("WARNING:") || l.starts_with("ERROR:") || l.contains("could not be decrypted"))
                && !l.contains("Unsupported URL")
                && !l.contains("Falling back on generic")
        })
        .map(short_error)
        .unwrap_or_else(|| "not signed in to YouTube".to_string())
}

/// True for `youtube.com`/`.youtube.com` or `google.com`/`.google.com`
/// or a subdomain of either. Input is the raw Netscape domain field
/// (leading dot optional, case-insensitive).
fn keep_domain(domain: &str) -> bool {
    let d = domain.trim().trim_start_matches('.').to_lowercase();
    d == "youtube.com"
        || d == "google.com"
        || d.ends_with(".youtube.com")
        || d.ends_with(".google.com")
}

/// Keep only comment/header lines and cookie lines for youtube/google
/// domains. Netscape format: 7 tab-separated fields, domain is field 0;
/// `#HttpOnly_`-prefixed lines are cookie lines (prefix preserved).
/// Malformed or off-domain lines are dropped. Result ends with a trailing
/// newline when non-empty.
pub fn filter_cookies(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("#HttpOnly_") {
            let mut cols = rest.split('\t');
            let keep = matches!(cols.next(), Some(d) if keep_domain(d))
                && rest.split('\t').count() == 7;
            if keep {
                out.push_str(line);
                out.push('\n');
            }
        } else if line.starts_with('#') {
            out.push_str(line);
            out.push('\n');
        } else if line.trim().is_empty() {
            continue;
        } else {
            let mut cols = line.split('\t');
            let keep = matches!(cols.next(), Some(d) if keep_domain(d))
                && line.split('\t').count() == 7;
            if keep {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    out
}
/// True iff the (already filtered) jar text holds at least one cookie
/// line: a non-empty, non-comment line, or a `#HttpOnly_`-prefixed cookie
/// line. Header-only text has no cookies.
pub fn has_cookies(filtered: &str) -> bool {
    filtered.lines().any(|line| {
        let line = line.trim();
        !line.is_empty() && (!line.starts_with('#') || line.starts_with("#HttpOnly_"))
    })
}

/// True iff the jar holds a first-party YouTube login cookie on `.youtube.com`,
/// the only domain rustypipe reads. `__Secure-3PAPISID` alone is a third-party
/// embed cookie that YouTube rejects as a session.
pub fn has_login_cookie(jar: &str) -> bool {
    jar.lines().any(|line| {
        let mut cols = line.trim_start_matches("#HttpOnly_").split('\t');
        cols.next() == Some(".youtube.com")
            && cols
                .nth(4)
                .is_some_and(|name| name == "SAPISID" || name == "__Secure-1PAPISID")
    })
}

/// Lock the cookie dir to mode 0700: create it when missing, tighten it
/// EVERY time (it may pre-exist with wider permissions). Refuses when the
/// path is a symlink or not a directory. Rationale: a live private copy
/// deleted at sign-out can be re-created at exit by a still-running yt-dlp
/// with umask permissions (0644); a 0700 parent dir keeps that copy
/// unreadable by other users. Never logs cookie contents.
pub fn secure_cookie_dir(dir: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(dir).map_err(|e| format!("Cannot create config dir: {e}"))?;
        }
        Err(e) => return Err(format!("Cannot stat config dir: {e}")),
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err("Refusing to write cookies: config dir is a symlink".to_string());
            }
            if !meta.is_dir() {
                return Err("Refusing to write cookies: config path is not a directory".to_string());
            }
        }
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("Cannot secure config dir: {e}"))?;
    Ok(())
}

/// Remove any existing file, then create with mode 0600 (`create_new`),
/// write, and re-assert 0600. Never logs cookie contents.
pub fn write_cookie_file_sync(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        secure_cookie_dir(parent)?;
    }
    let _ = std::fs::remove_file(path);
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true).mode(0o600);
    let mut file = opts.open(path).map_err(|e| format!("Cannot create cookie file: {e}"))?;
    {
        use std::io::Write;
        file.write_all(contents.as_bytes())
            .map_err(|e| format!("Cannot write cookie file: {e}"))?;
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("Cannot secure cookie file: {e}"))?;
    Ok(())
}

/// Async wrapper for backend use: same 0600 guarantees, off the async
/// executor via `spawn_blocking` so the UI thread never blocks.
pub async fn write_cookie_file(path: &Path, contents: &str) -> Result<(), String> {
    let path = path.to_path_buf();
    let contents = contents.to_string();
    tokio::task::spawn_blocking(move || write_cookie_file_sync(&path, &contents))
        .await
        .map_err(|e| format!("Cookie write task failed: {e}"))?
}

/// Expand a leading `~/` to the home dir. Returns the input unchanged
/// when there is no `~` prefix or no home dir.
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf())
    {
        return home.join(rest);
    }
    PathBuf::from(path)
}

/// Validate the user-typed cookie file path: expand `~/`, require a
/// regular file, reject files over [`MAX_COOKIE_FILE_BYTES`] via metadata
/// before reading. Returns the resolved path.
pub fn resolve_user_cookie_file(path: &str) -> Result<PathBuf, String> {
    let resolved = expand_tilde(path.trim());
    let meta = std::fs::metadata(&resolved).map_err(|e| format!("Cannot read cookie file: {e}"))?;
    if !meta.is_file() {
        return Err("Not a regular file".to_string());
    }
    if meta.len() > MAX_COOKIE_FILE_BYTES {
        return Err("Cookie file too large (max 1 MiB)".to_string());
    }
    Ok(resolved)
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

/// Put Liked Music at the front of the playlist list. YouTube Music already
/// lists it among the saved playlists (with its cover), so move that entry
/// instead of adding a second one.
pub fn with_liked_first(mut playlists: Vec<Card>, liked_playlist_id: &str) -> Vec<Card> {
    let liked = match playlists.iter().position(|p| p.id == liked_playlist_id) {
        Some(at) => playlists.remove(at),
        None => liked_card(liked_playlist_id),
    };
    playlists.insert(0, liked);
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
        let profile = Path::new("/home/u/.config/google-chrome/Profile 1");
        assert_eq!(
            import_args("chrome", Some(profile), true, out),
            Some(vec![
                "-v".to_string(),
                "--cookies-from-browser".to_string(),
                "chrome+gnomekeyring:/home/u/.config/google-chrome/Profile 1".to_string(),
                "--cookies".to_string(),
                "/home/u/.config/ytmfast/cookies.txt".to_string(),
                "--skip-download".to_string(),
                "https://music.youtube.com".to_string(),
            ])
        );
        assert_eq!(browser_spec("firefox", None, false), "firefox");
        assert_eq!(browser_spec("chrome", Some(profile), false), "chrome:/home/u/.config/google-chrome/Profile 1");
        for b in ALLOWED_BROWSERS {
            assert!(import_args(b, None, false, out).is_some(), "{b} must be allowed");
        }
    }

    #[test]
    fn test_import_args_rejects_non_allowlisted() {
        let out = Path::new("/tmp/cookies.txt");
        for b in ["firefox; rm -rf", "edge", "", "Firefox", " firefox"] {
            assert_eq!(import_args(b, None, false, out), None, "{b:?}");
        }
    }

    #[test]
    fn test_scan_chromium_and_firefox_profiles() {
        let root = std::env::temp_dir().join(format!("ytmfast-test-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let chrome = root.join("chrome");
        for (dir, db) in [
            ("Default", "Cookies"),
            ("Profile 1", "Network/Cookies"),
            ("Profile 2", "Cookies"),
            ("Guest Profile", "Cookies"),
            ("Crashpad", "settings.dat"),
        ] {
            let f = chrome.join(dir).join(db);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, "").unwrap();
        }
        std::fs::write(
            chrome.join("Local State"),
            r#"{"profile":{"info_cache":{
                "Default":{"name":"Personal","user_name":"me@gmail.com"},
                "Profile 1":{"name":"Work","user_name":""}}}}"#,
        )
        .unwrap();
        let labels: Vec<_> = chromium_profiles("chrome", "Chrome", &chrome)
            .into_iter()
            .map(|p| (p.label, p.profile.unwrap()))
            .collect();
        assert_eq!(
            labels,
            vec![
                ("Chrome · Personal (me@gmail.com)".to_string(), chrome.join("Default")),
                ("Chrome · Work".to_string(), chrome.join("Profile 1")),
                ("Chrome · Profile 2".to_string(), chrome.join("Profile 2")),
            ]
        );

        let ff = root.join("firefox");
        for dir in ["abcd1234.default-release", "zz.work"] {
            std::fs::create_dir_all(ff.join(dir)).unwrap();
            std::fs::write(ff.join(dir).join("cookies.sqlite"), "").unwrap();
        }
        std::fs::create_dir_all(ff.join("Crash Reports")).unwrap();
        let labels: Vec<_> = firefox_profiles(&ff).into_iter().map(|p| p.label).collect();
        assert_eq!(labels, vec!["Firefox · default-release", "Firefox · work"]);

        // Missing browser dir: nothing. Existing dir with no listable profile:
        // one whole-browser entry, so yt-dlp's own error reaches the user.
        assert!(or_whole_browser(Vec::new(), &root.join("nope"), "brave", "Brave").is_empty());
        let empty = root.join("brave");
        std::fs::create_dir_all(&empty).unwrap();
        assert_eq!(
            or_whole_browser(chromium_profiles("brave", "Brave", &empty), &empty, "brave", "Brave"),
            vec![BrowserProfile { browser: "brave", profile: None, label: "Brave".to_string() }]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_import_error_picks_reason() {
        let stderr = "[debug] Command-line config: ['-v']\n\
            Extracting cookies from chromium\n\
            [debug] Chosen keyring: BASICTEXT\n\
            WARNING: cannot decrypt v11 cookies: no key found\n\
            Extracted 0 cookies from chromium (1738 could not be decrypted)\n\
            WARNING: [generic] Falling back on generic information extractor\n\
            ERROR: Unsupported URL: https://music.youtube.com/\n\
            raise UnsupportedError(url)\n";
        assert_eq!(import_error(stderr), "WARNING: cannot decrypt v11 cookies: no key found");
        let clean = "Extracted 40 cookies from firefox\nERROR: Unsupported URL: https://music.youtube.com/\n";
        assert_eq!(import_error(clean), "not signed in to YouTube");
    }

    #[test]
    fn test_cookie_path_ends_with_your_config() {
        let Some(p) = cookie_path() else { return };
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

        // Already listed by YouTube Music: moved to the front, not duplicated.
        let listed = Card { id: "LM".to_string(), thumb_url: Some("cover".to_string()), ..mine.clone() };
        let out = with_liked_first(vec![mine.clone(), listed.clone()], "LM");
        assert_eq!(out, vec![listed, mine]);
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

    #[test]
    fn test_filter_cookies_keeps_youtube_google_only() {
        let line = |d: &str, name: &str| format!("{d}\tTRUE\t/\tTRUE\t0\t{name}\tval");
        let text = format!(
            "# Netscape HTTP Cookie File\n#HttpOnly_{}\n{}\n{}\n{}\nbadline\n{}\n",
            line(".youtube.com", "SID"),
            line(".google.com", "SSID"),
            line("music.youtube.com", "HSID"),
            line(".example.com", "evil"),
            line(".youtube.com", "short")
        );
        let text = text.replacen("short\tval", "short", 1);
        let filtered = filter_cookies(&text);
        assert!(filtered.contains("# Netscape HTTP Cookie File"));
        assert!(filtered.contains("#HttpOnly_.youtube.com\tTRUE"));
        assert!(filtered.contains(".google.com\tTRUE"));
        assert!(filtered.contains("music.youtube.com\tTRUE"));
        assert!(!filtered.contains("example.com"));
        assert!(!filtered.contains("badline"));
        assert!(!filtered.contains("short"));
        assert!(filtered.ends_with('\n'));
    }

    #[test]
    fn test_filter_cookies_case_and_dot_insensitive() {
        let text = ".YOUTUBE.COM\tTRUE\t/\tTRUE\t0\tA\t1\n\
            youtube.com\tTRUE\t/\tTRUE\t0\tB\t2\n\
            .GOOGLE.COM\tTRUE\t/\tTRUE\t0\tC\t3\n\
            .fakegoogle.com\tTRUE\t/\tTRUE\t0\tD\t4\n";
        let filtered = filter_cookies(text);
        assert!(filtered.contains(".YOUTUBE.COM"));
        assert!(filtered.contains("youtube.com\tTRUE"));
        assert!(filtered.contains(".GOOGLE.COM"));
        assert!(!filtered.contains("fakegoogle"));
    }

    #[test]
    fn test_expand_tilde_and_resolve_size_cap() {
        let home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf());
        if let Some(home) = home {
            assert_eq!(expand_tilde("~/a/b"), home.join("a/b"));
        }
        assert_eq!(expand_tilde("/abs/path"), PathBuf::from("/abs/path"));
        assert_eq!(expand_tilde("rel/path"), PathBuf::from("rel/path"));
        // Missing file errors, does not read.
        assert!(resolve_user_cookie_file("/nonexistent-ytmfast-cookies.txt").is_err());
        // Directory is not a regular file.
        let dir = std::env::temp_dir();
        assert!(resolve_user_cookie_file(dir.to_str().unwrap()).is_err());
    }

    #[test]
    fn test_has_cookies() {
        assert!(!has_cookies(""));
        assert!(!has_cookies(COOKIE_FILE_HEADER));
        assert!(!has_cookies("# Netscape HTTP Cookie File\n# some comment\n\n"));
        assert!(has_cookies(
            "# Netscape HTTP Cookie File\n.youtube.com\tTRUE\t/\tTRUE\t0\tSID\tval\n"
        ));
        assert!(has_cookies(
            "# Netscape HTTP Cookie File\n#HttpOnly_.youtube.com\tTRUE\t/\tTRUE\t0\tSID\tval\n"
        ));
    }

    #[test]
    fn test_has_login_cookie() {
        let line = |name: &str| format!(".youtube.com\tTRUE\t/\tTRUE\t0\t{name}\tval\n");
        assert!(!has_login_cookie(COOKIE_FILE_HEADER));
        assert!(!has_login_cookie(&line("VISITOR_INFO1_LIVE")));
        assert!(has_login_cookie(&line("SAPISID")));
        assert!(has_login_cookie(&format!("#HttpOnly_{}", line("__Secure-1PAPISID"))));
        assert!(!has_login_cookie(&line("__Secure-3PAPISID")));
        assert!(!has_login_cookie(".google.com\tTRUE\t/\tTRUE\t0\tSAPISID\tval\n"));
    }

    #[test]
    fn test_write_cookie_file_sync_creates_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir =
            std::env::temp_dir().join(format!("ytmfast-test-cookie-perms-{}", std::process::id()));
        let path = dir.join("cookies.txt");
        let _ = std::fs::remove_dir_all(&dir);
        // Fresh dir: created 0700, file written 0600.
        write_cookie_file_sync(&path, "FAKE-cookie-contents").unwrap();
        assert_eq!(
            std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        // Pre-existing 0755 dir + 0644 file: both tightened.
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(&path, "FAKE-old-contents").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_cookie_file_sync(&path, "FAKE-new-contents").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "FAKE-new-contents");
        assert_eq!(
            std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_download_cookie_paths_unique_and_private() {
        let canonical = Path::new("/home/u/.config/ytmfast/cookies.txt");
        let a = download_cookie_path(canonical, 123, 1);
        let b = download_cookie_path(canonical, 123, 2);
        assert_ne!(a, b);
        assert_eq!(a.parent(), canonical.parent());
        assert_eq!(
            a.file_name().and_then(|n| n.to_str()),
            Some("cookies.123.1.tmp")
        );
        assert!(is_download_cookie_leftover("cookies.123.1.tmp"));
        assert!(is_download_cookie_leftover("cookies.1.0.tmp"));
        assert!(is_download_cookie_leftover("cookie-db.1.0"));
        assert!(!is_download_cookie_leftover("cookie-db.1.0.1"));
    }

    #[test]
    fn test_download_cookie_leftover_pattern_narrow() {
        assert!(!is_download_cookie_leftover("cookies.txt"));
        assert!(!is_download_cookie_leftover("cookies.123.tmp"));
        assert!(!is_download_cookie_leftover("cookies.abc.1.tmp"));
        assert!(!is_download_cookie_leftover("cookies.1.2.tmpx"));
        assert!(!is_download_cookie_leftover("cookies..tmp"));
        assert!(!is_download_cookie_leftover("cookies.1.2.3.tmp"));
        assert!(!is_download_cookie_leftover("other.txt"));
        assert!(!is_download_cookie_leftover(""));
    }
}
