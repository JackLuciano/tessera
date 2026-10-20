//! Installed games from the launchers' own records (Windows): Steam, Epic, GOG, Ubisoft, Xbox app
//! folders, and EA / Blizzard / Riot / Rockstar titles from the uninstall list. Plus per-game
//! Windows settings (CPU priority, GPU preference, fullscreen optimizations) and cleanup helpers
//! (shader caches, leftovers of uninstalled games).

use crate::sysutil;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use winreg::enums::*;
use winreg::RegKey;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Game {
    /// `<launcher>:<launcher's id>`
    pub id: String,
    pub name: String,
    /// steam, epic, gog, ubisoft, xbox, ea, battlenet, riot, rockstar, custom
    pub launcher: String,
    pub dir: String,
    /// Bytes on disk.
    pub size: u64,
    /// Unix seconds, 0 when the launcher doesn't record it.
    pub last_played: u64,
    /// Minutes, 0 when unknown.
    pub playtime: u64,
    pub cover: Option<String>,
    /// URI or executable that starts the game; None when only the launcher can.
    pub launch: Option<String>,
    /// Shader cache folder and its size (Steam).
    pub shader_cache: Option<(String, u64)>,
}

// ---------- Valve KeyValues (VDF) ----------

#[derive(Debug, Clone)]
enum Vdf {
    Str(String),
    Map(Vec<(String, Vdf)>),
}

impl Vdf {
    fn get(&self, key: &str) -> Option<&Vdf> {
        match self {
            Vdf::Map(m) => m.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v),
            Vdf::Str(_) => None,
        }
    }
    fn str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Vdf::Str(s) => Some(s),
            Vdf::Map(_) => None,
        }
    }
    fn entries(&self) -> &[(String, Vdf)] {
        match self {
            Vdf::Map(m) => m,
            Vdf::Str(_) => &[],
        }
    }
}

fn parse_vdf(text: &str) -> Vdf {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            if let Some(n) = chars.next() {
                                s.push(match n {
                                    'n' => '\n',
                                    't' => '\t',
                                    other => other,
                                });
                            }
                        }
                        '"' => break,
                        _ => s.push(c),
                    }
                }
                tokens.push(Some(s));
            }
            '{' => tokens.push(None),
            '}' => tokens.push(Some("\u{0}}".into())),
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    fn map(it: &mut std::vec::IntoIter<Option<String>>) -> Vec<(String, Vdf)> {
        let mut out = Vec::new();
        while let Some(t) = it.next() {
            let Some(key) = t else { continue };
            if key == "\u{0}}" {
                break;
            }
            match it.next() {
                Some(Some(v)) if v != "\u{0}}" => out.push((key, Vdf::Str(v))),
                Some(None) => out.push((key, Vdf::Map(map(it)))),
                _ => break,
            }
        }
        out
    }
    let mut it = tokens.into_iter();
    Vdf::Map(map(&mut it))
}

// ---------- launchers ----------

fn hive(machine: bool) -> RegKey {
    RegKey::predef(if machine { HKEY_LOCAL_MACHINE } else { HKEY_CURRENT_USER })
}

/// Steam tools that are not games.
fn steam_tool(appid: &str, name: &str) -> bool {
    let n = name.to_lowercase();
    appid == "228980" || n.contains("redistributable") || n.starts_with("proton") || n.contains("steam linux runtime") || n.contains("steamvr") || n.contains("dedicated server") || n.ends_with(" sdk")
}

