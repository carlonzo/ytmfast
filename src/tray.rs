//! Linux desktop integration for background mode: a tray icon
//! (StatusNotifierItem, shown by Waybar's `tray` module on Omarchy, KDE, and
//! GNOME with the AppIndicator extension) and a single-instance socket, so
//! launching ytmfast again shows the running window.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Mutex, mpsc};

use crate::{Remote, Repaint, ui::Action};

/// The listener, between `SingleInstance::claim` and `listen`.
static LISTENER: Mutex<Option<UnixListener>> = Mutex::new(None);

pub enum Claim {
    /// This is the only instance; keep the guard alive until exit.
    Primary(SingleInstance),
    /// Another instance was running and has been asked to show itself.
    Forwarded,
    /// No socket could be made (no runtime dir); run without the check.
    Unavailable,
}

/// Removes the socket file when the primary instance exits.
pub struct SingleInstance {
    path: PathBuf,
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn socket_path() -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from)?;
    Some(dir.join("ytmfast.sock"))
}

impl SingleInstance {
    pub fn claim() -> Claim {
        let Some(path) = socket_path() else {
            return Claim::Unavailable;
        };
        if let Ok(mut stream) = UnixStream::connect(&path)
            && stream.write_all(b"show\n").is_ok()
        {
            return Claim::Forwarded;
        }
        // Nobody answered: the file (if any) is left over from a crash.
        let _ = std::fs::remove_file(&path);
        match UnixListener::bind(&path) {
            Ok(listener) => {
                *LISTENER.lock().unwrap_or_else(|e| e.into_inner()) = Some(listener);
                Claim::Primary(SingleInstance { path })
            }
            Err(_) => Claim::Unavailable,
        }
    }
}

/// Answer later launches: each one asks to show the window.
pub fn listen(tx: mpsc::Sender<Remote>, repaint: Repaint) {
    let Some(listener) = LISTENER.lock().unwrap_or_else(|e| e.into_inner()).take() else {
        return;
    };
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut line = String::new();
            if BufReader::new(stream).read_line(&mut line).is_ok() && line.trim() == "show" {
                let _ = tx.send(Remote::Show);
                repaint.request_repaint();
            }
        }
    });
}

struct Tray {
    tx: mpsc::Sender<Remote>,
    repaint: Repaint,
    icons: Vec<ksni::Icon>,
}

impl Tray {
    fn send(&self, remote: Remote) {
        let _ = self.tx.send(remote);
        self.repaint.request_repaint();
    }
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "ytmfast".into()
    }

    fn title(&self) -> String {
        "ytmfast".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icons.clone()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(Remote::Show);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        let item = |label: &str, remote: fn() -> Remote| -> ksni::MenuItem<Self> {
            StandardItem {
                label: label.into(),
                activate: Box::new(move |tray: &mut Self| tray.send(remote())),
                ..Default::default()
            }
            .into()
        };
        vec![
            item("Show ytmfast", || Remote::Show),
            ksni::MenuItem::Separator,
            item("Play/Pause", || Remote::Action(Action::TogglePlayPause)),
            item("Next", || Remote::Action(Action::NextTrack(true))),
            item("Previous", || Remote::Action(Action::PrevTrack)),
            ksni::MenuItem::Separator,
            item("Quit", || Remote::Quit),
        ]
    }
}

/// The app icon as ARGB32 pixmaps at the sizes trays ask for.
fn icons() -> Vec<ksni::Icon> {
    let Ok(image) = image::load_from_memory(include_bytes!("../assets/app-icon.png")) else {
        return Vec::new();
    };
    [22u32, 32, 64]
        .into_iter()
        .map(|size| {
            let rgba = image
                .resize_exact(size, size, image::imageops::FilterType::Triangle)
                .to_rgba8();
            let data = rgba
                .pixels()
                .flat_map(|p| {
                    let [r, g, b, a] = p.0;
                    [a, r, g, b]
                })
                .collect();
            ksni::Icon {
                width: size as i32,
                height: size as i32,
                data,
            }
        })
        .collect()
}

/// Show the tray icon. Desktops without a tray simply never show it; the
/// window can still be brought back by launching ytmfast again or through
/// MPRIS (`playerctl`, Waybar's `mpris` module).
pub fn spawn(tx: mpsc::Sender<Remote>, repaint: Repaint) {
    use ksni::blocking::TrayMethods;
    let tray = Tray {
        tx,
        repaint,
        icons: icons(),
    };
    match tray.spawn() {
        // Keep the service for the life of the process.
        Ok(handle) => std::mem::forget(handle),
        Err(error) => eprintln!("Tray icon unavailable: {error}"),
    }
}
