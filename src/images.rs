//! Cover art: fetched once, kept on disk, decoded by egui on demand.
//!
//! Adapted from spotifast `src/images.rs` (MIT, Copyright (c) 2026 Carmine
//! Paolino). The disk cache lives in `~/.cache/ytmfast/images/`, one file per
//! URL, so a cover seen in an earlier session paints without a network round
//! trip. It replaces egui_extras' `http` loader, which only kept images in
//! memory for the running session.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use egui::load::{Bytes, BytesLoadResult, BytesLoader, BytesPoll, LoadError};

/// Encoded bytes held in memory. egui keeps its own decoded copy once it has
/// made a texture, so these are only a fast path for a reload.
const HELD_BYTES: usize = 48 * 1024 * 1024;
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
/// The disk cache is trimmed back to `DISK_TRIM_TO` at startup once it grows
/// past `DISK_LIMIT`, oldest (least recently read) first.
const DISK_LIMIT: u64 = 256 * 1024 * 1024;
const DISK_TRIM_TO: u64 = 192 * 1024 * 1024;

/// egui offers every URI to every loader; this one answers for the network.
fn is_http(uri: &str) -> bool {
    uri.starts_with("https://") || uri.starts_with("http://")
}

enum Entry {
    Pending,
    /// `bytes` is `None` once dropped for memory; the file stays on disk.
    Ready { bytes: Option<Arc<[u8]>>, stamp: u64 },
    Failed(String),
}

struct Inner {
    entries: Mutex<HashMap<String, Entry>>,
    /// Cover colour per URL; `None` when the art has no usable colour.
    accents: Mutex<HashMap<String, Option<egui::Color32>>>,
    accents_pending: Mutex<std::collections::HashSet<String>>,
    clock: std::sync::atomic::AtomicU64,
    client: Result<reqwest::Client, String>,
    runtime: tokio::runtime::Runtime,
    dir: PathBuf,
}

#[derive(Clone)]
pub struct ImageCache {
    inner: Arc<Inner>,
}

/// The process-wide cache. It outlives any one window, so closing the window
/// and opening it again (Linux background mode) keeps what was fetched.
pub fn shared() -> &'static ImageCache {
    static CACHE: OnceLock<ImageCache> = OnceLock::new();
    CACHE.get_or_init(|| {
        let dir = directories::BaseDirs::new()
            .map(|b| b.cache_dir().join("ytmfast").join("images"))
            .unwrap_or_else(|| PathBuf::from("/tmp/ytmfast/images"));
        ImageCache::new(dir)
    })
}

/// Register the disk-backed loader on `ctx`. Call after
/// `egui_extras::install_image_loaders`: egui asks the newest loader first.
pub fn install(ctx: &egui::Context) {
    ctx.add_bytes_loader(Arc::new(shared().clone()));
}

