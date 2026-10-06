use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_secs: u32,
    pub thumb_url: Option<String>,
}

pub enum Cmd {
    Search(String),
    Fetch(Track),
}

pub enum Event {
    SearchResults { query: String, tracks: Vec<Track> },
    SearchError { query: String, error: String },
    Ready { id: String, path: PathBuf },
    FetchError { id: String, msg: String },
}

fn select_thumbnail(covers: &[rustypipe::model::Thumbnail]) -> Option<String> {
    covers
        .iter()
        .filter(|t| t.width >= 80)
        .min_by_key(|t| t.width)
        .or_else(|| covers.last())
        .map(|t| t.url.clone())
}

pub struct Backend {
    tx: mpsc::Sender<Cmd>,
    rx: mpsc::Receiver<Event>,
}

impl Backend {
    pub fn new(ctx: egui::Context) -> Self {
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

            let (rp, rp_err) = match rustypipe::client::RustyPipe::builder().storage_dir(&data_dir).build() {
                Ok(rp) => (Some(Arc::new(rp)), None),
                Err(e) => (None, Some(format!("RustyPipe init error: {e}"))),
            };

            let _guard = rt.as_ref().map(|r| r.enter());
            while let Ok(cmd) = cmd_rx.recv() {
                let event_tx = event_tx.clone();
                let ctx = ctx.clone();
                let rp = rp.clone();
                let rp_err = rp_err.clone();
                let rt_err = rt_err.clone();

                if let Some(rt_ref) = &rt {
                    rt_ref.spawn(async move {
                        match cmd {
                            Cmd::Search(query) => {
                                if let Some(rp) = rp {
                                    match rp.query().music_search_tracks(&query).await {
                                        Ok(res) => {
                                            let tracks = res
                                                .items
                                                .items
                                                .into_iter()
                                                .map(|item| {
                                                    let artist = item
                                                        .artists
                                                        .iter()
                                                        .map(|a| a.name.as_str())
                                                        .collect::<Vec<_>>()
                                                        .join(", ");
                                                    let album =
                                                        item.album.map(|a| a.name).unwrap_or_default();
                                                    let thumb_url = select_thumbnail(&item.cover);
                                                    Track {
                                                        id: item.id,
                                                        title: item.name,
                                                        artist,
                                                        album,
                                                        duration_secs: item.duration.unwrap_or(0),
                                                        thumb_url,
                                                    }
                                                })
                                                .collect();
                                            let _ = event_tx.send(Event::SearchResults { query, tracks });
                                        }
                                        Err(e) => {
                                            let _ = event_tx.send(Event::SearchError {
                                                query,
                                                error: e.to_string(),
                                            });
                                        }
                                    }
                                } else {
                                    let err = rp_err.unwrap_or_else(|| "RustyPipe init failed".to_string());
                                    let _ = event_tx.send(Event::SearchError { query, error: err });
                                }
                                ctx.request_repaint();
                            }
                            Cmd::Fetch(track) => {
                                match crate::audio::fetch(&track.id, None).await {
                                    Ok(path) => {
                                        let _ = event_tx.send(Event::Ready {
                                            id: track.id,
                                            path,
                                        });
                                    }
                                    Err(msg) => {
                                        let _ = event_tx.send(Event::FetchError {
                                            id: track.id,
                                            msg,
                                        });
                                    }
                                }
                                ctx.request_repaint();
                            }
                        }
                    });
                } else {
                    let err = rt_err.unwrap_or_else(|| "Runtime init failed".to_string());
                    match cmd {
                        Cmd::Search(query) => {
                            let _ = event_tx.send(Event::SearchError { query, error: err });
                        }
                        Cmd::Fetch(track) => {
                            let _ = event_tx.send(Event::FetchError { id: track.id, msg: err });
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
