use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use crate::backend::Track;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    Off,
    All,
    One,
}

#[derive(Debug, Clone)]
pub struct Queue {
    pub tracks: Vec<Track>,
    pub order: Vec<usize>,
    pub pos: usize,
    pub shuffle: bool,
    pub repeat: Repeat,
}

fn shuffle_slice<T>(slice: &mut [T]) {
    let mut state = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(123456789);
    if state == 0 {
        state = 123456789;
    }
    for i in (1..slice.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state as usize) % (i + 1);
        slice.swap(i, j);
    }
}

impl Queue {
    pub fn new(tracks: Vec<Track>, start_index: usize) -> Self {
        let len = tracks.len();
        let order = (0..len).collect();
        let pos = if len == 0 { 0 } else { start_index.min(len - 1) };
        Self {
            tracks,
            order,
            pos,
            shuffle: false,
            repeat: Repeat::Off,
        }
    }

    pub fn current(&self) -> Option<&Track> {
        self.order.get(self.pos).and_then(|&idx| self.tracks.get(idx))
    }

    pub fn next(&mut self, user: bool) -> Option<&Track> {
        if self.order.is_empty() {
            return None;
        }
        if self.repeat == Repeat::One && !user {
            return self.current();
        }
        if self.pos + 1 < self.order.len() {
            self.pos += 1;
            self.current()
        } else {
            match self.repeat {
                Repeat::All | Repeat::One => {
                    self.pos = 0;
                    self.current()
                }
                Repeat::Off => None,
            }
        }
    }

    pub fn prev(&mut self) -> Option<&Track> {
        if self.order.is_empty() {
            return None;
        }
        if self.pos > 0 {
            self.pos -= 1;
            self.current()
        } else {
            match self.repeat {
                Repeat::All => {
                    self.pos = self.order.len() - 1;
                    self.current()
                }
                Repeat::Off | Repeat::One => {
                    self.pos = 0;
                    self.current()
                }
            }
        }
    }

    pub fn jump(&mut self, index: usize) -> Option<&Track> {
        if index < self.order.len() {
            self.pos = index;
            self.current()
        } else {
            None
        }
    }

    pub fn set_shuffle(&mut self, shuffle: bool) {
        if self.shuffle == shuffle {
            return;
        }
        self.shuffle = shuffle;
        if self.tracks.is_empty() {
            return;
        }
        let current_track_idx = self.order[self.pos];
        if shuffle {
            let mut others: Vec<usize> = (0..self.tracks.len())
                .filter(|&idx| idx != current_track_idx)
                .collect();
            shuffle_slice(&mut others);
            let mut new_order = Vec::with_capacity(self.tracks.len());
            new_order.push(current_track_idx);
            new_order.extend(others);
            self.order = new_order;
            self.pos = 0;
        } else {
            self.order = (0..self.tracks.len()).collect();
            self.pos = current_track_idx;
        }
    }

    pub fn cycle_repeat(&mut self) {
        self.repeat = match self.repeat {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        };
    }
}

pub fn is_valid_id(id: &str) -> bool {
    id.len() == 11 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

pub fn cache_path(id: &str) -> PathBuf {
    let dir = directories::BaseDirs::new()
        .map(|b| b.cache_dir().join("ytmfast/audio"))
        .unwrap_or_else(|| PathBuf::from("/tmp/ytmfast/audio"));
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("{id}.m4a"))
}

pub fn ytdlp_args(id: &str, out: &Path, cookies: Option<&Path>) -> Vec<String> {
    let mut args = vec![
        "-q".to_string(),
        "--no-warnings".to_string(),
    ];
    if let Some(c) = cookies {
        args.push("--cookies".to_string());
        args.push(c.display().to_string());
    }
    args.extend([
        "-f".to_string(),
        "bestaudio[ext=m4a]".to_string(),
        "-o".to_string(),
        out.display().to_string(),
        format!("https://music.youtube.com/watch?v={id}"),
    ]);
    args
}

async fn cleanup_temp(temp: &Path) {
    let _ = tokio::fs::remove_file(temp).await;
    let mut part = temp.as_os_str().to_os_string();
    part.push(".part");
    let _ = tokio::fs::remove_file(Path::new(&part)).await;
}

