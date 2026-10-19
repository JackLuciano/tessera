//! Duplicate finder: group by exact size, then by a partial hash of the head and
//! tail, then confirm with a full BLAKE3 hash. Each stage only reads files that
//! are still candidates, so most files are never opened at all.

use crate::scanner::{Kind, Tree};
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const PARTIAL: u64 = 64 * 1024;
/// Files smaller than this are ignored: not worth the I/O for the space they free.
pub const MIN_SIZE: u64 = 1024 * 1024;

#[derive(Serialize, Clone)]
pub struct DupFile {
    pub id: u32,
    pub path: String,
    pub modified: u64,
}

#[derive(Serialize)]
pub struct DupGroup {
    pub size: u64,
    pub wasted: u64,
    pub files: Vec<DupFile>,
}

pub struct DupProgress {
    pub hashed_bytes: AtomicU64,
    pub total_bytes: AtomicU64,
    pub cancel: AtomicBool,
}

fn partial_hash(path: &Path, size: u64) -> Option<[u8; 32]> {
    let mut f = File::open(path).ok()?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; PARTIAL as usize];
    let n = f.read(&mut buf).ok()?;
    h.update(&buf[..n]);
    if size > PARTIAL * 2 {
        f.seek(SeekFrom::End(-(PARTIAL as i64))).ok()?;
        let n = f.read(&mut buf).ok()?;
        h.update(&buf[..n]);
    }
    Some(*h.finalize().as_bytes())
}

fn full_hash(path: &Path, progress: &DupProgress) -> Option<[u8; 32]> {
    let mut f = File::open(path).ok()?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if progress.cancel.load(Ordering::Relaxed) {
            return None;
        }
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        progress.hashed_bytes.fetch_add(n as u64, Ordering::Relaxed);
    }
    Some(*h.finalize().as_bytes())
}

/// Candidate list is built while holding the tree lock; hashing happens without it.
pub fn candidates(t: &Tree) -> Vec<(u32, PathBuf, u64, u64)> {
    let mut by_size: HashMap<u64, Vec<u32>> = HashMap::new();
    for (i, n) in t.nodes.iter().enumerate() {
        if n.kind == Kind::File && n.size >= MIN_SIZE {
            by_size.entry(n.size).or_default().push(i as u32);
        }
    }
    by_size
        .into_values()
        .filter(|v| v.len() > 1)
        .flatten()
        .map(|id| { let n = &t.nodes[id as usize]; (id, t.path_of(id), n.size, n.modified) })
        .collect()
}

/// (size, content hash) — files are duplicates when both match.
type Key = (u64, [u8; 32]);
/// (node id, path, modified)
type Cand = (u32, PathBuf, u64);

/// Drop paths that are hard links to a file already in the list: they share storage, so they are
/// not wasted space (and after "replace with hard links" they must not show up again).
fn drop_hardlinks(cands: Vec<(u32, PathBuf, u64, u64)>) -> Vec<(u32, PathBuf, u64, u64)> {
    let mut by_size: HashMap<u64, Vec<(u32, PathBuf, u64, u64)>> = HashMap::new();
    for c in cands {
        by_size.entry(c.2).or_default().push(c);
    }
    by_size
        .into_par_iter()
        .flat_map_iter(|(_, group)| {
            let mut seen: Vec<same_file::Handle> = Vec::new();
            let mut out = Vec::new();
            for c in group {
                match same_file::Handle::from_path(&c.1) {
                    Ok(h) if seen.contains(&h) => continue,
                    Ok(h) => seen.push(h),
                    Err(_) => {}
                }
                out.push(c);
            }
            out
        })
        .collect()
}

/// Replace `dup` with a hard link to `keep`. The link is created next to `dup` first and then renamed
/// over it, so `dup` is never missing if anything fails half-way.
pub fn hardlink(keep: &Path, dup: &Path) -> std::io::Result<()> {
    if same_file::is_same_file(keep, dup)? {
        return Ok(());
    }
    if std::fs::metadata(keep)?.len() != std::fs::metadata(dup)?.len() {
        return Err(std::io::Error::other("files differ in size"));
    }
    let name = dup.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dup.with_file_name(format!(".{name}.tessera-link"));
    std::fs::hard_link(keep, &tmp)?;
    std::fs::rename(&tmp, dup).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

pub fn find(cands: Vec<(u32, PathBuf, u64, u64)>, progress: &DupProgress) -> Vec<DupGroup> {
    let cands = drop_hardlinks(cands);
    // Stage 2: partial hash within each size group
    let partial: Vec<(Key, Cand)> = cands
        .into_par_iter()
        .filter_map(|(id, p, size, m)| partial_hash(&p, size).map(|h| ((size, h), (id, p, m))))
        .collect();
    let mut groups: HashMap<Key, Vec<Cand>> = HashMap::new();
    for (k, v) in partial {
        groups.entry(k).or_default().push(v);
    }
    let still: Vec<(u64, (u32, PathBuf, u64))> = groups
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .flat_map(|((size, _), v)| v.into_iter().map(move |x| (size, x)))
        .collect();
    progress.total_bytes.store(still.iter().map(|(s, _)| *s).sum(), Ordering::Relaxed);

    // Stage 3: full BLAKE3 only for files whose head and tail already match
    let full: Vec<((u64, [u8; 32]), DupFile)> = still
        .into_par_iter()
        .filter_map(|(size, (id, p, m))| {
            full_hash(&p, progress).map(|h| ((size, h), DupFile { id, path: p.to_string_lossy().into_owned(), modified: m }))
        })
        .collect();
    let mut fin: HashMap<(u64, [u8; 32]), Vec<DupFile>> = HashMap::new();
    for (k, v) in full {
        fin.entry(k).or_default().push(v);
    }
    let mut out: Vec<DupGroup> = fin
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|((size, _), mut files)| {
            // Newest first, so "keep the first" keeps the most recent copy by default
            files.sort_by_key(|f| std::cmp::Reverse(f.modified));
            DupGroup { size, wasted: size * (files.len() as u64 - 1), files }
        })
        .collect();
    out.sort_by_key(|g| std::cmp::Reverse(g.wasted));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardlinks_replace_copies_and_are_not_duplicates() {
        let dir = std::env::temp_dir().join(format!("tessera-dupes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let data = vec![7u8; (MIN_SIZE + 3) as usize];
        let (a, b) = (dir.join("a.bin"), dir.join("b.bin"));
        std::fs::write(&a, &data).unwrap();
        std::fs::write(&b, &data).unwrap();
        let size = data.len() as u64;
        let progress = DupProgress { hashed_bytes: AtomicU64::new(0), total_bytes: AtomicU64::new(0), cancel: AtomicBool::new(false) };
        let cands = || vec![(1, a.clone(), size, 0), (2, b.clone(), size, 0)];
        assert_eq!(find(cands(), &progress).len(), 1);

        hardlink(&a, &b).unwrap();
        assert!(same_file::is_same_file(&a, &b).unwrap());
        assert_eq!(std::fs::read(&b).unwrap(), data);
        assert!(find(cands(), &progress).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
