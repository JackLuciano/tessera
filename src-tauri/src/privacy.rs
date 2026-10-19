//! Privacy sweep: browser caches, cookies, history and sessions, plus the traces the OS keeps
//! (recent files, Run box history, Explorer search history, clipboard).
//!
//! Browsers keep their databases open, so everything except the cache is only touched while the
//! browser is closed. Firefox keeps bookmarks and history in the same database (places.sqlite), so
//! its history is deleted row by row and bookmarks stay.

// Which of these are needed depends on the platform's browser locations
#[allow(unused_imports)]
use crate::sysutil::{self, env_path, home};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq)]
enum Engine {
    Chromium,
    Firefox,
}

struct Browser {
    id: &'static str,
    engine: Engine,
    /// Process names (lowercase, no .exe) that mean the browser is running.
    process: &'static [&'static str],
    /// Folder holding the profiles (or being the profile, for Opera).
    data: PathBuf,
    /// Separate cache folder (macOS/Linux, and Firefox on Windows), mirroring the profile names.
    cache: Option<PathBuf>,
}

fn browsers() -> Vec<Browser> {
    let mut v = Vec::new();
    #[allow(unused_mut, unused_variables)]
    let mut push = |id: &'static str, engine: Engine, process: &'static [&'static str], data: Option<PathBuf>, cache: Option<PathBuf>| {
        if let Some(data) = data.filter(|d| d.is_dir()) {
            v.push(Browser { id, engine, process, data, cache });
        }
    };
    use Engine::*;
    #[cfg(windows)]
    {
        let local = env_path("LOCALAPPDATA");
        let roaming = env_path("APPDATA");
        let l = |p: &str| local.as_ref().map(|d| d.join(p));
        let r = |p: &str| roaming.as_ref().map(|d| d.join(p));
        push("chrome", Chromium, &["chrome"], l(r"Google\Chrome\User Data"), None);
        push("edge", Chromium, &["msedge"], l(r"Microsoft\Edge\User Data"), None);
        push("brave", Chromium, &["brave"], l(r"BraveSoftware\Brave-Browser\User Data"), None);
        push("vivaldi", Chromium, &["vivaldi"], l(r"Vivaldi\User Data"), None);
        push("opera", Chromium, &["opera"], r(r"Opera Software\Opera Stable"), l(r"Opera Software\Opera Stable"));
        push("opera_gx", Chromium, &["opera"], r(r"Opera Software\Opera GX Stable"), l(r"Opera Software\Opera GX Stable"));
        push("firefox", Firefox, &["firefox"], r(r"Mozilla\Firefox\Profiles"), l(r"Mozilla\Firefox\Profiles"));
    }
    #[cfg(target_os = "macos")]
    if let Some(h) = home() {
        let s = |p: &str| Some(h.join("Library/Application Support").join(p));
        let c = |p: &str| Some(h.join("Library/Caches").join(p));
        push("chrome", Chromium, &["google chrome"], s("Google/Chrome"), c("Google/Chrome"));
        push("edge", Chromium, &["microsoft edge"], s("Microsoft Edge"), c("Microsoft Edge"));
        push("brave", Chromium, &["brave browser"], s("BraveSoftware/Brave-Browser"), c("BraveSoftware/Brave-Browser"));
        push("vivaldi", Chromium, &["vivaldi"], s("Vivaldi"), c("Vivaldi"));
        push("opera", Chromium, &["opera"], s("com.operasoftware.Opera"), c("com.operasoftware.Opera"));
        push("firefox", Firefox, &["firefox"], s("Firefox/Profiles"), c("Firefox/Profiles"));
    }
    #[cfg(target_os = "linux")]
    if let Some(h) = home() {
        let cfg = env_path("XDG_CONFIG_HOME").unwrap_or_else(|| h.join(".config"));
        let cache = env_path("XDG_CACHE_HOME").unwrap_or_else(|| h.join(".cache"));
        for (id, dir, process) in [
            ("chrome", "google-chrome", &["chrome"][..]),
            ("chromium", "chromium", &["chromium"][..]),
            ("edge", "microsoft-edge", &["msedge"][..]),
            ("brave", "BraveSoftware/Brave-Browser", &["brave"][..]),
            ("vivaldi", "vivaldi", &["vivaldi-bin"][..]),
            ("opera", "opera", &["opera"][..]),
        ] {
            push(id, Chromium, process, Some(cfg.join(dir)), Some(cache.join(dir)));
        }
        push("firefox", Firefox, &["firefox"], Some(h.join(".mozilla/firefox")), Some(cache.join("mozilla/firefox")));
    }
    v
}