fn steam() -> Vec<Game> {
    let Ok(k) = hive(false).open_subkey(r"Software\Valve\Steam") else { return vec![] };
    let Ok(root) = k.get_value::<String, _>("SteamPath") else { return vec![] };
    let root = PathBuf::from(root.replace('/', "\\"));
    let mut libs = vec![root.clone()];
    if let Ok(t) = std::fs::read_to_string(root.join(r"steamapps\libraryfolders.vdf")) {
        let v = parse_vdf(&t);
        if let Some(lf) = v.get("libraryfolders") {
            for (_, lib) in lf.entries() {
                if let Some(p) = lib.str("path") {
                    let p = PathBuf::from(p.replace("\\\\", "\\"));
                    if !libs.iter().any(|l| l.to_string_lossy().eq_ignore_ascii_case(&p.to_string_lossy())) {
                        libs.push(p);
                    }
                }
            }
        }
    }
    // Last played and playtime live in each Steam user's localconfig.vdf
    let mut played: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for user in std::fs::read_dir(root.join("userdata")).into_iter().flatten().flatten() {
        let Ok(t) = std::fs::read_to_string(user.path().join(r"config\localconfig.vdf")) else { continue };
        let v = parse_vdf(&t);
        let apps = v.get("UserLocalConfigStore").and_then(|s| s.get("Software")).and_then(|s| s.get("Valve")).and_then(|s| s.get("Steam")).and_then(|s| s.get("apps"));
        for (id, a) in apps.map(|a| a.entries()).unwrap_or(&[]) {
            let last = a.str("LastPlayed").and_then(|s| s.parse().ok()).unwrap_or(0);
            let mins = a.str("Playtime").and_then(|s| s.parse().ok()).unwrap_or(0);
            let e = played.entry(id.clone()).or_default();
            e.0 = e.0.max(last);
            e.1 = e.1.max(mins);
        }
    }
    let mut out = Vec::new();
    for lib in libs {
        let apps = lib.join("steamapps");
        for e in std::fs::read_dir(&apps).into_iter().flatten().flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if !(n.starts_with("appmanifest_") && n.ends_with(".acf")) {
                continue;
            }
            let Ok(t) = std::fs::read_to_string(e.path()) else { continue };
            let v = parse_vdf(&t);
            let Some(st) = v.get("AppState") else { continue };
            let (Some(id), Some(name), Some(dir)) = (st.str("appid"), st.str("name"), st.str("installdir")) else { continue };
            if steam_tool(id, name) {
                continue;
            }
            let (last, mins) = played.get(id).copied().unwrap_or_default();
            let shader = apps.join("shadercache").join(id);
            out.push(Game {
                id: format!("steam:{id}"),
                name: name.to_string(),
                launcher: "steam".into(),
                dir: apps.join("common").join(dir).to_string_lossy().into_owned(),
                size: st.str("SizeOnDisk").and_then(|s| s.parse().ok()).unwrap_or(0),
                last_played: last.max(st.str("LastPlayed").and_then(|s| s.parse().ok()).unwrap_or(0)),
                playtime: mins,
                cover: Some(format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{id}/library_600x900.jpg")),
                launch: Some(format!("steam://rungameid/{id}")),
                shader_cache: shader.is_dir().then(|| (shader.to_string_lossy().into_owned(), sysutil::size_of(&shader))),
            });
        }
    }
    out
}

/// A game's installed build as the launcher records it, for noticing finished updates.
#[derive(Clone, Debug)]
pub struct BuildMark {
    pub id: String,
    pub name: String,
    pub build: String,
    /// The Steam library's steamapps folder (Steam only).
    pub steamapps: Option<PathBuf>,
    /// Steam app id or Epic app name.
    pub app: String,
}

