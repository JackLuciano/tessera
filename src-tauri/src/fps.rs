//! Frame-time recording with PresentMon (Windows). Nothing is injected into the game: PresentMon
//! reads the presents from Windows' own event tracing. It needs administrator rights, like every
//! ETW-based frame counter.
//!
//! PresentMon is not bundled in the installer: the first recording downloads the official release
//! from GitHub and checks it against a pinned SHA-256 before it is ever run.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

const VERSION: &str = "2.6.0";
const URL: &str = "https://github.com/GameTechDev/PresentMon/releases/download/v2.6.0/PresentMon-2.6.0-x64.exe";
const SHA256: &str = "b2a706bc6ad475749e3b7e3409263aa1e6906d45bdcf993f6dbc0f660188f1af";

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct FpsSummary {
    pub game: String,
    pub exe: String,
    pub start: u64,
    pub seconds: f32,
    pub frames: u64,
    pub avg: f32,
    /// Average FPS of the slowest 1% / 0.1% of frames.
    pub low1: f32,
    pub low01: f32,
    /// Average FPS per second of the session (downsampled to at most 600 points).
    pub timeline: Vec<f32>,
    #[serde(default)]
    pub latency: Option<Latency>,
    /// Hitches, worst first, with their most likely cause.
    #[serde(default)]
    pub stutters: Vec<Stutter>,
    /// Refresh rate of the main screen during the session.
    #[serde(default)]
    pub refresh: u32,
}

/// How frames reached the screen, from PresentMon's present metrics.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Latency {
    /// Average time from present to the frame being on screen (ms).
    pub display_ms: f32,
    /// Share of frames presented with V-Sync (sync interval above 0), 0–1.
    pub vsync: f32,
    /// Share of frames allowed to tear, 0–1.
    pub tearing: f32,
    /// Most common present mode ("Hardware: Independent Flip", "Composed: Flip"...).
    pub present_mode: String,
    /// Share of frames that never reached the screen, 0–1.
    pub dropped: f32,
    /// Average share of the frame time the GPU was busy, 0–1 (None when PresentMon didn't report it).
    pub gpu_busy: Option<f32>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Stutter {
    /// Seconds into the session.
    pub sec: u32,
    /// The longest frame in that second (ms).
    pub ms: f32,
    /// thermal, disk, background, cpu, gpu, game
    pub cause: String,
    /// Process name for "background", CPU/GPU for "thermal".
    pub detail: String,
}

/// What the PC was doing at one moment of a recording.
struct Ctx {
    sec: u32,
    cpu: f32,
    gpu: f32,
    cpu_limit: f32,
    gpu_throttle: bool,
    disk: f32,
    /// Busiest other process and its share of the whole CPU.
    top: (String, f32),
}

#[derive(Default)]
struct LatAcc {
    n: u64,
    vsync: u64,
    tearing: u64,
    dropped: u64,
    displayed_ms: f64,
    displayed_n: u64,
    gpu_ratio: f64,
    gpu_n: u64,
    modes: std::collections::HashMap<String, u64>,
}

#[derive(Serialize, Clone)]
pub struct FpsSample {
    pub fps: f32,
    pub low1: f32,
    pub frametime: f32,
    /// Last frame times (ms) for the sparkline.
    pub recent: Vec<f32>,
    pub game: String,
}

struct Recording {
    child: Child,
    game: String,
    exe: String,
    start: u64,
    started: Instant,
    frames: Vec<f32>,
    /// (seconds since start, frame time) for the per-second timeline.
    per_second: Vec<(u32, f32)>,
    lat: LatAcc,
    ctx: Vec<Ctx>,
}

fn rec() -> &'static Mutex<Option<Recording>> {
    static R: OnceLock<Mutex<Option<Recording>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(None))
}

/// What the recorder is doing, so a window opened later can show it ("downloading", "waiting",
/// an error...). Changes are also pushed as `fps-state`.
fn phase() -> &'static Mutex<Option<String>> {
    static P: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(None))
}

fn set_phase(app: &AppHandle, p: Option<&str>) {
    *phase().lock() = p.map(str::to_string);
    let _ = app.emit("fps-state", p);
}

pub fn current_phase() -> Option<String> {
    phase().lock().clone()
}

