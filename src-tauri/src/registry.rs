//! Registry cleaner (Windows). Deliberately conservative: an entry is only reported when it points
//! at a file or folder that no longer exists, in places Windows itself only uses as lists of
//! references (uninstall entries, App Paths, startup commands, shared DLL counts, installer folders,
//! the MUI cache). Nothing else in the registry is ever touched.
//!
//! Every fix is preceded by a full backup of what is removed (keys with all subkeys and values),
//! written to `registry-backups/<time>.json`, so it can be undone from the app.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use winreg::enums::*;
use winreg::{RegKey, RegValue};

const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
const UNINSTALL32: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";
const APP_PATHS: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths";
const RUN: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";
const RUN32: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
const SHARED_DLLS: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\SharedDLLs";
const INSTALLER_FOLDERS: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Installer\Folders";
const MUI_CACHE: &str = r"Software\Classes\Local Settings\Software\Microsoft\Windows\Shell\MuiCache";

#[derive(Serialize, Clone)]
pub struct Issue {
    /// Stable id: `<hive>\<key>` or `<hive>\<key>|<value>`.
    pub id: String,
    /// uninstall, app_paths, startup, shared_dlls, installer_folders, mui_cache
    pub cat: &'static str,
    /// Human-readable entry name.
    pub name: String,
    /// The missing file or folder it refers to.
    pub missing: String,
    /// HKLM entry: fixing it needs administrator rights.
    pub admin: bool,
}

fn hive_name(machine: bool) -> &'static str {
    if machine { "HKLM" } else { "HKCU" }
}

fn hive(machine: bool) -> RegKey {
    RegKey::predef(if machine { HKEY_LOCAL_MACHINE } else { HKEY_CURRENT_USER })
}

