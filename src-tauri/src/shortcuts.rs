//! Broken shortcuts (Windows `.lnk`, Linux `.desktop`) in the places people see them: desktop,
//! Start menu, taskbar / Quick Launch; and empty folders from the scanned tree.
//!
//! A shortcut only counts as broken when its target is on a drive that is present and the target
//! itself is gone. Network targets, unplugged drives and shortcuts without a plain file target
//! (shell items, store apps, MSI "advertised" shortcuts) are never reported.

use crate::scanner::{Kind, Tree, NO_PARENT};
// Which of these are needed depends on the platform's shortcut locations
#[allow(unused_imports)]
use crate::sysutil::{env_path, home};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone)]
pub struct BrokenShortcut {
    pub path: String,
    pub name: String,
    pub target: String,
    /// desktop, start_menu, taskbar, applications
    pub location: &'static str,
    /// In an all-users location: removing it needs administrator rights.
    pub admin: bool,
}

/// (folder, location key, all users?, recurse?)
fn locations() -> Vec<(PathBuf, &'static str, bool, bool)> {
    let mut v = Vec::new();
    #[cfg(windows)]
    {
        let desktop = shell_folder("Desktop").or_else(|| home().map(|h| h.join("Desktop")));
        if let Some(d) = desktop {
            v.push((d, "desktop", false, false));
        }
        if let Some(p) = env_path("PUBLIC") {
            v.push((p.join("Desktop"), "desktop", true, false));
        }
        if let Some(a) = env_path("APPDATA") {
            v.push((a.join(r"Microsoft\Windows\Start Menu"), "start_menu", false, true));
            v.push((a.join(r"Microsoft\Internet Explorer\Quick Launch"), "taskbar", false, true));
        }
        if let Some(p) = env_path("ProgramData") {
            v.push((p.join(r"Microsoft\Windows\Start Menu"), "start_menu", true, true));
        }
    }
    #[cfg(target_os = "linux")]
    if let Some(h) = home() {
        let data = env_path("XDG_DATA_HOME").unwrap_or_else(|| h.join(".local/share"));
        v.push((data.join("applications"), "applications", false, true));
        v.push((h.join("Desktop"), "desktop", false, false));
    }
    v
}

/// The real location of a user shell folder (the desktop may be redirected to OneDrive).
#[cfg(windows)]
fn shell_folder(name: &str) -> Option<PathBuf> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let k = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders").ok()?;
    k.get_value::<String, _>(name).ok().map(PathBuf::from)
}

fn walk(dir: &Path, recurse: bool, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() && recurse {
            walk(&p, recurse, ext, out);
        } else if ft.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case(ext)) {
            out.push(p);
        }
    }
}

pub fn find_broken() -> Vec<BrokenShortcut> {
    let ext = if cfg!(windows) { "lnk" } else { "desktop" };
    let mut out = Vec::new();
    for (dir, location, admin, recurse) in locations() {
        let mut files = Vec::new();
        walk(&dir, recurse, ext, &mut files);
        for f in files {
            let Some(target) = target_of(&f) else { continue };
            if is_broken(&target) {
                out.push(BrokenShortcut {
                    name: f.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
                    path: f.to_string_lossy().into_owned(),
                    target: target.to_string_lossy().into_owned(),
                    location,
                    admin,
                });
            }
        }
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    out
}

fn is_broken(target: &Path) -> bool {
    let s = target.to_string_lossy();
    if s.is_empty() || s.starts_with("\\\\") || s.contains('%') || s.contains("://") {
        return false;
    }
    if !target.is_absolute() {
        return false;
    }
    // Unplugged USB drive or unmounted volume: the shortcut may well work again later
    let root = target.ancestors().last().unwrap_or(target);
    let drive = target.components().take(1).collect::<PathBuf>();
    if !drive.exists() || !root.exists() {
        return false;
    }
    !target.exists()
}

/// Move confirmed-broken shortcuts to the Recycle Bin. Returns (removed, failures).
pub fn remove(paths: &[String]) -> (usize, Vec<String>) {
    let ext = if cfg!(windows) { "lnk" } else { "desktop" };
    let mut ok = 0;
    let mut failed = Vec::new();
    for p in paths.iter().map(PathBuf::from) {
        // Only shortcut files that are still broken: this command never deletes anything else
        let still_broken = p.extension().is_some_and(|x| x.eq_ignore_ascii_case(ext)) && target_of(&p).is_some_and(|t| is_broken(&t));
        if !still_broken {
            failed.push(format!("{}: not a broken shortcut", p.display()));
            continue;
        }
        match trash::delete(&p) {
            Ok(()) => ok += 1,
            Err(e) => failed.push(format!("{}: {e}", p.display())),
        }
    }
    (ok, failed)
}

// ---------- shortcut targets ----------

#[cfg(windows)]
fn target_of(lnk: &Path) -> Option<PathBuf> {
    let data = std::fs::read(lnk).ok()?;
    parse_lnk(&data).map(PathBuf::from)
}

#[cfg(not(windows))]
fn target_of(desktop: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(desktop).ok()?;
    // TryExec names the program precisely; Exec may carry arguments and field codes
    let mut try_exec = None;
    let mut exec = None;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("TryExec=") {
            try_exec = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("Exec=") {
            exec.get_or_insert_with(|| v.trim().to_string());
        }
    }
    let cmd = try_exec.or_else(|| exec.map(|e| e.trim_matches('"').split_whitespace().next().unwrap_or("").trim_matches('"').to_string()))?;
    // `env FOO=1 app` or a bare name resolved through PATH: not something we can judge
    let p = PathBuf::from(cmd);
    (p.is_absolute() && !p.ends_with("env")).then_some(p)
}

fn u16_at(b: &[u8], o: usize) -> Option<usize> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?) as usize)
}
fn u32_at(b: &[u8], o: usize) -> Option<usize> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?) as usize)
}

