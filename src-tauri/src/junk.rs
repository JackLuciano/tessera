//! System junk in well-known places: temp folders, crash dumps, thumbnail and shader caches and the
//! Recycle Bin. Unlike the scan-based cleanup this needs no drive scan, so Care can use it.
//!
//! Only folder *contents* are removed, never the folders themselves, and temp files touched in the
//! last 24 hours are left alone (an installer or app may still be using them).

// Which of these are needed depends on the platform's junk locations
#[allow(unused_imports)]
use crate::sysutil::{self, env_path, home};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const DAY: Duration = Duration::from_secs(86_400);

// Single-file and pattern targets are only used by the Windows rules
#[cfg_attr(not(windows), allow(dead_code))]
enum Target {
    /// Everything inside a folder that was not modified within `min_age`.
    Contents(PathBuf, Duration),
    /// Files in a folder whose name starts with a prefix and ends with a suffix.
    Files(PathBuf, &'static str, &'static str),
    /// One file.
    File(PathBuf),
    /// Big files directly in a folder, untouched for a long time (old downloads). These go to the
    /// Recycle Bin, not straight to deletion.
    OldLarge(PathBuf, u64, Duration),
    RecycleBin,
}

struct Rule {
    key: &'static str,
    /// Safe to clean without a second look.
    safe: bool,
    /// Lives in a machine-wide location: needs administrator rights.
    admin: bool,
    targets: Vec<Target>,
}

#[derive(Serialize, Clone)]
pub struct JunkItem {
    pub key: &'static str,
    pub safe: bool,
    pub admin: bool,
    pub size: u64,
    pub count: u64,
    pub sample: Vec<String>,
}

#[derive(Serialize, Default, Clone)]
pub struct CleanOutcome {
    pub freed: u64,
    pub cleaned: usize,
    /// Files in use or without permission, skipped.
    pub locked: usize,
    pub failed: Vec<String>,
}

/// The user's Downloads folder (it can be moved, so Windows is asked first).
fn downloads() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let k = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders").ok();
        if let Some(p) = k.and_then(|k| k.get_value::<String, _>("{374DE290-123F-4565-9164-39C4925E467B}").ok()) {
            return Some(PathBuf::from(p));
        }
    }
    home().map(|h| h.join("Downloads"))
}