/// Installed builds of Steam and Epic games. Steam games that are still downloading are skipped,
/// so an update is only seen once it has finished.
pub fn build_marks() -> Vec<BuildMark> {
    let mut out = Vec::new();
    if let Ok(k) = hive(false).open_subkey(r"Software\Valve\Steam")
        && let Ok(root) = k.get_value::<String, _>("SteamPath")
    {
        let root = PathBuf::from(root.replace('/', "\\"));
        let mut libs = vec![root.clone()];
        if let Ok(t) = std::fs::read_to_string(root.join(r"steamapps\libraryfolders.vdf"))
            && let Some(lf) = parse_vdf(&t).get("libraryfolders")
        {
            for (_, lib) in lf.entries() {
                if let Some(p) = lib.str("path") {
                    let p = PathBuf::from(p.replace("\\\\", "\\"));
                    if !libs.iter().any(|l| l.to_string_lossy().eq_ignore_ascii_case(&p.to_string_lossy())) {
                        libs.push(p);
                    }
                }
            }
        }
        for lib in libs {
            let apps = lib.join("steamapps");
            for e in std::fs::read_dir(&apps).into_iter().flatten().flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                if !(n.starts_with("appmanifest_") && n.ends_with(".acf")) {
                    continue;
                }
                let Ok(t) = std::fs::read_to_string(e.path()) else { continue };
                let v = parse_vdf(&t);
                let Some(st) = v.get("AppState") else { continue };
                let (Some(id), Some(name), Some(build)) = (st.str("appid"), st.str("name"), st.str("buildid")) else { continue };
                // StateFlags 4 = fully installed, nothing pending
                if steam_tool(id, name) || st.str("StateFlags") != Some("4") {
                    continue;
                }
                out.push(BuildMark { id: format!("steam:{id}"), name: name.into(), build: build.into(), steamapps: Some(apps.clone()), app: id.into() });
            }
        }
    }
    let dir = sysutil::env_path("ProgramData").unwrap_or_else(|| PathBuf::from(r"C:\ProgramData")).join(r"Epic\EpicGamesLauncher\Data\Manifests");
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Item {
        display_name: Option<String>,
        app_name: Option<String>,
        app_version_string: Option<String>,
        b_is_incomplete_install: Option<bool>,
    }
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten().filter(|e| e.path().extension().is_some_and(|x| x == "item")) {
        let Some(i) = std::fs::read(e.path()).ok().and_then(|b| serde_json::from_slice::<Item>(&b).ok()) else { continue };
        let (Some(app), Some(build)) = (i.app_name, i.app_version_string) else { continue };
        if i.b_is_incomplete_install.unwrap_or(false) {
            continue;
        }
        out.push(BuildMark { id: format!("epic:{app}"), name: i.display_name.unwrap_or_else(|| app.clone()), build, steamapps: None, app });
    }
    out
}

fn epic() -> Vec<Game> {
    let dir = sysutil::env_path("ProgramData").unwrap_or_else(|| PathBuf::from(r"C:\ProgramData")).join(r"Epic\EpicGamesLauncher\Data\Manifests");
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Item {
        display_name: Option<String>,
        install_location: Option<String>,
        install_size: Option<u64>,
        app_name: Option<String>,
        catalog_namespace: Option<String>,
        catalog_item_id: Option<String>,
        b_is_incomplete_install: Option<bool>,
        #[serde(default)]
        app_categories: Vec<String>,
    }
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "item"))
        .filter_map(|e| serde_json::from_slice::<Item>(&std::fs::read(e.path()).ok()?).ok())
        .filter(|i| !i.b_is_incomplete_install.unwrap_or(false) && (i.app_categories.is_empty() || i.app_categories.iter().any(|c| c == "games")))
        .filter_map(|i| {
            let app = i.app_name?;
            let launch = match (&i.catalog_namespace, &i.catalog_item_id) {
                (Some(ns), Some(item)) => format!("com.epicgames.launcher://apps/{ns}%3A{item}%3A{app}?action=launch&silent=true"),
                _ => format!("com.epicgames.launcher://apps/{app}?action=launch&silent=true"),
            };
            Some(Game {
                id: format!("epic:{app}"),
                name: i.display_name.unwrap_or_else(|| app.clone()),
                launcher: "epic".into(),
                dir: i.install_location.unwrap_or_default(),
                size: i.install_size.unwrap_or(0),
                launch: Some(launch),
                ..Default::default()
            })
        })
        .collect()
}

fn gog() -> Vec<Game> {
    let Ok(root) = hive(true).open_subkey(r"SOFTWARE\WOW6432Node\GOG.com\Games") else { return vec![] };
    root.enum_keys()
        .flatten()
        .filter_map(|id| {
            let k = root.open_subkey(&id).ok()?;
            let s = |n: &str| k.get_value::<String, _>(n).unwrap_or_default();
            let path = s("path");
            if path.is_empty() {
                return None;
            }
            let exe = s("exe");
            Some(Game {
                id: format!("gog:{id}"),
                name: s("gameName"),
                launcher: "gog".into(),
                size: 0,
                launch: (!exe.is_empty()).then_some(exe),
                dir: path,
                ..Default::default()
            })
        })
        .collect()
}