/// NUL-terminated UTF-16 string starting at `o`.
fn utf16z(b: &[u8], o: usize, max_units: usize) -> Option<String> {
    let mut units = Vec::new();
    let mut i = o;
    while units.len() < max_units {
        let u = u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?);
        if u == 0 {
            break;
        }
        units.push(u);
        i += 2;
    }
    Some(String::from_utf16_lossy(&units))
}

/// NUL-terminated string in the system ANSI code page starting at `o`.
#[cfg(windows)]
fn ansiz(b: &[u8], o: usize) -> Option<String> {
    use windows_sys::Win32::Globalization::{MultiByteToWideChar, CP_ACP};
    let rest = b.get(o..)?;
    let raw = &rest[..rest.iter().position(|c| *c == 0)?];
    if raw.is_empty() {
        return Some(String::new());
    }
    let mut wide = vec![0u16; raw.len()];
    // SAFETY: buffers are valid for the lengths passed; ANSI code pages need at most one UTF-16
    // unit per input byte.
    let n = unsafe { MultiByteToWideChar(CP_ACP, 0, raw.as_ptr(), raw.len() as i32, wide.as_mut_ptr(), wide.len() as i32) };
    Some(String::from_utf16_lossy(&wide[..n.max(0) as usize]))
}

/// Target path of a Shell Link file (MS-SHLLINK): LinkInfo's local base path, or the expanded
/// environment-variable target. `None` when the link has no plain file target.
#[cfg(windows)]
pub fn parse_lnk(b: &[u8]) -> Option<String> {
    const HAS_ID_LIST: usize = 0x1;
    const HAS_LINK_INFO: usize = 0x2;
    const STRING_FLAGS: [usize; 5] = [0x4, 0x8, 0x10, 0x20, 0x40];
    const IS_UNICODE: usize = 0x80;
    if u32_at(b, 0)? != 0x4C {
        return None;
    }
    let flags = u32_at(b, 20)?;
    let mut o = 0x4C;
    if flags & HAS_ID_LIST != 0 {
        o += 2 + u16_at(b, o)?;
    }
    let mut target = None;
    if flags & HAS_LINK_INFO != 0 {
        let info = o;
        let size = u32_at(b, info)?;
        let header = u32_at(b, info + 4)?;
        let info_flags = u32_at(b, info + 8)?;
        // VolumeIDAndLocalBasePath: a path on a local drive
        if info_flags & 1 != 0 {
            let base = if header >= 0x24 {
                let off = u32_at(b, info + 28)?;
                utf16z(b, info + off, 32_768)?
            } else {
                ansiz(b, info + u32_at(b, info + 16)?)?
            };
            let suffix = if header >= 0x24 { utf16z(b, info + u32_at(b, info + 32)?, 32_768)? } else { ansiz(b, info + u32_at(b, info + 24)?)? };
            if !base.is_empty() {
                target = Some(if suffix.is_empty() { base } else { format!("{}\\{suffix}", base.trim_end_matches('\\')) });
            }
        }
        o += size;
    }
    if target.is_some() {
        return target;
    }
    // No LinkInfo: skip the string section and look for an EnvironmentVariableDataBlock
    let unit = if flags & IS_UNICODE != 0 { 2 } else { 1 };
    for f in STRING_FLAGS {
        if flags & f != 0 {
            o += 2 + u16_at(b, o)? * unit;
        }
    }
    while let Some(size) = u32_at(b, o) {
        if size < 4 {
            break;
        }
        if u32_at(b, o + 4)? == 0xA000_0001 {
            let raw = utf16z(b, o + 8 + 260, 260)?;
            return expand_env(&raw);
        }
        o += size;
    }
    None
}

