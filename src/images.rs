//! Cover art: fetched once, kept on disk, decoded at the size on screen.
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

use std::task::Poll;

use egui::load::{
    Bytes, BytesLoadResult, BytesLoader, BytesPoll, ImageLoadResult, ImageLoader, ImagePoll,
    LoadError, SizeHint,
};

/// Encoded bytes held in memory, above the disk cache. A cover is decoded
/// straight to the size on screen and the bytes are dropped after upload, so
/// this only covers loads still in flight.
const HELD_BYTES: usize = 4 * 1024 * 1024;
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
/// The disk cache is trimmed back to `DISK_TRIM_TO` at startup once it grows
/// past `DISK_LIMIT`, oldest (least recently read) first.
const DISK_LIMIT: u64 = 256 * 1024 * 1024;
const DISK_TRIM_TO: u64 = 192 * 1024 * 1024;

/// egui offers every URI to every loader; this one answers for the network.
fn is_http(uri: &str) -> bool {
    uri.starts_with("https://") || uri.starts_with("http://")
}

/// Display URIs end with this plus the pixel bucket. egui's raster texture
/// cache ignores [`SizeHint`], so the size has to be part of the URI or a
/// 44 px row and a 160 px card share one (blurry) texture.
const SIZE_MARK: &str = "#ytm";

/// The URL to fetch. [`cover_uri_for`] appends the size; the server never sees it.
fn source_url(uri: &str) -> &str {
    match uri.rfind(SIZE_MARK) {
        Some(at)
            if uri.len() > at + SIZE_MARK.len()
                && uri[at + SIZE_MARK.len()..].bytes().all(|b| b.is_ascii_digit()) =>
        {
            &uri[..at]
        }
        _ => uri,
    }
}

/// URI egui caches. `points` is the widget size in points.
pub fn cover_uri_for(url: &str, points: egui::Vec2, pixels_per_point: f32) -> String {
    let url = source_url(url);
    let px = points * pixels_per_point;
    let hint = SizeHint::Size {
        width: px.x.round() as u32,
        height: px.y.round() as u32,
        maintain_aspect_ratio: true,
    };
    format!("{url}{SIZE_MARK}{}", target_side(hint))
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
    /// Caps concurrent JPEG decodes. One thread per cover would be a pile of
    /// stacks while a home feed arrives.
    decode_slots: Arc<tokio::sync::Semaphore>,
    /// Decoded covers keyed by the display URI (`url#ytm{side}`).
    scaled: Mutex<HashMap<String, Poll<Result<Arc<egui::ColorImage>, String>>>>,
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
    let cache = shared().clone();
    ctx.add_bytes_loader(Arc::new(cache.clone()));
    ctx.add_image_loader(Arc::new(cache));
}

/// Remember that `uri` was on screen this frame. [`evict_stale`] drops covers
/// that scrolled away, so a long home feed does not keep every texture.
pub fn touch(uri: &str) {
    if !is_http(uri) {
        return;
    }
    lock(touched()).insert(uri.to_string(), std::time::Instant::now());
}

fn touched() -> &'static Mutex<HashMap<String, std::time::Instant>> {
    static TOUCHED: OnceLock<Mutex<HashMap<String, std::time::Instant>>> = OnceLock::new();
    TOUCHED.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Forget covers not drawn for a few seconds. Disk still has the file.
pub fn evict_stale(ctx: &egui::Context) {
    let now = std::time::Instant::now();
    let stale: Vec<String> = lock(touched())
        .iter()
        .filter(|(_, seen)| now.duration_since(**seen) > Duration::from_secs(8))
        .map(|(uri, _)| uri.clone())
        .collect();
    if stale.is_empty() {
        return;
    }
    for uri in &stale {
        lock(touched()).remove(uri);
    }
    for uri in stale {
        ctx.forget_image(&uri);
    }
}

/// Longest side to decode, in pixels. Widget hints are already physical
/// pixels. Bucketed to 64 so a 1 px layout jitter does not decode twice,
/// and capped so one banner cannot become a multi-megabyte texture.
pub fn target_side(hint: SizeHint) -> u32 {
    let side = match hint {
        SizeHint::Size { width, height, .. } => width.max(height),
        SizeHint::Width(width) => width,
        SizeHint::Height(height) => height,
        SizeHint::Scale(_) => 256,
    };
    let side = side.clamp(32, 2048);
    side.div_ceil(64).saturating_mul(64).clamp(64, 2048)
}