impl ImageCache {
    fn new(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("ytmfast-images")
            .enable_all()
            .build()
            .expect("image runtime");
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| format!("Image HTTP client init error: {e}"));
        let trim_dir = dir.clone();
        runtime.spawn_blocking(move || trim_disk(&trim_dir, DISK_LIMIT, DISK_TRIM_TO));
        Self {
            inner: Arc::new(Inner {
                entries: Mutex::new(HashMap::new()),
                accents: Mutex::new(HashMap::new()),
                accents_pending: Mutex::new(Default::default()),
                clock: Default::default(),
                client,
                runtime,
                dir,
            }),
        }
    }

    /// The cover's representative colour, computed in the background the
    /// first time it is asked for. `None` while unknown or when the art is
    /// grey; callers fall back to their plain surface.
    pub fn accent(&self, ctx: &egui::Context, url: &str) -> Option<egui::Color32> {
        if let Some(found) = lock(&self.inner.accents).get(url) {
            return *found;
        }
        if !lock(&self.inner.accents_pending).insert(url.to_string()) {
            return None;
        }
        let inner = Arc::clone(&self.inner);
        let ctx = ctx.clone();
        let url = url.to_string();
        self.inner.runtime.spawn(async move {
            let accent = match inner.fetch(&url).await {
                Ok(bytes) => tokio::task::spawn_blocking(move || accent_color(&bytes))
                    .await
                    .ok()
                    .flatten()
                    .map(|[r, g, b]| egui::Color32::from_rgb(r, g, b)),
                Err(_) => None,
            };
            lock(&inner.accents).insert(url.clone(), accent);
            lock(&inner.accents_pending).remove(&url);
            ctx.request_repaint();
        });
        None
    }

    /// Warm the cache for `url` without drawing it.
    pub fn prefetch(&self, ctx: &egui::Context, url: &str) {
        if !is_http(url) {
            return;
        }
        let mut entries = lock(&self.inner.entries);
        if entries.contains_key(url) {
            return;
        }
        entries.insert(url.to_string(), Entry::Pending);
        drop(entries);
        self.inner.start(ctx, url.to_string());
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// File name for `url`: FNV-1a, stable across Rust versions (unlike
/// `DefaultHasher`), so the cache survives toolchain upgrades.
fn file_name(url: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in url.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}.img")
}

impl Inner {
    fn cache_path(&self, url: &str) -> PathBuf {
        self.dir.join(file_name(url))
    }

    async fn fetch(self: &Arc<Self>, url: &str) -> Result<Arc<[u8]>, String> {
        if let Some(Entry::Ready { bytes: Some(bytes), .. }) = lock(&self.entries).get(url) {
            return Ok(Arc::clone(bytes));
        }
        let path = self.cache_path(url);
        let cached = tokio::task::spawn_blocking({
            let path = path.clone();
            move || {
                let bytes = std::fs::read(&path).ok()?;
                // Reading counts as use for the startup trim (oldest first).
                let _ = std::fs::File::options()
                    .write(true)
                    .open(&path)
                    .and_then(|f| f.set_modified(SystemTime::now()));
                Some(bytes)
            }
        })
        .await
        .ok()
        .flatten();
        if let Some(bytes) = cached.filter(|b| !b.is_empty()) {
            return Ok(Arc::from(bytes));
        }
        let client = self.client.as_ref().map_err(Clone::clone)?;
        let response = client.get(url).send().await.map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("image request failed: {}", response.status()));
        }
        let bytes = response.bytes().await.map_err(|e| e.to_string())?;
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err("image is too large".to_string());
        }
        let bytes: Arc<[u8]> = Arc::from(bytes.as_ref());
        let payload = Arc::clone(&bytes);
        // Atomic write: a file that exists holds a complete response.
        tokio::task::spawn_blocking(move || {
            let part = path.with_extension("part");
            if std::fs::write(&part, &payload).is_ok() {
                let _ = std::fs::rename(&part, &path);
            }
        });
        Ok(bytes)
    }

    fn start(self: &Arc<Self>, ctx: &egui::Context, url: String) {
        let inner = Arc::clone(self);
        let ctx = ctx.clone();
        self.runtime.spawn(async move {
            let entry = match inner.fetch(&url).await {
                Ok(bytes) => Entry::Ready {
                    bytes: Some(bytes),
                    stamp: inner.tick(),
                },
                Err(error) => Entry::Failed(error),
            };
            let mut entries = lock(&inner.entries);
            entries.insert(url, entry);
            drop_over_budget(&mut entries, HELD_BYTES);
            drop(entries);
            ctx.request_repaint();
        });
    }

    fn tick(&self) -> u64 {
        self.clock.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }
}