fn expand(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            out.push('%');
            rest = after;
            continue;
        };
        match std::env::var(&after[..end]) {
            Ok(v) => out.push_str(&v),
            Err(_) => out.push_str(&rest[start..start + end + 2]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Executable path at the start of a command line: `"C:\a b\x.exe" /s` or `C:\x.exe -y`.
/// `None` when it isn't an absolute local path (rundll32 calls, bare names resolved via PATH, …).
pub fn command_path(cmd: &str) -> Option<PathBuf> {
    let cmd = expand(cmd.trim());
    let path = if let Some(rest) = cmd.strip_prefix('"') {
        rest.split('"').next()?.to_string()
    } else {
        // Unquoted paths may contain spaces: take the longest prefix ending in .exe
        let lower = cmd.to_lowercase();
        match lower.find(".exe") {
            Some(i) => cmd[..i + 4].to_string(),
            None => cmd.split_whitespace().next()?.to_string(),
        }
    };
    let p = PathBuf::from(path.trim());
    let s = p.to_string_lossy().to_lowercase();
    let local = s.len() > 3 && s.as_bytes()[1] == b':' && s.as_bytes()[2] == b'\\';
    let launcher = ["rundll32", "msiexec", "cmd.exe", "powershell", "wscript", "cscript"].iter().any(|l| s.contains(l));
    (local && !launcher).then_some(p)
}

/// The path is really gone: its drive exists, it doesn't. Unplugged drives never count.
fn gone(p: &Path) -> bool {
    let s = p.to_string_lossy();
    if s.len() < 3 {
        return false;
    }
    let root = PathBuf::from(&s[..3]);
    root.exists() && !p.exists()
}

pub fn scan() -> Vec<Issue> {
    let mut out = Vec::new();
    // Uninstall entries whose uninstaller *and* install folder are both gone
    for (machine, key) in [(true, UNINSTALL), (true, UNINSTALL32), (false, UNINSTALL)] {
        let Ok(root) = hive(machine).open_subkey(key) else { continue };
        for sub in root.enum_keys().flatten() {
            let Ok(k) = root.open_subkey(&sub) else { continue };
            let uninstall: String = k.get_value("UninstallString").unwrap_or_default();
            let location: String = k.get_value("InstallLocation").unwrap_or_default();
            let name: String = k.get_value("DisplayName").unwrap_or_else(|_| sub.clone());
            // MSI products are owned by Windows Installer and never touched here
            if uninstall.to_lowercase().contains("msiexec") || k.get_value::<u32, _>("WindowsInstaller").unwrap_or(0) == 1 {
                continue;
            }
            let Some(exe) = command_path(&uninstall) else { continue };
            let loc_gone = location.trim().is_empty() || gone(Path::new(expand(location.trim()).trim_matches('"')));
            if gone(&exe) && loc_gone {
                out.push(Issue { id: format!(r"{}\{key}\{sub}", hive_name(machine)), cat: "uninstall", name, missing: exe.to_string_lossy().into_owned(), admin: machine });
            }
        }
    }
    // App Paths: "Run" box / ShellExecute aliases for programs that are gone
    for machine in [true, false] {
        let Ok(root) = hive(machine).open_subkey(APP_PATHS) else { continue };
        for sub in root.enum_keys().flatten() {
            let Ok(k) = root.open_subkey(&sub) else { continue };
            let target: String = k.get_value("").unwrap_or_default();
            if let Some(p) = command_path(&target).filter(|p| gone(p)) {
                out.push(Issue { id: format!(r"{}\{APP_PATHS}\{sub}", hive_name(machine)), cat: "app_paths", name: sub, missing: p.to_string_lossy().into_owned(), admin: machine });
            }
        }
    }
    // Startup commands for programs that are gone
    for (machine, key) in [(false, RUN), (true, RUN), (true, RUN32)] {
        let Ok(k) = hive(machine).open_subkey(key) else { continue };
        for (name, v) in k.enum_values().flatten() {
            if let Some(p) = command_path(&v.to_string()).filter(|p| gone(p)) {
                out.push(Issue { id: format!(r"{}\{key}|{name}", hive_name(machine)), cat: "startup", name, missing: p.to_string_lossy().into_owned(), admin: machine });
            }
        }
    }
    // Shared DLL reference counts and installer folder records: the value name is the path.
    // Records inside the Windows folder belong to Windows' own servicing and are left alone.
    let windir = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()).to_lowercase();
    for (key, cat) in [(SHARED_DLLS, "shared_dlls"), (INSTALLER_FOLDERS, "installer_folders")] {
        let Ok(k) = hive(true).open_subkey(key) else { continue };
        for (name, _) in k.enum_values().flatten() {
            let p = PathBuf::from(expand(&name));
            if gone(&p) && !p.to_string_lossy().to_lowercase().starts_with(&windir) {
                out.push(Issue { id: format!(r"HKLM\{key}|{name}"), cat, name: name.clone(), missing: name, admin: true });
            }
        }
    }
    // MUI cache: display names Explorer remembered for programs ("C:\x.exe.FriendlyAppName")
    if let Ok(k) = hive(false).open_subkey(MUI_CACHE) {
        for (name, _) in k.enum_values().flatten() {
            let path = name.rsplit_once(".FriendlyAppName").or_else(|| name.rsplit_once(".ApplicationCompany")).map(|(p, _)| p).unwrap_or("");
            if path.len() > 3 && gone(Path::new(path)) {
                out.push(Issue { id: format!(r"HKCU\{MUI_CACHE}|{name}"), cat: "mui_cache", name: name.clone(), missing: path.to_string(), admin: false });
            }
        }
    }
    out
}

// ---------- backup and fix ----------

#[derive(Serialize, Deserialize, Clone)]
struct SavedValue {
    name: String,
    vtype: u32,
    bytes: Vec<u8>,
}

#[derive(Serialize, Deserialize, Clone)]
struct SavedKey {
    /// Path relative to the hive.
    path: String,
    values: Vec<SavedValue>,
    subkeys: Vec<SavedKey>,
}

#[derive(Serialize, Deserialize, Clone)]
enum Saved {
    Key { machine: bool, key: SavedKey },
    Value { machine: bool, key: String, value: SavedValue },
}

#[derive(Serialize, Deserialize)]
struct BackupFile {
    created: u64,
    items: Vec<Saved>,
}

fn save_key(k: &RegKey, path: &str) -> SavedKey {
    let values = k.enum_values().flatten().map(|(name, v)| SavedValue { name, vtype: v.vtype as u32, bytes: v.bytes.to_vec() }).collect();
    let subkeys = k
        .enum_keys()
        .flatten()
        .filter_map(|s| k.open_subkey(&s).ok().map(|sk| save_key(&sk, &format!(r"{path}\{s}"))))
        .collect();
    SavedKey { path: path.to_string(), values, subkeys }
}

fn vtype(n: u32) -> RegType {
    match n {
        1 => REG_SZ,
        2 => REG_EXPAND_SZ,
        3 => REG_BINARY,
        4 => REG_DWORD,
        7 => REG_MULTI_SZ,
        11 => REG_QWORD,
        _ => REG_BINARY,
    }
}

fn restore_key(machine: bool, k: &SavedKey) -> Result<(), String> {
    let (key, _) = hive(machine).create_subkey(&k.path).map_err(|e| format!("{}: {e}", k.path))?;
    for v in &k.values {
        key.set_raw_value(&v.name, &RegValue { bytes: v.bytes.clone().into(), vtype: vtype(v.vtype) }).map_err(|e| format!("{}\\{}: {e}", k.path, v.name))?;
    }
    for s in &k.subkeys {
        restore_key(machine, s)?;
    }
    Ok(())
}

fn parse_id(id: &str) -> Option<(bool, String, Option<String>)> {
    let (hive, rest) = id.split_once('\\')?;
    let machine = match hive {
        "HKLM" => true,
        "HKCU" => false,
        _ => return None,
    };
    let (key, value) = match rest.split_once('|') {
        Some((k, v)) => (k.to_string(), Some(v.to_string())),
        None => (rest.to_string(), None),
    };
    Some((machine, key, value))
}

fn backups_dir(base: &Path) -> PathBuf {
    base.join("registry-backups")
}

#[derive(Serialize, Default)]
pub struct FixOutcome {
    pub fixed: usize,
    pub failed: Vec<String>,
    /// Backup file name, for undo.
    pub backup: Option<String>,
}

/// Back up and remove the given issues. Ids are re-checked against a fresh scan, so only
/// entries that are still broken can be removed.
pub fn fix(base: &Path, ids: &[String]) -> Result<FixOutcome, String> {
    let current = scan();
    let todo: Vec<String> = current.iter().filter(|i| ids.contains(&i.id)).map(|i| i.id.clone()).collect();
    backup_and_delete(base, &todo)
}

/// Back up, then delete registry keys (`HKCU\path`) or values (`HKCU\path|name`). Shared with the
/// uninstaller's leftover cleanup, so both can be undone the same way.
pub fn backup_and_delete(base: &Path, ids: &[String]) -> Result<FixOutcome, String> {
    let mut items = Vec::new();
    for id in ids {
        let Some((machine, key, value)) = parse_id(id) else { continue };
        let Ok(k) = hive(machine).open_subkey(&key) else { continue };
        items.push(match value {
            Some(v) => match k.get_raw_value(&v) {
                Ok(raw) => Saved::Value { machine, key, value: SavedValue { name: v, vtype: raw.vtype as u32, bytes: raw.bytes.to_vec() } },
                Err(_) => continue,
            },
            None => Saved::Key { machine, key: save_key(&k, &key) },
        });
    }
    let mut out = FixOutcome::default();
    if items.is_empty() {
        return Ok(out);
    }
    std::fs::create_dir_all(backups_dir(base)).map_err(|e| e.to_string())?;
    let created = crate::sysutil::now();
    let name = format!("{created}.json");
    let file = BackupFile { created, items: items.clone() };
    std::fs::write(backups_dir(base).join(&name), serde_json::to_vec(&file).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    out.backup = Some(name);

    for s in items {
        let r = match &s {
            Saved::Value { machine, key, value } => hive(*machine)
                .open_subkey_with_flags(key, KEY_SET_VALUE)
                .and_then(|k| k.delete_value(&value.name))
                .map_err(|e| format!(r"{}\{key}\{}: {e}", hive_name(*machine), value.name)),
            Saved::Key { machine, key } => {
                let (parent, leaf) = key.path.rsplit_once('\\').unwrap_or(("", &key.path));
                hive(*machine)
                    .open_subkey_with_flags(parent, KEY_ALL_ACCESS)
                    .and_then(|p| p.delete_subkey_all(leaf))
                    .map_err(|e| format!(r"{}\{}: {e}", hive_name(*machine), key.path))
            }
        };
        match r {
            Ok(()) => out.fixed += 1,
            Err(e) => out.failed.push(if e.contains("os error 5") { format!("{e} (administrator rights needed)") } else { e }),
        }
    }
    Ok(out)
}

#[derive(Serialize)]
pub struct BackupInfo {
    pub name: String,
    pub created: u64,
    pub items: usize,
}

pub fn backups(base: &Path) -> Vec<BackupInfo> {
    let mut v: Vec<BackupInfo> = std::fs::read_dir(backups_dir(base))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let f: BackupFile = serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok()?;
            Some(BackupInfo { name: e.file_name().to_string_lossy().into_owned(), created: f.created, items: f.items.len() })
        })
        .collect();
    v.sort_by_key(|b| std::cmp::Reverse(b.created));
    v
}