/// Set when the FPS hotkey started the recording, so the same hotkey also ends it.
static HOTKEY_REC: AtomicBool = AtomicBool::new(false);

pub fn tool_path(base: &Path) -> PathBuf {
    base.join("tools").join(format!("PresentMon-{VERSION}-x64.exe"))
}

pub fn tool_ready(base: &Path) -> bool {
    tool_path(base).is_file()
}

/// Download PresentMon once and verify its hash.
pub fn ensure_tool(base: &Path) -> Result<PathBuf, String> {
    let path = tool_path(base);
    if path.is_file() {
        return Ok(path);
    }
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("download");
    let script = format!(
        "Invoke-WebRequest -UseBasicParsing -Uri {} -OutFile {}; (Get-FileHash -Algorithm SHA256 -LiteralPath {}).Hash",
        crate::sysutil::ps_quote(URL),
        crate::sysutil::ps_quote(&tmp.to_string_lossy()),
        crate::sysutil::ps_quote(&tmp.to_string_lossy())
    );
    let hash = crate::sysutil::powershell_raw(&script)?;
    if !hash.trim().eq_ignore_ascii_case(SHA256) {
        let _ = std::fs::remove_file(&tmp);
        return Err("PresentMon download did not match the expected checksum".into());
    }
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(path)
}

fn ms_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// FPS of the slowest `frac` of frame times.
fn low(frames: &[f32], frac: f32) -> f32 {
    if frames.is_empty() {
        return 0.0;
    }
    let mut s: Vec<f32> = frames.to_vec();
    s.sort_by(|a, b| b.total_cmp(a));
    let n = ((s.len() as f32 * frac).ceil() as usize).max(1);
    let avg = s[..n].iter().sum::<f32>() / n as f32;
    if avg > 0.0 { 1000.0 / avg } else { 0.0 }
}

/// Column positions in PresentMon's CSV. Only the frame time is required; the rest feed the
/// latency check and are skipped when this PresentMon version names them differently. Names are
/// matched ignoring case: the 1.x-style output writes `msBetweenPresents`, 2.x `MsBetweenPresents`.
struct Cols {
    between: usize,
    sync: Option<usize>,
    tearing: Option<usize>,
    mode: Option<usize>,
    dropped: Option<usize>,
    displayed: Option<usize>,
    gpu: Option<usize>,
}

impl Cols {
    fn from_header(cells: &[&str]) -> Option<Self> {
        let find = |names: &[&str]| cells.iter().position(|h| names.iter().any(|n| h.trim().eq_ignore_ascii_case(n)));
        Some(Self {
            between: find(&["MsBetweenPresents"])?,
            sync: find(&["SyncInterval"]),
            tearing: find(&["AllowsTearing"]),
            mode: find(&["PresentMode"]),
            dropped: find(&["Dropped"]),
            displayed: find(&["MsUntilDisplayed", "MsDisplayLatency", "DisplayLatency"]),
            gpu: find(&["MsGPUActive", "MsGPUBusy", "GPUBusy"]),
        })
    }

    fn add(&self, cells: &[&str], ft: f32, acc: &mut LatAcc) {
        let get = |i: Option<usize>| i.and_then(|i| cells.get(i)).map(|v| v.trim());
        let num = |i: Option<usize>| get(i).and_then(|v| v.parse::<f64>().ok());
        acc.n += 1;
        if num(self.sync).is_some_and(|s| s > 0.0) {
            acc.vsync += 1;
        }
        if num(self.tearing).is_some_and(|t| t > 0.0) {
            acc.tearing += 1;
        }
        if num(self.dropped).is_some_and(|d| d > 0.0) {
            acc.dropped += 1;
        } else if let Some(d) = num(self.displayed).filter(|d| *d > 0.0 && *d < 1000.0) {
            acc.displayed_ms += d;
            acc.displayed_n += 1;
        }
        if let Some(g) = num(self.gpu).filter(|g| *g >= 0.0) {
            acc.gpu_ratio += (g / ft as f64).min(1.0);
            acc.gpu_n += 1;
        }
        if let Some(m) = get(self.mode).filter(|m| !m.is_empty()) {
            *acc.modes.entry(m.to_string()).or_default() += 1;
        }
    }
}