impl ImageCache {
    fn new(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
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
                decode_slots: Arc::new(tokio::sync::Semaphore::new(2)),
                scaled: Mutex::new(HashMap::new()),
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
        // The colour only needs a postage stamp. The display cover is a
        // different, larger decode.
        let fetch_url = sized(&url, 64);
        self.inner.runtime.spawn(async move {
            let accent = match inner.fetch(&fetch_url).await {
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
        // Several on-screen sizes share one download.
        let uri = source_url(uri);
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
        lock(&self.inner.entries).remove(source_url(uri));
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

impl ImageLoader for ImageCache {
    fn id(&self) -> &str {
        "ytmfast::ScaledCovers"
    }

    fn load(&self, ctx: &egui::Context, uri: &str, size_hint: SizeHint) -> ImageLoadResult {
        if !is_http(uri) {
            return Err(LoadError::NotSupported);
        }
        match lock(&self.inner.scaled).get(uri) {
            Some(Poll::Ready(Ok(image))) => {
                return Ok(ImagePoll::Ready { image: Arc::clone(image) });
            }
            Some(Poll::Pending) => return Ok(ImagePoll::Pending { size: None }),
            Some(Poll::Ready(Err(err))) => return Err(LoadError::Loading(err.clone())),
            None => {}
        }
        let bytes = match ctx.try_load_bytes(uri) {
            Ok(BytesPoll::Ready { bytes, .. }) => bytes,
            Ok(BytesPoll::Pending { .. }) => return Ok(ImagePoll::Pending { size: None }),
            Err(err) => return Err(err),
        };
        // `reduce_texture_memory` drops this entry once the GPU has the
        // texture. Until then it keeps the decode from starting twice.
        let side = target_side(size_hint);
        lock(&self.inner.scaled).insert(uri.to_string(), Poll::Pending);
        let inner = Arc::clone(&self.inner);
        let ctx = ctx.clone();
        let uri = uri.to_string();
        let slots = Arc::clone(&self.inner.decode_slots);
        self.inner.runtime.spawn(async move {
            let Ok(_permit) = slots.acquire().await else {
                return;
            };
            let decoded = tokio::task::spawn_blocking(move || decode_scaled(bytes.as_ref(), side))
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let mut scaled = lock(&inner.scaled);
            if matches!(scaled.get(&uri), Some(Poll::Pending)) {
                scaled.insert(uri, Poll::Ready(decoded));
            }
            drop(scaled);
            drop(_permit);
            ctx.request_repaint();
        });
        Ok(ImagePoll::Pending { size: None })
    }

    fn forget(&self, uri: &str) {
        lock(&self.inner.scaled).remove(uri);
    }

    fn forget_all(&self) {
        lock(&self.inner.scaled).clear();
    }

    fn byte_size(&self) -> usize {
        lock(&self.inner.scaled)
            .values()
            .map(|entry| match entry {
                Poll::Ready(Ok(image)) => image.pixels.len() * std::mem::size_of::<egui::Color32>(),
                Poll::Ready(Err(err)) => err.len(),
                Poll::Pending => 0,
            })
            .sum()
    }

    fn has_pending(&self) -> bool {
        lock(&self.inner.scaled)
            .values()
            .any(|entry| matches!(entry, Poll::Pending))
    }
}

/// Decode `bytes` and shrink the long side to `side`. Applies EXIF orientation
/// the same way egui_extras does, so a cover does not land on its side.
fn decode_scaled(bytes: &[u8], side: u32) -> Result<Arc<egui::ColorImage>, String> {
    use image::{DynamicImage, ImageDecoder, ImageReader};
    use std::io::Cursor;

    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    image.apply_orientation(orientation);
    if image.width().max(image.height()) > side {
        image = image.thumbnail(side, side);
    }
    let rgba = image.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Ok(Arc::new(egui::ColorImage::from_rgba_unmultiplied(
        size,
        rgba.as_raw(),
    )))
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
    fn cover_uri_keeps_the_fetch_url_and_the_bucket() {
        let url = "https://lh3.googleusercontent.com/abc=w544-h544-l90-rj";
        let uri = cover_uri_for(url, egui::vec2(160.0, 160.0), 1.0);
        assert_eq!(source_url(&uri), url);
        assert!(uri.ends_with("#ytm192"), "{uri}");
        let retina = cover_uri_for(url, egui::vec2(160.0, 160.0), 2.0);
        assert!(retina.ends_with("#ytm320"), "{retina}");
        let banner = cover_uri_for(url, egui::vec2(5000.0, 400.0), 1.0);
        assert!(banner.ends_with("#ytm2048"), "{banner}");
        assert_eq!(source_url(url), url);
    }

    #[test]
    fn target_side_buckets_widget_pixels_and_caps() {
        use egui::load::SizeHint;
        let exact = |w, h| SizeHint::Size { width: w, height: h, maintain_aspect_ratio: true };
        assert_eq!(target_side(exact(40, 40)), 64);
        assert_eq!(target_side(exact(160, 160)), 192);
        assert_eq!(target_side(SizeHint::Width(5000)), 2048);
        assert_eq!(target_side(SizeHint::Height(10)), 64);
    }

    #[test]
    fn decode_scaled_shrinks_the_long_side() {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            100,
            40,
            image::Rgb([10, 20, 30]),
        ))
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
        let image = decode_scaled(bytes.get_ref(), 64).unwrap();
        assert!(image.width() <= 64);
        assert!(image.height() <= 64);
        assert!(image.width() > image.height());
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
