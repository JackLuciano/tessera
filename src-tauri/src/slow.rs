//! "Why is my PC slow right now": one snapshot of what uses the CPU, memory, disk and network,
//! turned into plain findings. Two process samples a second apart give per-process CPU and disk
//! rates; known background jobs (Defender scans, indexing, updates...) get a friendly name.

use serde::Serialize;
use std::time::Duration;
use sysinfo::{Networks, ProcessRefreshKind, ProcessesToUpdate, System};

#[derive(Serialize, Clone, Debug)]
pub struct Proc {
    pub name: String,
    pub pid: u32,
    /// Share of the whole CPU, 0–100.
    pub cpu: f32,
    pub memory: u64,
    /// Disk read + write, bytes per second.
    pub disk: u64,
    /// Friendly description for well-known background jobs (a key the UI translates).
    pub known: Option<&'static str>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Finding {
    /// cpu, memory, disk, thermal, known, startup, fine
    pub kind: &'static str,
    /// 0 info, 1 worth a look, 2 the likely cause
    pub level: u8,
    /// Process (or known job key) the finding is about.
    pub subject: String,
    pub value: f64,
    /// Tessera page that helps: startup, background, temps, cleanup...
    pub page: Option<&'static str>,
}

#[derive(Serialize, Clone, Debug)]
pub struct SlowReport {
    pub cpu: f32,
    pub ram_used: u64,
    pub ram_total: u64,
    pub disk_busy: Option<f32>,
    /// Bytes per second over all network adapters.
    pub net_down: u64,
    pub net_up: u64,
    pub top_cpu: Vec<Proc>,
    pub top_mem: Vec<Proc>,
    pub top_disk: Vec<Proc>,
    pub findings: Vec<Finding>,
}

/// Process names (lowercase, without .exe) of background jobs users often don't recognise.
const KNOWN: &[(&str, &str)] = &[
    ("msmpeng", "defender"),
    ("mpdefendercoreservice", "defender"),
    ("searchindexer", "indexing"),
    ("searchprotocolhost", "indexing"),
    ("tiworker", "update"),
    ("trustedinstaller", "update"),
    ("wuauclt", "update"),
    ("usocoreworker", "update"),
    ("musnotification", "update"),
    ("compattelrunner", "telemetry"),
    ("onedrive", "sync"),
    ("dropbox", "sync"),
    ("googledrivefs", "sync"),
    ("steam", "launcher"),
    ("steamservice", "launcher"),
    ("epicgameslauncher", "launcher"),
    ("battle.net", "launcher"),
    ("memory compression", "compression"),
    ("dwm", "compositor"),
    ("wmiprvse", "wmi"),
    ("mrt", "malware_tool"),
];

fn known(name: &str) -> Option<&'static str> {
    let n = name.to_lowercase();
    let n = n.trim_end_matches(".exe");
    KNOWN.iter().find(|(k, _)| *k == n).map(|(_, v)| *v)
}

pub fn check() -> SlowReport {
    let mut sys = System::new();
    let kind = ProcessRefreshKind::nothing().with_cpu().with_memory().with_disk_usage();
    let mut nets = Networks::new_with_refreshed_list();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
    sys.refresh_cpu_usage();
    std::thread::sleep(Duration::from_millis(1000));
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    nets.refresh(true);
    let cpus = sys.cpus().len().max(1) as f32;
    let me = std::process::id();

    // Several processes of one app (browsers, Discord) count as one
    let mut by_name: Vec<Proc> = Vec::new();
    for (pid, p) in sys.processes() {
        if pid.as_u32() == me || pid.as_u32() == 0 {
            continue;
        }
        let name = p.name().to_string_lossy().into_owned();
        if name.eq_ignore_ascii_case("System Idle Process") || name.eq_ignore_ascii_case("Idle") {
            continue;
        }
        let d = p.disk_usage();
        let entry = Proc { name: name.trim_end_matches(".exe").to_string(), pid: pid.as_u32(), cpu: p.cpu_usage() / cpus, memory: p.memory(), disk: d.read_bytes + d.written_bytes, known: known(&name) };
        match by_name.iter_mut().find(|e| e.name.eq_ignore_ascii_case(&entry.name)) {
            Some(e) => {
                e.cpu += entry.cpu;
                e.memory += entry.memory;
                e.disk += entry.disk;
            }
            None => by_name.push(entry),
        }
    }
    let top = |key: fn(&Proc) -> f64| {
        let mut v: Vec<Proc> = by_name.iter().filter(|p| key(p) > 0.0).cloned().collect();
        v.sort_by(|a, b| key(b).total_cmp(&key(a)));
        v.truncate(6);
        v
    };
    let top_cpu = top(|p| p.cpu as f64);
    let top_mem = top(|p| p.memory as f64);
    let top_disk = top(|p| p.disk as f64);

    let snap = crate::sensors::latest().unwrap_or_default();
    let cpu = if snap.cpu_load > 0.0 { snap.cpu_load } else { sys.global_cpu_usage() };
    let (down, up) = nets.iter().fold((0, 0), |(d, u), (_, n)| (d + n.received(), u + n.transmitted()));
    let report = SlowReport {
        cpu,
        ram_used: sys.used_memory(),
        ram_total: sys.total_memory(),
        disk_busy: snap.disk_busy,
        net_down: down,
        net_up: up,
        findings: Vec::new(),
        top_cpu,
        top_mem,
        top_disk,
    };
    let gpu_hot = snap.gpus.iter().any(|g| g.throttle.contains(&"thermal"));
    let cpu_limited = snap.cpu_limit.is_some_and(|l| l < 85.0);
    let findings = findings(&report, gpu_hot, cpu_limited, startup_count());
    SlowReport { findings, ..report }
}

