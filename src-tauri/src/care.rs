//! Care: one scan across the cleaning and health modules, a health score, and one Fix that applies
//! the picked results. Scheduled runs come from Windows Task Scheduler (`tessera --care`).
//!
//! Fixes only ever cover what the scan found and the user (or their saved Care settings) picked:
//! junk, privacy traces, registry issues and broken shortcuts. Startup apps, disk health and
//! updates are reported for review, never changed automatically.

use crate::{junk, privacy, shortcuts, sysutil};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct CareConfig {
    /// junk, privacy, registry, shortcuts, startup, disk, updates
    pub modules: Vec<String>,
    /// Privacy kinds Care cleans (cache, cookies, history, session, recent, run_history, …).
    pub privacy_kinds: Vec<String>,
    /// off, daily, weekly, logon
    pub schedule: String,
    /// HH:MM for daily and weekly runs.
    pub time: String,
    /// Scheduled runs also clean safe junk and the picked privacy items (never the registry).
    pub auto_clean: bool,
    pub last_run: u64,
    pub last_score: Option<u32>,
    /// Bytes / items found by the last run, for the tray and the Care page header.
    pub last_found: u64,
    pub last_issues: u64,
}

impl Default for CareConfig {
    fn default() -> Self {
        Self {
            modules: ["junk", "privacy", "registry", "shortcuts", "startup", "disk", "updates"].iter().map(|s| s.to_string()).collect(),
            // Cookies and history are off by default: losing logins or history surprises people
            privacy_kinds: ["cache", "session", "recent", "run_history", "typed_paths", "search_history"].iter().map(|s| s.to_string()).collect(),
            schedule: "off".into(),
            time: "12:00".into(),
            auto_clean: false,
            last_run: 0,
            last_score: None,
            last_found: 0,
            last_issues: 0,
        }
    }
}

fn config_path(base: &Path) -> std::path::PathBuf {
    base.join("care.json")
}

