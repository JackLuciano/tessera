//! Backups of chosen folders (documents, game saves...) to another drive.
//!
//! Every run writes a dated snapshot `<target>\Tessera Backup\<job>\<YYYY-MM-DD HH.MM>\<folder>`.
//! Files unchanged since the previous snapshot (same size and modification time) are hard-linked
//! to it instead of copied, so each snapshot is complete on its own while only changed files take
//! new space. Snapshots beyond the number to keep are removed, oldest first. Runs are started by
//! hand, by Task Scheduler (`--backup`) or, while Tessera runs, when a job is due.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const ROOT: &str = "Tessera Backup";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Job {
    pub id: String,
    pub name: String,
    pub sources: Vec<String>,
    /// Folder (usually a drive root) the snapshots go under.
    pub target: String,
    /// Snapshots to keep.
    pub keep: u32,
    /// off, daily, weekly
    pub schedule: String,
    pub last_run: u64,
    /// Empty when the last run worked.
    pub last_error: String,
    pub last_size: u64,
}

impl Default for Job {
    fn default() -> Self {
        Self { id: String::new(), name: String::new(), sources: Vec::new(), target: String::new(), keep: 5, schedule: "weekly".into(), last_run: 0, last_error: String::new(), last_size: 0 }
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub jobs: Vec<Job>,
    /// HH:MM for the Task Scheduler entry.
    pub time: String,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Outcome {
    pub copied: u64,
    pub linked: u64,
    /// Bytes written (new or changed files).
    pub bytes: u64,
    pub failed: Vec<String>,
    pub snapshot: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct Snapshot {
    pub name: String,
    pub path: String,
}

pub const FILE: &str = "backup.json";

pub fn load(base: &Path) -> Config {
    std::fs::read(base.join(FILE)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save(base: &Path, c: &Config) -> Result<(), String> {
    for j in &c.jobs {
        validate(j)?;
    }
    std::fs::create_dir_all(base).map_err(|e| e.to_string())?;
    std::fs::write(base.join(FILE), serde_json::to_vec_pretty(c).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Folder names inside the backup: safe for any file system.
fn safe_name(s: &str) -> String {
    let n: String = s.chars().map(|c| if c.is_alphanumeric() || " -_.".contains(c) { c } else { '_' }).collect();
    let n = n.trim().trim_matches('.').to_string();
    if n.is_empty() { "Backup".into() } else { n }
}

fn validate(j: &Job) -> Result<(), String> {
    if j.target.trim().is_empty() {
        return Err("choose where the backup goes".into());
    }
    let target = Path::new(&j.target);
    for s in &j.sources {
        let src = Path::new(s);
        // A backup inside its own source would copy itself forever
        if target.starts_with(src) {
            return Err(format!("the backup can't go inside {s}"));
        }
    }
    Ok(())
}

pub fn job_dir(j: &Job) -> PathBuf {
    Path::new(&j.target).join(ROOT).join(safe_name(&j.name))
}

/// Snapshots of a job, newest first.
pub fn snapshots(j: &Job) -> Vec<Snapshot> {
    let mut v: Vec<Snapshot> = std::fs::read_dir(job_dir(j))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| Snapshot { name: e.file_name().to_string_lossy().into_owned(), path: e.path().to_string_lossy().into_owned() })
        .filter(|s| s.name.len() == 16 && s.name.as_bytes()[4] == b'-')
        .collect();
    v.sort_by(|a, b| b.name.cmp(&a.name));
    v
}

fn stamp(unix: u64) -> String {
    // Civil date from days since the epoch (local time isn't needed for sorting)
    let days = (unix / 86400) as i64;
    let secs = unix % 86400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {:02}.{:02}", secs / 3600, (secs % 3600) / 60)
}

fn same_file(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    a.len() == b.len() && a.modified().ok() == b.modified().ok()
}

/// Copy `src` into `dst`, hard-linking files unchanged since `prev` (the same folder in the last
/// snapshot). Keeps the files' modification times so the next run can compare them.
fn mirror(src: &Path, dst: &Path, prev: Option<&Path>, out: &mut Outcome) {
    if let Err(e) = std::fs::create_dir_all(dst) {
        out.failed.push(format!("{}: {e}", dst.display()));
        return;
    }
    let Ok(rd) = std::fs::read_dir(src) else {
        out.failed.push(format!("{}: can't be read", src.display()));
        return;
    };
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        // Links are skipped: they could point anywhere, even back into the backup
        if ft.is_symlink() {
            continue;
        }
        let name = e.file_name();
        let (s, d) = (e.path(), dst.join(&name));
        let p = prev.map(|p| p.join(&name));
        if ft.is_dir() {
            mirror(&s, &d, p.as_deref().filter(|p| p.is_dir()), out);
            continue;
        }
        let Ok(meta) = e.metadata() else { continue };
        if let Some(p) = p.as_deref()
            && std::fs::metadata(p).is_ok_and(|pm| same_file(&meta, &pm))
            && std::fs::hard_link(p, &d).is_ok()
        {
            out.linked += 1;
            continue;
        }
        match std::fs::copy(&s, &d) {
            Ok(n) => {
                out.copied += 1;
                out.bytes += n;
                if let Ok(t) = meta.modified() {
                    let _ = std::fs::File::options().write(true).open(&d).and_then(|f| f.set_modified(t));
                }
            }
            Err(err) => out.failed.push(format!("{}: {err}", s.display())),
        }
    }
}

/// Run one job now.
pub fn run(j: &Job) -> Result<Outcome, String> {
    validate(j)?;
    if !Path::new(&j.target).is_dir() {
        return Err(format!("{} is not available", j.target));
    }
    let dir = job_dir(j);
    let prev = snapshots(j).into_iter().next().map(|s| PathBuf::from(s.path));
    let snap = dir.join(stamp(crate::sysutil::now()));
    if snap.exists() {
        return Err("a backup was made less than a minute ago".into());
    }
    let mut out = Outcome { snapshot: snap.to_string_lossy().into_owned(), ..Default::default() };
    for s in &j.sources {
        let src = Path::new(s);
        let leaf = src.file_name().map(|n| safe_name(&n.to_string_lossy())).unwrap_or_else(|| safe_name(s));
        let p = prev.as_ref().map(|p| p.join(&leaf));
        mirror(src, &snap.join(&leaf), p.as_deref().filter(|p| p.is_dir()), &mut out);
    }
    // Oldest snapshots beyond the number to keep
    for old in snapshots(j).into_iter().skip(j.keep.max(1) as usize) {
        let _ = std::fs::remove_dir_all(&old.path);
    }
    Ok(out)
}

/// Is a scheduled job due (daily: 20 h, weekly: 6.5 days since the last run)?
pub fn due(j: &Job, now: u64) -> bool {
    let gap = match j.schedule.as_str() {
        "daily" => 20 * 3600,
        "weekly" => 156 * 3600,
        _ => return false,
    };
    now.saturating_sub(j.last_run) >= gap
}

/// Run every due job and record the results. Returns (job name, outcome or error).
pub fn run_due(base: &Path) -> Vec<(String, Result<Outcome, String>)> {
    let mut cfg = load(base);
    let now = crate::sysutil::now();
    let mut done = Vec::new();
    for j in cfg.jobs.iter_mut().filter(|j| due(j, now) && Path::new(&j.target).is_dir()) {
        let r = run(j);
        record(j, &r);
        done.push((j.name.clone(), r));
    }
    if !done.is_empty() {
        let _ = save(base, &cfg);
    }
    done
}

pub fn record(j: &mut Job, r: &Result<Outcome, String>) {
    j.last_run = crate::sysutil::now();
    match r {
        Ok(o) => {
            j.last_error = if o.failed.is_empty() { String::new() } else { format!("{} files could not be copied", o.failed.len()) };
            j.last_size = o.bytes;
        }
        Err(e) => j.last_error = e.clone(),
    }
}

/// Folders worth backing up that exist on this PC: (kind, path, size).
pub fn suggestions() -> Vec<(String, String, u64)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    if let Some(home) = crate::sysutil::env_path("USERPROFILE").or_else(|| crate::sysutil::env_path("HOME")) {
        for (kind, rel) in [("documents", "Documents"), ("pictures", "Pictures"), ("desktop", "Desktop"), ("saves", "Saved Games"), ("saves", r"Documents\My Games")] {
            out.push((kind.into(), home.join(rel)));
        }
    }
    // Unreal Engine games keep saves in %LOCALAPPDATA%\<Project>\Saved\SaveGames
    if let Some(local) = crate::sysutil::env_path("LOCALAPPDATA") {
        for e in std::fs::read_dir(local).into_iter().flatten().flatten() {
            out.push(("saves".into(), e.path().join(r"Saved\SaveGames")));
        }
    }
    out.into_iter().filter(|(_, p)| p.is_dir()).map(|(k, p)| (k, p.to_string_lossy().into_owned(), crate::sysutil::size_of(&p))).filter(|(_, _, s)| *s > 0).collect()
}

#[cfg(windows)]
const TASK: &str = "Tessera Backup";

/// One Task Scheduler entry starts Tessera with --backup every day; it then runs only the jobs
/// that are due. Removed when no job has a schedule.
pub fn apply_schedule(cfg: &Config) -> Result<(), String> {
    #[cfg(windows)]
    {
        let _ = crate::sysutil::run("schtasks", &["/Delete", "/TN", TASK, "/F"]);
        if !cfg.jobs.iter().any(|j| j.schedule != "off") {
            return Ok(());
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let command = format!("\"{}\" --backup", exe.display());
        let t = cfg.time.as_bytes();
        let ok = t.len() == 5 && t[2] == b':' && cfg.time[..2].parse::<u8>().is_ok_and(|h| h < 24) && cfg.time[3..].parse::<u8>().is_ok_and(|m| m < 60);
        let time = if ok { cfg.time.as_str() } else { "20:00" };
        crate::sysutil::run("schtasks", &["/Create", "/TN", TASK, "/TR", &command, "/SC", "DAILY", "/ST", time, "/F"]).map(|_| ())
    }
    #[cfg(not(windows))]
    {
        let _ = cfg;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_link_unchanged_files_and_keep_n() {
        let root = std::env::temp_dir().join(format!("tessera-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let src = root.join("Saves");
        std::fs::create_dir_all(src.join("slot1")).unwrap();
        std::fs::write(src.join("slot1").join("a.sav"), b"one").unwrap();
        std::fs::write(src.join("b.sav"), b"two").unwrap();
        let target = root.join("Drive");
        std::fs::create_dir_all(&target).unwrap();
        let job = Job { id: "j".into(), name: "Game saves".into(), sources: vec![src.to_string_lossy().into()], target: target.to_string_lossy().into(), keep: 1, ..Default::default() };

        let first = run(&job).unwrap();
        assert_eq!((first.copied, first.linked), (2, 0));
        // Same minute again: refused rather than overwritten
        assert!(run(&job).is_err());
        // Fake an older first snapshot so the second run gets its own folder
        let snap = snapshots(&job)[0].path.clone();
        let older = job_dir(&job).join("2000-01-01 00.00");
        std::fs::rename(&snap, &older).unwrap();
        std::fs::write(src.join("b.sav"), b"changed").unwrap();
        let second = run(&job).unwrap();
        assert_eq!((second.copied, second.linked), (1, 1), "unchanged file linked, changed one copied");
        let snaps = snapshots(&job);
        assert_eq!(snaps.len(), 1, "keep = 1 removes the older snapshot");
        assert_eq!(std::fs::read(Path::new(&snaps[0].path).join("Saves").join("slot1").join("a.sav")).unwrap(), b"one");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn refuses_a_target_inside_a_source_and_formats_stamps() {
        let home = std::env::temp_dir().join("user");
        let j = Job { sources: vec![home.to_string_lossy().into()], target: home.join("Backup").to_string_lossy().into(), ..Default::default() };
        assert!(validate(&j).is_err());
        assert_eq!(stamp(0), "1970-01-01 00.00");
        assert_eq!(stamp(1_700_000_000), "2023-11-14 22.13");
        let mut d = Job { schedule: "daily".into(), last_run: 1000, ..Default::default() };
        assert!(!due(&d, 1000 + 3600));
        assert!(due(&d, 1000 + 21 * 3600));
        d.schedule = "off".into();
        assert!(!due(&d, u64::MAX));
    }
}
