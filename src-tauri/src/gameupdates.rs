//! Cleanup after game updates (Windows). Every 15 minutes the installed build of each Steam and
//! Epic game is compared with the last one seen; when a game finished updating, the space the
//! update left behind is measured: Steam's download / staging leftovers, the game's now outdated
//! shader cache, and the GPU drivers' shader caches (rebuilt automatically). Nothing is deleted
//! without a click, and deleting goes to the Recycle Bin.

use crate::games::{self, BuildMark};
use crate::sysutil;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Item {
    pub path: String,
    pub size: u64,
    /// download, staging, shader, driver_shader
    pub kind: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Found {
    pub game_id: String,
    pub game: String,
    /// Unix seconds the update was noticed.
    pub at: u64,
    pub from: String,
    pub to: String,
    pub items: Vec<Item>,
}

#[derive(Serialize, Deserialize, Default)]
struct Saved {
    /// game id → build
    builds: BTreeMap<String, String>,
    /// Updates with something still left to clean, newest first.
    found: Vec<Found>,
}

fn path(base: &Path) -> PathBuf {
    base.join("game-updates.json")
}

fn load(base: &Path) -> Saved {
    std::fs::read(path(base)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save(base: &Path, s: &Saved) {
    let _ = std::fs::write(path(base), serde_json::to_vec(s).unwrap_or_default());
}

/// The GPU drivers' shader caches; outdated entries pile up after game and driver updates.
fn driver_shader_dirs() -> Vec<PathBuf> {
    let Some(local) = sysutil::env_path("LOCALAPPDATA") else { return Vec::new() };
    [r"D3DSCache", r"NVIDIA\DXCache", r"NVIDIA\GLCache", r"AMD\DxCache", r"AMD\DxcCache", r"AMD\GLCache", r"Intel\ShaderCache"].iter().map(|d| local.join(d)).filter(|p| p.is_dir()).collect()
}

fn item(p: &Path, kind: &str) -> Option<Item> {
    if !p.is_dir() {
        return None;
    }
    let size = sysutil::size_of(p);
    (size > 0).then(|| Item { path: p.to_string_lossy().into_owned(), size, kind: kind.into() })
}

/// What an update of this game left behind.
fn leftovers(m: &BuildMark) -> Vec<Item> {
    let mut out = Vec::new();
    if let Some(apps) = &m.steamapps {
        out.extend(item(&apps.join("downloading").join(&m.app), "download"));
        out.extend(item(&apps.join("temp").join(&m.app), "staging"));
        out.extend(item(&apps.join("shadercache").join(&m.app), "shader"));
    }
    out
}

/// Compare builds with the last check; returns updates found now (also kept for the UI).
pub fn check(base: &Path) -> Vec<Found> {
    let mut saved = load(base);
    let first_run = saved.builds.is_empty();
    let marks = games::build_marks();
    let mut new = Vec::new();
    for m in &marks {
        let old = saved.builds.insert(m.id.clone(), m.build.clone());
        let Some(old) = old.filter(|o| *o != m.build && !first_run) else { continue };
        let mut items = leftovers(m);
        // Driver shader caches are shared by all games: list them once per round
        if new.is_empty() {
            items.extend(driver_shader_dirs().iter().filter_map(|d| item(d, "driver_shader")));
        }
        new.push(Found { game_id: m.id.clone(), game: m.name.clone(), at: sysutil::now(), from: old, to: m.build.clone(), items });
    }
    for f in new.iter().rev().filter(|f| !f.items.is_empty()) {
        saved.found.retain(|x| x.game_id != f.game_id);
        saved.found.insert(0, f.clone());
    }
    saved.found.truncate(30);
    save(base, &saved);
    new
}

/// Updates with leftovers, sizes refreshed (things the user removed by hand drop out).
pub fn pending(base: &Path) -> Vec<Found> {
    let mut saved = load(base);
    for f in &mut saved.found {
        f.items = f.items.iter().filter_map(|i| item(Path::new(&i.path), &i.kind)).collect();
    }
    saved.found.retain(|f| !f.items.is_empty());
    save(base, &saved);
    saved.found
}

/// Move the chosen leftovers to the Recycle Bin. Only paths from a recorded update are accepted;
/// driver shader caches are emptied instead of removed (the drivers expect the folder).
pub fn clean(base: &Path, paths: &[String]) -> (u64, Vec<String>) {
    let saved = load(base);
    let known: Vec<&Item> = saved.found.iter().flat_map(|f| f.items.iter()).collect();
    let mut freed = 0;
    let mut failed = Vec::new();
    for p in paths {
        let Some(it) = known.iter().find(|i| i.path.eq_ignore_ascii_case(p)) else {
            failed.push(format!("{p}: not an update leftover"));
            continue;
        };
        let dir = Path::new(&it.path);
        let size = sysutil::size_of(dir);
        let result = if it.kind == "driver_shader" {
            // Files in use by a running game are simply skipped
            let entries: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
            trash::delete_all(&entries).or_else(|_| {
                entries.iter().for_each(|e| {
                    let _ = trash::delete(e);
                });
                Ok::<(), trash::Error>(())
            })
        } else {
            trash::delete(dir)
        };
        match result {
            Ok(()) => freed += size.saturating_sub(sysutil::size_of(dir)),
            Err(e) => failed.push(format!("{p}: {e}")),
        }
    }
    (freed, failed)
}

/// Check every 15 minutes; a finished update with more than 200 MB to clean gets a notification.
pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let Ok(base) = app.path().app_data_dir() else { return };
        std::thread::sleep(Duration::from_secs(60));
        loop {
            let found = check(&base);
            for f in found.iter().filter(|f| f.items.iter().map(|i| i.size).sum::<u64>() > 200 * 1024 * 1024) {
                let total: u64 = f.items.iter().map(|i| i.size).sum();
                let l = app.state::<crate::AppState>().labels.lock().clone();
                crate::tray::notify(&app, &l.update_title.replace("{game}", &f.game), &l.update_body.replace("{size}", &crate::fmt_bytes(total)));
            }
            if found.iter().any(|f| !f.items.is_empty()) {
                let _ = app.emit("game-updates", ());
            }
            std::thread::sleep(Duration::from_secs(15 * 60));
        }
    });
}
