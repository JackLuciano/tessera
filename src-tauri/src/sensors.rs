//! Hardware sensors without kernel drivers.
//!
//! - Windows: performance counters (CPU load, clock, performance limit, thermal zones, GPU engine
//!   load, memory lists) and `nvidia-smi`, which ships with the NVIDIA driver (temperature, clocks,
//!   power, fan, throttle reasons). AMD/Intel GPUs report load only. ACPI thermal zones are what
//!   Windows exposes without a driver; on many boards they lag behind the real CPU temperature,
//!   so the UI labels the CPU value as approximate.
//! - Linux / macOS: sysinfo's components (hwmon / SMC) plus `nvidia-smi` when present.
//!
//! A sampler thread keeps ten minutes of history, records idle temperatures per day (for the
//! "time to clean the fans" trend) and logs throttling.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

#[derive(Serialize, Clone, Default, Debug)]
pub struct Gpu {
    pub name: String,
    pub load: Option<f32>,
    pub temp: Option<f32>,
    pub mhz: Option<f32>,
    pub max_mhz: Option<f32>,
    pub mem_used: Option<u64>,
    pub mem_total: Option<u64>,
    pub power: Option<f32>,
    pub power_limit: Option<f32>,
    pub fan: Option<f32>,
    /// thermal, power (current throttle reasons)
    pub throttle: Vec<&'static str>,
}

#[derive(Serialize, Clone, Default, Debug)]
pub struct Snapshot {
    /// Unix milliseconds.
    pub at: u64,
    pub cpu_load: f32,
    pub cpu_mhz: Option<f32>,
    pub cpu_base_mhz: Option<f32>,
    /// Below 100 the CPU is held back (power or heat).
    pub cpu_limit: Option<f32>,
    /// Hottest thermal zone / CPU package sensor.
    pub cpu_temp: Option<f32>,
    /// The CPU temperature comes from ACPI thermal zones (Windows): approximate.
    pub cpu_temp_approx: bool,
    pub gpus: Vec<Gpu>,
    pub ram_used: u64,
    pub ram_total: u64,
    /// Standby list (cached memory the system can drop), Windows.
    pub ram_standby: Option<u64>,
    /// Other temperature sensors (label, °C).
    pub sensors: Vec<(String, f32)>,
    /// Fan speeds (label, RPM) where the platform exposes them.
    pub fans: Vec<(String, f32)>,
    /// Share of time the disks were busy, 0–100 (Windows).
    pub disk_busy: Option<f32>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ThrottleEvent {
    pub start: u64,
    pub end: u64,
    /// cpu_limit, cpu_thermal, gpu_thermal, gpu_power
    pub kind: String,
    /// Worst value seen (limit % or °C).
    pub worst: f32,
}

#[derive(Serialize, Deserialize, Default, Clone)]
struct DayIdle {
    cpu: Vec<f32>,
    gpu: Vec<f32>,
}

#[derive(Serialize, Deserialize, Default)]
struct Trend {
    /// YYYY-MM-DD → idle samples (kept short: a median is stored per day once the day is over).
    days: BTreeMap<String, DayIdle>,
}

struct State {
    history: VecDeque<Snapshot>,
    events: VecDeque<ThrottleEvent>,
    open: Vec<ThrottleEvent>,
    trend: Trend,
    base: Option<PathBuf>,
    /// While a game session runs, throttling is also collected here for its report.
    session: Option<Vec<ThrottleEvent>>,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(State { history: VecDeque::new(), events: VecDeque::new(), open: Vec::new(), trend: Trend::default(), base: None, session: None }))
}

const HISTORY: usize = 300; // 10 minutes at one sample every 2 s
const INTERVAL: Duration = Duration::from_secs(2);

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

// ---------- NVIDIA (nvidia-smi) ----------

fn nvidia_smi() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let p = crate::sysutil::env_path("SystemRoot").unwrap_or_else(|| PathBuf::from(r"C:\Windows")).join(r"System32\nvidia-smi.exe");
        if p.exists() {
            return Some(p);
        }
        let p = PathBuf::from(r"C:\Program Files\NVIDIA Corporation\NVSMI\nvidia-smi.exe");
        if p.exists() {
            return Some(p);
        }
        None
    }
    #[cfg(not(windows))]
    {
        ["/usr/bin/nvidia-smi", "/usr/local/bin/nvidia-smi"].iter().map(PathBuf::from).find(|p| p.exists())
    }
}

const NV_FIELDS: &str = "name,temperature.gpu,utilization.gpu,clocks.gr,clocks.max.gr,memory.used,memory.total,power.draw,power.limit,fan.speed";