impl LatAcc {
    fn result(&self) -> Option<Latency> {
        if self.n == 0 {
            return None;
        }
        let n = self.n as f32;
        Some(Latency {
            display_ms: if self.displayed_n > 0 { (self.displayed_ms / self.displayed_n as f64) as f32 } else { 0.0 },
            vsync: self.vsync as f32 / n,
            tearing: self.tearing as f32 / n,
            present_mode: self.modes.iter().max_by_key(|(_, c)| **c).map(|(m, _)| m.clone()).unwrap_or_default(),
            dropped: self.dropped as f32 / n,
            gpu_busy: (self.gpu_n > 0).then(|| (self.gpu_ratio / self.gpu_n as f64) as f32),
        })
    }
}

/// Every 2 s while recording `start`: note the CPU / GPU load, throttling, disk activity and the
/// busiest other process, so hitches can be explained afterwards.
fn watch_context(start: u64, game_exe: String) {
    std::thread::spawn(move || {
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
        let mut sys = System::new();
        let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f32;
        let skip = |n: &str| {
            let n = n.to_lowercase();
            n == game_exe.to_lowercase() || n.starts_with("tessera") || n.starts_with("presentmon") || n == "system idle process" || n == "idle"
        };
        loop {
            std::thread::sleep(Duration::from_secs(2));
            sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_cpu());
            let top = sys
                .processes()
                .values()
                .map(|p| (p.name().to_string_lossy().into_owned(), p.cpu_usage() / cpus))
                .filter(|(n, _)| !skip(n))
                .fold((String::new(), 0.0f32), |best, p| if p.1 > best.1 { p } else { best });
            let snap = crate::sensors::latest().unwrap_or_default();
            let gpu = snap.gpus.first();
            let mut g = rec().lock();
            let Some(r) = g.as_mut().filter(|r| r.start == start) else { return };
            let ctx = Ctx {
                sec: r.started.elapsed().as_secs() as u32,
                cpu: snap.cpu_load,
                gpu: gpu.and_then(|g| g.load).unwrap_or(0.0),
                cpu_limit: snap.cpu_limit.unwrap_or(100.0),
                gpu_throttle: gpu.is_some_and(|g| g.throttle.contains(&"thermal")),
                disk: snap.disk_busy.unwrap_or(0.0),
                top,
            };
            if r.ctx.len() < 20_000 {
                r.ctx.push(ctx);
            }
        }
    });
}

/// The longest frame of each second that stands out from the session's typical frame time,
/// tagged with what the PC was doing then. Worst first, at most 50.
fn find_stutters(frames: &[f32], per_second: &[(u32, f32)], ctx: &[Ctx]) -> Vec<Stutter> {
    if frames.len() < 30 {
        return Vec::new();
    }
    let mut sorted = frames.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let median = sorted[sorted.len() / 2];
    let limit = (median * 2.5).max(median + 12.0).max(20.0);
    let mut worst: std::collections::BTreeMap<u32, f32> = std::collections::BTreeMap::new();
    for &(sec, ft) in per_second.iter().filter(|(_, ft)| *ft > limit) {
        let w = worst.entry(sec).or_default();
        *w = w.max(ft);
    }
    // A run of bad seconds is one hitch
    let mut out: Vec<Stutter> = Vec::new();
    for (sec, ms) in worst {
        if let Some(last) = out.last_mut().filter(|l| sec <= l.sec + 1 + (l.ms as u32 / 1000)) {
            last.ms = last.ms.max(ms);
            continue;
        }
        let (cause, detail) = explain(sec, ctx);
        out.push(Stutter { sec, ms, cause, detail });
    }
    out.sort_by(|a, b| b.ms.total_cmp(&a.ms));
    out.truncate(50);
    out
}

