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
}

fn rec() -> &'static Mutex<Option<Recording>> {
    static R: OnceLock<Mutex<Option<Recording>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(None))
}

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

/// Start recording a process by exe name. Errors are reported through `fps-error`.
pub fn start(app: &AppHandle, exe: &str, game: String) {
    stop();
    let app = app.clone();
    let exe = exe.to_string();
    std::thread::spawn(move || {
        let Ok(base) = app.path().app_data_dir() else { return };
        if !crate::mft::is_elevated() {
            let _ = app.emit("fps-error", "needs_admin");
            return;
        }
        let tool = match ensure_tool(&base) {
            Ok(t) => t,
            Err(e) => {
                let _ = app.emit("fps-error", e);
                return;
            }
        };
        let child = crate::sysutil::command(&tool.to_string_lossy())
            .args(["--process_name", &exe, "--output_stdout", "--no_console_stats", "--v1_metrics", "--session_name", "Tessera", "--stop_existing_session", "--terminate_on_proc_exit"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                let _ = app.emit("fps-error", e.to_string());
                return;
            }
        };
        let Some(stdout) = child.stdout.take() else { return };
        *rec().lock() = Some(Recording { child, game: game.clone(), exe: exe.clone(), start: ms_now(), started: Instant::now(), frames: Vec::new(), per_second: Vec::new() });
        let mut col: Option<usize> = None;
        let mut recent: VecDeque<f32> = VecDeque::new();
        let mut last_emit = Instant::now();
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let cells: Vec<&str> = line.split(',').collect();
            let Some(c) = col else {
                col = cells.iter().position(|h| *h == "MsBetweenPresents");
                continue;
            };
            let Some(ft) = cells.get(c).and_then(|v| v.parse::<f32>().ok()).filter(|f| *f > 0.0 && *f < 1000.0) else { continue };
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
            }
            if last_emit.elapsed() >= Duration::from_millis(500) {
                last_emit = Instant::now();
                let window: Vec<f32> = recent.iter().copied().collect();
                let last_sec: Vec<f32> = window.iter().rev().scan(0.0, |acc, f| { *acc += f; (*acc <= 1000.0).then_some(*f) }).collect();
                let fps = if last_sec.is_empty() { 0.0 } else { last_sec.len() as f32 * 1000.0 / last_sec.iter().sum::<f32>() };
                let _ = app.emit("fps-sample", FpsSample { fps, low1: low(&window, 0.01), frametime: ft, recent: window.iter().rev().take(120).rev().copied().collect(), game: game.clone() });
            }
        }
    });
}

/// Stop recording; returns the session summary (and saves it) when frames were captured.
pub fn stop() -> Option<FpsSummary> {
    let mut r = rec().lock().take()?;
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
    Some(FpsSummary {
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

/// The FPS window: top-left corner, unfocused, so the game keeps focus.
pub fn show_overlay(app: &AppHandle, on: bool) {
    crate::tray::small_window(app, "overlay", on, 236.0, 150.0, true, true);
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
}