pub async fn fetch(id: &str, cookies: Option<&Path>) -> Result<PathBuf, String> {
    if !is_valid_id(id) {
        return Err(format!("Invalid YouTube track ID: {id}"));
    }
    let final_path = cache_path(id);
    if final_path.exists() {
        return Ok(final_path);
    }

    let parent = final_path.parent().ok_or("Invalid cache path")?;

    for attempt in 0..2 {
        if final_path.exists() {
            return Ok(final_path);
        }

        let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp_path = parent.join(format!("{id}.{}-{n}.dl.m4a", std::process::id()));
        cleanup_temp(&temp_path).await;

        let args = ytdlp_args(id, &temp_path, cookies);
        let output_res = tokio::process::Command::new("yt-dlp")
            .args(&args)
            .output()
            .await;

        if final_path.exists() {
            cleanup_temp(&temp_path).await;
            return Ok(final_path);
        }

        let output = match output_res {
            Ok(o) => o,
            Err(e) => {
                cleanup_temp(&temp_path).await;
                if final_path.exists() {
                    return Ok(final_path);
                }
                return Err(format!("Failed to run yt-dlp: {e}"));
            }
        };

        if !output.status.success() {
            cleanup_temp(&temp_path).await;
            if final_path.exists() {
                return Ok(final_path);
            }
            if attempt == 0 {
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            let msg = stderr.lines().take(5).collect::<Vec<_>>().join(" ");
            return Err(format!(
                "yt-dlp failed ({}): {}",
                output.status,
                if msg.is_empty() { "unknown error" } else { &msg }
            ));
        }

        if !temp_path.exists() {
            cleanup_temp(&temp_path).await;
            if final_path.exists() {
                return Ok(final_path);
            }
            if attempt == 0 {
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            }
            return Err("yt-dlp completed but output file not found".to_string());
        }

        if let Err(e) = tokio::fs::rename(&temp_path, &final_path).await {
            cleanup_temp(&temp_path).await;
            if final_path.exists() {
                return Ok(final_path);
            }
            return Err(format!("Failed to save cached audio: {e}"));
        }

        return Ok(final_path);
    }

    unreachable!()
}

pub struct Player {
    sink: Option<rodio::MixerDeviceSink>,
    player: Option<rodio::Player>,
    volume: f32,
    current_duration: Option<Duration>,
    has_source: bool,
    pub error: Option<String>,
}

impl Player {
    pub fn new() -> Self {
        let (sink, error) = match rodio::DeviceSinkBuilder::open_default_sink() {
            Ok(sink) => (Some(sink), None),
            Err(e) => (None, Some(format!("Audio output device error: {e}"))),
        };
        Self {
            sink,
            player: None,
            volume: 1.0,
            current_duration: None,
            has_source: false,
            error,
        }
    }

    pub fn play_file(&mut self, path: &Path) -> Result<(), String> {
        if self.sink.is_none() {
            match rodio::DeviceSinkBuilder::open_default_sink() {
                Ok(s) => {
                    self.sink = Some(s);
                    self.error = None;
                }
                Err(e) => {
                    let err = format!("No audio output device: {e}");
                    self.error = Some(err.clone());
                    return Err(err);
                }
            }
        }
        let sink = self.sink.as_ref().unwrap();

        let file = std::fs::File::open(path)
            .map_err(|e| format!("Failed to open audio file: {e}"))?;
        let decoder = rodio::Decoder::try_from(file)
            .map_err(|e| format!("Failed to decode audio file: {e}"))?;

        let dur = rodio::Source::total_duration(&decoder);
        self.current_duration = dur;

        let new_player = rodio::Player::connect_new(sink.mixer());
        new_player.set_volume(self.volume);
        new_player.append(decoder);
        new_player.play();

        self.player = Some(new_player);
        self.has_source = true;
        self.error = None;
        Ok(())
    }

    pub fn toggle_pause(&mut self) {
        if let Some(p) = &self.player {
            if p.is_paused() {
                p.play();
            } else {
                p.pause();
            }
        }
    }

    pub fn is_paused(&self) -> bool {
        self.player.as_ref().map(|p| p.is_paused()).unwrap_or(true)
    }

    pub fn seek(&mut self, pos: Duration) {
        if let Some(p) = &self.player {
            let _ = p.try_seek(pos);
        }
    }

    pub fn position(&self) -> Duration {
        self.player.as_ref().map(|p| p.get_pos()).unwrap_or(Duration::ZERO)
    }

    pub fn duration(&self) -> Option<Duration> {
        self.current_duration
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(p) = &self.player {
            p.set_volume(self.volume);
        }
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn stop(&mut self) {
        if let Some(p) = self.player.take() {
            p.stop();
        }
        self.has_source = false;
        self.current_duration = None;
    }

    pub fn is_finished(&mut self) -> bool {
        if self.has_source && self.player.as_ref().is_some_and(|p| p.empty()) {
            self.has_source = false;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn make_test_tracks(count: usize) -> Vec<Track> {
        (0..count)
            .map(|i| Track {
                id: format!("test_id_{:04}", i),
                title: format!("Title {i}"),
                artist: format!("Artist {i}"),
                album: format!("Album {i}"),
                duration_secs: 180 + i as u32,
                thumb_url: None,
            })
            .collect()
    }

    #[test]
    fn test_id_validation() {
        assert!(is_valid_id("dQw4w9WgXcQ"));
        assert!(is_valid_id("a1B2c3D4e5_"));
        assert!(is_valid_id("-----------"));
        assert!(is_valid_id("___________"));

        assert!(!is_valid_id("short"));
        assert!(!is_valid_id(""));
        assert!(!is_valid_id("toolongvideo1"));
        assert!(!is_valid_id("with space!"));
        assert!(!is_valid_id("with/slash1"));
        assert!(!is_valid_id("with.dot.12"));
    }

    #[test]
    fn test_ytdlp_args_without_cookies() {
        let out = Path::new("/tmp/cache/dQw4w9WgXcQ.m4a");
        let args = ytdlp_args("dQw4w9WgXcQ", out, None);
        assert_eq!(
            args,
            vec![
                "-q",
                "--no-warnings",
                "-f",
                "bestaudio[ext=m4a]",
                "-o",
                "/tmp/cache/dQw4w9WgXcQ.m4a",
                "https://music.youtube.com/watch?v=dQw4w9WgXcQ"
            ]
        );
    }

    #[test]
    fn test_ytdlp_args_with_cookies() {
        let out = Path::new("/tmp/cache/dQw4w9WgXcQ.m4a");
        let cookies = Path::new("/path/to/cookies.txt");
        let args = ytdlp_args("dQw4w9WgXcQ", out, Some(cookies));
        assert_eq!(
            args,
            vec![
                "-q",
                "--no-warnings",
                "--cookies",
                "/path/to/cookies.txt",
                "-f",
                "bestaudio[ext=m4a]",
                "-o",
                "/tmp/cache/dQw4w9WgXcQ.m4a",
                "https://music.youtube.com/watch?v=dQw4w9WgXcQ"
            ]
        );
    }

    #[test]
    fn test_queue_navigation_and_repeat() {
        let tracks = make_test_tracks(3);
        let mut q = Queue::new(tracks, 0);

        // Initial track
        assert_eq!(q.current().unwrap().id, "test_id_0000");

        // Next with repeat Off
        assert_eq!(q.next(false).unwrap().id, "test_id_0001");
        assert_eq!(q.next(false).unwrap().id, "test_id_0002");
        // End of list with repeat Off returns None
        assert!(q.next(false).is_none());

        // Prev from last track
        assert_eq!(q.prev().unwrap().id, "test_id_0001");
        assert_eq!(q.prev().unwrap().id, "test_id_0000");
        // At index 0 with repeat Off stays at 0
        assert_eq!(q.prev().unwrap().id, "test_id_0000");

        // Repeat All wraps on next and prev
        q.repeat = Repeat::All;
        assert_eq!(q.prev().unwrap().id, "test_id_0002"); // wrapped backwards
        assert_eq!(q.next(false).unwrap().id, "test_id_0000"); // wrapped forward from end

        // Repeat One auto-advance vs user next
        q.repeat = Repeat::One;
        // Auto-advance keeps current track
        assert_eq!(q.next(false).unwrap().id, "test_id_0000");
        assert_eq!(q.next(false).unwrap().id, "test_id_0000");
        // User next advances even in Repeat One
        assert_eq!(q.next(true).unwrap().id, "test_id_0001");
        assert_eq!(q.next(true).unwrap().id, "test_id_0002");
        // User next at end of list in Repeat One wraps to index 0
        assert_eq!(q.next(true).unwrap().id, "test_id_0000");
    }

    #[test]
    fn test_empty_queue() {
        let mut q = Queue::new(Vec::new(), 0);
        assert_eq!(q.repeat, Repeat::Off);
        assert!(q.current().is_none());
        assert!(q.next(false).is_none());
        assert!(q.next(true).is_none());
        assert!(q.prev().is_none());
    }

    #[test]
    fn test_queue_jump() {
        let tracks = make_test_tracks(5);
        let mut q = Queue::new(tracks, 0);
        assert_eq!(q.jump(3).unwrap().id, "test_id_0003");
        assert_eq!(q.current().unwrap().id, "test_id_0003");
        assert!(q.jump(10).is_none());
    }

    #[test]
    fn test_queue_shuffle() {
        let tracks = make_test_tracks(20);
        let mut q = Queue::new(tracks, 15);
        let current_id = q.current().unwrap().id.clone();
        assert_eq!(current_id, "test_id_0015");

        // Enable shuffle
        q.set_shuffle(true);
        // Current track is at pos 0 and order[0] == 15
        assert_eq!(q.order[0], 15);
        assert_eq!(q.pos, 0);
        assert_eq!(q.current().unwrap().id, current_id);

        // Order is a valid permutation of 0..20
        let mut sorted_order = q.order.clone();
        sorted_order.sort();
        assert_eq!(sorted_order, (0..20).collect::<Vec<usize>>());

        // Repeat Off yields exactly 19 more user-next tracks before None
        assert_eq!(q.repeat, Repeat::Off);
        for _ in 0..19 {
            assert!(q.next(true).is_some());
        }
        assert!(q.next(true).is_none());

        // Toggle shuffle off restores original order with pos = the current track's original index
        let last_idx = q.order[q.pos];
        q.set_shuffle(false);
        assert_eq!(q.order, (0..20).collect::<Vec<usize>>());
        assert_eq!(q.pos, last_idx);

        // Immediate toggle off from initial shuffled state restores original index 15
        let mut q2 = Queue::new(make_test_tracks(20), 15);
        q2.set_shuffle(true);
        assert_eq!(q2.order[0], 15);
        assert_eq!(q2.pos, 0);
        q2.set_shuffle(false);
        assert_eq!(q2.order, (0..20).collect::<Vec<usize>>());
        assert_eq!(q2.pos, 15);
        assert_eq!(q2.current().unwrap().id, "test_id_0015");
    }

    #[tokio::test]
    async fn test_cleanup_temp_removes_temp_and_part() {
        let dir = std::env::temp_dir().join(format!("ytmfast_test_cleanup_{}", std::process::id()));
        let _ = tokio::fs::create_dir_all(&dir).await;
        let temp_file = dir.join("test_track.dl.m4a");
        let part_file = dir.join("test_track.dl.m4a.part");

        tokio::fs::write(&temp_file, b"temp").await.unwrap();
        tokio::fs::write(&part_file, b"part").await.unwrap();
        assert!(temp_file.exists());
        assert!(part_file.exists());

        cleanup_temp(&temp_file).await;
        assert!(!temp_file.exists());
        assert!(!part_file.exists());

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_network_search_and_playback_smoke() {
        let data_dir = std::env::temp_dir().join("ytmfast_smoke_data");
        let rp = rustypipe::client::RustyPipe::builder()
            .storage_dir(&data_dir)
            .build()
            .expect("init rustypipe");

        let res = rp
            .query()
            .music_search_tracks("daft punk")
            .await
            .expect("search daft punk");
        assert!(!res.items.items.is_empty(), "expected >0 search results");

        let first = &res.items.items[0];
        let file_path = fetch(&first.id, None)
            .await
            .expect("fetch audio via yt-dlp");
        assert!(file_path.exists(), "fetched audio file must exist");

        let file = std::fs::File::open(&file_path).expect("open audio file");
        let decoder = rodio::Decoder::try_from(file).expect("decode m4a with rodio");
        let duration = rodio::Source::total_duration(&decoder).expect("track has duration");
        assert!(
            duration > std::time::Duration::from_secs(60),
            "expected duration > 60 s, got {:?}",
            duration
        );
    }
}