fn nvidia(smi: &Path, reason_field: &mut Option<&'static str>) -> Vec<Gpu> {
    // Older drivers call the throttle field clocks_throttle_reasons, newer clocks_event_reasons
    let field = reason_field.unwrap_or("clocks_event_reasons.active");
    let run = |f: &str| crate::sysutil::command(&smi.to_string_lossy()).args([&format!("--query-gpu={NV_FIELDS},{f}"), "--format=csv,noheader,nounits"]).output().ok();
    let mut out = run(field);
    if out.as_ref().is_some_and(|o| !o.status.success()) && reason_field.is_none() {
        out = run("clocks_throttle_reasons.active");
        if out.as_ref().is_some_and(|o| o.status.success()) {
            *reason_field = Some("clocks_throttle_reasons.active");
        }
    } else if reason_field.is_none() {
        *reason_field = Some(field);
    }
    let Some(out) = out.filter(|o| o.status.success()) else { return Vec::new() };
    String::from_utf8_lossy(&out.stdout).lines().filter_map(parse_nvidia_line).collect()
}

fn parse_nvidia_line(line: &str) -> Option<Gpu> {
    let f: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
    if f.len() < 11 {
        return None;
    }
    let num = |i: usize| f[i].parse::<f32>().ok();
    let reasons = u64::from_str_radix(f[10].trim_start_matches("0x"), 16).unwrap_or(0);
    let mut throttle = Vec::new();
    // SwThermal 0x20, HwThermal 0x40, HwSlowdown 0x8; SwPowerCap 0x4, HwPowerBrake 0x80
    if reasons & (0x20 | 0x40 | 0x8) != 0 {
        throttle.push("thermal");
    }
    if reasons & (0x4 | 0x80) != 0 {
        throttle.push("power");
    }
    Some(Gpu {
        name: f[0].to_string(),
        temp: num(1),
        load: num(2),
        mhz: num(3),
        max_mhz: num(4),
        mem_used: num(5).map(|m| (m as u64) * 1024 * 1024),
        mem_total: num(6).map(|m| (m as u64) * 1024 * 1024),
        power: num(7),
        power_limit: num(8),
        fan: num(9),
        throttle,
    })
}

// ---------- sampling ----------

#[cfg(windows)]
struct Sampler {
    query: Option<crate::perf::Query>,
    smi: Option<PathBuf>,
    reason_field: Option<&'static str>,
    sys: sysinfo::System,
    gpu_names: Vec<String>,
}

#[cfg(windows)]
const COUNTERS: &[&str] = &[
    r"\Processor Information(_Total)\% Processor Utility",
    r"\Processor Information(_Total)\% Processor Performance",
    r"\Processor Information(_Total)\% Performance Limit",
    r"\Processor Information(_Total)\Processor Frequency",
    r"\Thermal Zone Information(*)\High Precision Temperature",
    r"\GPU Engine(*engtype_3D)\Utilization Percentage",
    r"\Memory\Standby Cache Normal Priority Bytes",
    r"\Memory\Standby Cache Reserve Bytes",
    r"\Memory\Standby Cache Core Bytes",
    r"\PhysicalDisk(_Total)\% Idle Time",
];

#[cfg(windows)]
impl Sampler {
    fn new() -> Self {
        // Adapter names once, for AMD / Intel cards that nvidia-smi doesn't cover
        let gpu_names = crate::sysutil::powershell::<Vec<String>>("ConvertTo-Json -Compress -InputObject @(Get-CimInstance Win32_VideoController | ForEach-Object { \"$($_.Name)\" })").unwrap_or_default();
        Self { query: crate::perf::Query::new(COUNTERS), smi: nvidia_smi(), reason_field: None, sys: sysinfo::System::new(), gpu_names }
    }