/// Profile folders: Chromium has a `Preferences` file per profile, Firefox a `prefs.js`.
fn profiles(b: &Browser) -> Vec<PathBuf> {
    let marker = if b.engine == Engine::Chromium { "Preferences" } else { "prefs.js" };
    let mut out = Vec::new();
    if b.data.join(marker).is_file() {
        out.push(b.data.clone());
    }
    if let Ok(rd) = std::fs::read_dir(&b.data) {
        out.extend(rd.flatten().map(|e| e.path()).filter(|p| p.join(marker).is_file()));
    }
    out
}

const CHROMIUM_CACHE: &[&str] = &["Cache", "Code Cache", "GPUCache", "Service Worker/CacheStorage", "Service Worker/ScriptCache", "DawnGraphiteCache", "DawnWebGPUCache"];
const CHROMIUM_COOKIES: &[&str] = &["Network/Cookies", "Network/Cookies-journal", "Cookies", "Cookies-journal"];
const CHROMIUM_HISTORY: &[&str] = &[
    "History", "History-journal", "Visited Links", "Top Sites", "Top Sites-journal", "Shortcuts", "Shortcuts-journal",
    "Network Action Predictor", "Network Action Predictor-journal",
];
const CHROMIUM_SESSION: &[&str] = &["Sessions", "Current Session", "Current Tabs", "Last Session", "Last Tabs"];
const FIREFOX_CACHE: &[&str] = &["cache2", "startupCache", "thumbnails"];
const FIREFOX_COOKIES: &[&str] = &["cookies.sqlite", "cookies.sqlite-wal", "cookies.sqlite-shm"];
const FIREFOX_SESSION: &[&str] = &["sessionstore.jsonlz4", "sessionstore-backups"];
const FIREFOX_FORMS: &[&str] = &["formhistory.sqlite", "formhistory.sqlite-wal", "formhistory.sqlite-shm"];

const KINDS: &[&str] = &["cache", "cookies", "history", "session"];

/// Paths a browser item covers. Cache folders are emptied; everything else is deleted.
fn paths(b: &Browser, kind: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for p in profiles(b) {
        let name = p.file_name().map(|n| n.to_os_string()).unwrap_or_default();
        let mirror = b.cache.as_ref().map(|c| if p == b.data { c.clone() } else { c.join(&name) });
        let list: &[&str] = match (b.engine, kind) {
            (Engine::Chromium, "cache") => CHROMIUM_CACHE,
            (Engine::Chromium, "cookies") => CHROMIUM_COOKIES,
            (Engine::Chromium, "history") => CHROMIUM_HISTORY,
            (Engine::Chromium, "session") => CHROMIUM_SESSION,
            (Engine::Firefox, "cache") => FIREFOX_CACHE,
            (Engine::Firefox, "cookies") => FIREFOX_COOKIES,
            // places.sqlite is handled separately; form and search history can simply go
            (Engine::Firefox, "history") => FIREFOX_FORMS,
            (Engine::Firefox, "session") => FIREFOX_SESSION,
            _ => &[],
        };
        for rel in list {
            out.push(p.join(rel));
            if kind == "cache"
                && let Some(m) = &mirror
                && m != &p
            {
                out.push(m.join(rel));
            }
        }
    }
    out.retain(|p| p.exists());
    out
}

#[derive(Serialize, Clone)]
pub struct PrivacyItem {
    /// `<group>:<kind>`
    pub id: String,
    /// Browser id, or "system" for the OS traces.
    pub group: &'static str,
    pub kind: &'static str,
    pub size: u64,
    /// Entries (files, history entries, list items); 0 when unknown.
    pub count: u64,
    /// The browser is running: only its cache can be cleaned now.
    pub running: bool,
}

pub fn scan() -> Vec<PrivacyItem> {
    let procs = sysutil::running_processes();
    let mut out = Vec::new();
    for b in browsers() {
        let running = b.process.iter().any(|p| procs.contains(*p));
        for kind in KINDS {
            let ps = paths(&b, kind);
            let size: u64 = ps.iter().map(|p| sysutil::size_of(p)).sum();
            let mut count = ps.len() as u64;
            // Firefox history is rows in a shared database: count visits instead of files
            if b.engine == Engine::Firefox && *kind == "history" {
                count = profiles(&b).iter().filter_map(|p| firefox_visits(p)).sum();
            }
            if size > 0 || count > 0 {
                out.push(PrivacyItem { id: format!("{}:{kind}", b.id), group: b.id, kind, size, count, running });
            }
        }
    }
    out.extend(system_items());
    out
}

