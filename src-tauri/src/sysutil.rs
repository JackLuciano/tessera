//! Small helpers shared by the system tools: running console programs without a flashing window,
//! decoding their output, PowerShell-as-JSON, and Windows restore points.

use std::process::{Command, Output};

/// A command that never opens a console window (Tessera is a GUI app; a flashing `cmd` looks broken).
pub fn command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// Console output as text: UTF-8 when it is valid UTF-8, otherwise the OEM code page on Windows.
pub fn text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        #[cfg(windows)]
        Err(_) => decode_oem(bytes),
        #[cfg(not(windows))]
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Console programs print in the OEM code page (852 on Hungarian Windows), not UTF-8.
#[cfg(windows)]
pub fn decode_oem(b: &[u8]) -> String {
    use windows_sys::Win32::Globalization::{MultiByteToWideChar, CP_OEMCP};
    if b.is_empty() {
        return String::new();
    }
    let mut wide = vec![0u16; b.len()];
    // SAFETY: input and output buffers are valid for the lengths passed; one byte never yields
    // more than one UTF-16 unit in OEM code pages.
    let n = unsafe { MultiByteToWideChar(CP_OEMCP, 0, b.as_ptr(), b.len() as i32, wide.as_mut_ptr(), wide.len() as i32) };
    String::from_utf16_lossy(&wide[..n.max(0) as usize])
}

/// Run a program and return stdout, or stderr (falling back to stdout) as the error.
pub fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let out = command(program).args(args).output().map_err(|e| format!("{program}: {e}"))?;
    checked(program, out)
}

fn checked(program: &str, out: Output) -> Result<String, String> {
    if out.status.success() {
        return Ok(text(&out.stdout));
    }
    let err = text(&out.stderr);
    let msg = if err.trim().is_empty() { text(&out.stdout) } else { err };
    Err(format!("{program}: {}", msg.trim()))
}

/// Run a Windows PowerShell script and parse its output as JSON. Scripts should end with
/// `ConvertTo-Json -Compress -Depth 4 -InputObject @(...)` so a single result still comes back as a list.
#[cfg(windows)]
pub fn powershell<T: serde::de::DeserializeOwned>(script: &str) -> Result<T, String> {
    let out = powershell_raw(script)?;
    let body = out.trim();
    if body.is_empty() {
        return serde_json::from_str("[]").map_err(|e| e.to_string());
    }
    serde_json::from_str(body).map_err(|e| format!("unexpected PowerShell output ({e}): {}", body.chars().take(200).collect::<String>()))
}

/// Run a Windows PowerShell script, returning its text output. Output is forced to UTF-8.
#[cfg(windows)]
pub fn powershell_raw(script: &str) -> Result<String, String> {
    let full = format!("$ProgressPreference='SilentlyContinue';$ErrorActionPreference='Stop';[Console]::OutputEncoding=[Text.Encoding]::UTF8;{script}");
    run("powershell.exe", &["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &full])
}

/// Quote a string for a single-quoted PowerShell literal.
#[cfg(windows)]
pub fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

#[derive(serde::Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum RestorePoint {
    /// A new restore point was created.
    Created,
    /// Windows already has one from the last 24 hours (it allows only one per day).
    Recent,
    /// Created earlier in this session; not repeated for every change.
    Session,
    /// Needs administrator rights.
    NeedsAdmin,
    /// System Protection is off for the system drive, or the call failed.
    Unavailable,
    /// Not a Windows system.
    #[cfg_attr(windows, allow(dead_code))]
    Unsupported,
}

/// Create a Windows restore point before registry or system changes (once per session).
pub fn restore_point(description: &str) -> RestorePoint {
    #[cfg(windows)]
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        static DONE: AtomicBool = AtomicBool::new(false);
        if DONE.load(Ordering::Relaxed) {
            return RestorePoint::Session;
        }
        if !crate::mft::is_elevated() {
            return RestorePoint::NeedsAdmin;
        }
        // Checkpoint-Computer only warns (it does not fail) when one exists from the last 24 hours,
        // so the warning stream is captured and checked
        let script = format!(
            "$w=$null;Checkpoint-Computer -Description {} -RestorePointType MODIFY_SETTINGS -WarningVariable w -WarningAction SilentlyContinue;if($w){{'recent'}}else{{'created'}}",
            ps_quote(description)
        );
        let r = match powershell_raw(&script) {
            Ok(s) if s.trim() == "recent" => RestorePoint::Recent,
            Ok(_) => RestorePoint::Created,
            Err(e) => {
                eprintln!("restore point failed: {e}");
                RestorePoint::Unavailable
            }
        };
        if matches!(r, RestorePoint::Created | RestorePoint::Recent) {
            DONE.store(true, Ordering::Relaxed);
        }
        r
    }
    #[cfg(not(windows))]
    {
        let _ = description;
        RestorePoint::Unsupported
    }
}

/// Seconds since the Unix epoch.
pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Total size of a file or folder (links are not followed).
pub fn size_of(p: &std::path::Path) -> u64 {
    let Ok(m) = std::fs::symlink_metadata(p) else { return 0 };
    if !m.is_dir() {
        return m.len();
    }
    let Ok(rd) = std::fs::read_dir(p) else { return 0 };
    rd.flatten().map(|e| size_of(&e.path())).sum()
}

/// Environment variable as a path, if set.
pub fn env_path(var: &str) -> Option<std::path::PathBuf> {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(std::path::PathBuf::from)
}

/// The user's home folder.
pub fn home() -> Option<std::path::PathBuf> {
    env_path(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
}

/// Names of the running processes, lowercase and without `.exe`.
pub fn running_processes() -> std::collections::HashSet<String> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    sys.processes().values().map(|p| p.name().to_string_lossy().to_lowercase().trim_end_matches(".exe").to_string()).collect()
}

/// Delete a file or folder tree, returning the bytes it held.
pub fn remove_all(p: &std::path::Path) -> Result<u64, String> {
    let size = size_of(p);
    let m = std::fs::symlink_metadata(p).map_err(|e| e.to_string())?;
    let r = if m.is_dir() { std::fs::remove_dir_all(p) } else { std::fs::remove_file(p) };
    r.map(|_| size).map_err(|e| e.to_string())
}

/// Delete everything inside a folder but keep the folder. Files in use are skipped and counted.
/// Returns (bytes freed, entries that could not be removed).
pub fn empty_dir(p: &std::path::Path) -> (u64, usize) {
    let Ok(rd) = std::fs::read_dir(p) else { return (0, 0) };
    let (mut freed, mut locked) = (0, 0);
    for e in rd.flatten() {
        let path = e.path();
        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            // Folders are emptied first so a locked file only keeps its own folder alive
            let (f, l) = empty_dir(&path);
            freed += f;
            locked += l;
            if std::fs::remove_dir(&path).is_err() && l == 0 {
                locked += 1;
            }
        } else {
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            match std::fs::remove_file(&path) {
                Ok(()) => freed += size,
                Err(_) => locked += 1,
            }
        }
    }
    (freed, locked)
}