fn explain(sec: u32, ctx: &[Ctx]) -> (String, String) {
    let Some(c) = ctx.iter().filter(|c| c.sec.abs_diff(sec) <= 2).min_by_key(|c| c.sec.abs_diff(sec)) else {
        return ("game".into(), String::new());
    };
    let (cause, detail) = if c.gpu_throttle {
        ("thermal", "GPU".to_string())
    } else if c.cpu_limit < 85.0 {
        ("thermal", "CPU".to_string())
    } else if c.disk > 85.0 {
        ("disk", String::new())
    } else if c.top.1 > 20.0 {
        ("background", c.top.0.trim_end_matches(".exe").to_string())
    } else if c.cpu > 90.0 {
        ("cpu", String::new())
    } else if c.gpu > 97.0 {
        ("gpu", String::new())
    } else {
        ("game", String::new())
    };
    (cause.to_string(), detail)
}

/// Start recording a process by exe name. Errors are reported through `fps-error`.
pub fn start(app: &AppHandle, exe: &str, game: String) {
    stop();
    let app = app.clone();
    let exe = exe.to_string();
    std::thread::spawn(move || {
        let fail = |e: String| {
            set_phase(&app, Some(&e));
            let _ = app.emit("fps-error", e);
        };
        let Ok(base) = app.path().app_data_dir() else { return };
        if !crate::mft::is_elevated() {
            return fail("needs_admin".into());
        }
        if !tool_ready(&base) {
            set_phase(&app, Some("downloading"));
        }
        let tool = match ensure_tool(&base) {
            Ok(t) => t,
            Err(e) => return fail(e),
        };
        let child = crate::sysutil::command(&tool.to_string_lossy())
            .args(["--process_name", &exe, "--output_stdout", "--no_console_stats", "--v1_metrics", "--session_name", "Tessera", "--stop_existing_session", "--terminate_on_proc_exit"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => return fail(e.to_string()),
        };
        let Some(stdout) = child.stdout.take() else { return };
        let stderr = child.stderr.take();
        set_phase(&app, Some("waiting"));
        let start = ms_now();
        *rec().lock() = Some(Recording { child, game: game.clone(), exe: exe.clone(), start, started: Instant::now(), frames: Vec::new(), per_second: Vec::new(), lat: LatAcc::default(), ctx: Vec::new() });
        watch_context(start, exe.clone());
        let mut got_frames = false;
        let mut cols: Option<Cols> = None;
        let mut recent: VecDeque<f32> = VecDeque::new();
        let mut last_emit = Instant::now();
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let cells: Vec<&str> = line.split(',').collect();
            let Some(c) = &cols else {
                cols = Cols::from_header(&cells);
                continue;
            };
            let Some(ft) = cells.get(c.between).and_then(|v| v.parse::<f32>().ok()).filter(|f| *f > 0.0 && *f < 1000.0) else { continue };
            if !got_frames {
                got_frames = true;
                set_phase(&app, None);
            }
            recent.push_back(ft);
            while recent.len() > 240 {
                recent.pop_front();
            }
            {
                let mut g = rec().lock();
                let Some(r) = g.as_mut() else { break };
                let sec = r.started.elapsed().as_secs() as u32;
                if r.frames.len() < 2_000_000 {
                    r.frames.push(ft);
                }
                r.per_second.push((sec, ft));
                c.add(&cells, ft, &mut r.lat);
            }
            if last_emit.elapsed() >= Duration::from_millis(500) {
                last_emit = Instant::now();
                let window: Vec<f32> = recent.iter().copied().collect();
                let last_sec: Vec<f32> = window.iter().rev().scan(0.0, |acc, f| { *acc += f; (*acc <= 1000.0).then_some(*f) }).collect();
                let fps = if last_sec.is_empty() { 0.0 } else { last_sec.len() as f32 * 1000.0 / last_sec.iter().sum::<f32>() };
                let _ = app.emit("fps-sample", FpsSample { fps, low1: low(&window, 0.01), frametime: ft, recent: window.iter().rev().take(120).rev().copied().collect(), game: game.clone() });
            }
        }
        // PresentMon quit without a single frame: show its own message instead of an empty window
        if !got_frames && rec().lock().is_some() {
            stop();
            let msg = stderr.map(|e| BufReader::new(e).lines().map_while(Result::ok).filter(|l| !l.trim().is_empty()).last().unwrap_or_default()).unwrap_or_default();
            set_phase(&app, Some(if msg.is_empty() { "no_frames" } else { msg.trim() }));
        }
    });
}

