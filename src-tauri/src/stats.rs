//! A small local event log behind the weekly report and the Care history: space freed (by which
//! tool), game sessions, Care scores. Stays on this PC; capped at the last 2000 events.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Event {
    pub at: u64,
    /// freed, game, care
    pub kind: String,
    /// Tool that freed space, or game / module name.
    pub name: String,
    pub bytes: u64,
    /// Minutes played, or Care score.
    pub value: f32,
    /// Average FPS for game sessions.
    pub fps: Option<f32>,
}

#[derive(Serialize, Deserialize, Default)]
struct Log {
    events: Vec<Event>,
    /// Last weekly summary notification (Unix seconds).
    last_weekly: u64,
}

fn path(base: &Path) -> PathBuf {
    base.join("stats.json")
}
fn load(base: &Path) -> Log {
    std::fs::read(path(base)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}
fn save(base: &Path, l: &Log) {
    let _ = std::fs::create_dir_all(base);
    let _ = std::fs::write(path(base), serde_json::to_vec(l).unwrap_or_default());
}

fn push(base: &Path, e: Event) {
    let mut l = load(base);
    l.events.push(e);
    if l.events.len() > 2000 {
        let drop = l.events.len() - 2000;
        l.events.drain(..drop);
    }
    save(base, &l);
}

pub fn record_freed(base: &Path, tool: &str, bytes: u64) {
    if bytes > 0 {
        push(base, Event { at: crate::sysutil::now(), kind: "freed".into(), name: tool.into(), bytes, value: 0.0, fps: None });
    }
}

pub fn record_care(base: &Path, score: u32, found: u64) {
    push(base, Event { at: crate::sysutil::now(), kind: "care".into(), name: String::new(), bytes: found, value: score as f32, fps: None });
}

#[cfg(windows)]
pub fn record_game(base: &Path, r: &crate::gamemode::SessionReport) {
    let minutes = r.end.saturating_sub(r.start) as f32 / 60.0;
    if minutes >= 1.0 {
        push(base, Event { at: r.end, kind: "game".into(), name: r.game.clone(), bytes: 0, value: minutes, fps: r.fps.as_ref().map(|f| f.avg) });
    }
}

#[derive(Serialize)]
pub struct Report {
    pub from: u64,
    pub to: u64,
    pub freed: u64,
    /// (tool, bytes)
    pub freed_by: Vec<(String, u64)>,
    /// (game, minutes, average FPS)
    pub games: Vec<(String, f32, Option<f32>)>,
    /// (time, score)
    pub care: Vec<(u64, f32)>,
    /// Freed bytes per day over the period (oldest first).
    pub daily_freed: Vec<u64>,
}

/// Summary of the last `days` days.
pub fn report(base: &Path, days: u64) -> Report {
    let to = crate::sysutil::now();
    let from = to.saturating_sub(days * 86_400);
    let l = load(base);
    let recent: Vec<&Event> = l.events.iter().filter(|e| e.at >= from).collect();
    let mut freed_by: Vec<(String, u64)> = Vec::new();
    let mut daily = vec![0u64; days as usize];
    for e in recent.iter().filter(|e| e.kind == "freed") {
        match freed_by.iter_mut().find(|f| f.0 == e.name) {
            Some(f) => f.1 += e.bytes,
            None => freed_by.push((e.name.clone(), e.bytes)),
        }
        // An event at exactly "now" belongs to today, the last bucket
        let d = (((e.at - from) / 86_400) as usize).min(daily.len().saturating_sub(1));
        if let Some(slot) = daily.get_mut(d) {
            *slot += e.bytes;
        }
    }
    freed_by.sort_by_key(|f| std::cmp::Reverse(f.1));
    let mut games: Vec<(String, f32, Vec<f32>)> = Vec::new();
    for e in recent.iter().filter(|e| e.kind == "game") {
        match games.iter_mut().find(|g| g.0 == e.name) {
            Some(g) => {
                g.1 += e.value;
                g.2.extend(e.fps);
            }
            None => games.push((e.name.clone(), e.value, e.fps.into_iter().collect())),
        }
    }
    games.sort_by(|a, b| b.1.total_cmp(&a.1));
    // Care history reaches further back than the period, so the chart has a trend
    let care = l.events.iter().filter(|e| e.kind == "care").rev().take(60).map(|e| (e.at, e.value)).collect::<Vec<_>>().into_iter().rev().collect();
    Report {
        from,
        to,
        freed: freed_by.iter().map(|f| f.1).sum(),
        freed_by,
        games: games.into_iter().map(|(n, m, f)| (n, m, (!f.is_empty()).then(|| f.iter().sum::<f32>() / f.len() as f32))).collect(),
        care,
        daily_freed: daily,
    }
}

/// True (and remembered) once a week, when there is something to summarize.
pub fn weekly_due(base: &Path) -> bool {
    let mut l = load(base);
    let now = crate::sysutil::now();
    if l.last_weekly == 0 {
        // First run: start the clock instead of notifying right away
        l.last_weekly = now;
        save(base, &l);
        return false;
    }
    if now < l.last_weekly + 7 * 86_400 || l.events.iter().all(|e| e.at < l.last_weekly) {
        return false;
    }
    l.last_weekly = now;
    save(base, &l);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_events() {
        let dir = std::env::temp_dir().join(format!("tessera-stats-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        record_freed(&dir, "junk", 1000);
        record_freed(&dir, "junk", 500);
        record_freed(&dir, "privacy", 200);
        record_care(&dir, 80, 0);
        let r = report(&dir, 7);
        assert_eq!(r.freed, 1700);
        assert_eq!(r.freed_by[0], ("junk".to_string(), 1500));
        assert_eq!(r.care.len(), 1);
        assert_eq!(r.daily_freed.iter().sum::<u64>(), 1700);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