fn ubisoft() -> Vec<Game> {
    let Ok(root) = hive(true).open_subkey(r"SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs") else { return vec![] };
    root.enum_keys()
        .flatten()
        .filter_map(|id| {
            let dir: String = root.open_subkey(&id).ok()?.get_value("InstallDir").ok()?;
            let dir = dir.replace('/', "\\");
            let name = Path::new(dir.trim_end_matches('\\')).file_name()?.to_string_lossy().into_owned();
            Some(Game { id: format!("ubisoft:{id}"), name, launcher: "ubisoft".into(), launch: Some(format!("uplay://launch/{id}/0")), dir, ..Default::default() })
        })
        .collect()
}

/// Xbox app / Microsoft Store PC games installed to the default `X:\XboxGames` folders.
fn xbox() -> Vec<Game> {
    let mut out = Vec::new();
    for d in sysinfo::Disks::new_with_refreshed_list().list() {
        let root = d.mount_point().join("XboxGames");
        for e in std::fs::read_dir(&root).into_iter().flatten().flatten() {
            if !e.path().join("Content").is_dir() {
                continue;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            out.push(Game { id: format!("xbox:{name}"), name, launcher: "xbox".into(), dir: e.path().to_string_lossy().into_owned(), ..Default::default() });
        }
    }
    out
}

/// Launchers without an easy library file: their games come from the uninstall list by publisher.
fn from_uninstall() -> Vec<Game> {
    const PUBLISHERS: &[(&str, &str)] = &[("electronic arts", "ea"), ("blizzard entertainment", "battlenet"), ("riot games", "riot"), ("rockstar games", "rockstar")];
    const NOT_GAMES: &[&str] = &["ea app", "origin", "battle.net", "riot client", "rockstar games launcher", "social club", "riot vanguard"];
    let mut out: Vec<Game> = Vec::new();
    for p in crate::programs::list() {
        let publisher = p.publisher.to_lowercase();
        let Some((_, launcher)) = PUBLISHERS.iter().find(|(pubname, _)| publisher.contains(pubname)) else { continue };
        let name = p.name.to_lowercase();
        if NOT_GAMES.iter().any(|n| name.starts_with(n)) || p.location.is_empty() || out.iter().any(|g| g.name == p.name) {
            continue;
        }
        out.push(Game { id: format!("{launcher}:{}", p.name), name: p.name, launcher: launcher.to_string(), dir: p.location, size: p.size, ..Default::default() });
    }
    out
}

/// Every installed game, plus games the user added by hand.
pub fn library(custom: &[(String, String)]) -> Vec<Game> {
    let mut all: Vec<Game> = Vec::new();
    for g in steam().into_iter().chain(epic()).chain(gog()).chain(ubisoft()).chain(xbox()).chain(from_uninstall()) {
        // The same folder from two sources (e.g. Ubisoft game also in the uninstall list) counts once;
        // launchers keep records of uninstalled games whose folder is gone
        if !g.dir.is_empty() && Path::new(&g.dir).is_dir() && !all.iter().any(|a| a.dir.eq_ignore_ascii_case(&g.dir)) {
            all.push(g);
        }
    }
    for (name, exe) in custom {
        let dir = Path::new(exe).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        all.push(Game { id: format!("custom:{exe}"), name: name.clone(), launcher: "custom".into(), dir, launch: Some(exe.clone()), ..Default::default() });
    }
    // Sizes the launcher didn't report are measured (only for folders that exist)
    for g in all.iter_mut().filter(|g| g.size == 0 && !g.dir.is_empty()) {
        let p = Path::new(&g.dir);
        if p.is_dir() {
            g.size = sysutil::size_of(p);
        }
    }
    all.sort_by_key(|g| std::cmp::Reverse(g.last_played));
    all
}

/// Start a game through its launcher URI or executable.
pub fn launch(target: &str) -> Result<(), String> {
    let is_uri = target.contains("://");
    if is_uri {
        let ok = ["steam://", "com.epicgames.launcher://", "uplay://"].iter().any(|p| target.starts_with(p));
        if !ok {
            return Err("unknown launcher link".into());
        }
        sysutil::command("cmd").args(["/C", "start", "", target]).spawn().map(|_| ()).map_err(|e| e.to_string())
    } else {
        let exe = Path::new(target);
        if !exe.is_file() || !exe.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")) {
            return Err("game executable not found".into());
        }
        std::process::Command::new(exe).current_dir(exe.parent().unwrap_or(Path::new("."))).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
}

// ---------- per-game Windows settings ----------

const IFEO: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options";
const GPU_PREF: &str = r"Software\Microsoft\DirectX\UserGpuPreferences";
const LAYERS: &str = r"Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers";

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ExeSettings {
    /// normal, above, high
    pub priority: String,
    pub gpu_high: bool,
    pub no_fso: bool,
}

/// Executables in a game folder worth configuring (the biggest .exe files, helpers skipped).
pub fn game_exes(dir: &str) -> Vec<String> {
    const SKIP: &[&str] = &["unins", "crash", "report", "redist", "setup", "install", "vcredist", "dxsetup", "eac", "easyanticheat", "battleye", "beservice", "launcherpatcher", "cefprocess", "helper", "update"];
    let mut found: Vec<(u64, PathBuf)> = Vec::new();
    fn walk(d: &Path, depth: u8, out: &mut Vec<(u64, PathBuf)>) {
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            let p = e.path();
            match e.file_type() {
                Ok(t) if t.is_dir() && depth < 3 => walk(&p, depth + 1, out),
                Ok(t) if t.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")) => out.push((e.metadata().map(|m| m.len()).unwrap_or(0), p)),
                _ => {}
            }
        }
    }
    walk(Path::new(dir), 0, &mut found);
    found.retain(|(_, p)| {
        let n = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        !SKIP.iter().any(|s| n.contains(s))
    });
    found.sort_by_key(|(s, _)| std::cmp::Reverse(*s));
    found.into_iter().take(4).map(|(_, p)| p.to_string_lossy().into_owned()).collect()
}