fn rules() -> Vec<Rule> {
    let mut r = Vec::new();
    if let Some(d) = downloads() {
        r.push(Rule { key: "old_downloads", safe: false, admin: false, targets: vec![Target::OldLarge(d, 100 * 1024 * 1024, DAY * 180)] });
    }
    #[cfg(windows)]
    {
        let local = env_path("LOCALAPPDATA");
        let windir = env_path("SystemRoot").unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let programdata = env_path("ProgramData").unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
        if let Some(t) = env_path("TEMP") {
            r.push(Rule { key: "user_temp", safe: true, admin: false, targets: vec![Target::Contents(t, DAY)] });
        }
        r.push(Rule { key: "win_temp", safe: true, admin: true, targets: vec![Target::Contents(windir.join("Temp"), DAY)] });
        if let Some(l) = &local {
            let explorer = l.join(r"Microsoft\Windows\Explorer");
            r.push(Rule { key: "thumbs", safe: true, admin: false, targets: vec![Target::Files(explorer, "thumbcache_", ".db")] });
        }
        let mut crash = vec![
            Target::Contents(windir.join("Minidump"), Duration::ZERO),
            Target::File(windir.join("MEMORY.DMP")),
            Target::Contents(programdata.join(r"Microsoft\Windows\WER\ReportArchive"), Duration::ZERO),
            Target::Contents(programdata.join(r"Microsoft\Windows\WER\ReportQueue"), Duration::ZERO),
        ];
        if let Some(l) = &local {
            crash.push(Target::Contents(l.join("CrashDumps"), Duration::ZERO));
            crash.push(Target::Contents(l.join(r"Microsoft\Windows\WER\ReportArchive"), Duration::ZERO));
            crash.push(Target::Contents(l.join(r"Microsoft\Windows\WER\ReportQueue"), Duration::ZERO));
        }
        r.push(Rule { key: "crash", safe: true, admin: false, targets: crash });
        // Games rebuild these on the next launch, which can stutter for a minute: review, not safe
        if let Some(l) = &local {
            r.push(Rule {
                key: "shaders",
                safe: false,
                admin: false,
                targets: ["D3DSCache", r"NVIDIA\DXCache", r"NVIDIA\GLCache", r"AMD\DxCache", r"AMD\DxcCache", r"Intel\ShaderCache"]
                    .iter()
                    .map(|p| Target::Contents(l.join(p), Duration::ZERO))
                    .collect(),
            });
        }
        r.push(Rule { key: "wu_cache", safe: false, admin: true, targets: vec![Target::Contents(windir.join(r"SoftwareDistribution\Download"), DAY * 3)] });
        r.push(Rule { key: "recycle", safe: false, admin: false, targets: vec![Target::RecycleBin] });
    }
    #[cfg(target_os = "macos")]
    if let Some(h) = home() {
        r.push(Rule { key: "logs", safe: true, admin: false, targets: vec![Target::Contents(h.join("Library/Logs"), DAY)] });
        r.push(Rule { key: "app_cache", safe: false, admin: false, targets: vec![Target::Contents(h.join("Library/Caches"), DAY)] });
        r.push(Rule { key: "recycle", safe: false, admin: false, targets: vec![Target::RecycleBin] });
    }
    #[cfg(target_os = "linux")]
    if let Some(h) = home() {
        let cache = env_path("XDG_CACHE_HOME").unwrap_or_else(|| h.join(".cache"));
        r.push(Rule { key: "thumbs", safe: true, admin: false, targets: vec![Target::Contents(cache.join("thumbnails"), Duration::ZERO)] });
        r.push(Rule { key: "app_cache", safe: false, admin: false, targets: vec![Target::Contents(cache, DAY)] });
        r.push(Rule { key: "recycle", safe: false, admin: false, targets: vec![Target::RecycleBin] });
    }
    r
}

fn old_enough(p: &Path, min_age: Duration) -> bool {
    if min_age.is_zero() {
        return true;
    }
    let Ok(m) = std::fs::symlink_metadata(p) else { return false };
    let modified = m.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    SystemTime::now().duration_since(modified).map(|d| d >= min_age).unwrap_or(false)
}

/// Entries a target would remove.
fn entries(t: &Target) -> Vec<PathBuf> {
    match t {
        Target::Contents(dir, age) => std::fs::read_dir(dir)
            .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| old_enough(p, *age)).collect())
            .unwrap_or_default(),
        Target::Files(dir, prefix, suffix) => std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| {
                        let n = e.file_name().to_string_lossy().to_lowercase();
                        n.starts_with(prefix) && n.ends_with(suffix)
                    })
                    .map(|e| e.path())
                    .collect()
            })
            .unwrap_or_default(),
        Target::File(f) => if f.is_file() { vec![f.clone()] } else { vec![] },
        Target::OldLarge(dir, min, age) => std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.metadata().is_ok_and(|m| m.is_file() && m.len() >= *min))
                    .map(|e| e.path())
                    .filter(|p| old_enough(p, *age))
                    .collect()
            })
            .unwrap_or_default(),
        Target::RecycleBin => vec![],
    }
}

pub fn scan() -> Vec<JunkItem> {
    let mut out: Vec<JunkItem> = rules()
        .into_iter()
        .map(|r| {
            let mut item = JunkItem { key: r.key, safe: r.safe, admin: r.admin, size: 0, count: 0, sample: vec![] };
            for t in &r.targets {
                if let Target::RecycleBin = t {
                    let (size, count) = recycle_bin_size();
                    item.size += size;
                    item.count += count;
                    continue;
                }
                for e in entries(t) {
                    item.size += sysutil::size_of(&e);
                    item.count += 1;
                    if item.sample.len() < 3 {
                        item.sample.push(e.to_string_lossy().into_owned());
                    }
                }
            }
            item
        })
        .filter(|i| i.size > 0 || i.count > 0)
        .collect();
    out.sort_by_key(|i| std::cmp::Reverse(i.size));
    out
}

