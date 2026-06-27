use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::{path::PathBuf, sync::mpsc, time::Duration};

pub fn watch<F>(dir: PathBuf, debounce: Duration, callback: F)
where
    F: Fn() + Send + 'static,
{
    std::thread::spawn(move || {
        let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
        let mut watcher = match RecommendedWatcher::new(tx, Config::default()) {
            Ok(w) => w,
            Err(_) => return,
        };
        if watcher.watch(&dir, RecursiveMode::Recursive).is_err() {
            return;
        }

        loop {
            if rx.recv().is_err() {
                break;
            }
            // Drain rapid-fire events during the debounce window
            let deadline = std::time::Instant::now() + debounce;
            loop {
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                if remaining.is_zero() {
                    break;
                }
                match rx.recv_timeout(remaining) {
                    Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
                if std::time::Instant::now() >= deadline {
                    break;
                }
            }
            callback();
        }
    });
}