    fn sample(&mut self) -> Snapshot {
        let mut s = Snapshot { at: now_ms(), ..Default::default() };
        self.sys.refresh_memory();
        s.ram_used = self.sys.used_memory();
        s.ram_total = self.sys.total_memory();
        if let Some(q) = &self.query {
            q.collect();
            let v = q.values();
            let get = |k: &str| v.get(k).copied();
            s.cpu_load = get(COUNTERS[0]).unwrap_or(0.0).clamp(0.0, 100.0) as f32;
            let perf = get(COUNTERS[1]);
            s.cpu_limit = get(COUNTERS[2]).map(|x| x as f32);
            s.cpu_base_mhz = get(COUNTERS[3]).map(|x| x as f32);
            s.cpu_mhz = match (perf, s.cpu_base_mhz) {
                (Some(p), Some(b)) => Some((p as f32 / 100.0) * b),
                _ => None,
            };
            // Thermal zones report tenths of a kelvin; implausible values (some boards report 0
            // or a fixed 27.8 °C) are dropped
            let mut zones: Vec<(String, f32)> = v
                .iter()
                .filter(|(k, _)| k.starts_with(r"\Thermal Zone Information"))
                .map(|(k, t)| (k.rsplit('|').next().unwrap_or("").to_string(), (*t as f32) / 10.0 - 273.15))
                .filter(|(_, c)| *c > 5.0 && *c < 125.0)
                .collect();
            zones.sort_by(|a, b| a.0.cmp(&b.0));
            s.cpu_temp = zones.iter().map(|z| z.1).reduce(f32::max);
            s.cpu_temp_approx = true;
            s.sensors = zones;
            let standby: f64 = [6, 7, 8].iter().filter_map(|i| get(COUNTERS[*i])).sum();
            s.ram_standby = (standby > 0.0).then_some(standby as u64);
            s.disk_busy = get(COUNTERS[9]).map(|idle| (100.0 - idle).clamp(0.0, 100.0) as f32);
            // 3D engine load per adapter (instance names contain luid_0x..._0x...)
            let mut per_adapter: BTreeMap<String, f64> = BTreeMap::new();
            for (k, val) in v.iter().filter(|(k, _)| k.starts_with(r"\GPU Engine")) {
                let inst = k.rsplit('|').next().unwrap_or("");
                let luid = inst.split("_phys").next().unwrap_or(inst).split("luid_").nth(1).unwrap_or("").to_string();
                *per_adapter.entry(luid).or_default() += val;
            }
            let generic_load = per_adapter.values().copied().fold(0.0, f64::max).min(100.0) as f32;
            if let Some(smi) = &self.smi {
                s.gpus = nvidia(smi, &mut self.reason_field);
            }
            // Cards nvidia-smi doesn't know (AMD, Intel): name and load only
            for name in &self.gpu_names {
                let lower = name.to_lowercase();
                if lower.contains("nvidia") || lower.contains("microsoft basic") || lower.contains("remote") {
                    continue;
                }
                s.gpus.push(Gpu { name: name.clone(), load: Some(generic_load), ..Default::default() });
            }
            if s.gpus.is_empty() && generic_load > 0.0 {
                s.gpus.push(Gpu { name: "GPU".into(), load: Some(generic_load), ..Default::default() });
            }
        }
        s
    }
}

#[cfg(not(windows))]
struct Sampler {
    sys: sysinfo::System,
    comps: sysinfo::Components,
    smi: Option<PathBuf>,
    reason_field: Option<&'static str>,
}

#[cfg(not(windows))]
impl Sampler {
    fn new() -> Self {
        Self { sys: sysinfo::System::new(), comps: sysinfo::Components::new_with_refreshed_list(), smi: nvidia_smi(), reason_field: None }
    }

    fn sample(&mut self) -> Snapshot {
        let mut s = Snapshot { at: now_ms(), ..Default::default() };
        self.sys.refresh_memory();
        self.sys.refresh_cpu_usage();
        self.sys.refresh_cpu_frequency();
        s.ram_used = self.sys.used_memory();
        s.ram_total = self.sys.total_memory();
        s.cpu_load = self.sys.global_cpu_usage();
        s.cpu_mhz = self.sys.cpus().first().map(|c| c.frequency() as f32).filter(|f| *f > 0.0);
        self.comps.refresh(true);
        for c in self.comps.iter() {
            if let Some(t) = c.temperature().filter(|t| *t > 5.0 && *t < 125.0) {
                s.sensors.push((c.label().to_string(), t));
            }
        }
        // The CPU package sensor has a recognizable label on both hwmon and SMC
        let cpu_like = |l: &str| {
            let l = l.to_lowercase();
            l.contains("package") || l.contains("tctl") || l.contains("tdie") || l.contains("cpu") || l.contains("coretemp")
        };
        s.cpu_temp = s.sensors.iter().filter(|(l, _)| cpu_like(l)).map(|x| x.1).reduce(f32::max);
        if let Some(smi) = &self.smi {
            s.gpus = nvidia(smi, &mut self.reason_field);
        }
        #[cfg(target_os = "linux")]
        {
            s.fans = linux_fans();
            if s.gpus.is_empty() {
                s.gpus = linux_amdgpu(&s.sensors);
            }
        }
        s
    }
}