fn firefox_visits(profile: &Path) -> Option<u64> {
    let db = profile.join("places.sqlite");
    if !db.is_file() {
        return None;
    }
    let c = rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    c.query_row("SELECT count(*) FROM moz_historyvisits", [], |r| r.get::<_, i64>(0)).ok().map(|n| n.max(0) as u64)
}

/// Remove visits and every place that is not bookmarked; bookmarks and their favicons stay.
fn firefox_clear_history(profile: &Path) -> Result<(), String> {
    let db = profile.join("places.sqlite");
    if !db.is_file() {
        return Ok(());
    }
    let c = rusqlite::Connection::open(&db).map_err(|e| e.to_string())?;
    c.execute_batch(
        "BEGIN;
         DELETE FROM moz_historyvisits;
         DELETE FROM moz_inputhistory;
         DELETE FROM moz_places WHERE id NOT IN (SELECT fk FROM moz_bookmarks WHERE fk IS NOT NULL);
         UPDATE moz_places SET visit_count = 0, last_visit_date = NULL;
         DELETE FROM moz_origins WHERE id NOT IN (SELECT origin_id FROM moz_places);
         COMMIT;",
    )
    .map_err(|e| format!("places.sqlite: {e}"))
}

#[derive(Serialize, Default)]
pub struct PrivacyOutcome {
    pub freed: u64,
    pub cleaned: usize,
    pub failed: Vec<String>,
    /// Browsers that have to be closed first.
    pub running: Vec<String>,
}

pub fn clean(ids: &[String]) -> PrivacyOutcome {
    let procs = sysutil::running_processes();
    let mut out = PrivacyOutcome::default();
    let all = browsers();
    for id in ids {
        let Some((group, kind)) = id.split_once(':') else { continue };
        if group == "system" {
            match clean_system(kind) {
                Ok(n) => out.cleaned += n,
                Err(e) => out.failed.push(format!("{kind}: {e}")),
            }
            continue;
        }
        let Some(b) = all.iter().find(|b| b.id == group) else { continue };
        if kind != "cache" && b.process.iter().any(|p| procs.contains(*p)) {
            if !out.running.iter().any(|r| r == b.id) {
                out.running.push(b.id.to_string());
            }
            continue;
        }
        for p in paths(b, kind) {
            if kind == "cache" && p.is_dir() {
                let (freed, _) = sysutil::empty_dir(&p);
                out.freed += freed;
                out.cleaned += 1;
            } else {
                match sysutil::remove_all(&p) {
                    Ok(freed) => {
                        out.freed += freed;
                        out.cleaned += 1;
                    }
                    Err(e) => out.failed.push(format!("{}: {e}", p.display())),
                }
            }
        }
        if b.engine == Engine::Firefox && kind == "history" {
            for p in profiles(b) {
                match firefox_clear_history(&p) {
                    Ok(()) => out.cleaned += 1,
                    Err(e) => out.failed.push(e),
                }
            }
        }
    }
    out
}

// ---------- operating system traces ----------

#[cfg(windows)]
const EXPLORER: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer";
/// (kind, registry key under HKCU whose values are the list)
#[cfg(windows)]
const MRU_KEYS: &[(&str, &str)] = &[("run_history", "RunMRU"), ("typed_paths", "TypedPaths"), ("search_history", "WordWheelQuery")];

#[cfg(windows)]
fn recent_dirs() -> Vec<PathBuf> {
    let Some(r) = env_path("APPDATA").map(|a| a.join(r"Microsoft\Windows\Recent")) else { return vec![] };
    vec![r.join("AutomaticDestinations"), r.join("CustomDestinations"), r]
}

/// Recent-file shortcuts and jump lists (files only: the Recent folder also holds folders Windows needs).
fn recent_files() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        recent_dirs()
            .iter()
            .flat_map(|d| std::fs::read_dir(d).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect::<Vec<_>>()).unwrap_or_default())
            .filter(|p| p.file_name().is_some_and(|n| n != "desktop.ini"))
            .collect()
    }
    #[cfg(target_os = "linux")]
    {
        let f = env_path("XDG_DATA_HOME").or_else(|| home().map(|h| h.join(".local/share"))).map(|d| d.join("recently-used.xbel"));
        f.into_iter().filter(|p| p.is_file()).collect()
    }
    #[cfg(target_os = "macos")]
    {
        Vec::new()
    }
}