#[cfg(windows)]
fn startup_count() -> usize {
    crate::optimizer::startup_list().iter().filter(|a| a.enabled).count()
}

#[cfg(not(windows))]
fn startup_count() -> usize {
    0
}

/// Turn the numbers into findings, most important first.
pub fn findings(r: &SlowReport, gpu_hot: bool, cpu_limited: bool, startup_apps: usize) -> Vec<Finding> {
    let mut out = Vec::new();
    let f = |kind, level, subject: &str, value: f64, page| Finding { kind, level, subject: subject.to_string(), value, page };
    if cpu_limited || gpu_hot {
        out.push(f("thermal", 2, if gpu_hot { "GPU" } else { "CPU" }, 0.0, Some("temps")));
    }
    // A known background job doing real work explains a lot on its own
    for p in r.top_cpu.iter().chain(&r.top_disk).filter(|p| p.known.is_some() && (p.cpu > 8.0 || p.disk > 8 * 1024 * 1024)) {
        let key = p.known.unwrap_or_default();
        if !out.iter().any(|x: &Finding| x.kind == "known" && x.subject == key) {
            out.push(f("known", 2, key, p.cpu as f64, None));
        }
    }
    if r.cpu > 85.0 {
        let who = r.top_cpu.first().map(|p| p.name.as_str()).unwrap_or("");
        out.push(f("cpu", 2, who, r.cpu as f64, Some("background")));
    } else if let Some(p) = r.top_cpu.first().filter(|p| p.cpu > 25.0 && p.known.is_none()) {
        out.push(f("cpu", 1, &p.name, p.cpu as f64, Some("background")));
    }
    let ram = if r.ram_total > 0 { r.ram_used as f64 / r.ram_total as f64 * 100.0 } else { 0.0 };
    if ram > 85.0 {
        let who = r.top_mem.first().map(|p| p.name.as_str()).unwrap_or("");
        out.push(f("memory", 2, who, ram, Some("background")));
    }
    if r.disk_busy.is_some_and(|d| d > 80.0) {
        let who = r.top_disk.first().map(|p| p.name.as_str()).unwrap_or("");
        out.push(f("disk", 2, who, r.disk_busy.unwrap_or(0.0) as f64, None));
    }
    if startup_apps > 12 {
        out.push(f("startup", 1, "", startup_apps as f64, Some("startup")));
    }
    if out.is_empty() {
        out.push(f("fine", 0, "", 0.0, None));
    }
    out.sort_by_key(|x| std::cmp::Reverse(x.level));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(name: &str, cpu: f32) -> Proc {
        Proc { name: name.into(), pid: 1, cpu, memory: 0, disk: 0, known: known(name) }
    }

    #[test]
    fn names_known_jobs_and_flags_load() {
        assert_eq!(known("MsMpEng.exe"), Some("defender"));
        let r = SlowReport { cpu: 92.0, ram_used: 15, ram_total: 16, disk_busy: Some(10.0), net_down: 0, net_up: 0, top_cpu: vec![proc("MsMpEng", 40.0), proc("chrome", 30.0)], top_mem: vec![proc("chrome", 0.0)], top_disk: vec![], findings: vec![] };
        let f = findings(&r, false, false, 3);
        let kinds: Vec<&str> = f.iter().map(|x| x.kind).collect();
        assert_eq!(kinds, vec!["known", "cpu", "memory"]);
        assert_eq!(f[0].subject, "defender");
        let calm = SlowReport { cpu: 5.0, ram_used: 4, ram_total: 16, top_cpu: vec![], top_mem: vec![], ..r };
        assert_eq!(findings(&calm, false, false, 3)[0].kind, "fine");
    }

    /// Reads this PC (run with --ignored --nocapture).
    #[test]
    #[ignore]
    fn smoke_this_pc() {
        let r = check();
        println!("cpu {:.0}% ram {}/{} MB", r.cpu, r.ram_used >> 20, r.ram_total >> 20);
        println!("top cpu: {:?}", r.top_cpu.iter().map(|p| (&p.name, p.cpu)).collect::<Vec<_>>());
        println!("top disk: {:?}", r.top_disk.iter().map(|p| (&p.name, p.disk)).collect::<Vec<_>>());
        println!("findings: {:?}", r.findings);
    }
}
