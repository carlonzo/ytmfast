use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_secs: u32,
    pub thumb_url: Option<String>,
    pub artist_id: Option<String>,
    pub album_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CardKind {
    Album,
    Playlist,
    Artist,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub kind: CardKind,
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub thumb_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKind {
    Collection,
    Artist,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collection {
    pub id: String,
    pub title: String,
    pub kind: CardKind,
    pub subtitle: String,
    pub thumb_url: Option<String>,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistPage {
    pub id: String,
    pub name: String,
    pub thumb_url: Option<String>,
    pub subscribers: Option<String>,
    pub top_songs: Vec<Track>,
    pub albums: Vec<Card>,
    pub singles: Vec<Card>,
    pub radio_id: Option<String>,
    pub tracks_playlist_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Home {
    pub top_songs: Vec<Track>,
    pub new_releases: Vec<Card>,
    pub chart_playlists: Vec<Card>,
    pub chart_artists: Vec<Card>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchAll {
    pub query: String,
    pub songs: Vec<Track>,
    pub albums: Vec<Card>,
    pub artists: Vec<Card>,
    pub playlists: Vec<Card>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Library {
    pub history: Vec<Track>,
    pub playlists: Vec<Card>,
    pub albums: Vec<Card>,
    pub artists: Vec<Card>,
    pub liked: Vec<Track>,
}

#[derive(Debug, Clone)]
pub enum SignInSource {
    Browser(String),
    File(PathBuf),
}

pub enum Cmd {
    LoadHome,
    Search(String),
    OpenAlbum(String, u64),
    OpenPlaylist(String, u64),
    OpenArtist(String, u64),
    Radio(String),
    Fetch(Track),
    Lyrics(Track),
    Prefetch(Track),
    SetCachePolicy { max_bytes: u64, protect: HashSet<String> },
    EnforceCache { max_bytes: u64, protect: HashSet<String> },
    CacheUsage,
    ClearCache { protect: HashSet<String> },
    SignIn(SignInSource),
    SignOut,
    LoadLibrary(u64),
}

pub enum Event {
    Home(Home),
    Lyrics { id: String, result: Result<Option<crate::lyrics::Lyrics>, String> },
    HomeError(String),
    SearchResults(SearchAll),
    SearchError { query: String, error: String },
    Collection { generation: u64, collection: Collection },
    Artist { generation: u64, artist: ArtistPage },
    Radio { id: String, tracks: Vec<Track> },
    RadioError { id: String, error: String },
    PageError {
        id: String,
        kind: PageKind,
        error: String,
        generation: u64,
    },
    Ready { id: String, path: PathBuf },
    CacheUsage { bytes: u64, files: usize },
    FetchError { id: String, msg: String },
    Auth { signed_in: bool, error: Option<String> },
    Library { generation: u64, library: Library },
    LibraryError { generation: u64, error: String },
}

pub fn select_thumbnail_min(covers: &[rustypipe::model::Thumbnail], min_width: u32) -> Option<String> {
    if covers.is_empty() {
        return None;
    }
    covers
        .iter()
        .filter(|t| t.width >= min_width)
        .min_by_key(|t| t.width)
        .or_else(|| covers.iter().max_by_key(|t| t.width))
        .map(|t| t.url.clone())
}

pub fn select_thumbnail_largest(covers: &[rustypipe::model::Thumbnail]) -> Option<String> {
    covers.iter().max_by_key(|t| t.width).map(|t| t.url.clone())
}

pub fn select_thumbnail(covers: &[rustypipe::model::Thumbnail]) -> Option<String> {
    select_thumbnail_min(covers, 80)
}

pub fn join_artists(artists: &[rustypipe::model::ArtistId]) -> String {
    artists
        .iter()
        .map(|a| a.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn format_subscriber_count(count: u64) -> String {
    if count >= 1_000_000 {
        let val = count as f64 / 1_000_000.0;
        let s = format!("{val:.1}");
        let s = s.strip_suffix(".0").unwrap_or(&s);
        format!("{s}M")
    } else if count >= 1_000 {
        let val = count as f64 / 1_000.0;
        let s = format!("{val:.1}");
        let s = s.strip_suffix(".0").unwrap_or(&s);
        format!("{s}K")
    } else {
        count.to_string()
    }
}

pub fn format_subscribers_label(count: Option<u64>) -> Option<String> {
    count.map(|c| format!("{} subscribers", format_subscriber_count(c)))
}

pub fn format_duration_human(total_secs: u32) -> String {
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    if hours > 0 {
        if mins > 0 {
            format!("{hours} hr {mins} min")
        } else {
            format!("{hours} hr")
        }
    } else if mins == 1 {
        "1 minute".to_string()
    } else {
        format!("{mins} minutes")
    }
}

pub fn format_songs_and_duration(count: usize, total_secs: u32) -> String {
    let song_str = if count == 1 {
        "1 song".to_string()
    } else {
        format!("{count} songs")
    };
    if total_secs > 0 {
        format!("{song_str} • {}", format_duration_human(total_secs))
    } else {
        song_str
    }
}

pub fn format_collection_subtitle(
    kind: CardKind,
    artist: Option<&str>,
    year: Option<u16>,
    track_count: usize,
    total_secs: u32,
) -> String {
    let mut parts = Vec::new();
    match kind {
        CardKind::Album => parts.push("Album".to_string()),
        CardKind::Playlist => parts.push("Playlist".to_string()),
        CardKind::Artist => parts.push("Artist".to_string()),
    }
    if let Some(art) = artist {
        let trimmed = art.trim();
        if !trimmed.is_empty() {
            parts.push(trimmed.to_string());
        }
    }
    if let Some(y) = year {
        parts.push(y.to_string());
    }
    let songs_dur = format_songs_and_duration(track_count, total_secs);
    if !songs_dur.is_empty() {
        parts.push(songs_dur);
    }
    parts.join(" • ")
}

pub fn track_from(item: rustypipe::model::TrackItem) -> Track {
    let artist = join_artists(&item.artists);
    let album = item.album.as_ref().map(|a| a.name.clone()).unwrap_or_default();
    let album_id = item.album.map(|a| a.id);
    let thumb_url = select_thumbnail(&item.cover);
    Track {
        id: item.id,
        title: item.name,
        artist,
        album,
        duration_secs: item.duration.unwrap_or(0),
        thumb_url,
        artist_id: item.artist_id,
        album_id,
    }
}

pub fn track_from_album_track(
    item: rustypipe::model::TrackItem,
    album: &rustypipe::model::MusicAlbum,
) -> Track {
    let artist = if item.artists.is_empty() {
        join_artists(&album.artists)
    } else {
        join_artists(&item.artists)
    };
    let album_title = item
        .album
        .as_ref()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| album.name.clone());
    let thumb_url = select_thumbnail(&item.cover).or_else(|| select_thumbnail(&album.cover));
    Track {
        id: item.id,
        title: item.name,
        artist,
        album: album_title,
        duration_secs: item.duration.unwrap_or(0),
        thumb_url,
        artist_id: item.artist_id.or_else(|| album.artist_id.clone()),
        album_id: Some(album.id.clone()),
    }
}

pub fn album_card(a: rustypipe::model::AlbumItem) -> Card {
    let artist = join_artists(&a.artists);
    let year_str = a.year.map(|y| y.to_string());
    let mut subtitle_parts = vec!["Album"];
    if !artist.is_empty() {
        subtitle_parts.push(&artist);
    }
    let year_formatted = year_str.as_deref().unwrap_or("");
    if !year_formatted.is_empty() {
        subtitle_parts.push(year_formatted);
    }
    Card {
        kind: CardKind::Album,
        id: a.id,
        title: a.name,
        subtitle: subtitle_parts.join(" • "),
        thumb_url: select_thumbnail_min(&a.cover, 226),
    }
}

pub fn playlist_card(p: rustypipe::model::MusicPlaylistItem) -> Card {
    let channel_name = p
        .channel
        .as_ref()
        .map(|c| c.name.as_str())
        .unwrap_or("YouTube Music");
    Card {
        kind: CardKind::Playlist,
        id: p.id,
        title: p.name,
        subtitle: format!("Playlist • {channel_name}"),
        thumb_url: select_thumbnail_min(&p.thumbnail, 226),
    }
}

pub fn artist_card(a: rustypipe::model::ArtistItem) -> Card {
    let subtitle = match a.subscriber_count {
        Some(count) => format!("Artist • {} subscribers", format_subscriber_count(count)),
        None => "Artist".to_string(),
    };
    Card {
        kind: CardKind::Artist,
        id: a.id,
        title: a.name,
        subtitle,
        thumb_url: select_thumbnail_min(&a.avatar, 226),
    }
}

pub fn split_artist_albums(
    items: Vec<rustypipe::model::AlbumItem>,
) -> (Vec<Card>, Vec<Card>) {
    let mut albums = Vec::new();
    let mut singles = Vec::new();
    for item in items {
        if item.album_type == rustypipe::model::AlbumType::Single {
            singles.push(album_card(item));
        } else {
            albums.push(album_card(item));
        }
    }
    (albums, singles)
}

// The backend enforces after every successful fetch/prefetch using the shared
// max_bytes + protect set updated by the App via Cmd::SetCachePolicy.
// The in_flight counter also protects downloads; one lock makes updates atomic.
#[derive(Default)]
struct CachePolicy {
    max_bytes: u64,
    protect: HashSet<String>,
    in_flight: HashMap<String, usize>,
}

async fn enforce_policy(policy: Arc<Mutex<CachePolicy>>, clear: Option<HashSet<String>>) -> (u64, usize) {
    tokio::task::spawn_blocking(move || {
        // ponytail: hold the std Mutex across scan/deletes for atomic policy updates; briefly stalls dispatcher/finish_download, fine at current cache sizes; snapshot and re-check per file if it matters.
        let policy = policy.lock().unwrap_or_else(|e| e.into_inner());
        let mut protect = policy.protect.clone();
        protect.extend(policy.in_flight.keys().cloned());
        let max_bytes = if let Some(extra) = clear {
            protect.extend(extra);
            0
        } else {
            policy.max_bytes
        };
        crate::audio::enforce_cache_limit(&crate::audio::cache_dir(), max_bytes, &protect)
    }).await.unwrap_or_default()
}

fn finish_download(policy: &Mutex<CachePolicy>, id: &str) {
    let mut policy = policy.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(count) = policy.in_flight.get_mut(id) {
        *count -= 1;
        if *count == 0 {
            policy.in_flight.remove(id);
        }
    }
}

pub struct Backend {
    tx: mpsc::Sender<Cmd>,
    rx: mpsc::Receiver<Event>,
}

fn cookie_path_now(cookies: &std::sync::RwLock<Option<PathBuf>>) -> Option<PathBuf> {
    cookies.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Counter for unique per-download cookie copies (`download_cookie_path`).
static DOWNLOAD_COOKIE_COUNTER: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Paths of this process's live private cookie copies. `prepare` inserts
/// (sync: runs before the first `.await` that touches them), `cleanup`
/// removes, and the SignOut handler deletes whatever is still present via
/// `take_live_download_cookies` — so a sign-out leaves no jar copy on disk
/// even while a download is running. That download may then fail or continue
/// unauthenticated; that is acceptable. `std::sync::Mutex` (never held across
/// `.await`): lock, clone/remove/insert, drop, then do async I/O.
static LIVE_DOWNLOAD_COOKIES: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<PathBuf>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashSet::new()));

fn track_live_download_cookies(path: &Path) {
    LIVE_DOWNLOAD_COOKIES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(path.to_path_buf());
}

fn is_live_download_cookie(path: &Path) -> bool {
    LIVE_DOWNLOAD_COOKIES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(path)
}

fn untrack_live_download_cookies(path: &Path) {
    LIVE_DOWNLOAD_COOKIES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(path);
}

/// Drain the live set (sync, no `.await` under the lock). Caller deletes the
/// returned paths with async I/O after the lock is dropped.
fn take_live_download_cookies() -> Vec<PathBuf> {
    std::mem::take(
        &mut *LIVE_DOWNLOAD_COOKIES
            .lock()
            .unwrap_or_else(|e| e.into_inner()),
    )
    .into_iter()
    .collect()
}

/// Copy the canonical jar to a private 0600 file for one download: yt-dlp
/// rewrites the `--cookies` file when it exits, so it must never see the
/// canonical path. `None` when signed out, when the jar holds no cookie
/// lines (yt-dlp rejects an empty `--cookies` file), or when the copy fails
/// (the caller then downloads without cookies). Runs on the backend
/// runtime, never the UI thread.
async fn prepare_download_cookies(canonical: Option<&Path>) -> Option<PathBuf> {
    let src = canonical?;
    let text = tokio::fs::read_to_string(src).await.ok()?;
    if !crate::auth::has_cookies(&text) {
        return None;
    }
    let tmp = crate::auth::download_cookie_path(
        src,
        std::process::id(),
        DOWNLOAD_COOKIE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    );
    // Track BEFORE the write await: a SignOut landing in between drains a
    // set that already contains this path, so the copy cannot survive
    // sign-out. Lock is released before every `.await`.
    track_live_download_cookies(&tmp);
    if crate::auth::write_cookie_file(&tmp, &text).await.is_err() {
        untrack_live_download_cookies(&tmp);
        return None;
    }
    if !is_live_download_cookie(&tmp) {
        // Sign-out happened during the write: it already deleted whatever
        // was on disk, but the file landed after the drain — remove it.
        let _ = tokio::fs::remove_file(&tmp).await;
        return None;
    }
    Some(tmp)
}

/// Delete a private download copy after the download returns, on every path.
async fn cleanup_download_cookies(temp: Option<PathBuf>) {
    if let Some(path) = temp {
        untrack_live_download_cookies(&path);
        let _ = tokio::fs::remove_file(path).await;
    }
}

/// Key page loads on rustypipe's own auth state, not on the file on disk.
fn authed_query(rp: &Arc<rustypipe::client::RustyPipe>) -> rustypipe::client::RustyPipeQuery {
    let q = rp.query();
    if q.auth_enabled(rustypipe::client::ClientType::DesktopMusic) {
        q.authenticated()
    } else {
        q
    }
}

async fn fail_auth(
    event_tx: &mpsc::Sender<Event>,
    ctx: &crate::Repaint,
    rp: Option<&Arc<rustypipe::client::RustyPipe>>,
    cookies: &std::sync::RwLock<Option<PathBuf>>,
    cookie_file: &Path,
    error: String,
) {
    let _ = tokio::fs::remove_file(cookie_file).await;
    if let Some(rp) = rp {
        let _ = rp.user_auth_remove_cookie().await;
    }
    *cookies.write().unwrap_or_else(|e| e.into_inner()) = None;
    let _ = event_tx.send(Event::Auth { signed_in: false, error: Some(error) });
    ctx.request_repaint();
}

fn is_not_found(e: &rustypipe::error::Error) -> bool {
    matches!(
        e,
        rustypipe::error::Error::Extraction(rustypipe::error::ExtractionError::NotFound { .. })
    )
}

async fn fetch_lyrics(
    http: &Result<reqwest::Client, String>,
    rp: Option<&Arc<rustypipe::client::RustyPipe>>,
    track: &Track,
) -> Result<Option<crate::lyrics::Lyrics>, String> {
    let cache_dir = directories::BaseDirs::new()
        .map(|b| b.cache_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let path = crate::lyrics::cache_path(&cache_dir, &track.id)?;
    if let Some(found) = crate::lyrics::cached(&path).await {
        return Ok(found);
    }
    let http = http.as_ref().map_err(Clone::clone)?;
    let query = crate::lyrics::Query {
        artist: track.artist.clone(),
        title: track.title.clone(),
        album: track.album.clone(),
        duration_ms: track.duration_secs.saturating_mul(1000),
    };
    let mut found = crate::lyrics::lookup(http, &query).await?;
    if found.is_none() {
        let Some(rp) = rp else {
            return Ok(None);
        };
        let lyrics_id = match authed_query(rp).music_details(&track.id).await {
            Ok(details) => details.lyrics_id,
            Err(e) if is_not_found(&e) => None,
            Err(e) => return Err(e.to_string()),
        };
        if let Some(id) = lyrics_id {
            match authed_query(rp).music_lyrics(&id).await {
                Ok(lyrics) => found = crate::lyrics::from_youtube(&lyrics.body),
                Err(e) if is_not_found(&e) => {}
                Err(e) => return Err(e.to_string()),
            }
        }
    }
    crate::lyrics::store(&path, &found).await;
    Ok(found)
}

impl Backend {
    pub fn new(ctx: crate::Repaint) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();
        let (event_tx, event_rx) = mpsc::channel::<Event>();

        std::thread::spawn(move || {
            let (rt, rt_err) = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => (Some(rt), None),
                Err(e) => (None, Some(format!("Runtime init error: {e}"))),
            };

            let data_dir = directories::BaseDirs::new()
                .map(|b| b.data_dir().join("ytmfast"))
                .unwrap_or_else(|| PathBuf::from("/tmp/ytmfast"));
            let _ = std::fs::create_dir_all(&data_dir);

            let (rp, rp_err) = match rustypipe::client::RustyPipe::builder()
                .storage_dir(&data_dir)
                .build()
            {
                Ok(rp) => (Some(Arc::new(rp)), None),
                Err(e) => (None, Some(format!("RustyPipe init error: {e}"))),
            };
            let cookies: std::sync::Arc<std::sync::RwLock<Option<PathBuf>>> =
                std::sync::Arc::new(std::sync::RwLock::new(None));
            let cookie_file: Option<PathBuf> = crate::auth::cookie_path();
            // 0700 the parent dir when it exists (same rationale as
            // `secure_cookie_dir`: a 0644 copy re-created at exit by a
            // still-running yt-dlp stays unreadable by other users).
            // Missing dirs are created lazily at write time instead.
            if let Some(path) = &cookie_file
                && let Some(dir) = path.parent()
                && std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
            {
                let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
            }
            // Delete leftover `cookies.<pid>.<counter>.tmp` copies from
            // crashed downloads; the canonical `cookies.txt` is untouched.
            if let Some(path) = &cookie_file
                && let Some(dir) = path.parent()
                && let Ok(entries) = std::fs::read_dir(dir)
            {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if crate::auth::is_download_cookie_leftover(&name) {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
            // Signed in only when the cookie file is a regular file AND
            // rustypipe actually holds a cookie (downloads get private
            // copies, never the canonical file, so the file alone is not
            // trustworthy). Otherwise delete and start out.
            if let Some(path) = &cookie_file {
                let regular = std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file());
                let authed = rp.as_ref().is_some_and(|rp| {
                    rp.query().auth_enabled(rustypipe::client::ClientType::DesktopMusic)
                });
                if regular && authed {
                    let _ =
                        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
                    *cookies.write().unwrap_or_else(|e| e.into_inner()) = Some(path.clone());
                    let _ = event_tx.send(Event::Auth { signed_in: true, error: None });
                } else {
                    let _ = std::fs::remove_file(path);
                    // File missing or rustypipe holds nothing useful: drop any
                    // stale account cookie so authed page requests cannot
                    // leak it. Runs on the backend thread before the loop.
                    if let (Some(rt), Some(rp)) = (rt.as_ref(), rp.as_ref()) {
                        let _ = rt.block_on(rp.user_auth_remove_cookie());
                    }
                }
            } else if let (Some(rt), Some(rp)) = (rt.as_ref(), rp.as_ref()) {
                // No config dir: rustypipe may still hold an account cookie
                // from its storage dir, so drop it and start signed out.
                let _ = rt.block_on(rp.user_auth_remove_cookie());
            }
            let _guard = rt.as_ref().map(|r| r.enter());
            let lyrics_http = reqwest::Client::builder().timeout(Duration::from_secs(10))
                .build().map_err(|e| format!("Lyrics HTTP client init error: {e}"));
            let prefetch_slots = Arc::new(tokio::sync::Semaphore::new(2));
            let cache_policy = Arc::new(Mutex::new(CachePolicy {
                max_bytes: 2048 * 1024 * 1024,
                ..Default::default()
            }));
            while let Ok(cmd) = cmd_rx.recv() {
                // Update before spawning: a clicked track is protected before any eviction.
                {
                    let mut policy = cache_policy.lock().unwrap_or_else(|e| e.into_inner());
                    match &cmd {
                        Cmd::SetCachePolicy { max_bytes, protect }
                        | Cmd::EnforceCache { max_bytes, protect } => {
                            policy.max_bytes = *max_bytes;
                            policy.protect = protect.clone();
                        }
                        Cmd::Fetch(track) | Cmd::Prefetch(track) => {
                            *policy.in_flight.entry(track.id.clone()).or_default() += 1;
                        }
                        _ => {}
                    }
                }
                let cache_policy = cache_policy.clone();
                let prefetch_slots = prefetch_slots.clone();
                let event_tx = event_tx.clone();
                let ctx = ctx.clone();
                let lyrics_http = lyrics_http.clone();
                let rp = rp.clone();
                let rp_err = rp_err.clone();
                let rt_err = rt_err.clone();

                let cookies = cookies.clone();
                let cookie_file = cookie_file.clone();
                if let Some(rt_ref) = &rt {
                    rt_ref.spawn(async move {
                        match cmd {
                            Cmd::Lyrics(track) => {
                                let result = fetch_lyrics(&lyrics_http, rp.as_ref(), &track).await;
                                let _ = event_tx.send(Event::Lyrics { id: track.id, result });
                                ctx.request_repaint();
                            }
                            Cmd::LoadHome => {
                                if let Some(rp) = rp {
                                    let q1 = rp.query();
                                    let q2 = rp.query();
                                    let (charts_res, new_albums_res) = tokio::join!(
                                        q1.music_charts(None),
                                        q2.music_new_albums(),
                                    );
                                    match charts_res {
                                        Ok(charts) => {
                                            let mut top_songs: Vec<Track> = charts
                                                .top_tracks
                                                .into_iter()
                                                .map(track_from)
                                                .collect();
                                            if top_songs.is_empty()
                                                && let Some(first_pl) = charts.playlists.first()
                                            {
                                                let res = rp.query().music_playlist(&first_pl.id).await;
                                                if let Ok(pl) = res {
                                                    top_songs = pl
                                                        .tracks
                                                        .items
                                                        .into_iter()
                                                        .map(track_from)
                                                        .collect();
                                                }
                                            }
                                            let chart_artists = charts
                                                .artists
                                                .into_iter()
                                                .map(artist_card)
                                                .collect();
                                            let chart_playlists = charts
                                                .playlists
                                                .into_iter()
                                                .map(playlist_card)
                                                .collect();
                                            let new_releases = new_albums_res
                                                .map(|albums| {
                                                    albums.into_iter().map(album_card).collect()
                                                })
                                                .unwrap_or_default();
                                            let _ = event_tx.send(Event::Home(Home {
                                                top_songs,
                                                new_releases,
                                                chart_playlists,
                                                chart_artists,
                                            }));
                                        }
                                        Err(e) => {
                                            let _ = event_tx.send(Event::HomeError(e.to_string()));
                                        }
                                    }
                                } else {
                                    let err = rp_err.unwrap_or_else(|| {
                                        "RustyPipe init failed".to_string()
                                    });
                                    let _ = event_tx.send(Event::HomeError(err));
                                }
                                ctx.request_repaint();
                            }
                            Cmd::Search(query) => {
                                if let Some(rp) = rp {
                                    let q1 = rp.query();
                                    let q2 = rp.query();
                                    let q3 = rp.query();
                                    let q4 = rp.query();
                                    let (tracks_res, albums_res, artists_res, playlists_res) =
                                        tokio::join!(
                                            q1.music_search_tracks(&query),
                                            q2.music_search_albums(&query),
                                            q3.music_search_artists(&query),
                                            q4.music_search_playlists(&query, false),
                                        );
                                    match tracks_res {
                                        Ok(res) => {
                                            let songs = res
                                                .items
                                                .items
                                                .into_iter()
                                                .map(track_from)
                                                .collect();
                                            let albums = albums_res
                                                .map(|r| {
                                                    r.items
                                                        .items
                                                        .into_iter()
                                                        .map(album_card)
                                                        .collect()
                                                })
                                                .unwrap_or_default();
                                            let artists = artists_res
                                                .map(|r| {
                                                    r.items
                                                        .items
                                                        .into_iter()
                                                        .map(artist_card)
                                                        .collect()
                                                })
                                                .unwrap_or_default();
                                            let playlists = playlists_res
                                                .map(|r| {
                                                    r.items
                                                        .items
                                                        .into_iter()
                                                        .map(playlist_card)
                                                        .collect()
                                                })
                                                .unwrap_or_default();
                                            let _ = event_tx.send(Event::SearchResults(
                                                SearchAll {
                                                    query,
                                                    songs,
                                                    albums,
                                                    artists,
                                                    playlists,
                                                },
                                            ));
                                        }
                                        Err(e) => {
                                            let _ = event_tx.send(Event::SearchError {
                                                query,
                                                error: e.to_string(),
                                            });
                                        }
                                    }
                                } else {
                                    let err = rp_err.unwrap_or_else(|| {
                                        "RustyPipe init failed".to_string()
                                    });
                                    let _ = event_tx.send(Event::SearchError {
                                        query,
                                        error: err,
                                    });
                                }
                                ctx.request_repaint();
                            }
                            Cmd::OpenAlbum(id, generation) => {
                                if let Some(rp) = rp {
                                    let q = authed_query(&rp);
                                    match q.music_album(&id).await {
                                        Ok(album) => {
                                            let total_secs: u32 = album
                                                .tracks
                                                .iter()
                                                .map(|t| t.duration.unwrap_or(0))
                                                .sum();
                                            let artist_line = join_artists(&album.artists);
                                            let subtitle = format_collection_subtitle(
                                                CardKind::Album,
                                                if artist_line.is_empty() {
                                                    None
                                                } else {
                                                    Some(&artist_line)
                                                },
                                                album.year,
                                                album.tracks.len(),
                                                total_secs,
                                            );
                                            let thumb_url =
                                                select_thumbnail_largest(&album.cover);
                                            let tracks = album
                                                .tracks
                                                .iter()
                                                .cloned()
                                                .map(|t| track_from_album_track(t, &album))
                                                .collect();
                                            let _ = event_tx.send(Event::Collection {
                                                generation,
                                                collection: Collection {
                                                    id: album.id,
                                                    title: album.name,
                                                    kind: CardKind::Album,
                                                    subtitle,
                                                    thumb_url,
                                                    tracks,
                                                },
                                            });
                                        }
                                        Err(e) => {
                                            let _ = event_tx.send(Event::PageError {
                                                id,
                                                kind: PageKind::Collection,
                                                error: e.to_string(),
                                                generation,
                                            });
                                        }
                                    }
                                } else {
                                    let err = rp_err.unwrap_or_else(|| {
                                        "RustyPipe init failed".to_string()
                                    });
                                    let _ = event_tx.send(Event::PageError { id, kind: PageKind::Collection, error: err, generation });
                                }
                                ctx.request_repaint();
                            }
                            Cmd::OpenPlaylist(id, generation) => {
                                if let Some(rp) = rp {
                                    let q = authed_query(&rp);
                                    match q.music_playlist(&id).await {
                                        Ok(playlist) => {
                                            let total_secs: u32 = playlist
                                                .tracks
                                                .items
                                                .iter()
                                                .map(|t| t.duration.unwrap_or(0))
                                                .sum();
                                            let channel_name =
                                                playlist.channel.as_ref().map(|c| c.name.clone());
                                            let subtitle = format_collection_subtitle(
                                                CardKind::Playlist,
                                                channel_name.as_deref().or(if playlist.from_ytm {
                                                    Some("YouTube Music")
                                                } else {
                                                    None
                                                }),
                                                None,
                                                playlist.tracks.items.len(),
                                                total_secs,
                                            );
                                            let thumb_url =
                                                select_thumbnail_largest(&playlist.thumbnail);
                                            let tracks = playlist
                                                .tracks
                                                .items
                                                .into_iter()
                                                .map(track_from)
                                                .collect();
                                            let _ = event_tx.send(Event::Collection {
                                                generation,
                                                collection: Collection {
                                                    id: playlist.id,
                                                    title: playlist.name,
                                                    kind: CardKind::Playlist,
                                                    subtitle,
                                                    thumb_url,
                                                    tracks,
                                                },
                                            });
                                        }
                                        Err(e) => {
                                            let _ = event_tx.send(Event::PageError {
                                                id,
                                                kind: PageKind::Collection,
                                                error: e.to_string(),
                                                generation,
                                            });
                                        }
                                    }
                                } else {
                                    let err = rp_err.unwrap_or_else(|| {
                                        "RustyPipe init failed".to_string()
                                    });
                                    let _ = event_tx.send(Event::PageError { id, kind: PageKind::Collection, error: err, generation });
                                }
                                ctx.request_repaint();
                            }
                            Cmd::OpenArtist(id, generation) => {
                                if let Some(rp) = rp {
                                    let q = authed_query(&rp);
                                    match q.music_artist(&id, false).await {
                                        Ok(artist) => {
                                            let thumb_url =
                                                select_thumbnail_largest(&artist.header_image);
                                            let subscribers =
                                                format_subscribers_label(artist.subscriber_count);
                                            let top_songs = artist
                                                .tracks
                                                .into_iter()
                                                .map(track_from)
                                                .collect();
                                            let (albums, singles) =
                                                split_artist_albums(artist.albums);
                                            let _ = event_tx.send(Event::Artist {
                                                generation,
                                                artist: ArtistPage {
                                                    id: artist.id,
                                                    name: artist.name,
                                                    thumb_url,
                                                    subscribers,
                                                    top_songs,
                                                    albums,
                                                    singles,
                                                    radio_id: artist.radio_id,
                                                    tracks_playlist_id: artist.tracks_playlist_id,
                                                },
                                            });
                                        }
                                        Err(e) => {
                                            let _ = event_tx.send(Event::PageError {
                                                id,
                                                kind: PageKind::Artist,
                                                error: e.to_string(),
                                                generation,
                                            });
                                        }
                                    }
                                } else {
                                    let err = rp_err.unwrap_or_else(|| {
                                        "RustyPipe init failed".to_string()
                                    });
                                    let _ = event_tx.send(Event::PageError { id, kind: PageKind::Artist, error: err, generation });
                                }
                                ctx.request_repaint();
                            }
                            Cmd::Radio(id) => {
                                if let Some(rp) = rp {
                                    match rp.query().music_radio_track(&id).await {
                                        Ok(paginator) => {
                                            let tracks = paginator
                                                .items
                                                .into_iter()
                                                .map(track_from)
                                                .collect();
                                            let _ = event_tx.send(Event::Radio { id, tracks });
                                        }
                                        Err(e) => {
                                            let _ = event_tx.send(Event::RadioError {
                                                id,
                                                error: e.to_string(),
                                            });
                                        }
                                    }
                                } else {
                                    let err = rp_err.unwrap_or_else(|| {
                                        "RustyPipe init failed".to_string()
                                    });
                                    let _ = event_tx.send(Event::RadioError { id, error: err });
                                }
                                ctx.request_repaint();
                            }
                            Cmd::SignIn(source) => {
                                let result: Result<(), String> = async {
                                    let rp_ref = rp.as_ref().ok_or_else(|| {
                                        rp_err
                                            .clone()
                                            .unwrap_or_else(|| "RustyPipe init failed".to_string())
                                    })?;
                                    let cookie_file = cookie_file.as_ref().ok_or_else(|| {
                                        "No config dir: cannot store cookies".to_string()
                                    })?;
                                    // Each arm returns the raw jar text; filtering,
                                    // the cookie check, and the 0600 write below
                                    // happen ONCE in shared code.
                                    let (raw, import_stderr) = match source {
                                        SignInSource::Browser(browser) => {
                                            let args =
                                                crate::auth::import_args(&browser, cookie_file)
                                                    .ok_or_else(|| {
                                                        format!("Unsupported browser: {browser}")
                                                    })?;
                                            // 0600 placeholder BEFORE content lands.
                                            crate::auth::write_cookie_file(
                                                cookie_file,
                                                crate::auth::COOKIE_FILE_HEADER,
                                            )
                                            .await?;
                                            let out = tokio::time::timeout(
                                                Duration::from_secs(60),
                                                tokio::process::Command::new("yt-dlp")
                                                    .args(&args)
                                                    .kill_on_drop(true)
                                                    .output(),
                                            )
                                            .await
                                            .map_err(|_| {
                                                "Browser import timed out".to_string()
                                            })?
                                            .map_err(|e| format!("Failed to run yt-dlp: {e}"))?;
                                            let stderr =
                                                String::from_utf8_lossy(&out.stderr).into_owned();
                                            // yt-dlp exits 1 on the probe URL ("Unsupported
                                            // URL") but still saves the jar on close: do NOT
                                            // gate on exit status. Only the file counts.
                                            let raw =
                                                tokio::fs::read_to_string(cookie_file).await.map_err(
                                                    |_| {
                                                        format!(
                                                            "Browser import failed: {}",
                                                            crate::auth::short_error(&stderr)
                                                        )
                                                    },
                                                )?;
                                            (raw, Some(stderr))
                                        }
                                        SignInSource::File(path) => {
                                            let src = tokio::task::spawn_blocking(move || {
                                                crate::auth::resolve_user_cookie_file(
                                                    &path.to_string_lossy(),
                                                )
                                            })
                                            .await
                                            .map_err(|e| {
                                                format!("Cookie check task failed: {e}")
                                            })??;
                                            let raw = tokio::fs::read_to_string(&src)
                                                .await
                                                .map_err(|e| {
                                                    format!("Cannot read cookie file: {e}")
                                                })?;
                                            (raw, None)
                                        }
                                    };
                                    // Drop every non-youtube/google cookie, and feed the
                                    // FILTERED text. Skip the pointless write+remove
                                    // when the jar holds no cookies at all.
                                    let filtered = crate::auth::filter_cookies(&raw);
                                    if !crate::auth::has_cookies(&filtered) {
                                        match import_stderr.as_deref() {
                                            Some(stderr) => {
                                                return Err(format!(
                                                    "Browser import failed: {}",
                                                    crate::auth::short_error(stderr)
                                                ));
                                            }
                                            None => return Err("No cookies found".to_string()),
                                        }
                                    }
                                    crate::auth::write_cookie_file(cookie_file, &filtered).await?;
                                    rp_ref
                                        .user_auth_set_cookie_txt(&filtered)
                                        .await
                                        .map_err(|e| format!("Invalid cookies: {e}"))?;
                                    rp_ref
                                        .user_auth_check_cookie()
                                        .await
                                        .map_err(|e| format!("Cookie check failed: {e}"))?;
                                    Ok(())
                                }
                                .await;
                                match result {
                                    Ok(()) => {
                                        if let Some(cookie_file) = &cookie_file {
                                            *cookies.write().unwrap_or_else(|e| e.into_inner()) =
                                                Some(cookie_file.clone());
                                        }
                                        let _ = event_tx.send(Event::Auth {
                                            signed_in: true,
                                            error: None,
                                        });
                                    }
                                    Err(error) => {
                                        if let Some(cookie_file) = &cookie_file {
                                            fail_auth(
                                                &event_tx,
                                                &ctx,
                                                rp.as_ref(),
                                                &cookies,
                                                cookie_file,
                                                error,
                                            )
                                            .await;
                                        } else {
                                            let _ = event_tx.send(Event::Auth {
                                                signed_in: false,
                                                error: Some(error),
                                            });
                                            ctx.request_repaint();
                                        }
                                    }
                                }
                                ctx.request_repaint();
                            }
                            Cmd::SignOut => {
                                if let Some(cookie_file) = &cookie_file {
                                    let _ = tokio::fs::remove_file(cookie_file).await;
                                }
                                if let Some(rp) = &rp {
                                    let _ = rp.user_auth_remove_cookie().await;
                                }
                                *cookies.write().unwrap_or_else(|e| e.into_inner()) = None;
                                // Delete live private copies even while a
                                // download is still running (paths drained
                                // synchronously above; removal is async I/O
                                // after the lock is dropped). The in-flight
                                // download may then fail or continue
                                // unauthenticated; that is acceptable.
                                for stale in take_live_download_cookies() {
                                    let _ = tokio::fs::remove_file(stale).await;
                                }
                                let _ = event_tx.send(Event::Auth {
                                    signed_in: false,
                                    error: None,
                                });
                                ctx.request_repaint();
                            }
                            Cmd::LoadLibrary(generation) => {
                                if let Some(rp) = rp {
                                    let q = rp.query();
                                    let (
                                        history_res,
                                        playlists_res,
                                        albums_res,
                                        artists_res,
                                        liked_res,
                                    ) = tokio::join!(
                                        q.music_history(),
                                        q.music_saved_playlists(),
                                        q.music_saved_albums(),
                                        q.music_saved_artists(),
                                        q.music_liked_tracks(),
                                    );
                                    let mut failures = 0;
                                    let history = match history_res {
                                        Ok(p) => crate::auth::dedupe_history(
                                            p.items
                                                .into_iter()
                                                .map(|h| track_from(h.item))
                                                .collect(),
                                        ),
                                        Err(_) => {
                                            failures += 1;
                                            Vec::new()
                                        }
                                    };
                                    let playlists = match playlists_res {
                                        Ok(p) => {
                                            p.items.into_iter().map(playlist_card).collect()
                                        }
                                        Err(_) => {
                                            failures += 1;
                                            Vec::new()
                                        }
                                    };
                                    let albums = match albums_res {
                                        Ok(p) => p.items.into_iter().map(album_card).collect(),
                                        Err(_) => {
                                            failures += 1;
                                            Vec::new()
                                        }
                                    };
                                    let artists = match artists_res {
                                        Ok(p) => p.items.into_iter().map(artist_card).collect(),
                                        Err(_) => {
                                            failures += 1;
                                            Vec::new()
                                        }
                                    };
                                    let (liked_id, liked) = match liked_res {
                                        Ok(pl) => (
                                            Some(pl.id),
                                            pl.tracks
                                                .items
                                                .into_iter()
                                                .map(track_from)
                                                .collect(),
                                        ),
                                        Err(_) => {
                                            failures += 1;
                                            (None, Vec::new())
                                        }
                                    };
                                    if failures == 5 {
                                        let _ = event_tx.send(Event::LibraryError {
                                            generation,
                                            error: "Failed to load library".to_string(),
                                        });
                                    } else {
                                        let playlists = match liked_id {
                                            Some(id) => {
                                                crate::auth::with_liked_first(playlists, &id)
                                            }
                                            None => playlists,
                                        };
                                        let _ = event_tx.send(Event::Library {
                                            generation,
                                            library: Library {
                                                history,
                                                playlists,
                                                albums,
                                                artists,
                                                liked,
                                            },
                                        });
                                    }
                                } else {
                                    let err = rp_err.unwrap_or_else(|| {
                                        "RustyPipe init failed".to_string()
                                    });
                                    let _ = event_tx.send(Event::LibraryError {
                                        generation,
                                        error: err,
                                    });
                                }
                                ctx.request_repaint();
                            }
                            Cmd::Fetch(track) => {
                                // Skip the private cookie copy entirely when
                                // the file is already cached: `fetch`
                                // returns it without spawning yt-dlp.
                                let temp = if crate::audio::cache_path(&track.id).exists() {
                                    None
                                } else {
                                    prepare_download_cookies(
                                        cookie_path_now(&cookies).as_deref(),
                                    )
                                    .await
                                };
                                let result = crate::audio::fetch(&track.id, temp.as_deref()).await;
                                let success = result.is_ok();
                                match result {
                                    Ok(path) => {
                                        let _ = event_tx.send(Event::Ready {
                                            id: track.id.clone(),
                                            path,
                                        });
                                    }
                                    Err(msg) => {
                                        let _ = event_tx.send(Event::FetchError {
                                            id: track.id.clone(),
                                            msg,
                                        });
                                    }
                                }
                                cleanup_download_cookies(temp).await;
                                finish_download(&cache_policy, &track.id);
                                if success {
                                    let (bytes, files) = enforce_policy(cache_policy, None).await;
                                    let _ = event_tx.send(Event::CacheUsage { bytes, files });
                                }
                                ctx.request_repaint();
                            }
                            Cmd::Prefetch(track) => {
                                if crate::audio::is_fetching(&track.id)
                                    || tokio::fs::try_exists(crate::audio::cache_path(&track.id))
                                        .await.unwrap_or(false)
                                {
                                    finish_download(&cache_policy, &track.id);
                                    return;
                                }
                                let permit = match prefetch_slots.acquire().await {
                                    Ok(permit) => permit,
                                    Err(_) => {
                                        finish_download(&cache_policy, &track.id);
                                        return;
                                    }
                                };
                                if crate::audio::is_fetching(&track.id)
                                    || tokio::fs::try_exists(crate::audio::cache_path(&track.id))
                                        .await.unwrap_or(false)
                                {
                                    drop(permit);
                                    finish_download(&cache_policy, &track.id);
                                    return;
                                }
                                let temp = prepare_download_cookies(cookie_path_now(&cookies).as_deref()).await;
                                let success = crate::audio::fetch(&track.id, temp.as_deref()).await.is_ok();
                                cleanup_download_cookies(temp).await;
                                drop(permit);
                                finish_download(&cache_policy, &track.id);
                                if success {
                                    let (bytes, files) = enforce_policy(cache_policy, None).await;
                                    let _ = event_tx.send(Event::CacheUsage { bytes, files });
                                }
                                ctx.request_repaint();
                            }
                            Cmd::SetCachePolicy { .. } | Cmd::EnforceCache { .. } => {
                                let (bytes, files) = enforce_policy(cache_policy, None).await;
                                let _ = event_tx.send(Event::CacheUsage { bytes, files });
                                ctx.request_repaint();
                            }
                            Cmd::CacheUsage => {
                                let (bytes, files) = tokio::task::spawn_blocking(|| {
                                    crate::audio::cache_usage(&crate::audio::cache_dir())
                                }).await.unwrap_or_default();
                                let _ = event_tx.send(Event::CacheUsage { bytes, files });
                                ctx.request_repaint();
                            }
                            Cmd::ClearCache { protect } => {
                                let (bytes, files) = enforce_policy(cache_policy, Some(protect)).await;
                                let _ = event_tx.send(Event::CacheUsage { bytes, files });
                                ctx.request_repaint();
                            }
                        }
                    });
                } else {
                    let err = rt_err.unwrap_or_else(|| "Runtime init failed".to_string());
                    match cmd {
                        Cmd::LoadHome => {
                            let _ = event_tx.send(Event::HomeError(err));
                        }
                        Cmd::Search(query) => {
                            let _ = event_tx.send(Event::SearchError { query, error: err });
                        }
                        Cmd::OpenAlbum(id, generation) | Cmd::OpenPlaylist(id, generation) => {
                            let _ = event_tx.send(Event::PageError { id, kind: PageKind::Collection, error: err, generation });
                        }
                        Cmd::OpenArtist(id, generation) => {
                            let _ = event_tx.send(Event::PageError { id, kind: PageKind::Artist, error: err, generation });
                        }
                        Cmd::Radio(id) => {
                            let _ = event_tx.send(Event::RadioError { id, error: err });
                        }
                        Cmd::Lyrics(track) => {
                            let _ = event_tx.send(Event::Lyrics { id: track.id, result: Err(err) });
                        }
                        Cmd::Fetch(track) => {
                            let _ = event_tx.send(Event::FetchError { id: track.id, msg: err });
                        }
                        Cmd::Prefetch(_) | Cmd::SetCachePolicy { .. } | Cmd::EnforceCache { .. }
                        | Cmd::CacheUsage | Cmd::ClearCache { .. } => {}
                        Cmd::SignIn(_) => {
                            let _ = event_tx.send(Event::Auth {
                                signed_in: false,
                                error: Some(err),
                            });
                        }
                        Cmd::SignOut => {
                            let _ = event_tx.send(Event::Auth {
                                signed_in: false,
                                error: None,
                            });
                        }
                        Cmd::LoadLibrary(generation) => {
                            let _ = event_tx.send(Event::LibraryError {
                                generation,
                                error: err,
                            });
                        }
                    }
                    ctx.request_repaint();
                }
            }
        });

        Self {
            tx: cmd_tx,
            rx: event_rx,
        }
    }

    pub fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }

    pub fn try_recv(&self) -> Option<Event> {
        self.rx.try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustypipe::model::Thumbnail;

    #[test]
    fn only_extraction_not_found_is_a_lyrics_miss() {
        use rustypipe::error::{Error, ExtractionError};

        for id in ["dQw4w9WgXcQ", "MPLYt_missing"] {
            assert!(is_not_found(&Error::Extraction(ExtractionError::NotFound {
                id: id.into(),
                msg: "Lyrics not available".into(),
            })));
        }
        assert!(!is_not_found(&Error::Extraction(ExtractionError::InvalidData(
            "bad response".into(),
        ))));
        assert!(!is_not_found(&Error::Http("connection failed".into())));
        assert!(!is_not_found(&Error::HttpStatus(404, "not found".into())));
    }

    #[test]
    fn test_in_flight_fetch_join_stays_protected_until_both_finish() {
        let policy = Mutex::new(CachePolicy {
            in_flight: HashMap::from([("dQw4w9WgXcQ".to_owned(), 2)]),
            ..Default::default()
        });
        finish_download(&policy, "dQw4w9WgXcQ");
        assert_eq!(policy.lock().unwrap().in_flight.get("dQw4w9WgXcQ"), Some(&1));
        finish_download(&policy, "dQw4w9WgXcQ");
        assert!(policy.lock().unwrap().in_flight.is_empty());
        finish_download(&policy, "missing");
    }

    fn make_thumbnail(url: &str, width: u32, height: u32) -> Thumbnail {
        serde_json::from_value(serde_json::json!({
            "url": url,
            "width": width,
            "height": height,
        }))
        .unwrap()
    }

    #[test]
    fn test_select_thumbnail_min_and_largest() {
        let empty: Vec<Thumbnail> = vec![];
        assert_eq!(select_thumbnail_min(&empty, 100), None);
        assert_eq!(select_thumbnail_largest(&empty), None);

        let thumbs = vec![
            make_thumbnail("https://example.com/small.jpg", 60, 60),
            make_thumbnail("https://example.com/medium.jpg", 120, 120),
            make_thumbnail("https://example.com/large.jpg", 500, 500),
        ];

        // min 80 -> picks medium (120 >= 80, smallest among >= 80)
        assert_eq!(
            select_thumbnail_min(&thumbs, 80),
            Some("https://example.com/medium.jpg".to_string())
        );

        // min 200 -> picks large (500)
        assert_eq!(
            select_thumbnail_min(&thumbs, 200),
            Some("https://example.com/large.jpg".to_string())
        );

        // min 600 (none >= 600) -> picks largest available (500)
        assert_eq!(
            select_thumbnail_min(&thumbs, 600),
            Some("https://example.com/large.jpg".to_string())
        );

        // largest -> picks 500
        assert_eq!(
            select_thumbnail_largest(&thumbs),
            Some("https://example.com/large.jpg".to_string())
        );
    }

    #[test]
    fn test_format_subscriber_count() {
        assert_eq!(format_subscriber_count(1_200_000), "1.2M");
        assert_eq!(format_subscriber_count(1_000_000), "1M");
        assert_eq!(format_subscriber_count(450_000), "450K");
        assert_eq!(format_subscriber_count(1_500), "1.5K");
        assert_eq!(format_subscriber_count(999), "999");
        assert_eq!(
            format_subscribers_label(Some(1_200_000)),
            Some("1.2M subscribers".to_string())
        );
        assert_eq!(format_subscribers_label(None), None);
    }

    #[test]
    fn test_duration_and_collection_formatting() {
        assert_eq!(format_duration_human(60), "1 minute");
        assert_eq!(format_duration_human(300), "5 minutes");
        assert_eq!(format_duration_human(3600), "1 hr");
        assert_eq!(format_duration_human(3660), "1 hr 1 min");

        assert_eq!(
            format_songs_and_duration(1, 180),
            "1 song • 3 minutes"
        );
        assert_eq!(
            format_songs_and_duration(12, 48 * 60),
            "12 songs • 48 minutes"
        );

        assert_eq!(
            format_collection_subtitle(
                CardKind::Album,
                Some("Daft Punk"),
                Some(2001),
                14,
                3660,
            ),
            "Album • Daft Punk • 2001 • 14 songs • 1 hr 1 min"
        );
    }

    #[test]
    fn test_split_artist_albums() {
        use rustypipe::model::AlbumItem;

        fn make_album_item(id: &str, name: &str, album_type: &str, year: Option<u16>) -> AlbumItem {
            serde_json::from_value(serde_json::json!({
                "id": id,
                "name": name,
                "cover": [],
                "artists": [],
                "album_type": album_type,
                "year": year,
                "by_va": false,
            }))
            .unwrap()
        }

        let items = vec![
            make_album_item("album_1", "Discovery", "album", Some(2001)),
            make_album_item("single_1", "One More Time", "single", Some(2000)),
            make_album_item("ep_1", "Tron EP", "ep", Some(2010)),
        ];

        let (albums, singles) = split_artist_albums(items);
        assert_eq!(albums.len(), 2);
        assert_eq!(albums[0].id, "album_1");
        assert_eq!(albums[1].id, "ep_1");
        assert_eq!(singles.len(), 1);
        assert_eq!(singles[0].id, "single_1");
    }

    #[tokio::test]
    #[ignore]
    async fn test_network_home_and_album_mapping() {
        let rp = rustypipe::client::RustyPipe::builder().build().unwrap();
        let q1 = rp.query();
        let q2 = rp.query();
        let (charts_res, new_albums_res) = tokio::join!(
            q1.music_charts(Some(rustypipe::param::Country::Us)),
            q2.music_new_albums(),
        );

        let charts = charts_res.expect("music_charts failed");
        let new_albums = new_albums_res.expect("music_new_albums failed");

        let mut top_songs: Vec<Track> = charts.top_tracks.into_iter().map(track_from).collect();
        if top_songs.is_empty()
            && let Some(first_pl) = charts.playlists.first()
        {
            let res = rp.query().music_playlist(&first_pl.id).await;
            if let Ok(pl) = res {
                top_songs = pl.tracks.items.into_iter().map(track_from).collect();
            }
        }
        let new_releases: Vec<Card> = new_albums.into_iter().map(album_card).collect();

        assert!(!top_songs.is_empty(), "top songs should not be empty");
        assert!(!new_releases.is_empty(), "new releases should not be empty");

        // Fetch first album
        let first_album_id = &new_releases[0].id;
        let album = rp.query().music_album(first_album_id).await.expect("music_album failed");
        assert_eq!(&album.id, first_album_id);
        assert!(!album.tracks.is_empty(), "album tracks should not be empty");

        let tracks: Vec<Track> = album
            .tracks
            .iter()
            .cloned()
            .map(|t| track_from_album_track(t, &album))
            .collect();
        assert!(!tracks.is_empty());
        assert_eq!(tracks[0].album, album.name);
    }
}