/// Write a backup back into the registry, then delete the backup file.
pub fn undo(base: &Path, name: &str) -> Result<usize, String> {
    if name.contains(['/', '\\']) || !name.ends_with(".json") {
        return Err("invalid backup".into());
    }
    let path = backups_dir(base).join(name);
    let f: BackupFile = serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let mut restored = 0;
    for s in &f.items {
        match s {
            Saved::Key { machine, key } => restore_key(*machine, key)?,
            Saved::Value { machine, key, value } => {
                let (k, _) = hive(*machine).create_subkey(key).map_err(|e| format!("{key}: {e}"))?;
                k.set_raw_value(&value.name, &RegValue { bytes: value.bytes.clone().into(), vtype: vtype(value.vtype) }).map_err(|e| format!(r"{key}\{}: {e}", value.name))?;
            }
        }
        restored += 1;
    }
    let _ = std::fs::remove_file(path);
    Ok(restored)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_executables_from_command_lines() {
        assert_eq!(command_path(r#""C:\Program Files\App\unins000.exe" /SILENT"#), Some(PathBuf::from(r"C:\Program Files\App\unins000.exe")));
        assert_eq!(command_path(r"C:\Program Files\App\app.exe --minimized"), Some(PathBuf::from(r"C:\Program Files\App\app.exe")));
        assert_eq!(command_path(r"MsiExec.exe /X{1234}"), None);
        assert_eq!(command_path(r"rundll32.exe C:\x.dll,Entry"), None);
        assert_eq!(command_path("app.exe"), None);
    }

    /// Round trip through a scratch key under HKCU: backup, delete, undo.
    #[test]
    fn backs_up_and_restores_keys() {
        let path = format!(r"Software\TesseraTest{}", std::process::id());
        let (k, _) = hive(false).create_subkey(format!(r"{path}\Sub")).unwrap();
        k.set_value("Name", &"value").unwrap();
        k.set_value("Count", &7u32).unwrap();
        let saved = save_key(&hive(false).open_subkey(&path).unwrap(), &path);
        hive(false).delete_subkey_all(&path).unwrap();
        assert!(hive(false).open_subkey(&path).is_err());
        restore_key(false, &saved).unwrap();
        let back = hive(false).open_subkey(format!(r"{path}\Sub")).unwrap();
        assert_eq!(back.get_value::<String, _>("Name").unwrap(), "value");
        assert_eq!(back.get_value::<u32, _>("Count").unwrap(), 7);
        hive(false).delete_subkey_all(&path).unwrap();
    }

    #[test]
    fn parses_issue_ids() {
        assert_eq!(parse_id(r"HKCU\Software\X|Val"), Some((false, r"Software\X".into(), Some("Val".into()))));
        assert_eq!(parse_id(r"HKLM\Software\X"), Some((true, r"Software\X".into(), None)));
        assert_eq!(parse_id(r"HKCR\x"), None);
    }
}