#[cfg(target_os = "linux")]
fn linux_fans() -> Vec<(String, f32)> {
    let mut out = Vec::new();
    for hw in std::fs::read_dir("/sys/class/hwmon").into_iter().flatten().flatten() {
        let dir = hw.path();
        let chip = std::fs::read_to_string(dir.join("name")).unwrap_or_default().trim().to_string();
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.starts_with("fan") && n.ends_with("_input")
                && let Some(rpm) = std::fs::read_to_string(e.path()).ok().and_then(|s| s.trim().parse::<f32>().ok())
            {
                out.push((format!("{chip} {}", n.trim_end_matches("_input")), rpm));
            }
        }
    }
    out
}

#[cfg(target_os = "linux")]
fn linux_amdgpu(sensors: &[(String, f32)]) -> Vec<Gpu> {
    let mut out = Vec::new();
    for card in std::fs::read_dir("/sys/class/drm").into_iter().flatten().flatten() {
        let busy = card.path().join("device/gpu_busy_percent");
        if let Some(load) = std::fs::read_to_string(&busy).ok().and_then(|s| s.trim().parse::<f32>().ok()) {
            let temp = sensors.iter().find(|(l, _)| l.to_lowercase().contains("amdgpu") || l.to_lowercase().contains("edge")).map(|x| x.1);
            out.push(Gpu { name: "AMD GPU".into(), load: Some(load), temp, ..Default::default() });
        }
    }
    out
}

/// Fold one sample into the throttle log: an event stays open while the condition holds.
fn track_throttle(st: &mut State, s: &Snapshot) {
    let mut active: Vec<(String, f32)> = Vec::new();
    if let Some(l) = s.cpu_limit.filter(|l| *l < 90.0 && s.cpu_load > 30.0) {
        active.push(("cpu_limit".into(), l));
    }
    if let Some(t) = s.cpu_temp.filter(|t| *t >= 95.0) {
        active.push(("cpu_thermal".into(), t));
    }
    for g in &s.gpus {
        if g.load.unwrap_or(0.0) < 30.0 {
            continue;
        }
        if g.throttle.contains(&"thermal") {
            active.push(("gpu_thermal".into(), g.temp.unwrap_or(0.0)));
        }
        if g.throttle.contains(&"power") {
            active.push(("gpu_power".into(), g.power.unwrap_or(0.0)));
        }
    }
    // Close events whose condition ended
    let (still, ended): (Vec<_>, Vec<_>) = st.open.drain(..).partition(|e| active.iter().any(|(k, _)| *k == e.kind));
    st.open = still;
    for e in ended {
        // Ignore blips shorter than one sample interval
        if e.end > e.start {
            if let Some(sess) = st.session.as_mut() {
                sess.push(e.clone());
            }
            st.events.push_back(e);
            while st.events.len() > 200 {
                st.events.pop_front();
            }
        }
    }
    for (kind, v) in active {
        match st.open.iter_mut().find(|e| e.kind == kind) {
            Some(e) => {
                e.end = s.at;
                e.worst = if kind == "cpu_limit" { e.worst.min(v) } else { e.worst.max(v) };
            }
            None => st.open.push(ThrottleEvent { start: s.at, end: s.at, kind, worst: v }),
        }
    }
}