/// The FPS hotkey: open the FPS window and, when nothing is being recorded yet, record the program
/// in the foreground (the game you are in). Pressed again, it closes the window and ends that
/// recording, saving it like any other session.
pub fn hotkey(app: &AppHandle) {
    if overlay_open(app) {
        close_overlay(app);
        return;
    }
    show_overlay(app, true);
    if recording().is_some() {
        return;
    }
    match foreground_exe() {
        Some((exe, path, name)) => {
            // A game that game mode already knows keeps its library name
            let path = path.to_lowercase();
            let game = crate::gamemode::status().game.filter(|g| !g.dir.is_empty() && path.starts_with(&g.dir.to_lowercase())).map(|g| g.name).unwrap_or(name);
            HOTKEY_REC.store(true, Ordering::SeqCst);
            start(app, &exe, game);
        }
        None => set_phase(app, Some("no_game")),
    }
}

/// Close the FPS window; a recording the hotkey started ends with it and is saved.
pub fn close_overlay(app: &AppHandle) {
    show_overlay(app, false);
    if HOTKEY_REC.swap(false, Ordering::SeqCst)
        && let Some(s) = stop()
        && let Ok(base) = app.path().app_data_dir()
    {
        save_session(&base, &s);
        let _ = app.emit("fps-saved", ());
    }
}

/// The executable behind the foreground window (file name, full path, display name), unless it is
/// Tessera itself or the desktop.
fn foreground_exe() -> Option<(String, String, String)> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    let mut pid = 0u32;
    // SAFETY: plain Win32 queries; a null window simply yields pid 0
    unsafe { GetWindowThreadProcessId(GetForegroundWindow(), &mut pid) };
    if pid == 0 || pid == std::process::id() {
        return None;
    }
    let mut sys = sysinfo::System::new();
    let p = sysinfo::Pid::from_u32(pid);
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[p]), true);
    let proc = sys.process(p)?;
    let exe = proc.name().to_string_lossy().into_owned();
    const SHELL: &[&str] = &["explorer.exe", "searchhost.exe", "startmenuexperiencehost.exe", "shellexperiencehost.exe"];
    if SHELL.iter().any(|s| exe.eq_ignore_ascii_case(s)) {
        return None;
    }
    let path = proc.exe().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    let name = exe.trim_end_matches(".exe").trim_end_matches(".EXE").to_string();
    Some((exe, path, name))
}

/// Stop recording; returns the session summary (and saves it) when frames were captured.
pub fn stop() -> Option<FpsSummary> {
    let mut r = rec().lock().take()?;
    *phase().lock() = None;
    let _ = r.child.kill();
    let _ = r.child.wait();
    if r.frames.len() < 30 {
        return None;
    }
    let total_ms: f32 = r.frames.iter().sum();
    let mut timeline: Vec<f32> = Vec::new();
    let mut i = 0;
    while i < r.per_second.len() {
        let sec = r.per_second[i].0;
        let mut n = 0;
        let mut ms = 0.0;
        while i < r.per_second.len() && r.per_second[i].0 == sec {
            n += 1;
            ms += r.per_second[i].1;
            i += 1;
        }
        timeline.push(if ms > 0.0 { n as f32 * 1000.0 / ms } else { 0.0 });
    }
    // Downsample long sessions to at most 600 points
    if timeline.len() > 600 {
        let step = timeline.len().div_ceil(600);
        timeline = timeline.chunks(step).map(|c| c.iter().sum::<f32>() / c.len() as f32).collect();
    }
    let refresh = crate::display::monitors().iter().find(|m| m.primary).map(|m| m.hz).unwrap_or(0);
    Some(FpsSummary {
        stutters: find_stutters(&r.frames, &r.per_second, &r.ctx),
        latency: r.lat.result(),
        refresh,
        game: r.game,
        exe: r.exe,
        start: r.start,
        seconds: total_ms / 1000.0,
        frames: r.frames.len() as u64,
        avg: r.frames.len() as f32 * 1000.0 / total_ms,
        low1: low(&r.frames, 0.01),
        low01: low(&r.frames, 0.001),
        timeline,
    })
}

pub fn recording() -> Option<String> {
    rec().lock().as_ref().map(|r| r.game.clone())
}