pub fn clean(keys: &[String]) -> CleanOutcome {
    let mut out = CleanOutcome::default();
    for r in rules().into_iter().filter(|r| keys.iter().any(|k| k == r.key)) {
        for t in &r.targets {
            if let Target::RecycleBin = t {
                let (size, _) = recycle_bin_size();
                match empty_recycle_bin() {
                    Ok(()) => {
                        out.freed += size;
                        out.cleaned += 1;
                    }
                    Err(e) => out.failed.push(e),
                }
                continue;
            }
            if let Target::OldLarge(..) = t {
                for e in entries(t) {
                    let size = sysutil::size_of(&e);
                    match trash::delete(&e) {
                        Ok(()) => {
                            out.freed += size;
                            out.cleaned += 1;
                        }
                        Err(err) => out.failed.push(format!("{}: {err}", e.display())),
                    }
                }
                continue;
            }
            for e in entries(t) {
                let is_dir = std::fs::symlink_metadata(&e).map(|m| m.is_dir()).unwrap_or(false);
                if is_dir {
                    let (freed, locked) = sysutil::empty_dir(&e);
                    out.freed += freed;
                    out.locked += locked;
                    if locked == 0 && std::fs::remove_dir(&e).is_ok() {
                        out.cleaned += 1;
                    }
                } else {
                    match sysutil::remove_all(&e) {
                        Ok(freed) => {
                            out.freed += freed;
                            out.cleaned += 1;
                        }
                        Err(_) => out.locked += 1,
                    }
                }
            }
        }
    }
    out
}

#[cfg(windows)]
fn recycle_bin_size() -> (u64, u64) {
    use windows_sys::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};
    let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, i64Size: 0, i64NumItems: 0 };
    // SAFETY: a null root path queries every drive; `info` is a valid, sized out-struct.
    let hr = unsafe { SHQueryRecycleBinW(std::ptr::null(), &mut info) };
    if hr == 0 { (info.i64Size.max(0) as u64, info.i64NumItems.max(0) as u64) } else { (0, 0) }
}

#[cfg(windows)]
fn empty_recycle_bin() -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::{SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND};
    // SAFETY: null window and root path (all drives); flags suppress every dialog.
    let hr = unsafe { SHEmptyRecycleBinW(std::ptr::null_mut(), std::ptr::null(), SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND) };
    // S_OK, or E_UNEXPECTED (0x8000FFFF) when the bin was already empty
    if hr == 0 || hr as u32 == 0x8000_FFFF { Ok(()) } else { Err(format!("Recycle Bin: error {:#x}", hr as u32)) }
}

#[cfg(not(windows))]
fn trash_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    return home().map(|h| h.join(".Trash"));
    #[cfg(not(target_os = "macos"))]
    return env_path("XDG_DATA_HOME").or_else(|| home().map(|h| h.join(".local/share"))).map(|d| d.join("Trash"));
}

#[cfg(not(windows))]
fn recycle_bin_size() -> (u64, u64) {
    let Some(t) = trash_dir() else { return (0, 0) };
    // freedesktop Trash keeps the data in files/ and the metadata in info/
    let files = if cfg!(target_os = "macos") { t } else { t.join("files") };
    let count = std::fs::read_dir(&files).map(|rd| rd.count() as u64).unwrap_or(0);
    (sysutil::size_of(&files), count)
}

#[cfg(not(windows))]
fn empty_recycle_bin() -> Result<(), String> {
    let Some(t) = trash_dir() else { return Ok(()) };
    if cfg!(target_os = "macos") {
        sysutil::empty_dir(&t);
    } else {
        sysutil::empty_dir(&t.join("files"));
        sysutil::empty_dir(&t.join("info"));
    }
    Ok(())
}
