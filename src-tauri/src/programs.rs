//! Software uninstaller (Windows): the installed-programs list from the registry, running a
//! program's own uninstaller, and finding what it left behind (folders and registry keys).
//!
//! The uninstall command is always read from the registry here, never taken from the UI. Leftovers
//! are only proposed, the user picks what goes; folders go to the Recycle Bin and registry keys are
//! backed up first (undo from the registry cleaner's backups).

use crate::sysutil::env_path;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use winreg::enums::*;
use winreg::RegKey;

const ROOTS: &[(bool, &str)] = &[
    (true, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
    (true, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
    (false, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
];

#[derive(Serialize, Deserialize, Clone)]
pub struct Program {
    /// `HKLM\<uninstall key>\<sub>`
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub version: String,
    /// Unix seconds, 0 when unknown.
    pub installed: u64,
    /// Bytes, as reported by the installer (0 when unknown).
    pub size: u64,
    pub location: String,
    /// Installed for all users: the uninstaller usually asks for administrator rights.
    pub machine: bool,
}

fn hive(machine: bool) -> RegKey {
    RegKey::predef(if machine { HKEY_LOCAL_MACHINE } else { HKEY_CURRENT_USER })
}

/// `20250131` → Unix seconds (midnight UTC).
fn parse_date(s: &str) -> u64 {
    let s = s.trim();
    if s.len() != 8 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return 0;
    }
    let (y, m, d): (i64, i64, i64) = (s[..4].parse().unwrap_or(0), s[4..6].parse().unwrap_or(0), s[6..].parse().unwrap_or(0));
    if !(1990..=2100).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return 0;
    }
    // Days from civil (Howard Hinnant's algorithm)
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    ((era * 146_097 + doe - 719_468) * 86_400) as u64
}

pub fn list() -> Vec<Program> {
    let mut out: Vec<Program> = Vec::new();
    for (machine, root) in ROOTS {
        let Ok(r) = hive(*machine).open_subkey(root) else { continue };
        for sub in r.enum_keys().flatten() {
            let Ok(k) = r.open_subkey(&sub) else { continue };
            let s = |n: &str| k.get_value::<String, _>(n).unwrap_or_default().trim().to_string();
            let name = s("DisplayName");
            // Components, updates and hotfixes are part of another entry
            if name.is_empty()
                || s("UninstallString").is_empty()
                || k.get_value::<u32, _>("SystemComponent").unwrap_or(0) == 1
                || !s("ParentKeyName").is_empty()
                || matches!(s("ReleaseType").as_str(), "Update" | "Hotfix" | "Security Update")
            {
                continue;
            }
            let version = s("DisplayVersion");
            if out.iter().any(|p| p.name == name && p.version == version) {
                continue;
            }
            out.push(Program {
                id: format!(r"{}\{root}\{sub}", if *machine { "HKLM" } else { "HKCU" }),
                publisher: s("Publisher"),
                installed: parse_date(&s("InstallDate")),
                size: k.get_value::<u32, _>("EstimatedSize").map(|kb| kb as u64 * 1024).unwrap_or(0),
                location: s("InstallLocation").trim_matches('"').to_string(),
                machine: *machine,
                name,
                version,
            });
        }
    }
    out.sort_by_key(|p| p.name.to_lowercase());
    out
}

fn open(id: &str) -> Option<(bool, String, RegKey)> {
    let (h, path) = id.split_once('\\')?;
    let machine = h == "HKLM";
    // Only entries under the uninstall roots can be run from here
    if !ROOTS.iter().any(|(m, r)| *m == machine && path.starts_with(r)) {
        return None;
    }
    let k = hive(machine).open_subkey(path).ok()?;
    Some((machine, path.to_string(), k))
}

/// Run the program's own uninstaller and wait for it (including the copy of itself many
/// uninstallers relaunch from %TEMP%). Returns whether the entry is gone afterwards.
pub fn uninstall(id: &str) -> Result<bool, String> {
    let (_, _, k) = open(id).ok_or("program not found")?;
    let cmd: String = k.get_value("UninstallString").map_err(|_| "no uninstaller registered")?;
    drop(k);
    let cmd = cmd.trim().to_string();
    // ShellExecute (Start-Process) shows the UAC prompt uninstallers need; CreateProcess would just fail
    let (file, args) = split_command(&cmd);
    let script = if args.is_empty() {
        format!("$p = Start-Process -FilePath {} -Wait -PassThru; exit $p.ExitCode", crate::sysutil::ps_quote(&file))
    } else {
        format!("$p = Start-Process -FilePath {} -ArgumentList {} -Wait -PassThru; exit $p.ExitCode", crate::sysutil::ps_quote(&file), crate::sysutil::ps_quote(&args))
    };
    let out = crate::sysutil::command("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", &script]).output().map_err(|e| e.to_string())?;
    let stderr = crate::sysutil::text(&out.stderr);
    if stderr.contains("canceled by the user") || stderr.contains("1223") {
        return Err("cancelled".into());
    }
    Ok(open(id).is_none())
}