pub fn exe_settings(exe: &str) -> ExeSettings {
    let name = Path::new(exe).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let priority = hive(true)
        .open_subkey(format!(r"{IFEO}\{name}\PerfOptions"))
        .and_then(|k| k.get_value::<u32, _>("CpuPriorityClass"))
        .map(|v| match v {
            3 => "high",
            6 => "above",
            _ => "normal",
        })
        .unwrap_or("normal")
        .to_string();
    let gpu_high = hive(false).open_subkey(GPU_PREF).and_then(|k| k.get_value::<String, _>(exe)).is_ok_and(|v| v.contains("GpuPreference=2"));
    let no_fso = hive(false).open_subkey(LAYERS).and_then(|k| k.get_value::<String, _>(exe)).is_ok_and(|v| v.contains("DISABLEDXMAXIMIZEDWINDOWEDMODE"));
    ExeSettings { priority, gpu_high, no_fso }
}

/// Write per-exe settings. The CPU priority lives in HKLM (administrator rights); GPU preference
/// and fullscreen optimizations are per user. Turning an option off removes Tessera's value.
pub fn set_exe_settings(exe: &str, s: &ExeSettings) -> Result<(), String> {
    let path = Path::new(exe);
    let name = path.file_name().ok_or("bad exe")?.to_string_lossy().into_owned();
    if !name.to_lowercase().ends_with(".exe") {
        return Err("bad exe".into());
    }
    let current = exe_settings(exe);
    if current.priority != s.priority {
        let key = format!(r"{IFEO}\{name}\PerfOptions");
        let r = if s.priority == "normal" {
            hive(true).delete_subkey_all(&key).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) })
        } else {
            hive(true).create_subkey(&key).and_then(|(k, _)| k.set_value("CpuPriorityClass", &(if s.priority == "high" { 3u32 } else { 6u32 })))
        };
        r.map_err(|e| format!("CPU priority (administrator rights needed): {e}"))?;
    }
    let set_user = |key: &str, on: bool, value: &str| -> Result<(), String> {
        let (k, _) = hive(false).create_subkey(key).map_err(|e| e.to_string())?;
        if on {
            k.set_value(exe, &value).map_err(|e| e.to_string())
        } else {
            match k.delete_value(exe) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            }
        }
    };
    if current.gpu_high != s.gpu_high {
        set_user(GPU_PREF, s.gpu_high, "GpuPreference=2;")?;
    }
    if current.no_fso != s.no_fso {
        set_user(LAYERS, s.no_fso, "~ DISABLEDXMAXIMIZEDWINDOWEDMODE")?;
    }
    Ok(())
}