fn system_items() -> Vec<PrivacyItem> {
    let mut out = Vec::new();
    let recent = recent_files();
    if !recent.is_empty() {
        out.push(PrivacyItem {
            id: "system:recent".into(),
            group: "system",
            kind: "recent",
            size: recent.iter().map(|p| sysutil::size_of(p)).sum(),
            count: recent.len() as u64,
            running: false,
        });
    }
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        for (kind, sub) in MRU_KEYS {
            let n = hkcu.open_subkey(format!(r"{EXPLORER}\{sub}")).map(|k| k.enum_values().flatten().filter(|(name, _)| name != "MRUList" && name != "MRUListEx").count()).unwrap_or(0);
            if n > 0 {
                out.push(PrivacyItem { id: format!("system:{kind}"), group: "system", kind, size: 0, count: n as u64, running: false });
            }
        }
        out.push(PrivacyItem { id: "system:clipboard".into(), group: "system", kind: "clipboard", size: 0, count: 0, running: false });
    }
    out
}

fn clean_system(kind: &str) -> Result<usize, String> {
    match kind {
        "recent" => {
            let files = recent_files();
            let n = files.iter().filter(|p| std::fs::remove_file(p).is_ok()).count();
            Ok(n)
        }
        #[cfg(windows)]
        "run_history" | "typed_paths" | "search_history" => {
            use winreg::{enums::*, RegKey};
            let sub = MRU_KEYS.iter().find(|(k, _)| *k == kind).map(|(_, s)| *s).unwrap_or_default();
            let path = format!(r"{EXPLORER}\{sub}");
            let k = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(&path, KEY_READ | KEY_WRITE).map_err(|e| e.to_string())?;
            let names: Vec<String> = k.enum_values().flatten().map(|(n, _)| n).collect();
            // Search history keeps per-file-type subkeys as well
            let subkeys: Vec<String> = k.enum_keys().flatten().collect();
            for s in &subkeys {
                let _ = k.delete_subkey_all(s);
            }
            Ok(names.iter().filter(|n| k.delete_value(n).is_ok()).count() + subkeys.len())
        }
        #[cfg(windows)]
        "clipboard" => {
            use windows_sys::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard};
            // SAFETY: plain Win32 calls; the clipboard is opened without an owner window and closed again.
            unsafe {
                if OpenClipboard(std::ptr::null_mut()) != 0 {
                    EmptyClipboard();
                    CloseClipboard();
                }
            }
            // Win+V history lives in WinRT; Windows PowerShell can reach it
            let _ = sysutil::powershell_raw("[void][Windows.ApplicationModel.DataTransfer.Clipboard,Windows.ApplicationModel.DataTransfer,ContentType=WindowsRuntime];[void][Windows.ApplicationModel.DataTransfer.Clipboard]::ClearHistory()");
            Ok(1)
        }
        _ => Err("unknown item".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firefox_history_keeps_bookmarks() {
        let dir = std::env::temp_dir().join(format!("tessera-ff-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("places.sqlite");
        let _ = std::fs::remove_file(&db);
        let c = rusqlite::Connection::open(&db).unwrap();
        c.execute_batch(
            "CREATE TABLE moz_origins (id INTEGER PRIMARY KEY, host TEXT);
             CREATE TABLE moz_places (id INTEGER PRIMARY KEY, url TEXT, origin_id INTEGER, visit_count INTEGER, last_visit_date INTEGER);
             CREATE TABLE moz_historyvisits (id INTEGER PRIMARY KEY, place_id INTEGER);
             CREATE TABLE moz_inputhistory (place_id INTEGER, input TEXT);
             CREATE TABLE moz_bookmarks (id INTEGER PRIMARY KEY, fk INTEGER);
             INSERT INTO moz_origins VALUES (1, 'kept.example'), (2, 'gone.example');
             INSERT INTO moz_places VALUES (1, 'https://kept.example', 1, 3, 1), (2, 'https://gone.example', 2, 5, 1);
             INSERT INTO moz_historyvisits VALUES (1, 1), (2, 2), (3, 2);
             INSERT INTO moz_bookmarks VALUES (1, 1), (2, NULL);",
        )
        .unwrap();
        drop(c);
        assert_eq!(firefox_visits(&dir), Some(3));
        firefox_clear_history(&dir).unwrap();
        let c = rusqlite::Connection::open(&db).unwrap();
        let places: Vec<String> = c.prepare("SELECT url FROM moz_places").unwrap().query_map([], |r| r.get(0)).unwrap().flatten().collect();
        assert_eq!(places, vec!["https://kept.example".to_string()]);
        assert_eq!(firefox_visits(&dir), Some(0));
        let origins: i64 = c.query_row("SELECT count(*) FROM moz_origins", [], |r| r.get(0)).unwrap();
        assert_eq!(origins, 1);
        drop(c);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
