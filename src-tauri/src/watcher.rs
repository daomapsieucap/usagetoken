use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    path::PathBuf,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

pub fn watch(path: PathBuf, debounce: Duration, on_change: impl Fn() + Send + 'static) {
    thread::spawn(move || {
        let (tx, rx) = mpsc::channel::<Result<Event, notify::Error>>();

        let mut watcher = match RecommendedWatcher::new(tx, Config::default()) {
            Ok(w) => w,
            Err(e) => {
                log::warn!("file watcher init failed: {e}");
                return;
            }
        };

        if let Err(e) = watcher.watch(&path, RecursiveMode::Recursive) {
            log::warn!("file watcher watch failed on {}: {e}", path.display());
            return;
        }

        log::info!("watching {} for changes", path.display());

        let mut last_event: Option<Instant> = None;

        loop {
            match rx.recv_timeout(debounce / 2) {
                Ok(Ok(_)) => {
                    last_event = Some(Instant::now());
                }
                Ok(Err(e)) => {
                    log::warn!("watcher error: {e}");
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if let Some(t) = last_event {
                        if t.elapsed() >= debounce {
                            last_event = None;
                            on_change();
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });
}
