//! Fast deletion.
//!
//! - Permanent: folders are removed in parallel. Files of a folder are deleted on all cores, sub-folders
//!   recurse in parallel, and the folder itself goes last. On SSDs this is several times faster than
//!   `remove_dir_all` for trees with many small files (`node_modules`, `__pycache__`, build output).
//! - Recycle Bin: items are handed to the shell in batches, one operation per batch instead of per item.
//!
//! Progress is counted in bytes actually deleted, so it moves smoothly even inside one huge folder.

use rayon::prelude::*;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

/// Items per Recycle Bin operation: big enough to save the per-call shell overhead, small enough
/// that progress and cancelling stay responsive.
const TRASH_BATCH: usize = 64;

pub struct Progress {
    pub done: AtomicUsize,
    pub bytes: AtomicU64,
    pub cancel: AtomicBool,
}

impl Progress {
    pub fn new() -> Self {
        Self { done: AtomicUsize::new(0), bytes: AtomicU64::new(0), cancel: AtomicBool::new(false) }
    }
}

/// Windows refuses to delete read-only files; clear the flag and retry once.
fn remove_file(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            let mut perm = fs::symlink_metadata(path)?.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perm.set_readonly(false);
            fs::set_permissions(path, perm)?;
            fs::remove_file(path)
        }
        r => r,
    }
}

/// Parallel recursive delete. Symlinks and junctions are removed as links, never followed.
fn remove_tree(dir: &Path, p: &Progress) -> io::Result<()> {
    if p.cancel.load(Ordering::Relaxed) {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
    }
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    let mut links = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_symlink() {
            links.push(entry.path());
        } else if ft.is_dir() {
            dirs.push(entry.path());
        } else {
            files.push((entry.path(), entry.metadata().map(|m| m.len()).unwrap_or(0)));
        }
    }
    files.par_iter().try_for_each(|(f, len)| {
        remove_file(f)?;
        p.bytes.fetch_add(*len, Ordering::Relaxed);
        Ok::<_, io::Error>(())
    })?;
    for l in &links {
        // A directory link (junction) is removed with remove_dir; a file link with remove_file
        fs::remove_dir(l).or_else(|_| fs::remove_file(l))?;
    }
    dirs.par_iter().try_for_each(|d| remove_tree(d, p))?;
    fs::remove_dir(dir)
}

/// Delete one item permanently.
pub fn remove_permanent(path: &Path, is_dir: bool, size: u64, p: &Progress) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() {
        return fs::remove_dir(path).or_else(|_| fs::remove_file(path));
    }
    if is_dir && meta.is_dir() {
        remove_tree(path, p)
    } else {
        remove_file(path)?;
        p.bytes.fetch_add(size, Ordering::Relaxed);
        Ok(())
    }
}

/// Outcome per input index: `Ok` or the error text.
pub type Outcome = Vec<Result<(), String>>;

/// Permanently delete many items, several at a time.
pub fn permanent_all(items: &[(PathBuf, bool, u64)], p: &Progress) -> Outcome {
    items
        .par_iter()
        .map(|(path, is_dir, size)| {
            if p.cancel.load(Ordering::Relaxed) {
                return Err("cancelled".to_string());
            }
            let r = remove_permanent(path, *is_dir, *size, p).map_err(|e| e.to_string());
            p.done.fetch_add(1, Ordering::Relaxed);
            r
        })
        .collect()
}

/// Move many items to the Recycle Bin in batches. If a batch fails, its items are retried one by
/// one so each failure is reported against the right path.
pub fn trash_all(items: &[(PathBuf, bool, u64)], p: &Progress) -> Outcome {
    let mut out = Vec::with_capacity(items.len());
    for chunk in items.chunks(TRASH_BATCH) {
        if p.cancel.load(Ordering::Relaxed) {
            out.extend(chunk.iter().map(|_| Err("cancelled".to_string())));
            continue;
        }
        let paths: Vec<&PathBuf> = chunk.iter().map(|i| &i.0).collect();
        if trash::delete_all(&paths).is_ok() {
            out.extend(chunk.iter().map(|_| Ok(())));
            p.bytes.fetch_add(chunk.iter().map(|i| i.2).sum(), Ordering::Relaxed);
            p.done.fetch_add(chunk.len(), Ordering::Relaxed);
            continue;
        }
        for (path, _, size) in chunk {
            let r = trash::delete(path).map_err(|e| e.to_string());
            if r.is_ok() {
                p.bytes.fetch_add(*size, Ordering::Relaxed);
            }
            p.done.fetch_add(1, Ordering::Relaxed);
            out.push(r);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deletes_a_deep_tree_in_parallel() {
        let root = std::env::temp_dir().join(format!("tessera-del-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for i in 0..20 {
            let d = root.join(format!("a{i}/b/c"));
            fs::create_dir_all(&d).unwrap();
            for j in 0..10 {
                fs::write(d.join(format!("f{j}.txt")), b"12345").unwrap();
            }
        }
        // A read-only file must not stop the delete
        let ro = root.join("a0/b/c/f0.txt");
        let mut perm = fs::metadata(&ro).unwrap().permissions();
        perm.set_readonly(true);
        fs::set_permissions(&ro, perm).unwrap();

        let p = Progress::new();
        let out = permanent_all(&[(root.clone(), true, 0)], &p);
        assert!(out[0].is_ok(), "{:?}", out[0]);
        assert!(!root.exists());
        assert_eq!(p.bytes.load(Ordering::Relaxed), 20 * 10 * 5);
        assert_eq!(p.done.load(Ordering::Relaxed), 1);
    }
}
