//! Live change tracking. After a scan the root is watched recursively (ReadDirectoryChangesW on
//! Windows, FSEvents on macOS, inotify on Linux). Changed folders are collected and handed over in
//! batches every few seconds, so the caller can re-read just those folders with `Tree::refresh_dir`
//! instead of rescanning the drive.

use notify::{EventKind, RecursiveMode, Watcher};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const TICK: Duration = Duration::from_secs(3);
/// Upper bound on queued folders; beyond this, changes keep coming faster than they matter.
const MAX_PENDING: usize = 20_000;
/// Folders handed over per tick, so one burst (an unzip, a git checkout) can't stall the UI.
pub const MAX_BATCH: usize = 400;

pub struct LiveWatch {
    _watcher: notify::RecommendedWatcher,
    stop: Arc<AtomicBool>,
}

impl Drop for LiveWatch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Watch `root`; `on_batch` runs on a background thread with the folders whose contents changed.
pub fn start(root: &Path, on_batch: impl Fn(Vec<PathBuf>) + Send + 'static) -> notify::Result<LiveWatch> {
    let pending: Arc<Mutex<HashSet<PathBuf>>> = Arc::default();
    let sink = pending.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(ev) = res else { return };
        if matches!(ev.kind, EventKind::Access(_)) {
            return;
        }
        let mut p = sink.lock();
        for path in ev.paths {
            if p.len() >= MAX_PENDING {
                break;
            }
            // A change to an entry means its folder's listing or totals changed
            if let Some(parent) = path.parent() {
                p.insert(parent.to_path_buf());
            }
        }
    })?;
    watcher.watch(root, RecursiveMode::Recursive)?;

    let stop = Arc::new(AtomicBool::new(false));
    let halt = stop.clone();
    std::thread::spawn(move || {
        while !halt.load(Ordering::Relaxed) {
            std::thread::sleep(TICK);
            let batch: Vec<PathBuf> = {
                let mut p = pending.lock();
                let take: Vec<PathBuf> = p.iter().take(MAX_BATCH).cloned().collect();
                for x in &take {
                    p.remove(x);
                }
                take
            };
            if !batch.is_empty() && !halt.load(Ordering::Relaxed) {
                on_batch(batch);
            }
        }
    });
    Ok(LiveWatch { _watcher: watcher, stop })
}