/// `"C:\x\unins000.exe" /S` → (`C:\x\unins000.exe`, `/S`); unquoted paths with spaces end at `.exe`.
fn split_command(cmd: &str) -> (String, String) {
    if let Some(rest) = cmd.strip_prefix('"')
        && let Some((file, args)) = rest.split_once('"')
    {
        return (file.to_string(), args.trim().to_string());
    }
    let lower = cmd.to_lowercase();
    match lower.find(".exe") {
        Some(i) => (cmd[..i + 4].to_string(), cmd[i + 4..].trim().to_string()),
        None => match cmd.split_once(' ') {
            Some((f, a)) => (f.to_string(), a.trim().to_string()),
            None => (cmd.to_string(), String::new()),
        },
    }
}

// ---------- leftovers ----------

#[derive(Serialize, Deserialize, Clone)]
pub struct Leftover {
    /// folder or registry
    pub kind: String,
    /// Folder path, or registry id (`HKCU\Software\Vendor\App`).
    pub path: String,
    pub size: u64,
    pub admin: bool,
}

/// Words that are too generic to match folders on (a program called "Update" must not match
/// every Update folder on the disk).
const GENERIC: &[&str] = &["common", "common files", "update", "updates", "tools", "data", "temp", "cache", "microsoft", "windows", "intel", "amd", "nvidia", "packages", "programs", "app", "apps"];

/// "Foo Bar 2.1 (64-bit)" → "foo bar"; "Contoso, Inc." → "contoso".
fn clean(s: &str) -> String {
    let mut t = s.to_lowercase();
    for noise in ["(64-bit)", "(32-bit)", "(x64)", "(x86)", "x64", "x86", "64-bit", "32-bit", ", inc.", " inc.", " inc", " llc", " ltd.", " ltd", " gmbh", " corporation", " corp.", " co.", " s.a.", " ag"] {
        t = t.replace(noise, "");
    }
    // Trailing version numbers ("2.1.3", "v2", "24.08"), but not names that start with a digit ("7-Zip")
    let is_version = |w: &str| {
        let v = w.strip_prefix('v').unwrap_or(w);
        !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || c == '.')
    };
    let words: Vec<&str> = t.split_whitespace().collect();
    let keep = words.iter().rposition(|w| !is_version(w)).map(|i| i + 1).unwrap_or(0);
    words[..keep].join(" ").trim_matches([',', '-', ' ']).to_string()
}

fn usable(name: &str) -> bool {
    name.chars().filter(|c| c.is_alphanumeric()).count() >= 3 && !GENERIC.contains(&name)
}

fn same(a: &str, b: &str) -> bool {
    let squash = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    !a.is_empty() && squash(a) == squash(b)
}

/// Folders that may hold leftovers.
fn folder_roots() -> Vec<(PathBuf, bool)> {
    let mut v = Vec::new();
    for (var, admin) in [("ProgramFiles", true), ("ProgramFiles(x86)", true), ("ProgramData", true), ("APPDATA", false), ("LOCALAPPDATA", false)] {
        if let Some(p) = env_path(var) {
            v.push((p, admin));
        }
    }
    if let Some(l) = env_path("LOCALAPPDATA") {
        v.push((l.join("Programs"), false));
    }
    v
}

pub fn leftovers(p: &Program) -> Vec<Leftover> {
    let name = clean(&p.name);
    let publisher = clean(&p.publisher);
    let mut out: Vec<Leftover> = Vec::new();
    let add_folder = |path: PathBuf, admin: bool, out: &mut Vec<Leftover>| {
        let s = path.to_string_lossy().into_owned();
        if path.is_dir() && !crate::is_protected_strict(&path) && !out.iter().any(|l| l.path.eq_ignore_ascii_case(&s) || s.to_lowercase().starts_with(&format!("{}\\", l.path.to_lowercase()))) {
            out.push(Leftover { kind: "folder".into(), size: crate::sysutil::size_of(&path), path: s, admin });
        }
    };
    if !p.location.is_empty() {
        let loc = PathBuf::from(&p.location);
        if loc.components().count() > 2 {
            add_folder(loc, p.machine, &mut out);
        }
    }
    if usable(&name) {
        for (root, admin) in folder_roots() {
            let Ok(rd) = std::fs::read_dir(&root) else { continue };
            for e in rd.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())) {
                let dir_name = e.file_name().to_string_lossy().to_string();
                if same(&dir_name, &name) {
                    add_folder(e.path(), admin, &mut out);
                } else if usable(&publisher) && same(&dir_name, &publisher) {
                    // Vendor folder: the program's own folder inside it, or the vendor folder itself
                    // when that program was all it held
                    let kids: Vec<_> = std::fs::read_dir(e.path()).into_iter().flatten().flatten().collect();
                    for k in &kids {
                        if same(&k.file_name().to_string_lossy(), &name) {
                            add_folder(if kids.len() == 1 { e.path() } else { k.path() }, admin, &mut out);
                        }
                    }
                }
            }
        }
    }
    // Registry: the program's settings key, and its uninstall entry if the uninstaller left it
    if usable(&name) {
        for (machine, base) in [(false, r"Software"), (true, r"SOFTWARE"), (true, r"SOFTWARE\WOW6432Node")] {
            let Ok(sw) = hive(machine).open_subkey(base) else { continue };
            for sub in sw.enum_keys().flatten() {
                let hk = if machine { "HKLM" } else { "HKCU" };
                if same(&sub, &name) {
                    out.push(Leftover { kind: "registry".into(), path: format!(r"{hk}\{base}\{sub}"), size: 0, admin: machine });
                } else if usable(&publisher) && same(&sub, &publisher)
                    && let Ok(vk) = sw.open_subkey(&sub)
                {
                    for app in vk.enum_keys().flatten().filter(|a| same(a, &name)) {
                        out.push(Leftover { kind: "registry".into(), path: format!(r"{hk}\{base}\{sub}\{app}"), size: 0, admin: machine });
                    }
                }
            }
        }
    }
    if open(&p.id).is_some() {
        out.push(Leftover { kind: "registry".into(), path: p.id.clone(), size: 0, admin: p.machine });
    }
    out
}

