//! Automatic game mode (Windows). A watcher notices when an installed game starts and, depending
//! on the settings for that game, asks first (small prompt window), boosts right away, or does
//! nothing. A boost switches to a high-performance power plan, closes the apps on the user's
//! close list (never the ones on their keep list), optionally clears the RAM standby list, holds a
//! fine timer resolution and unparks CPU cores. When the game exits everything is put back and the
//! closed apps can be reopened. Everything here is the user's choice; nothing is hard-coded except
//! sensible defaults.

use crate::games::Game;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct GameSettings {
    /// ask, always, never
    pub boost: String,
    /// Extra apps to close for this game.
    pub close: Vec<String>,
    /// Apps to keep open for this game even if they are on the global close list.
    pub keep: Vec<String>,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self { boost: "ask".into(), close: Vec::new(), keep: Vec::new() }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Config {
    pub enabled: bool,
    /// Global default for games without their own setting: ask, always, never.
    pub default_boost: String,
    pub switch_plan: bool,
    /// Apply the full Gaming optimizer profile during the session (and the previous one after).
    pub apply_profile: bool,
    /// Process names (lowercase, without .exe) closed when a game is boosted.
    pub close: Vec<String>,
    /// Never closed, whatever else says so.
    pub keep: Vec<String>,
    pub reopen: bool,
    pub clear_standby: bool,
    pub timer: bool,
    pub unpark: bool,
    /// Record frame times with PresentMon during boosted sessions.
    pub record_fps: bool,
    /// Show the small FPS window during boosted sessions.
    pub overlay: bool,
    /// Games added by hand: (name, exe path).
    pub custom: Vec<(String, String)>,
    pub per_game: BTreeMap<String, GameSettings>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            default_boost: "ask".into(),
            switch_plan: true,
            apply_profile: false,
            // Sync and torrent clients grab disk and network bandwidth; chat and music stay by default
            close: ["onedrive", "dropbox", "googledrivefs", "qbittorrent", "utorrent", "bittorrent", "btweb"].iter().map(|s| s.to_string()).collect(),
            keep: ["discord", "spotify", "obs64", "steam", "nvidia app", "nvcontainer"].iter().map(|s| s.to_string()).collect(),
            reopen: true,
            clear_standby: true,
            timer: true,
            unpark: false,
            record_fps: true,
            overlay: true,
            custom: Vec::new(),
            per_game: BTreeMap::new(),
        }
    }
}