/// Drop the oldest held bytes until the rest fit `budget`. The entries stay
/// `Ready`, so a later request reloads them from disk.
fn drop_over_budget(entries: &mut HashMap<String, Entry>, budget: usize) {
    let mut held: Vec<(u64, usize, &String)> = entries
        .iter()
        .filter_map(|(url, e)| match e {
            Entry::Ready { bytes: Some(b), stamp } => Some((*stamp, b.len(), url)),
            _ => None,
        })
        .collect();
    let mut total: usize = held.iter().map(|(_, len, _)| len).sum();
    if total <= budget {
        return;
    }
    held.sort_by_key(|(stamp, _, _)| *stamp);
    let drop_urls: Vec<String> = held
        .into_iter()
        .take_while(|(_, len, _)| {
            let over = total > budget;
            total = total.saturating_sub(*len);
            over
        })
        .map(|(_, _, url)| url.clone())
        .collect();
    for url in drop_urls {
        if let Some(Entry::Ready { bytes, .. }) = entries.get_mut(&url) {
            *bytes = None;
        }
    }
}

impl BytesLoader for ImageCache {
    fn id(&self) -> &'static str {
        "ytmfast::ImageCache"
    }

    fn load(&self, ctx: &egui::Context, uri: &str) -> BytesLoadResult {
        if !is_http(uri) {
            return Err(LoadError::NotSupported);
        }
        let mut entries = lock(&self.inner.entries);
        match entries.get_mut(uri) {
            Some(Entry::Ready { bytes: Some(bytes), stamp }) => {
                *stamp = self.inner.tick();
                Ok(BytesPoll::Ready {
                    size: None,
                    bytes: Bytes::Shared(Arc::clone(bytes)),
                    mime: None,
                })
            }
            Some(Entry::Pending) => Ok(BytesPoll::Pending { size: None }),
            Some(Entry::Failed(error)) => Err(LoadError::Loading(error.clone())),
            Some(Entry::Ready { bytes: None, .. }) | None => {
                entries.insert(uri.to_string(), Entry::Pending);
                drop(entries);
                self.inner.start(ctx, uri.to_string());
                Ok(BytesPoll::Pending { size: None })
            }
        }
    }

    fn forget(&self, uri: &str) {
        lock(&self.inner.entries).remove(uri);
    }

    fn forget_all(&self) {
        lock(&self.inner.entries).clear();
    }

    fn byte_size(&self) -> usize {
        lock(&self.inner.entries)
            .values()
            .map(|e| match e {
                Entry::Ready { bytes: Some(b), .. } => b.len(),
                _ => 0,
            })
            .sum()
    }
}