pub fn load_config(base: &Path) -> CareConfig {
    std::fs::read(config_path(base)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save_config(base: &Path, c: &CareConfig) -> Result<(), String> {
    std::fs::create_dir_all(base).map_err(|e| e.to_string())?;
    std::fs::write(config_path(base), serde_json::to_vec_pretty(c).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

#[derive(Serialize, Clone, Default)]
pub struct CareReport {
    pub at: u64,
    pub score: u32,
    pub junk: Vec<junk::JunkItem>,
    pub privacy: Vec<privacy::PrivacyItem>,
    #[cfg(windows)]
    pub registry: Vec<crate::registry::Issue>,
    #[cfg(not(windows))]
    pub registry: Vec<()>,
    pub shortcuts: Vec<shortcuts::BrokenShortcut>,
    /// Enabled startup apps (Windows).
    pub startup: usize,
    pub disks: Vec<crate::diskhealth::PhysDisk>,
    #[cfg(windows)]
    pub updates: Vec<crate::winget::Upgrade>,
    #[cfg(not(windows))]
    pub updates: Vec<()>,
    /// Bytes the default fix would free, and the number of issues it would fix.
    pub fixable_bytes: u64,
    pub fixable_issues: u64,
}

pub fn scan(cfg: &CareConfig) -> CareReport {
    let on = |m: &str| cfg.modules.iter().any(|x| x == m);
    let mut r = CareReport { at: sysutil::now(), ..Default::default() };
    // Every module runs in its own thread: winget and the disk query take a few seconds each
    std::thread::scope(|s| {
        let junk = on("junk").then(|| s.spawn(junk::scan));
        let privacy = on("privacy").then(|| s.spawn(privacy::scan));
        let links = on("shortcuts").then(|| s.spawn(shortcuts::find_broken));
        let disks = on("disk").then(|| s.spawn(|| crate::diskhealth::report().disks));
        #[cfg(windows)]
        let registry = on("registry").then(|| s.spawn(crate::registry::scan));
        #[cfg(windows)]
        let updates = on("updates").then(|| s.spawn(|| crate::winget::list().unwrap_or_default()));
        #[cfg(windows)]
        let startup = on("startup").then(|| s.spawn(|| crate::optimizer::startup_list().iter().filter(|a| a.enabled).count()));
        r.junk = junk.and_then(|h| h.join().ok()).unwrap_or_default();
        r.privacy = privacy.and_then(|h| h.join().ok()).unwrap_or_default();
        r.shortcuts = links.and_then(|h| h.join().ok()).unwrap_or_default();
        r.disks = disks.and_then(|h| h.join().ok()).unwrap_or_default();
        #[cfg(windows)]
        {
            r.registry = registry.and_then(|h| h.join().ok()).unwrap_or_default();
            r.updates = updates.and_then(|h| h.join().ok()).unwrap_or_default();
            r.startup = startup.and_then(|h| h.join().ok()).unwrap_or(0);
        }
    });
    let plan = default_plan(&r, cfg);
    r.fixable_bytes = r.junk.iter().filter(|j| plan.junk.contains(&j.key.to_string())).map(|j| j.size).sum::<u64>()
        + r.privacy.iter().filter(|p| plan.privacy.contains(&p.id)).map(|p| p.size).sum::<u64>();
    r.fixable_issues = (plan.junk.len() + plan.privacy.len() + plan.registry.len() + plan.shortcuts.len()) as u64;
    r.score = score(&r);
    r
}

/// 100 = nothing to do. Each area can only take off a capped share, so one big item
/// (say 40 GB of junk) can't make an otherwise healthy PC look broken.
pub fn score(r: &CareReport) -> u32 {
    let gb = r.junk.iter().map(|j| j.size).sum::<u64>() as f64 / 1024f64.powi(3);
    // Housekeeping (junk, traces, stale entries, updates) costs at most 56 points together; only a
    // failing drive can push the score below that
    let mut p = 0.0f64;
    p += (gb * 2.0).min(15.0);
    p += (r.privacy.iter().filter(|i| i.size > 0 || i.count > 0).count() as f64).min(5.0);
    p += (r.registry.len() as f64 * 0.2).min(10.0);
    p += (r.shortcuts.len() as f64).min(8.0);
    p += ((r.startup as f64 - 8.0).max(0.0) * 1.5).min(8.0);
    p += (r.updates.len() as f64 * 0.5).min(10.0);
    for d in &r.disks {
        p += match d.health.as_str() {
            "unhealthy" => 30.0,
            "warning" => 15.0,
            _ => 0.0,
        };
        if d.temperature.is_some_and(|t| t >= 60) {
            p += 5.0;
        }
    }
    (100.0 - p).clamp(0.0, 100.0).round() as u32
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct FixPlan {
    pub junk: Vec<String>,
    pub privacy: Vec<String>,
    pub registry: Vec<String>,
    pub shortcuts: Vec<String>,
}

/// What Fix does when the user doesn't change the selection: safe junk, the configured privacy
/// kinds (for browsers that are closed), all registry issues the user can fix, broken shortcuts.
pub fn default_plan(r: &CareReport, cfg: &CareConfig) -> FixPlan {
    #[cfg(windows)]
    let elevated = crate::mft::is_elevated();
    FixPlan {
        junk: r.junk.iter().filter(|j| j.safe).map(|j| j.key.to_string()).collect(),
        privacy: r.privacy.iter().filter(|p| cfg.privacy_kinds.iter().any(|k| k == p.kind) && (!p.running || p.kind == "cache")).map(|p| p.id.clone()).collect(),
        #[cfg(windows)]
        registry: r.registry.iter().filter(|i| !i.admin || elevated).map(|i| i.id.clone()).collect(),
        #[cfg(not(windows))]
        registry: Vec::new(),
        shortcuts: r.shortcuts.iter().filter(|s| !s.admin).map(|s| s.path.clone()).collect(),
    }
}

#[derive(Serialize, Default)]
pub struct FixOutcome {
    pub freed: u64,
    pub fixed: usize,
    pub failed: Vec<String>,
    /// Browsers that were open, so their cookies/history were skipped.
    pub running: Vec<String>,
    pub restore_point: Option<sysutil::RestorePoint>,
    pub registry_backup: Option<String>,
}

pub fn fix(base: &Path, plan: &FixPlan) -> FixOutcome {
    let mut out = FixOutcome::default();
    if !plan.registry.is_empty() {
        out.restore_point = Some(sysutil::restore_point("Tessera Care"));
    }
    if !plan.junk.is_empty() {
        let j = junk::clean(&plan.junk);
        out.freed += j.freed;
        out.fixed += j.cleaned;
        out.failed.extend(j.failed);
    }
    if !plan.privacy.is_empty() {
        let p = privacy::clean(&plan.privacy);
        out.freed += p.freed;
        out.fixed += p.cleaned;
        out.failed.extend(p.failed);
        out.running = p.running;
    }
    #[cfg(windows)]
    if !plan.registry.is_empty() {
        match crate::registry::fix(base, &plan.registry) {
            Ok(r) => {
                out.fixed += r.fixed;
                out.failed.extend(r.failed);
                out.registry_backup = r.backup;
            }
            Err(e) => out.failed.push(e),
        }
    }
    #[cfg(not(windows))]
    let _ = base;
    if !plan.shortcuts.is_empty() {
        let (n, failed) = shortcuts::remove(&plan.shortcuts);
        out.fixed += n;
        out.failed.extend(failed);
    }
    out
}

/// Store the latest results in the config (for the tray tooltip and the Care page header).
pub fn remember(base: &Path, r: &CareReport) {
    let mut c = load_config(base);
    c.last_run = r.at;
    c.last_score = Some(r.score);
    c.last_found = r.fixable_bytes;
    c.last_issues = r.fixable_issues;
    let _ = save_config(base, &c);
    crate::stats::record_care(base, r.score, r.fixable_bytes);
}

// ---------- schedule (Windows Task Scheduler) ----------

#[cfg(windows)]
const TASK: &str = r"Tessera\Care";
/// At sign-in runs from the user's Run key: a logon trigger in Task Scheduler needs admin rights.
#[cfg(windows)]
const RUN_VALUE: &str = "TesseraCare";
#[cfg(windows)]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Register (or remove) the scheduled Care run.
pub fn apply_schedule(cfg: &CareConfig) -> Result<(), String> {
    #[cfg(windows)]
    {
        use winreg::{enums::*, RegKey};
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let command = format!("\"{}\" --care", exe.display());
        // Start clean: an old task or Run value from a previous setting must not linger
        let _ = sysutil::run("schtasks", &["/Delete", "/TN", TASK, "/F"]);
        if let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE) {
            let _ = k.delete_value(RUN_VALUE);
        }
        let time = if valid_time(&cfg.time) { cfg.time.as_str() } else { "12:00" };
        match cfg.schedule.as_str() {
            "daily" => sysutil::run("schtasks", &["/Create", "/TN", TASK, "/TR", &command, "/SC", "DAILY", "/ST", time, "/F"]).map(|_| ()),
            "weekly" => sysutil::run("schtasks", &["/Create", "/TN", TASK, "/TR", &command, "/SC", "WEEKLY", "/D", "MON", "/ST", time, "/F"]).map(|_| ()),
            "logon" => {
                let (k, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN_KEY).map_err(|e| e.to_string())?;
                k.set_value(RUN_VALUE, &command).map_err(|e| e.to_string())
            }
            _ => Ok(()),
        }
    }
    #[cfg(not(windows))]
    {
        if cfg.schedule == "off" { Ok(()) } else { Err("scheduled Care runs are only available on Windows".into()) }
    }
}

#[cfg(windows)]
fn valid_time(t: &str) -> bool {
    let b = t.as_bytes();
    b.len() == 5 && b[2] == b':' && t[..2].parse::<u8>().is_ok_and(|h| h < 24) && t[3..].parse::<u8>().is_ok_and(|m| m < 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_are_capped_per_area() {
        let mut r = CareReport::default();
        assert_eq!(score(&r), 100);
        r.junk.push(junk::JunkItem { key: "user_temp", safe: true, admin: false, size: 200 * 1024u64.pow(3), count: 1, sample: vec![] });
        assert_eq!(score(&r), 85);
        r.disks.push(crate::diskhealth::PhysDisk { health: "unhealthy".into(), temperature: Some(65), ..Default::default() });
        assert_eq!(score(&r), 50);
    }

    #[test]
    fn default_plan_respects_privacy_kinds_and_running_browsers() {
        let mut r = CareReport::default();
        let item = |id: &str, kind: &'static str, running: bool| privacy::PrivacyItem { id: id.into(), group: "chrome", kind, size: 1, count: 1, running };
        r.privacy = vec![item("chrome:cache", "cache", true), item("chrome:cookies", "cookies", false), item("chrome:session", "session", true)];
        let plan = default_plan(&r, &CareConfig::default());
        assert_eq!(plan.privacy, vec!["chrome:cache".to_string()]);
    }
}