fn config_path(base: &Path) -> PathBuf {
    base.join("gamemode.json")
}
pub fn load(base: &Path) -> Config {
    std::fs::read(config_path(base)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}
pub fn save(base: &Path, c: &Config) -> Result<(), String> {
    std::fs::create_dir_all(base).map_err(|e| e.to_string())?;
    std::fs::write(config_path(base), serde_json::to_vec_pretty(c).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

// ---------- session ----------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SessionReport {
    pub game_id: String,
    pub game: String,
    pub start: u64,
    pub end: u64,
    pub boosted: bool,
    pub closed: Vec<String>,
    pub throttle: Vec<crate::sensors::ThrottleEvent>,
    pub fps: Option<crate::fps::FpsSummary>,
}

struct Session {
    game: Game,
    exe: String,
    pid: u32,
    start: u64,
    boosted: bool,
    prev_plan: Option<String>,
    prev_profile: Option<String>,
    prev_parking: Option<u32>,
    /// Closed apps: (name, exe path to reopen).
    closed: Vec<(String, String)>,
    last_purge: Option<Instant>,
}

struct State {
    session: Option<Session>,
    /// Game waiting for the user's answer in the prompt window.
    pending: Option<(Game, String, u32)>,
    /// Games the user said "not now" to, until they exit.
    declined: Vec<String>,
}

fn state() -> &'static parking_lot::Mutex<State> {
    static S: std::sync::OnceLock<parking_lot::Mutex<State>> = std::sync::OnceLock::new();
    S.get_or_init(|| parking_lot::Mutex::new(State { session: None, pending: None, declined: Vec::new() }))
}

#[derive(Serialize, Clone)]
pub struct Status {
    pub game: Option<Game>,
    pub boosted: bool,
    pub since: u64,
    pub closed: Vec<String>,
    pub pending: Option<Game>,
}

pub fn status() -> Status {
    let st = state().lock();
    Status {
        game: st.session.as_ref().map(|s| s.game.clone()),
        boosted: st.session.as_ref().is_some_and(|s| s.boosted),
        since: st.session.as_ref().map(|s| s.start).unwrap_or(0),
        closed: st.session.as_ref().map(|s| s.closed.iter().map(|c| c.0.clone()).collect()).unwrap_or_default(),
        pending: st.pending.as_ref().map(|p| p.0.clone()),
    }
}

fn stem(name: &str) -> String {
    name.to_lowercase().trim_end_matches(".exe").to_string()
}

/// Helpers inside game folders that are not the game itself.
fn helper(name: &str) -> bool {
    const H: &[&str] = &["crash", "report", "unins", "redist", "setup", "easyanticheat", "eac", "battleye", "beservice", "cefprocess", "helper", "webhelper", "update", "launcherpatcher"];
    let n = name.to_lowercase();
    H.iter().any(|h| n.contains(h))
}

/// The running game, if any: a big enough process whose executable sits inside a game folder.
fn find_running(sys: &sysinfo::System, games: &[Game]) -> Option<(Game, String, u32)> {
    let dirs: Vec<(String, &Game)> = games.iter().filter(|g| !g.dir.is_empty()).map(|g| (format!("{}\\", g.dir.to_lowercase().trim_end_matches('\\')), g)).collect();
    let mut best: Option<(u64, Game, String, u32)> = None;
    for (pid, p) in sys.processes() {
        let Some(exe) = p.exe() else { continue };
        let lower = exe.to_string_lossy().to_lowercase();
        let Some((_, g)) = dirs.iter().find(|(d, _)| lower.starts_with(d.as_str())) else { continue };
        let name = p.name().to_string_lossy().into_owned();
        // A game holds a few hundred MB once it's past its launcher splash
        if helper(&name) || p.memory() < 250 * 1024 * 1024 {
            continue;
        }
        if best.as_ref().is_none_or(|b| p.memory() > b.0) {
            best = Some((p.memory(), (*g).clone(), exe.to_string_lossy().into_owned(), pid.as_u32()));
        }
    }
    best.map(|(_, g, exe, pid)| (g, exe, pid))
}

pub fn boost_mode(cfg: &Config, game_id: &str) -> String {
    cfg.per_game.get(game_id).map(|s| s.boost.clone()).unwrap_or_else(|| cfg.default_boost.clone())
}

static RUNNING: AtomicBool = AtomicBool::new(false);

/// Start the watcher thread (once).
pub fn start(app: AppHandle) {
    if RUNNING.swap(true, Ordering::Relaxed) {
        return;
    }
    std::thread::spawn(move || {
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, UpdateKind};
        let Ok(base) = app.path().app_data_dir() else { return };
        let mut sys = sysinfo::System::new();
        let mut games: Vec<Game> = Vec::new();
        let mut games_at: Option<Instant> = None;
        let mut gone_ticks = 0;
        loop {
            std::thread::sleep(Duration::from_secs(3));
            let cfg = load(&base);
            if !cfg.enabled {
                if state().lock().session.is_some() {
                    end(&app, &base, &cfg);
                }
                continue;
            }
            // The library is re-read every 10 minutes (games get installed while Tessera runs)
            if games_at.is_none_or(|t| t.elapsed() > Duration::from_secs(600)) {
                games = crate::games::library(&cfg.custom);
                games_at = Some(Instant::now());
            }
            sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_memory().with_exe(UpdateKind::OnlyIfNotSet));
            let running = find_running(&sys, &games);
            let active_pid = state().lock().session.as_ref().map(|s| s.pid);
            match (active_pid, running) {
                (Some(pid), _) if sys.process(sysinfo::Pid::from_u32(pid)).is_some() => {
                    gone_ticks = 0;
                    maybe_purge(&cfg, &sys);
                }
                (Some(_), _) => {
                    // Two checks in a row without the game: it really exited (not a quick restart)
                    gone_ticks += 1;
                    if gone_ticks >= 2 {
                        gone_ticks = 0;
                        end(&app, &base, &cfg);
                    }
                }
                (None, Some((g, exe, pid))) => {
                    let mut st = state().lock();
                    if st.declined.contains(&g.id) || st.pending.as_ref().is_some_and(|p| p.0.id == g.id) {
                        continue;
                    }
                    match boost_mode(&cfg, &g.id).as_str() {
                        "never" => {
                            st.session = Some(new_session(g, exe, pid, false));
                            drop(st);
                            let _ = app.emit("game-status", status());
                        }
                        "always" => {
                            drop(st);
                            begin(&app, &cfg, g, exe, pid);
                        }
                        _ => {
                            st.pending = Some((g.clone(), exe, pid));
                            drop(st);
                            show_prompt(&app, &g);
                        }
                    }
                }
                (None, None) => {
                    let mut st = state().lock();
                    st.declined.clear();
                    // The game closed before the user answered
                    if st.pending.take().is_some() {
                        drop(st);
                        close_prompt(&app);
                    }
                }
            }
        }
    });
}

