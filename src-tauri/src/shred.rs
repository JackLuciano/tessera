//! File shredder: overwrite a file's contents before deleting it, so undelete tools can't bring
//! it back. Then the name is overwritten too (renamed to random characters) and the file removed.
//!
//! On SSDs the drive's wear levelling may keep old copies of blocks out of reach of any overwrite;
//! the UI says so. Files with more than one hard link are refused: overwriting would destroy the
//! data behind every other path as well.

pub use crate::delete::Progress;
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

#[derive(Serialize, Clone)]
pub struct ShredTarget {
    pub path: String,
    pub size: u64,
    pub files: u64,
    pub is_dir: bool,
}

/// What `paths` would shred, for the confirmation.
pub fn inspect(paths: &[String]) -> Vec<ShredTarget> {
    paths
        .iter()
        .filter_map(|p| {
            let path = Path::new(p);
            let m = std::fs::symlink_metadata(path).ok()?;
            let (size, files) = if m.is_dir() { totals(path) } else { (m.len(), 1) };
            Some(ShredTarget { path: p.clone(), size, files, is_dir: m.is_dir() })
        })
        .collect()
}

fn totals(dir: &Path) -> (u64, u64) {
    let Ok(rd) = std::fs::read_dir(dir) else { return (0, 0) };
    rd.flatten().fold((0, 0), |(s, f), e| match e.file_type() {
        Ok(t) if t.is_dir() => {
            let (s2, f2) = totals(&e.path());
            (s + s2, f + f2)
        }
        Ok(_) => (s + e.metadata().map(|m| m.len()).unwrap_or(0), f + 1),
        Err(_) => (s, f),
    })
}

/// Shred files and folders. `passes`: 1 = random data, 3 = zeros, ones, random.
pub fn shred_all(paths: &[PathBuf], passes: u8, p: &Progress) -> Vec<Result<u64, String>> {
    paths
        .iter()
        .map(|path| {
            if p.cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            let r = shred_path(path, passes, p);
            p.done.fetch_add(1, Ordering::Relaxed);
            r
        })
        .collect()
}

fn shred_path(path: &Path, passes: u8, p: &Progress) -> Result<u64, String> {
    let m = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if m.file_type().is_symlink() || is_reparse_point(&m) {
        // Links point at data elsewhere: remove the link only
        remove_link(path, m.is_dir()).map_err(|e| e.to_string())?;
        return Ok(0);
    }
    if !m.is_dir() {
        return shred_file(path, passes, p);
    }
    let mut total = 0;
    let mut first_err = None;
    for e in std::fs::read_dir(path).map_err(|e| e.to_string())?.flatten() {
        if p.cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        match shred_path(&e.path(), passes, p) {
            Ok(n) => total += n,
            Err(err) => {
                first_err.get_or_insert(format!("{}: {err}", e.path().display()));
            }
        }
    }
    if let Some(e) = first_err {
        return Err(e);
    }
    let renamed = rename_random(path).unwrap_or_else(|_| path.to_path_buf());
    std::fs::remove_dir(&renamed).map_err(|e| e.to_string())?;
    Ok(total)
}