/// `%ProgramFiles%\App\app.exe` → `C:\Program Files\App\app.exe`; `None` if a variable is unknown.
#[cfg(windows)]
fn expand_env(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after.find('%')?;
        out.push_str(&std::env::var(&after[..end]).ok()?);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    (!out.is_empty()).then_some(out)
}

// ---------- empty folders ----------

#[derive(Serialize, Clone)]
pub struct EmptyFolder {
    pub id: u32,
    pub path: String,
    /// Empty sub-folders inside it (they go with it).
    pub nested: u64,
}

/// Folders people and the system expect to exist even when empty.
const KEEP_NAMES: &[&str] = &[
    "desktop", "documents", "downloads", "music", "pictures", "videos", "favorites", "links", "contacts", "saved games",
    "searches", "3d objects", "onedrive", "public", "templates", "movies", "applications", "library",
];
/// Whole areas that are skipped: apps keep (and need) empty folders there.
const SKIP_PARTS: &[&str] = &[
    "windows", "program files", "program files (x86)", "programdata", "appdata", "$recycle.bin", "system volume information",
    "recovery", "msocache", "node_modules", "library", "proc", "sys", "dev", "usr", "etc", "var", "bin", "sbin", "lib", "opt", "snap",
    "boot", "system", "private", "cores",
];

/// Top-most folders whose whole subtree holds no files.
pub fn empty_folders(t: &Tree, limit: usize) -> Vec<EmptyFolder> {
    let mut out = Vec::new();
    let mut stack: Vec<u32> = vec![0];
    while let Some(id) = stack.pop() {
        let n = &t.nodes[id as usize];
        if n.kind != Kind::Dir {
            continue;
        }
        let name = n.name.to_lowercase();
        if id != 0 && (name.starts_with('.') || name.starts_with('$') || SKIP_PARTS.contains(&name.as_str())) {
            continue;
        }
        if id != 0 && n.files == 0 && n.parent != NO_PARENT && !KEEP_NAMES.contains(&name.as_str()) {
            let path = t.path_of(id);
            if !crate::is_protected(&path) {
                out.push(EmptyFolder { id, path: path.to_string_lossy().into_owned(), nested: n.dirs });
                if out.len() >= limit {
                    break;
                }
            }
            continue;
        }
        stack.extend(n.children.iter().copied());
    }
    out.sort_by_key(|a| a.path.to_lowercase());
    out
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// Minimal link: header + LinkInfo with a Unicode local base path.
    fn lnk_to(path: &str) -> Vec<u8> {
        let mut b = vec![0u8; 0x4C];
        b[0] = 0x4C;
        b[20] = 0x02 | 0x80; // HasLinkInfo | IsUnicode
        let wide: Vec<u8> = path.encode_utf16().chain([0]).flat_map(|u| u.to_le_bytes()).collect();
        let header = 0x24usize;
        let base_off = header;
        let suffix_off = base_off + wide.len();
        let size = suffix_off + 2;
        let mut info = vec![0u8; size];
        info[0..4].copy_from_slice(&(size as u32).to_le_bytes());
        info[4..8].copy_from_slice(&(header as u32).to_le_bytes());
        info[8..12].copy_from_slice(&1u32.to_le_bytes());
        info[28..32].copy_from_slice(&(base_off as u32).to_le_bytes());
        info[32..36].copy_from_slice(&(suffix_off as u32).to_le_bytes());
        info[base_off..suffix_off].copy_from_slice(&wide);
        b.extend(info);
        b
    }

    #[test]
    fn reads_link_targets() {
        assert_eq!(parse_lnk(&lnk_to(r"C:\Tools\app.exe")).as_deref(), Some(r"C:\Tools\app.exe"));
        assert_eq!(parse_lnk(b"not a link"), None);
    }

    #[test]
    fn judges_targets_conservatively() {
        let sys = std::env::var("SystemRoot").unwrap();
        assert!(!is_broken(Path::new(&format!(r"{sys}\notepad.exe"))));
        assert!(is_broken(Path::new(&format!(r"{sys}\tessera-surely-missing-{}.exe", std::process::id()))));
        assert!(!is_broken(Path::new(r"\\server\share\app.exe")));
        assert!(!is_broken(Path::new(r"%ProgramFiles%\x.exe")));
    }
}