// ---------- cleanup ----------

#[derive(Serialize, Clone)]
pub struct Leftover {
    pub path: String,
    pub size: u64,
    pub launcher: String,
}

/// Folders in launcher libraries that no installed game owns any more (uninstalled games whose
/// files stayed behind).
pub fn leftovers(games: &[Game]) -> Vec<Leftover> {
    let mut roots: Vec<(PathBuf, &str)> = Vec::new();
    for g in games.iter().filter(|g| g.launcher == "steam") {
        if let Some(common) = Path::new(&g.dir).parent()
            && !roots.iter().any(|(r, _)| r == common)
        {
            roots.push((common.to_path_buf(), "steam"));
        }
    }
    let owned: Vec<String> = games.iter().map(|g| g.dir.to_lowercase().trim_end_matches('\\').to_string()).collect();
    let mut out = Vec::new();
    for (root, launcher) in roots {
        for e in std::fs::read_dir(&root).into_iter().flatten().flatten() {
            let p = e.path();
            let key = p.to_string_lossy().to_lowercase();
            if p.is_dir() && !owned.contains(&key) {
                // Steamworks shared redistributables are used by many games
                if key.ends_with("steamworks shared") || key.ends_with("steam controller configs") {
                    continue;
                }
                out.push(Leftover { size: sysutil::size_of(&p), path: p.to_string_lossy().into_owned(), launcher: launcher.into() });
            }
        }
    }
    out.retain(|l| l.size > 0);
    out
}

/// Move leftover folders or shader caches to the Recycle Bin. Only paths that are still not owned
/// by an installed game (or are a Steam shader cache folder) are accepted.
pub fn remove_paths(paths: &[String], games: &[Game]) -> (u64, Vec<String>) {
    let allowed_leftovers: Vec<String> = leftovers(games).into_iter().map(|l| l.path.to_lowercase()).collect();
    let mut freed = 0;
    let mut failed = Vec::new();
    for p in paths {
        let lower = p.to_lowercase();
        let shader = lower.contains(r"\steamapps\shadercache\");
        if !shader && !allowed_leftovers.contains(&lower) {
            failed.push(format!("{p}: not a leftover"));
            continue;
        }
        let size = sysutil::size_of(Path::new(p));
        match trash::delete(p) {
            Ok(()) => freed += size,
            Err(e) => failed.push(format!("{p}: {e}")),
        }
    }
    (freed, failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vdf() {
        let v = parse_vdf(
            r#""AppState"
{
	"appid"		"730"
	"name"		"Counter-Strike 2"
	"installdir"		"Counter-Strike Global Offensive"
	"SizeOnDisk"		"36000000000"
	"UserConfig" { "language" "english" }
}"#,
        );
        let st = v.get("AppState").unwrap();
        assert_eq!(st.str("name"), Some("Counter-Strike 2"));
        assert_eq!(st.get("UserConfig").unwrap().str("language"), Some("english"));
        assert!(steam_tool("228980", "Steamworks Common Redistributables"));
        assert!(!steam_tool("730", "Counter-Strike 2"));
    }

    #[test]
    fn reads_the_library_without_failing() {
        let games = library(&[]);
        assert!(games.iter().all(|g| !g.name.is_empty() && g.id.contains(':')));
    }
}
