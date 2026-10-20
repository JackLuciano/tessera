//! Scan history: after every scan a small snapshot (totals + folder sizes up to
//! depth 3) is written to the app data folder, so usage can be charted over time
//! and any snapshot can be diffed against the current tree.

use crate::scanner::{Kind, Tree, NO_PARENT};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const DEPTH: u32 = 3;
/// Folders smaller than this are not stored, keeping snapshots to a few KB.
const MIN_FOLDER: u64 = 50 * 1024 * 1024;
const KEEP: usize = 200;

#[derive(Serialize, Deserialize, Clone)]
pub struct Snapshot {
    pub ts: u64,
    pub root: String,
    pub size: u64,
    pub files: u64,
    pub folders: HashMap<String, u64>,
}

#[derive(Serialize)]
pub struct SnapshotInfo {
    pub ts: u64,
    pub size: u64,
    pub files: u64,
}

#[derive(Serialize)]
pub struct DiffRow {
    pub path: String,
    pub before: u64,
    pub after: u64,
    pub delta: i64,
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Folder sizes keyed by path relative to the scan root.
pub fn folder_map(t: &Tree) -> HashMap<String, u64> {
    let mut out = HashMap::new();
    let mut stack: Vec<(u32, u32, String)> = vec![(0, 0, String::new())];
    while let Some((id, depth, rel)) = stack.pop() {
        let n = &t.nodes[id as usize];
        if id != 0 {
            out.insert(rel.clone(), n.size);
        }
        if depth >= DEPTH {
            continue;
        }
        for c in &n.children {
            let cn = &t.nodes[*c as usize];
            if cn.kind == Kind::Dir && cn.size >= MIN_FOLDER && cn.parent != NO_PARENT {
                let r = if rel.is_empty() { cn.name.to_string() } else { format!("{rel}/{}", cn.name) };
                stack.push((*c, depth + 1, r));
            }
        }
    }
    out
}

fn key(root: &str) -> String {
    // Stable, filesystem-safe folder name per scan root
    let h = blake3::hash(root.to_lowercase().as_bytes());
    h.to_hex()[..16].to_string()
}

fn dir(base: &Path, root: &str) -> PathBuf {
    base.join("history").join(key(root))
}

pub fn save(base: &Path, t: &Tree) -> std::io::Result<()> {
    let root = t.root_path.to_string_lossy().into_owned();
    let d = dir(base, &root);
    fs::create_dir_all(&d)?;
    let r = &t.nodes[0];
    let snap = Snapshot { ts: now(), root, size: r.size, files: r.files.into(), folders: folder_map(t) };
    fs::write(d.join(format!("{}.json", snap.ts)), serde_json::to_vec(&snap)?)?;
    // Keep the newest KEEP snapshots
    let mut files: Vec<_> = fs::read_dir(&d)?.flatten().map(|e| e.path()).collect();
    files.sort();
    if files.len() > KEEP {
        for f in &files[..files.len() - KEEP] {
            let _ = fs::remove_file(f);
        }
    }
    Ok(())
}

fn load_all(base: &Path, root: &str) -> Vec<Snapshot> {
    let Ok(rd) = fs::read_dir(dir(base, root)) else { return vec![] };
    let mut v: Vec<Snapshot> = rd
        .flatten()
        .filter_map(|e| fs::read(e.path()).ok())
        .filter_map(|b| serde_json::from_slice(&b).ok())
        .collect();
    v.sort_by_key(|s| s.ts);
    v
}

pub fn list(base: &Path, root: &str) -> Vec<SnapshotInfo> {
    load_all(base, root).into_iter().map(|s| SnapshotInfo { ts: s.ts, size: s.size, files: s.files }).collect()
}

/// Folders that changed most between snapshot `ts` and the current tree.
pub fn diff(base: &Path, t: &Tree, ts: u64) -> Option<Vec<DiffRow>> {
    let root = t.root_path.to_string_lossy().into_owned();
    let snap = load_all(base, &root).into_iter().find(|s| s.ts == ts)?;
    let now = folder_map(t);
    let mut keys: Vec<&String> = snap.folders.keys().chain(now.keys()).collect();
    keys.sort();
    keys.dedup();
    let mut rows: Vec<DiffRow> = keys
        .into_iter()
        .map(|k| {
            let before = *snap.folders.get(k).unwrap_or(&0);
            let after = *now.get(k).unwrap_or(&0);
            DiffRow { path: k.clone(), before, after, delta: after as i64 - before as i64 }
        })
        .filter(|r| r.delta.unsigned_abs() >= 10 * 1024 * 1024)
        .collect();
    // Prefer the deepest changed folder: drop a parent whose change is fully explained by one child
    rows.sort_by_key(|r| std::cmp::Reverse(r.delta.unsigned_abs()));
    let mut out: Vec<DiffRow> = Vec::new();
    for r in rows {
        let explained = out.iter().any(|o| o.path.starts_with(&format!("{}/", r.path)) && (o.delta - r.delta).abs() < r.delta.abs() / 10 + 1);
        if !explained {
            out.push(r);
        }
        if out.len() >= 40 {
            break;
        }
    }
    Some(out)
}