fn new_session(game: Game, exe: String, pid: u32, boosted: bool) -> Session {
    Session { game, exe, pid, start: crate::sysutil::now(), boosted, prev_plan: None, prev_profile: None, prev_parking: None, closed: Vec::new(), last_purge: None }
}

/// The user's answer from the prompt window (or the "Boost now" button / hotkey).
pub fn answer(app: &AppHandle, yes: bool, remember: bool) {
    let Ok(base) = app.path().app_data_dir() else { return };
    let mut cfg = load(&base);
    let pending = state().lock().pending.take();
    close_prompt(app);
    let Some((g, exe, pid)) = pending else { return };
    if remember {
        cfg.per_game.entry(g.id.clone()).or_default().boost = if yes { "always".into() } else { "never".into() };
        let _ = save(&base, &cfg);
    }
    if yes {
        begin(app, &cfg, g, exe, pid);
    } else {
        let mut st = state().lock();
        st.declined.push(g.id.clone());
        st.session = Some(new_session(g, exe, pid, false));
        drop(st);
        let _ = app.emit("game-status", status());
    }
}

/// Boost the running (not yet boosted) game by hand.
pub fn boost_now(app: &AppHandle) -> Result<(), String> {
    let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let cfg = load(&base);
    let pending = state().lock().pending.is_some();
    if pending {
        answer(app, true, false);
        return Ok(());
    }
    let s = state().lock().session.take();
    match s {
        Some(s) if !s.boosted => {
            begin(app, &cfg, s.game, s.exe, s.pid);
            Ok(())
        }
        Some(s) => {
            state().lock().session = Some(s);
            Err("already boosted".into())
        }
        None => Err("no game is running".into()),
    }
}

fn begin(app: &AppHandle, cfg: &Config, game: Game, exe: String, pid: u32) {
    let mut s = new_session(game, exe.clone(), pid, true);
    crate::sensors::session_begin();
    let elevated = crate::mft::is_elevated();
    if cfg.apply_profile {
        if let Ok(base) = app.path().app_data_dir() {
            s.prev_profile = crate::optimizer::active_profile(&base);
            let _ = crate::optimizer::apply_profile(&base, "gaming", elevated);
        }
    } else if cfg.switch_plan {
        let plans = crate::optimizer::power_plans();
        s.prev_plan = plans.iter().find(|p| p.active).map(|p| p.guid.clone());
        const PREFERRED: [&str; 2] = ["e9a42b02-d5df-448d-aa00-03f14749eb61", "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c"];
        if let Some(g) = PREFERRED.iter().find(|g| plans.iter().any(|p| p.guid.eq_ignore_ascii_case(g))) {
            let _ = crate::optimizer::set_power_plan(g);
        }
    }
    if cfg.unpark {
        s.prev_parking = crate::memory::core_parking_min();
        let _ = crate::memory::set_core_parking_min(100);
    }
    if cfg.timer {
        crate::memory::hold_timer();
    }
    if cfg.clear_standby && crate::memory::purge_standby().is_ok() {
        s.last_purge = Some(Instant::now());
    }
    s.closed = close_apps(cfg, &s.game.id);
    if cfg.record_fps {
        let exe_name = Path::new(&exe).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        crate::fps::start(app, &exe_name, s.game.name.clone());
    }
    if cfg.overlay {
        crate::fps::show_overlay(app, true);
    }
    let name = s.game.name.clone();
    state().lock().session = Some(s);
    let _ = app.emit("game-status", status());
    let l = app.state::<crate::AppState>().labels.lock().clone();
    crate::tray::notify(app, &l.boost_title, &l.boost_body.replace("{game}", &name));
}

/// Close the apps on the close list (plus this game's extras), skipping keep lists.
fn close_apps(cfg: &Config, game_id: &str) -> Vec<(String, String)> {
    let per = cfg.per_game.get(game_id).cloned().unwrap_or_default();
    let close: Vec<String> = cfg.close.iter().chain(per.close.iter()).map(|s| stem(s)).collect();
    let keep: Vec<String> = cfg.keep.iter().chain(per.keep.iter()).map(|s| stem(s)).collect();
    let mut sys = sysinfo::System::new();
    sys.refresh_processes_specifics(sysinfo::ProcessesToUpdate::All, true, sysinfo::ProcessRefreshKind::nothing().with_exe(sysinfo::UpdateKind::OnlyIfNotSet));
    let mut closed: Vec<(String, String)> = Vec::new();
    for p in sys.processes().values() {
        let n = stem(&p.name().to_string_lossy());
        if !close.contains(&n) || keep.contains(&n) {
            continue;
        }
        if p.kill() && !closed.iter().any(|c| c.0 == n) {
            closed.push((n, p.exe().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default()));
        }
    }
    closed
}