#[derive(Serialize, Default)]
pub struct LeftoverOutcome {
    pub removed: usize,
    pub freed: u64,
    pub failed: Vec<String>,
    pub backup: Option<String>,
}

/// Remove picked leftovers. Each one is re-validated: folders must sit inside a leftover root and
/// not be protected; registry keys must be vendor/app keys or an uninstall entry.
pub fn remove_leftovers(base: &Path, items: &[Leftover]) -> LeftoverOutcome {
    let mut out = LeftoverOutcome::default();
    let roots = folder_roots();
    let mut keys = Vec::new();
    for l in items {
        match l.kind.as_str() {
            "folder" => {
                let p = PathBuf::from(&l.path);
                let inside = roots.iter().any(|(r, _)| p.starts_with(r) && p != *r);
                if !inside || crate::is_protected_strict(&p) || !p.is_dir() {
                    out.failed.push(format!("{}: not a removable leftover", l.path));
                    continue;
                }
                let size = crate::sysutil::size_of(&p);
                match trash::delete(&p) {
                    Ok(()) => {
                        out.removed += 1;
                        out.freed += size;
                    }
                    Err(e) => out.failed.push(format!("{}: {e}", l.path)),
                }
            }
            "registry" => {
                let lower = l.path.to_lowercase();
                let depth = l.path.matches('\\').count();
                let vendor_key = (lower.starts_with(r"hkcu\software\") || lower.starts_with(r"hklm\software\"))
                    && !["\\microsoft", "\\classes", "\\policies", "\\wow6432node\\microsoft", "\\clients", "\\registeredapplications"].iter().any(|bad| lower[13..].starts_with(bad) || lower.contains(&format!("{bad}\\")))
                    && depth >= 2;
                let uninstall_entry = ROOTS.iter().any(|(_, r)| lower.contains(&format!("{}\\", r.to_lowercase())));
                if vendor_key || uninstall_entry {
                    keys.push(l.path.clone());
                } else {
                    out.failed.push(format!("{}: not a removable leftover", l.path));
                }
            }
            _ => {}
        }
    }
    if !keys.is_empty() {
        match crate::registry::backup_and_delete(base, &keys) {
            Ok(r) => {
                out.removed += r.fixed;
                out.failed.extend(r.failed);
                out.backup = r.backup;
            }
            Err(e) => out.failed.push(e),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_names_for_matching() {
        assert_eq!(clean("Foo Bar 2.1.3 (64-bit)"), "foo bar");
        assert_eq!(clean("Contoso, Inc."), "contoso");
        assert_eq!(clean("7-Zip 24.08 (x64)"), "7-zip");
        assert_eq!(clean("Tool v2"), "tool");
        assert!(!usable("update"));
        assert!(same("Foo-Bar", "foo bar"));
    }

    #[test]
    fn parses_install_dates_and_commands() {
        assert_eq!(parse_date("19700101"), 0);
        assert_eq!(parse_date("20240229"), 1_709_164_800);
        assert_eq!(parse_date("garbage"), 0);
        assert_eq!(split_command(r#""C:\A B\unins000.exe" /SILENT"#), (r"C:\A B\unins000.exe".into(), "/SILENT".into()));
        assert_eq!(split_command(r"C:\A B\u.exe --x"), (r"C:\A B\u.exe".into(), "--x".into()));
        assert_eq!(split_command(r"MsiExec.exe /X{1}"), ("MsiExec.exe".into(), "/X{1}".into()));
    }

    #[test]
    fn lists_installed_programs() {
        let all = list();
        assert!(all.iter().all(|p| !p.name.is_empty() && p.id.contains(r"\Uninstall\")));
    }
}