fn today() -> String {
    // Local date is not worth a chrono dependency: UTC day boundaries are fine for a weekly trend
    let days = now_ms() / 86_400_000;
    let (y, m, d) = civil(days as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Days since 1970-01-01 → (year, month, day).
fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn median(v: &[f32]) -> Option<f32> {
    if v.is_empty() {
        return None;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    Some(s[s.len() / 2])
}

fn trend_path(base: &Path) -> PathBuf {
    base.join("thermal-trend.json")
}

/// Record idle temperatures (low load) once a minute at most, so a day's median reflects idle.
fn track_idle(st: &mut State, s: &Snapshot, last_idle: &mut u64) {
    let gpu_idle = s.gpus.iter().all(|g| g.load.unwrap_or(0.0) < 10.0);
    if s.cpu_load > 10.0 || !gpu_idle || s.at < *last_idle + 60_000 {
        return;
    }
    *last_idle = s.at;
    let day = st.trend.days.entry(today()).or_default();
    if let Some(t) = s.cpu_temp {
        day.cpu.push(t);
    }
    if let Some(t) = s.gpus.iter().filter_map(|g| g.temp).reduce(f32::max) {
        day.gpu.push(t);
    }
    // Earlier days are reduced to their median; at most 180 days are kept
    let key = today();
    for (k, d) in st.trend.days.iter_mut() {
        if *k != key && (d.cpu.len() > 1 || d.gpu.len() > 1) {
            d.cpu = median(&d.cpu).into_iter().collect();
            d.gpu = median(&d.gpu).into_iter().collect();
        }
    }
    while st.trend.days.len() > 180 {
        let first = st.trend.days.keys().next().cloned().unwrap_or_default();
        st.trend.days.remove(&first);
    }
    if let Some(base) = &st.base {
        let _ = std::fs::write(trend_path(base), serde_json::to_vec(&st.trend).unwrap_or_default());
    }
}

/// Start the background sampler (once).
pub fn start(base: PathBuf) {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    {
        let mut st = state().lock();
        st.trend = std::fs::read(trend_path(&base)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        st.base = Some(base);
    }
    std::thread::spawn(|| {
        let mut sampler = Sampler::new();
        let mut last_idle = 0u64;
        loop {
            let s = sampler.sample();
            {
                let mut st = state().lock();
                track_throttle(&mut st, &s);
                track_idle(&mut st, &s, &mut last_idle);
                st.history.push_back(s);
                while st.history.len() > HISTORY {
                    st.history.pop_front();
                }
            }
            std::thread::sleep(INTERVAL);
        }
    });
}

pub fn latest() -> Option<Snapshot> {
    state().lock().history.back().cloned()
}

pub fn history() -> Vec<Snapshot> {
    state().lock().history.iter().cloned().collect()
}

pub fn throttle_events() -> Vec<ThrottleEvent> {
    let st = state().lock();
    st.events.iter().chain(st.open.iter()).cloned().collect()
}

/// Begin / end collecting throttling for a game session (game mode is Windows-only).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn session_begin() {
    state().lock().session = Some(Vec::new());
}
#[cfg_attr(not(windows), allow(dead_code))]
pub fn session_end() -> Vec<ThrottleEvent> {
    let mut st = state().lock();
    let mut v = st.session.take().unwrap_or_default();
    v.extend(st.open.iter().cloned());
    v
}

#[derive(Serialize)]
pub struct TrendReport {
    pub days: Vec<DayTemps>,
    /// Idle CPU rise between the first and the last week of data, °C (needs 3 weeks of data).
    pub cpu_rise: Option<f32>,
    pub gpu_rise: Option<f32>,
}

/// (day, idle CPU °C, idle GPU °C)
type DayTemps = (String, Option<f32>, Option<f32>);

pub fn trend() -> TrendReport {
    let st = state().lock();
    let days: Vec<DayTemps> = st.trend.days.iter().map(|(k, d)| (k.clone(), median(&d.cpu), median(&d.gpu))).collect();
    let rise = |pick: fn(&DayTemps) -> Option<f32>| {
        let vals: Vec<f32> = days.iter().filter_map(pick).collect();
        if vals.len() < 21 {
            return None;
        }
        let avg = |s: &[f32]| s.iter().sum::<f32>() / s.len() as f32;
        Some(avg(&vals[vals.len() - 7..]) - avg(&vals[..7]))
    };
    TrendReport { cpu_rise: rise(|d| d.1), gpu_rise: rise(|d| d.2), days }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nvidia_smi_lines() {
        let g = parse_nvidia_line("NVIDIA GeForce RTX 4070, 61, 97, 2610, 3105, 7012, 12282, 182.40, 200.00, 54, 0x0000000000000004").unwrap();
        assert_eq!(g.temp, Some(61.0));
        assert_eq!(g.load, Some(97.0));
        assert_eq!(g.throttle, vec!["power"]);
        let idle = parse_nvidia_line("NVIDIA GeForce GTX 1060, 35, 0, 139, 2050, 300, 6144, [N/A], [N/A], 0, 0x0000000000000001").unwrap();
        assert!(idle.throttle.is_empty());
        assert_eq!(idle.power, None);
    }

    #[test]
    fn computes_civil_dates() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(19_782), (2024, 2, 29));
    }

    #[test]
    fn opens_and_closes_throttle_events() {
        let mut st = State { history: VecDeque::new(), events: VecDeque::new(), open: Vec::new(), trend: Trend::default(), base: None, session: Some(Vec::new()) };
        let hot = |at| Snapshot { at, cpu_load: 80.0, cpu_limit: Some(70.0), ..Default::default() };
        track_throttle(&mut st, &hot(1000));
        track_throttle(&mut st, &hot(3000));
        track_throttle(&mut st, &Snapshot { at: 5000, cpu_load: 80.0, cpu_limit: Some(100.0), ..Default::default() });
        assert_eq!(st.events.len(), 1);
        assert_eq!((st.events[0].start, st.events[0].end), (1000, 3000));
        assert_eq!(st.session.as_ref().unwrap().len(), 1);
    }
}