// ---------- saved sessions ----------

fn sessions_path(base: &Path) -> PathBuf {
    base.join("fps-sessions.json")
}

pub fn save_session(base: &Path, s: &FpsSummary) {
    let mut all = sessions(base);
    all.insert(0, s.clone());
    all.truncate(100);
    let _ = std::fs::write(sessions_path(base), serde_json::to_vec(&all).unwrap_or_default());
}

pub fn sessions(base: &Path) -> Vec<FpsSummary> {
    std::fs::read(sessions_path(base)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn delete_session(base: &Path, start: u64) {
    let mut all = sessions(base);
    all.retain(|s| s.start != start);
    let _ = std::fs::write(sessions_path(base), serde_json::to_vec(&all).unwrap_or_default());
}

// ---------- small always-on-top windows ----------

/// The FPS window: in the corner picked in the settings, unfocused, so the game keeps focus. The
/// page measures itself once loaded and calls [`fit_overlay`] with its real size.
pub fn show_overlay(app: &AppHandle, on: bool) {
    let corner = app.state::<crate::AppState>().prefs.lock().overlay.corner.clone();
    crate::tray::small_window(app, "overlay", on, 236.0, 150.0, &corner, true);
}

pub fn fit_overlay(app: &AppHandle, w: f64, h: f64) {
    let corner = app.state::<crate::AppState>().prefs.lock().overlay.corner.clone();
    crate::tray::fit_small_window(app, "overlay", &corner, w.clamp(80.0, 800.0), h.clamp(24.0, 600.0));
}

pub fn overlay_open(app: &AppHandle) -> bool {
    app.get_webview_window("overlay").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_lows() {
        let mut frames = vec![10.0f32; 990];
        frames.extend(vec![40.0f32; 10]);
        assert!((low(&frames, 0.01) - 25.0).abs() < 0.01);
        assert_eq!(low(&[], 0.01), 0.0);
    }

    fn ctx(sec: u32) -> Ctx {
        Ctx { sec, cpu: 40.0, gpu: 60.0, cpu_limit: 100.0, gpu_throttle: false, disk: 5.0, top: ("discord.exe".into(), 2.0) }
    }

    #[test]
    fn finds_and_explains_stutters() {
        let frames = vec![10.0f32; 600];
        // 60 frames per second for 10 s, one 80 ms hitch at second 4 and a 50 ms one at second 8
        let mut per_second: Vec<(u32, f32)> = (0..10).flat_map(|s| std::iter::repeat_n((s, 10.0f32), 60)).collect();
        per_second.push((4, 80.0));
        per_second.push((8, 50.0));
        let mut busy = ctx(8);
        busy.disk = 95.0;
        let st = find_stutters(&frames, &per_second, &[ctx(4), busy]);
        assert_eq!(st.len(), 2);
        assert_eq!((st[0].sec, st[0].cause.as_str()), (4, "game"));
        assert_eq!((st[1].sec, st[1].cause.as_str()), (8, "disk"));
        // A background process eating the CPU is named
        let mut bg = ctx(4);
        bg.top = ("MsMpEng.exe".into(), 35.0);
        assert_eq!(explain(4, &[bg]), ("background".to_string(), "MsMpEng".to_string()));
    }

    #[test]
    fn reads_latency_columns() {
        let header = ["Application", "SyncInterval", "AllowsTearing", "PresentMode", "Dropped", "MsBetweenPresents", "MsUntilDisplayed", "MsGPUActive"];
        let cols = Cols::from_header(&header).unwrap();
        let mut acc = LatAcc::default();
        cols.add(&["cs2.exe", "1", "0", "Composed: Flip", "0", "16.6", "20.0", "8.3"], 16.6, &mut acc);
        cols.add(&["cs2.exe", "1", "0", "Composed: Flip", "1", "16.6", "NA", "8.3"], 16.6, &mut acc);
        let l = acc.result().unwrap();
        assert_eq!(l.vsync, 1.0);
        assert_eq!(l.dropped, 0.5);
        assert_eq!(l.display_ms, 20.0);
        assert_eq!(l.present_mode, "Composed: Flip");
        assert!((l.gpu_busy.unwrap() - 0.5).abs() < 0.01);
    }
}