fn shred_file(path: &Path, passes: u8, p: &Progress) -> Result<u64, String> {
    if link_count(path) > 1 {
        return Err("has other hard links; shredding would destroy them too".into());
    }
    let mut perms = std::fs::metadata(path).map_err(|e| e.to_string())?.permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    if perms.readonly() {
        perms.set_readonly(false);
        std::fs::set_permissions(path, perms).map_err(|e| e.to_string())?;
    }
    let mut f = OpenOptions::new().write(true).open(path).map_err(|e| e.to_string())?;
    let len = f.metadata().map_err(|e| e.to_string())?.len();
    let mut buf = vec![0u8; 1 << 20];
    let seed = format!("{}{:?}{}", path.display(), std::time::SystemTime::now(), std::process::id());
    let mut rng = blake3::Hasher::new().update(seed.as_bytes()).finalize_xof();
    for pass in 0..passes.max(1) {
        // 3 passes: 0x00, 0xFF, random (the classic DoD 5220.22-M sequence); 1 pass: random
        let pattern = if passes >= 3 { [Some(0x00u8), Some(0xFF), None][pass as usize % 3] } else { None };
        f.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let mut left = len;
        while left > 0 {
            if p.cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            let n = left.min(buf.len() as u64) as usize;
            match pattern {
                Some(b) => buf[..n].fill(b),
                None => rng.fill(&mut buf[..n]),
            }
            f.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            left -= n as u64;
            p.bytes.fetch_add(n as u64 / passes.max(1) as u64, Ordering::Relaxed);
        }
        // Each pass must reach the disk, not just the cache, or the next one simply replaces it there
        f.sync_all().map_err(|e| e.to_string())?;
    }
    f.set_len(0).map_err(|e| e.to_string())?;
    drop(f);
    let renamed = rename_random(path).unwrap_or_else(|_| path.to_path_buf());
    std::fs::remove_file(&renamed).map_err(|e| e.to_string())?;
    Ok(len)
}

/// Rename to random characters of the same length, so the original name is gone from the directory.
fn rename_random(path: &Path) -> std::io::Result<PathBuf> {
    let len = path.file_name().map(|n| n.len()).unwrap_or(8).clamp(8, 32);
    let mut bytes = vec![0u8; len];
    blake3::Hasher::new().update(format!("{}{:?}", path.display(), std::time::Instant::now()).as_bytes()).finalize_xof().fill(&mut bytes);
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let name: String = bytes.iter().map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char).collect();
    let to = path.with_file_name(name);
    std::fs::rename(path, &to)?;
    Ok(to)
}

fn remove_link(path: &Path, is_dir: bool) -> std::io::Result<()> {
    if is_dir { std::fs::remove_dir(path) } else { std::fs::remove_file(path) }
}

#[cfg(windows)]
fn is_reparse_point(m: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    m.file_attributes() & 0x400 != 0
}
#[cfg(not(windows))]
fn is_reparse_point(_: &std::fs::Metadata) -> bool {
    false
}

#[cfg(windows)]
fn link_count(path: &Path) -> u32 {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION};
    let Ok(f) = std::fs::File::open(path) else { return 1 };
    // SAFETY: zeroed POD out-struct, valid open handle for the duration of the call.
    unsafe {
        let mut info: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
        if GetFileInformationByHandle(f.as_raw_handle(), &mut info) != 0 { info.nNumberOfLinks } else { 1 }
    }
}
#[cfg(unix)]
fn link_count(path: &Path) -> u32 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).map(|m| m.nlink() as u32).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shreds_files_and_folders() {
        let dir = std::env::temp_dir().join(format!("tessera-shred-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.txt"), vec![7u8; 3_000_000]).unwrap();
        std::fs::write(dir.join("sub/b.txt"), b"secret").unwrap();
        let ro = dir.join("sub/ro.txt");
        std::fs::write(&ro, b"read only").unwrap();
        let mut perms = std::fs::metadata(&ro).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&ro, perms).unwrap();

        let info = inspect(&[dir.to_string_lossy().into_owned()]);
        assert_eq!(info[0].files, 3);
        let p = Progress::new();
        let r = shred_all(std::slice::from_ref(&dir), 3, &p);
        assert_eq!(r[0].as_ref().copied(), Ok(3_000_000 + 6 + 9));
        assert!(!dir.exists());
    }

    #[test]
    fn refuses_hard_linked_files() {
        let dir = std::env::temp_dir().join(format!("tessera-shred-hl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a"), b"shared").unwrap();
        std::fs::hard_link(dir.join("a"), dir.join("b")).unwrap();
        let r = shred_all(&[dir.join("a")], 1, &Progress::new());
        assert!(r[0].is_err());
        assert_eq!(std::fs::read(dir.join("b")).unwrap(), b"shared");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