/// Delete the least recently used files until the cache is at or under
/// `trim_to`, but only once it has grown past `limit`.
fn trim_disk(dir: &Path, limit: u64, trim_to: u64) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(SystemTime, u64, PathBuf)> = read
        .flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            meta.is_file().then(|| {
                (
                    meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                    meta.len(),
                    e.path(),
                )
            })
        })
        .collect();
    let mut total: u64 = files.iter().map(|(_, len, _)| len).sum();
    if total <= limit {
        return;
    }
    files.sort_by_key(|(modified, _, _)| *modified);
    for (_, len, path) in files {
        if total <= trim_to {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
}

/// The same cover at about `px` pixels square, for Google image-server URLs
/// (`lh3.googleusercontent.com`, `yt3.ggpht.com`), which take the size in a
/// `=w120-h120-…` or `=s120-…` suffix. Other URLs come back unchanged.
pub fn sized(url: &str, px: u32) -> String {
    let google = url.contains(".googleusercontent.com/") || url.contains(".ggpht.com/");
    let Some(eq) = url.rfind('=').filter(|_| google) else {
        return url.to_string();
    };
    let suffix = &url[eq + 1..];
    let sized_suffix = suffix.starts_with('s')
        || (suffix.starts_with('w') && suffix.contains("-h"));
    if !sized_suffix || suffix.contains('/') {
        return url.to_string();
    }
    format!("{}=w{px}-h{px}-l90-rj", &url[..eq])
}

/// A colour that represents an album cover, suitable for tinting a dark
/// surface: the most common saturated hue, weighted toward vivid mid-tones
/// so black borders and white text lose. From spotifast (MIT).
pub fn accent_color(bytes: &[u8]) -> Option<[u8; 3]> {
    let decoded = image::load_from_memory(bytes).ok()?;
    let small = decoded.thumbnail(48, 48).to_rgb8();
    let mut buckets: HashMap<(u8, u8, u8), (u64, [u64; 3])> = HashMap::new();
    for pixel in small.pixels() {
        let [r, g, b] = pixel.0;
        let (max, min) = (r.max(g).max(b) as f32, r.min(g).min(b) as f32);
        let saturation = if max == 0.0 { 0.0 } else { (max - min) / max };
        let lightness = (max + min) / 510.0;
        let weight = (1.0 + saturation * 6.0) * (1.0 - (lightness - 0.5).abs() * 1.4).max(0.05);
        let weight = (weight * 100.0) as u64;
        let bucket = buckets.entry((r >> 4, g >> 4, b >> 4)).or_insert((0, [0, 0, 0]));
        bucket.0 += weight;
        bucket.1[0] += r as u64 * weight;
        bucket.1[1] += g as u64 * weight;
        bucket.1[2] += b as u64 * weight;
    }
    let (_, (weight, sum)) = buckets.into_iter().max_by_key(|(_, (weight, _))| *weight)?;
    if weight == 0 {
        return None;
    }
    Some([
        (sum[0] / weight) as u8,
        (sum[1] / weight) as u8,
        (sum[2] / weight) as u8,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sized_rewrites_google_suffixes_only() {
        assert_eq!(
            sized("https://lh3.googleusercontent.com/abc=w120-h120-l90-rj", 544),
            "https://lh3.googleusercontent.com/abc=w544-h544-l90-rj"
        );
        assert_eq!(
            sized("https://yt3.ggpht.com/xyz=s88-c-k-c0x00ffffff-no-rj", 300),
            "https://yt3.ggpht.com/xyz=w300-h300-l90-rj"
        );
        let ytimg = "https://i.ytimg.com/vi/abc/hqdefault.jpg?sqp=-oay=w";
        assert_eq!(sized(ytimg, 544), ytimg);
        let plain = "https://lh3.googleusercontent.com/abc";
        assert_eq!(sized(plain, 544), plain);
    }

    #[test]
    fn file_names_are_stable_and_distinct() {
        assert_eq!(file_name("a"), file_name("a"));
        assert_ne!(file_name("a"), file_name("b"));
        assert_eq!(file_name("").len(), 20);
    }

    #[test]
    fn over_budget_drops_oldest_bytes_first() {
        let mut entries = HashMap::new();
        for (i, url) in ["old", "mid", "new"].into_iter().enumerate() {
            entries.insert(
                url.to_string(),
                Entry::Ready { bytes: Some(Arc::from(vec![0u8; 10])), stamp: i as u64 },
            );
        }
        drop_over_budget(&mut entries, 20);
        let held = |url: &str| matches!(entries.get(url), Some(Entry::Ready { bytes: Some(_), .. }));
        assert!(!held("old"));
        assert!(held("mid"));
        assert!(held("new"));
    }

    #[test]
    fn trim_disk_removes_oldest_over_limit() {
        let dir = std::env::temp_dir().join(format!("ytmfast-img-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let base = SystemTime::now() - Duration::from_secs(100);
        for (i, name) in ["a", "b", "c"].into_iter().enumerate() {
            let path = dir.join(name);
            std::fs::write(&path, [0u8; 10]).unwrap();
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(base + Duration::from_secs(i as u64 * 10))
                .unwrap();
        }
        trim_disk(&dir, 100, 10);
        assert!(dir.join("c").exists(), "under the limit: nothing removed");
        trim_disk(&dir, 25, 10);
        assert!(!dir.join("a").exists());
        assert!(!dir.join("b").exists());
        assert!(dir.join("c").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