/// Clear the standby list again when free memory runs low during the game (at most every 5 min).
fn maybe_purge(cfg: &Config, sys: &sysinfo::System) {
    if !cfg.clear_standby {
        return;
    }
    let mut st = state().lock();
    let Some(s) = st.session.as_mut().filter(|s| s.boosted) else { return };
    let low = sys.available_memory() < 1024 * 1024 * 1024;
    if low && s.last_purge.is_none_or(|t| t.elapsed() > Duration::from_secs(300)) && crate::memory::purge_standby().is_ok() {
        s.last_purge = Some(Instant::now());
    }
}

fn end(app: &AppHandle, base: &Path, cfg: &Config) {
    let Some(s) = state().lock().session.take() else { return };
    let mut report = SessionReport { game_id: s.game.id.clone(), game: s.game.name.clone(), start: s.start, end: crate::sysutil::now(), boosted: s.boosted, closed: s.closed.iter().map(|c| c.0.clone()).collect(), throttle: Vec::new(), fps: None };
    if s.boosted {
        report.throttle = crate::sensors::session_end();
        report.fps = crate::fps::stop();
        if let Some(f) = &report.fps {
            crate::fps::save_session(base, f);
        }
        crate::fps::show_overlay(app, false);
        if let Some(p) = &s.prev_profile {
            let _ = crate::optimizer::apply_profile(base, p, crate::mft::is_elevated());
        }
        if let Some(g) = &s.prev_plan {
            let _ = crate::optimizer::set_power_plan(g);
        }
        if let Some(v) = s.prev_parking {
            let _ = crate::memory::set_core_parking_min(v);
        }
        crate::memory::release_timer();
        if cfg.reopen {
            for (_, exe) in s.closed.iter().filter(|c| !c.1.is_empty()) {
                let _ = std::process::Command::new(exe).spawn();
            }
        }
    }
    save_report(base, &report);
    crate::stats::record_game(base, &report);
    let _ = app.emit("game-status", status());
    let _ = app.emit("game-ended", &report);
}

fn reports_path(base: &Path) -> PathBuf {
    base.join("game-sessions.json")
}

fn save_report(base: &Path, r: &SessionReport) {
    let mut all = reports(base);
    all.insert(0, r.clone());
    all.truncate(200);
    let _ = std::fs::write(reports_path(base), serde_json::to_vec(&all).unwrap_or_default());
}

pub fn reports(base: &Path) -> Vec<SessionReport> {
    std::fs::read(reports_path(base)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

// ---------- prompt window ----------

fn show_prompt(app: &AppHandle, g: &Game) {
    close_prompt(app);
    let url = format!("index.html?window=prompt&game={}", urlencode(&g.name));
    let (w, h) = (380.0, 128.0);
    let mut b = tauri::WebviewWindowBuilder::new(app, "prompt", tauri::WebviewUrl::App(url.into()))
        .title("Tessera")
        .inner_size(w, h)
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .transparent(true)
        .shadow(false);
    // Bottom right of the primary screen, above the taskbar
    if let Ok(Some(m)) = app.primary_monitor() {
        let scale = m.scale_factor();
        let size = m.size().to_logical::<f64>(scale);
        let pos = m.position().to_logical::<f64>(scale);
        b = b.position(pos.x + size.width - w - 24.0, pos.y + size.height - h - 72.0);
    }
    if let Err(e) = b.build() {
        eprintln!("prompt window: {e}");
    }
}

fn close_prompt(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("prompt") {
        let _ = w.close();
    }
}

pub fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_game_settings_override_the_default() {
        let mut cfg = Config::default();
        assert_eq!(boost_mode(&cfg, "steam:730"), "ask");
        cfg.per_game.insert("steam:730".into(), GameSettings { boost: "always".into(), ..Default::default() });
        assert_eq!(boost_mode(&cfg, "steam:730"), "always");
        assert!(helper("UnityCrashHandler64.exe"));
        assert!(!helper("cs2.exe"));
        assert_eq!(urlencode("Hades II"), "Hades%20II");
    }
}
