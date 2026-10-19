//! Windows optimizer: one-click profiles (gaming, work, balanced, battery) that switch the power plan
//! and a small set of well-known, reversible Windows settings, plus startup-app and background-app
//! management.
//!
//! Safety rules:
//! - Only registry settings Windows itself reads for these features are touched; Defender, VBS,
//!   Windows Update, security mitigations and services are never changed and nothing is deleted.
//! - Before the first change, the original value of every setting a tweak touches is saved to
//!   `optimizer-backup.json`; "Restore original" writes those values back (or deletes values that
//!   did not exist before).
//! - Machine-wide (HKLM) tweaks need administrator rights and simply report failure without them.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use winreg::enums::*;
use winreg::RegKey;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "t", content = "v")]
enum Val {
    Dword(u32),
    Sz(String),
    Bin(Vec<u8>),
}

/// Value a tweak writes when on; `static`-friendly twin of `Val`.
enum On {
    D(u32),
    S(&'static str),
    B(&'static [u8]),
}

/// One registry value a tweak sets: (hive is HKLM?, key path, value name, value when the tweak is on).
struct Set {
    machine: bool,
    path: &'static str,
    name: &'static str,
    on: On,
}

pub struct Tweak {
    pub key: &'static str,
    sets: &'static [Set],
    /// Needs a restart (or sign-out) before it fully applies.
    pub reboot: bool,
    /// Switched on/off by profiles. Situational tweaks (e.g. multi-monitor fixes) are left alone
    /// when a profile is applied and only change when toggled by hand.
    pub profile: bool,
}

impl Tweak {
    /// Machine-wide: needs administrator rights.
    pub fn machine(&self) -> bool {
        self.sets.iter().any(|s| s.machine)
    }
}

macro_rules! dw {
    ($m:expr, $p:expr, $n:expr, $v:expr) => {
        Set { machine: $m, path: $p, name: $n, on: On::D($v) }
    };
}
macro_rules! sz {
    ($m:expr, $p:expr, $n:expr, $v:expr) => {
        Set { machine: $m, path: $p, name: $n, on: On::S($v) }
    };
}

const GAMEBAR: &str = r"Software\Microsoft\GameBar";
const DESKTOP: &str = r"Control Panel\Desktop";
const METRICS: &str = r"Control Panel\Desktop\WindowMetrics";
const MOUSE: &str = r"Control Panel\Mouse";
const MM: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile";
const MM_GAMES: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games";
const CDM: &str = r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager";
const ACTIVITY: &str = r"SOFTWARE\Policies\Microsoft\Windows\System";
/// "Adjust for best performance" visual-effects mask; font smoothing is a separate value and stays on.
const PERF_MASK: &[u8] = &[0x90, 0x12, 0x03, 0x80, 0x10, 0x00, 0x00, 0x00];

pub static TWEAKS: &[Tweak] = &[
    // --- gaming ---
    Tweak { key: "game_mode", reboot: false, profile: true, sets: &[dw!(false, GAMEBAR, "AutoGameModeEnabled", 1), dw!(false, GAMEBAR, "AllowAutoGameMode", 1)] },
    // Background recording (Xbox Game DVR) costs a few percent of GPU time while playing
    Tweak { key: "no_game_dvr", reboot: false, profile: true, sets: &[
        dw!(false, r"System\GameConfigStore", "GameDVR_Enabled", 0),
        dw!(false, r"Software\Microsoft\Windows\CurrentVersion\GameDVR", "AppCaptureEnabled", 0),
    ] },
    // Xbox Game Bar overlay (Win+G); Game Mode keeps working without it
    Tweak { key: "no_gamebar", reboot: false, profile: true, sets: &[dw!(false, GAMEBAR, "UseNexusForGameBarEnabled", 0)] },
    // "Adjust for best performance" in Performance Options, with smooth fonts kept
    Tweak { key: "perf_visuals", reboot: true, profile: true, sets: &[
        dw!(false, r"Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects", "VisualFXSetting", 3),
        Set { machine: false, path: DESKTOP, name: "UserPreferencesMask", on: On::B(PERF_MASK) },
        sz!(false, DESKTOP, "MenuShowDelay", "0"),
        sz!(false, METRICS, "MinAnimate", "0"),
        dw!(false, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "TaskbarAnimations", 0),
    ] },
    Tweak { key: "no_transparency", reboot: false, profile: true, sets: &[dw!(false, r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "EnableTransparency", 0)] },
    Tweak { key: "no_background_apps", reboot: false, profile: true, sets: &[dw!(false, r"Software\Microsoft\Windows\CurrentVersion\BackgroundAccessApplications", "GlobalUserDisabled", 1)] },
    // "Enhance pointer precision" off: 1:1 mouse movement for aiming
    Tweak { key: "raw_mouse", reboot: false, profile: true, sets: &[
        sz!(false, MOUSE, "MouseSpeed", "0"),
        sz!(false, MOUSE, "MouseThreshold1", "0"),
        sz!(false, MOUSE, "MouseThreshold2", "0"),
    ] },
    // Toast notifications off (a simple "do not disturb")
    Tweak { key: "focus", reboot: false, profile: true, sets: &[dw!(false, r"Software\Microsoft\Windows\CurrentVersion\Notifications\Settings", "NOC_GLOBAL_SETTING_TOASTS_ENABLED", 0)] },
    // Hardware-accelerated GPU scheduling
    Tweak { key: "hags", reboot: true, profile: true, sets: &[dw!(true, r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers", "HwSchMode", 2)] },
    // Multimedia class scheduler: don't throttle network or reserve CPU for background work while gaming
    Tweak { key: "net_throttling", reboot: true, profile: true, sets: &[dw!(true, MM, "NetworkThrottlingIndex", 0xffff_ffff), dw!(true, MM, "SystemResponsiveness", 10)] },
    // Same scheduler: give the "Games" task class high CPU, GPU and I/O priority
    Tweak { key: "game_priority", reboot: true, profile: true, sets: &[
        dw!(true, MM_GAMES, "Priority", 6),
        sz!(true, MM_GAMES, "Scheduling Category", "High"),
        sz!(true, MM_GAMES, "SFIO Priority", "High"),
        dw!(true, MM_GAMES, "GPU Priority", 8),
    ] },
    // Power throttling off: background and foreground apps always get full CPU speed
    Tweak { key: "no_power_throttling", reboot: true, profile: true, sets: &[dw!(true, r"SYSTEM\CurrentControlSet\Control\Power\PowerThrottling", "PowerThrottlingOff", 1)] },
    // --- work ---
    // Start menu search stays local: no Bing / web results
    Tweak { key: "no_web_search", reboot: false, profile: true, sets: &[
        dw!(false, r"Software\Policies\Microsoft\Windows\Explorer", "DisableSearchBoxSuggestions", 1),
        dw!(false, r"Software\Microsoft\Windows\CurrentVersion\Search", "BingSearchEnabled", 0),
    ] },
    // Silently installed "suggested" apps, tips and Start menu ads
    Tweak { key: "no_suggestions", reboot: false, profile: true, sets: &[
        dw!(false, CDM, "SilentInstalledAppsEnabled", 0),
        dw!(false, CDM, "SystemPaneSuggestionsEnabled", 0),
        dw!(false, CDM, "SoftLandingEnabled", 0),
        dw!(false, CDM, "SubscribedContent-338388Enabled", 0),
        dw!(false, CDM, "SubscribedContent-338389Enabled", 0),
    ] },
    // --- situational: never changed by profiles ---
    // Multiplane Overlay off: fixes stutter/flicker with monitors at different refresh rates (NVIDIA)
    Tweak { key: "no_mpo", reboot: true, profile: false, sets: &[dw!(true, r"SOFTWARE\Microsoft\Windows\Dwm", "OverlayTestMode", 5)] },
    // Right-click a taskbar app → "End task"
    Tweak { key: "end_task", reboot: false, profile: false, sets: &[dw!(false, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced\TaskbarDeveloperSettings", "TaskbarEndTask", 1)] },
    // Activity history (timeline of opened apps and files) off
    Tweak { key: "no_activity", reboot: false, profile: false, sets: &[
        dw!(true, ACTIVITY, "EnableActivityFeed", 0),
        dw!(true, ACTIVITY, "PublishUserActivities", 0),
        dw!(true, ACTIVITY, "UploadUserActivities", 0),
    ] },
    // Delivery Optimization: download updates from Microsoft only, never upload them to other PCs
    Tweak { key: "no_update_sharing", reboot: false, profile: false, sets: &[dw!(true, r"SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization", "DODownloadMode", 0)] },
];

fn on_value(s: &Set) -> Val {
    match s.on {
        On::D(d) => Val::Dword(d),
        On::S(x) => Val::Sz(x.into()),
        On::B(b) => Val::Bin(b.to_vec()),
    }
}

/// Values Windows ships with, used when a tweak is switched off and no backup exists.
/// `None` means Windows doesn't set the value at all (switching off deletes it).
fn default_value(s: &Set) -> Option<Val> {
    Some(match (s.path, s.name) {
        (_, "AutoGameModeEnabled") | (_, "AllowAutoGameMode") | (_, "UseNexusForGameBarEnabled") => Val::Dword(1),
        (_, "GameDVR_Enabled") | (_, "AppCaptureEnabled") => Val::Dword(1),
        (_, "VisualFXSetting") => Val::Dword(0),
        (_, "UserPreferencesMask") => Val::Bin(vec![0x9e, 0x1e, 0x07, 0x80, 0x12, 0x00, 0x00, 0x00]),
        (_, "MenuShowDelay") => Val::Sz("400".into()),
        (METRICS, "MinAnimate") => Val::Sz("1".into()),
        (_, "TaskbarAnimations") => Val::Dword(1),
        (_, "EnableTransparency") => Val::Dword(1),
        (_, "GlobalUserDisabled") => Val::Dword(0),
        (MOUSE, "MouseSpeed") => Val::Sz("1".into()),
        (MOUSE, "MouseThreshold1") => Val::Sz("6".into()),
        (MOUSE, "MouseThreshold2") => Val::Sz("10".into()),
        (_, "NOC_GLOBAL_SETTING_TOASTS_ENABLED") => Val::Dword(1),
        (_, "HwSchMode") => Val::Dword(1),
        (_, "NetworkThrottlingIndex") => Val::Dword(10),
        (_, "SystemResponsiveness") => Val::Dword(20),
        (MM_GAMES, "Priority") => Val::Dword(2),
        (MM_GAMES, "Scheduling Category") => Val::Sz("Medium".into()),
        (MM_GAMES, "SFIO Priority") => Val::Sz("Normal".into()),
        (MM_GAMES, "GPU Priority") => Val::Dword(8),
        (_, "BingSearchEnabled") => Val::Dword(1),
        (CDM, _) => Val::Dword(1),
        _ => return None,
    })
}

fn hive(machine: bool) -> RegKey {
    RegKey::predef(if machine { HKEY_LOCAL_MACHINE } else { HKEY_CURRENT_USER })
}

fn read(s: &Set) -> Option<Val> {
    let k = hive(s.machine).open_subkey(s.path).ok()?;
    let raw = k.get_raw_value(s.name).ok()?;
    Some(match raw.vtype {
        REG_DWORD => Val::Dword(u32::from_le_bytes(raw.bytes.get(..4)?.try_into().ok()?)),
        REG_SZ | REG_EXPAND_SZ => Val::Sz(k.get_value::<String, _>(s.name).ok()?),
        _ => Val::Bin(raw.bytes.to_vec()),
    })
}

fn write(s: &Set, v: &Option<Val>) -> Result<(), String> {
    let (k, _) = hive(s.machine).create_subkey(s.path).map_err(|e| format!("{}: {e}", s.path))?;
    match v {
        Some(Val::Dword(d)) => k.set_value(s.name, d),
        Some(Val::Sz(x)) => k.set_value(s.name, x),
        Some(Val::Bin(b)) => k.set_raw_value(s.name, &winreg::RegValue { bytes: b.clone().into(), vtype: REG_BINARY }),
        None => match k.delete_value(s.name) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            r => r,
        },
    }
    .map_err(|e| format!("{}\\{}: {e}", s.path, s.name))
}

fn reg_id(s: &Set) -> String {
    format!("{}\\{}\\{}", if s.machine { "HKLM" } else { "HKCU" }, s.path, s.name)
}

// ---------- backup ----------

#[derive(Serialize, Deserialize, Default)]
struct Backup {
    /// Original value per registry id; `None` = the value did not exist.
    values: BTreeMap<String, Option<Val>>,
    power_plan: Option<String>,
    startup: BTreeMap<String, bool>,
}

fn backup_path(base: &Path) -> std::path::PathBuf {
    base.join("optimizer-backup.json")
}
fn load_backup(base: &Path) -> Backup {
    std::fs::read(backup_path(base)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}
fn save_backup(base: &Path, b: &Backup) -> Result<(), String> {
    std::fs::create_dir_all(base).map_err(|e| e.to_string())?;
    std::fs::write(backup_path(base), serde_json::to_vec_pretty(b).unwrap()).map_err(|e| e.to_string())
}

/// Record the current value of `s` unless an original is already stored.
fn remember(b: &mut Backup, s: &Set) {
    b.values.entry(reg_id(s)).or_insert_with(|| read(s));
}

// ---------- tweaks ----------

pub fn tweak_on(t: &Tweak) -> bool {
    t.sets.iter().all(|s| read(s).as_ref() == Some(&on_value(s)))
}

#[derive(Serialize, Default)]
pub struct Outcome {
    pub applied: usize,
    pub failed: Vec<String>,
    pub reboot: bool,
}

fn set_tweak_inner(backup: &mut Backup, t: &Tweak, on: bool, out: &mut Outcome) {
    for s in t.sets {
        remember(backup, s);
        let target = if on {
            Some(on_value(s))
        } else {
            // Off: back to what the user had, or to the Windows default
            backup.values.get(&reg_id(s)).cloned().flatten().or_else(|| default_value(s))
        };
        if read(s) == target {
            continue;
        }
        match write(s, &target) {
            Ok(()) => {
                out.applied += 1;
                out.reboot |= t.reboot;
            }
            Err(e) => out.failed.push(if s.machine { format!("{} (administrator rights needed): {e}", t.key) } else { format!("{}: {e}", t.key) }),
        }
    }
}

pub fn set_tweak(base: &Path, key: &str, on: bool) -> Result<Outcome, String> {
    let t = TWEAKS.iter().find(|t| t.key == key).ok_or("unknown tweak")?;
    let mut backup = load_backup(base);
    let mut out = Outcome::default();
    set_tweak_inner(&mut backup, t, on, &mut out);
    save_backup(base, &backup)?;
    broadcast_change();
    Ok(out)
}

// ---------- profiles ----------

const PLAN_BALANCED: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";
const PLAN_HIGH: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
const PLAN_ULTIMATE: &str = "e9a42b02-d5df-448d-aa00-03f14749eb61";
const PLAN_SAVER: &str = "a1841308-3541-4fab-bc81-f71556f20b4a";

/// (profile, preferred power plans in order, tweaks switched on; every other tweak is switched off)
const PROFILES: &[(&str, &[&str], &[&str])] = &[
    ("gaming", &[PLAN_ULTIMATE, PLAN_HIGH, PLAN_BALANCED], &[
        "game_mode", "no_game_dvr", "no_gamebar", "perf_visuals", "no_background_apps", "raw_mouse", "focus", "hags",
        "net_throttling", "game_priority", "no_power_throttling",
    ]),
    ("work", &[PLAN_BALANCED], &["focus", "no_web_search", "no_suggestions"]),
    ("balanced", &[PLAN_BALANCED], &["game_mode"]),
    ("battery", &[PLAN_SAVER, PLAN_BALANCED], &["no_background_apps", "no_transparency", "perf_visuals"]),
];

pub fn apply_profile(base: &Path, profile: &str, elevated: bool) -> Result<Outcome, String> {
    let (_, plans, on) = PROFILES.iter().find(|p| p.0 == profile).ok_or("unknown profile")?;
    let mut backup = load_backup(base);
    let mut out = Outcome::default();
    for t in TWEAKS.iter().filter(|t| t.profile) {
        // Machine-wide tweaks are skipped silently without admin rights instead of failing the profile
        if !elevated && t.machine() {
            continue;
        }
        set_tweak_inner(&mut backup, t, on.contains(&t.key), &mut out);
    }
    let available = power_plans();
    if backup.power_plan.is_none() {
        backup.power_plan = available.iter().find(|p| p.active).map(|p| p.guid.clone());
    }
    if let Some(plan) = plans.iter().find(|g| available.iter().any(|p| p.guid.eq_ignore_ascii_case(g))) {
        match set_power_plan(plan) {
            Ok(()) => out.applied += 1,
            Err(e) => out.failed.push(e),
        }
    }
    save_backup(base, &backup)?;
    std::fs::write(base.join("optimizer-profile.txt"), profile).map_err(|e| e.to_string())?;
    broadcast_change();
    Ok(out)
}

pub fn active_profile(base: &Path) -> Option<String> {
    std::fs::read_to_string(base.join("optimizer-profile.txt")).ok().map(|s| s.trim().to_string())
}

pub fn has_backup(base: &Path) -> bool {
    backup_path(base).exists()
}

/// Put back every original value, power plan and startup state, then forget the backup.
pub fn restore(base: &Path) -> Result<Outcome, String> {
    let backup = load_backup(base);
    let mut out = Outcome::default();
    for t in TWEAKS {
        for s in t.sets {
            if let Some(orig) = backup.values.get(&reg_id(s)) {
                if read(s) == *orig {
                    continue;
                }
                match write(s, orig) {
                    Ok(()) => {
                        out.applied += 1;
                        out.reboot |= t.reboot;
                    }
                    Err(e) => out.failed.push(e),
                }
            }
        }
    }
    if let Some(plan) = &backup.power_plan {
        match set_power_plan(plan) {
            Ok(()) => out.applied += 1,
            Err(e) => out.failed.push(e),
        }
    }
    for (id, enabled) in &backup.startup {
        match set_startup(id, *enabled) {
            Ok(()) => out.applied += 1,
            Err(e) => out.failed.push(e),
        }
    }
    if out.failed.is_empty() {
        let _ = std::fs::remove_file(backup_path(base));
        let _ = std::fs::remove_file(base.join("optimizer-profile.txt"));
    }
    broadcast_change();
    Ok(out)
}

/// Let running apps (Explorer, the taskbar) pick up changed settings without a sign-out.
fn broadcast_change() {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    let area: Vec<u16> = "ImmersiveColorSet\0".encode_utf16().collect();
    let mut res = 0usize;
    // SAFETY: plain Win32 calls with valid, NUL-terminated buffers that outlive the calls.
    unsafe {
        SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, area.as_ptr() as isize, SMTO_ABORTIFHUNG, 200, &mut res);
        // Mouse acceleration is read from the registry only at sign-in unless pushed like this.
        // SPI_SETMOUSE takes [threshold1, threshold2, speed].
        let mouse = |name: &'static str, def: i32| match read(&sz!(false, MOUSE, name, "")) {
            Some(Val::Sz(s)) => s.parse().unwrap_or(def),
            _ => def,
        };
        let mut acc = [mouse("MouseThreshold1", 6), mouse("MouseThreshold2", 10), mouse("MouseSpeed", 1)];
        SystemParametersInfoW(SPI_SETMOUSE, 0, acc.as_mut_ptr().cast(), SPIF_SENDCHANGE);
    }
}

// ---------- power plans ----------

#[derive(Serialize)]
pub struct PowerPlan {
    pub guid: String,
    pub name: String,
    pub active: bool,
}

fn powercfg(args: &[&str]) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("powercfg").args(args).creation_flags(CREATE_NO_WINDOW).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("powercfg {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(decode_oem(&out.stdout))
}

/// Console programs print in the OEM code page (852 on Hungarian Windows), not UTF-8.
fn decode_oem(b: &[u8]) -> String {
    use windows_sys::Win32::Globalization::{MultiByteToWideChar, CP_OEMCP};
    if b.is_empty() {
        return String::new();
    }
    let mut wide = vec![0u16; b.len()];
    // SAFETY: input and output buffers are valid for the lengths passed; one byte never yields
    // more than one UTF-16 unit in OEM code pages.
    let n = unsafe { MultiByteToWideChar(CP_OEMCP, 0, b.as_ptr(), b.len() as i32, wide.as_mut_ptr(), wide.len() as i32) };
    String::from_utf16_lossy(&wide[..n.max(0) as usize])
}

/// Parses `powercfg /list`: "Power Scheme GUID: <guid>  (<name>) *" (the label text is localized).
pub fn power_plans() -> Vec<PowerPlan> {
    let Ok(text) = powercfg(&["/list"]) else { return Vec::new() };
    text.lines()
        .filter_map(|l| {
            let colon = l.find(':')?;
            let rest = l[colon + 1..].trim();
            let (guid, tail) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            if guid.len() != 36 {
                return None;
            }
            let name = tail.trim().trim_end_matches('*').trim().trim_start_matches('(').trim_end_matches(')').to_string();
            Some(PowerPlan { guid: guid.to_lowercase(), name, active: tail.trim_end().ends_with('*') })
        })
        .collect()
}

pub fn set_power_plan(guid: &str) -> Result<(), String> {
    if guid.len() != 36 || !guid.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return Err("invalid power plan".into());
    }
    powercfg(&["/setactive", guid]).map(|_| ())
}

// ---------- startup apps ----------

#[derive(Serialize)]
pub struct StartupApp {
    pub id: String,
    pub name: String,
    pub command: String,
    pub enabled: bool,
    /// "user" entries can always be changed; "machine" ones need admin rights.
    pub scope: &'static str,
}

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const APPROVED32: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32";

const APPROVED_FOLDER: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder";

/// (id prefix, hive is HKLM?, Run key, matching StartupApproved key)
const SOURCES: &[(&str, bool, &str, &str)] = &[("hkcu", false, RUN, APPROVED), ("hklm", true, RUN, APPROVED), ("hklm32", true, RUN32, APPROVED32)];
/// Startup folders: (id prefix, all users?, base folder variable). Their StartupApproved values are
/// named after the shortcut file.
const FOLDERS: &[(&str, bool, &str)] = &[("folder", false, "APPDATA"), ("folder_all", true, "ProgramData")];

fn startup_folder(var: &str) -> Option<std::path::PathBuf> {
    crate::sysutil::env_path(var).map(|b| b.join(r"Microsoft\Windows\Start Menu\Programs\Startup"))
}

/// Same switch Task Manager's Startup tab flips: the first byte of the StartupApproved value is
/// even when enabled and odd when disabled; a missing value means enabled.
fn approved(machine: bool, key: &str, name: &str) -> bool {
    hive(machine)
        .open_subkey(key)
        .and_then(|k| k.get_raw_value(name))
        .map(|v| v.bytes.first().is_none_or(|b| b % 2 == 0))
        .unwrap_or(true)
}

pub fn startup_list() -> Vec<StartupApp> {
    let mut out = Vec::new();
    for (prefix, machine, run, appr) in SOURCES {
        let Ok(k) = hive(*machine).open_subkey(run) else { continue };
        for (name, value) in k.enum_values().flatten() {
            out.push(StartupApp {
                id: format!("{prefix}:{name}"),
                enabled: approved(*machine, appr, &name),
                command: value.to_string().trim_matches('"').to_string(),
                name,
                scope: if *machine { "machine" } else { "user" },
            });
        }
    }
    for (prefix, machine, var) in FOLDERS {
        let Some(dir) = startup_folder(var) else { continue };
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_file())) {
            let file = e.file_name().to_string_lossy().into_owned();
            if file.eq_ignore_ascii_case("desktop.ini") {
                continue;
            }
            let command = std::fs::read(e.path()).ok().and_then(|b| crate::shortcuts::parse_lnk(&b)).unwrap_or_else(|| e.path().to_string_lossy().into_owned());
            out.push(StartupApp {
                id: format!("{prefix}:{file}"),
                enabled: approved(*machine, APPROVED_FOLDER, &file),
                name: std::path::Path::new(&file).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or(file.clone()),
                command,
                scope: if *machine { "machine" } else { "user" },
            });
        }
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    out
}

pub fn set_startup(id: &str, enabled: bool) -> Result<(), String> {
    let (prefix, name) = id.split_once(':').ok_or("bad id")?;
    let folder = FOLDERS.iter().find(|f| f.0 == prefix).map(|f| (f.0, f.1, "", APPROVED_FOLDER));
    let (_, machine, _, appr) = SOURCES.iter().copied().find(|s| s.0 == prefix).or(folder).ok_or("bad id")?;
    let machine = &machine;
    let (k, _) = hive(*machine).create_subkey(appr).map_err(|e| format!("{name}: {e}"))?;
    let mut bytes = vec![0u8; 12];
    bytes[0] = if enabled { 2 } else { 3 };
    k.set_raw_value(name, &winreg::RegValue { bytes: bytes.into(), vtype: REG_BINARY })
        .map_err(|e| if *machine { format!("{name}: administrator rights needed ({e})") } else { format!("{name}: {e}") })
}

pub fn set_startup_tracked(base: &Path, id: &str, enabled: bool) -> Result<(), String> {
    let mut backup = load_backup(base);
    if !backup.startup.contains_key(id) {
        let was = startup_list().iter().find(|a| a.id == id).map(|a| a.enabled).unwrap_or(true);
        backup.startup.insert(id.to_string(), was);
    }
    set_startup(id, enabled)?;
    save_backup(base, &backup)
}

// ---------- background apps ----------

/// Process names (lowercase, without .exe) worth closing before a game, by group.
const KNOWN: &[(&str, &[&str])] = &[
    ("browser", &["chrome", "msedge", "firefox", "opera", "brave", "vivaldi"]),
    ("chat", &["discord", "teams", "ms-teams", "slack", "skype", "telegram", "whatsapp", "zoom"]),
    ("launcher", &["epicgameslauncher", "eadesktop", "origin", "ubisoftconnect", "upc", "battle.net", "galaxyclient", "riotclientservices"]),
    ("sync", &["onedrive", "dropbox", "googledrivefs", "icloudservices"]),
    ("media", &["spotify", "itunes"]),
    ("office", &["winword", "excel", "powerpnt", "outlook", "onenote"]),
    // The WSL VM (Docker's backend) holds gigabytes of RAM; it is stopped with `wsl --shutdown`
    ("dev", &["docker desktop", "com.docker.backend", "vmmemwsl", "fork"]),
    ("server", &["plex media server", "plexscripthost"]),
    ("torrent", &["btweb", "bittorrent", "utorrent", "qbittorrent"]),
];

/// Processes that can't be killed directly but go away when WSL shuts down.
const WSL: &[&str] = &["vmmemwsl"];

#[derive(Serialize)]
pub struct AppProcess {
    pub name: String,
    pub pids: Vec<u32>,
    pub memory: u64,
    pub cpu: f32,
    pub group: &'static str,
}

pub fn processes(sys: &mut sysinfo::System) -> Vec<AppProcess> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, UpdateKind};
    let kind = ProcessRefreshKind::nothing().with_memory().with_cpu().with_exe(UpdateKind::Never);
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
    let cores = sys.cpus().len().max(1) as f32;
    let mut by_name: BTreeMap<String, AppProcess> = BTreeMap::new();
    for (pid, p) in sys.processes() {
        let name = p.name().to_string_lossy().into_owned();
        let stem = name.to_lowercase().trim_end_matches(".exe").to_string();
        let Some((group, _)) = KNOWN.iter().find(|(_, names)| names.contains(&stem.as_str())) else { continue };
        let e = by_name.entry(stem).or_insert_with(|| AppProcess { name, pids: vec![], memory: 0, cpu: 0.0, group });
        e.pids.push(pid.as_u32());
        e.memory += p.memory();
        e.cpu += p.cpu_usage() / cores;
    }
    let mut v: Vec<AppProcess> = by_name.into_values().collect();
    v.sort_by_key(|p| std::cmp::Reverse(p.memory));
    v
}

/// Only processes from the known list can be closed, never arbitrary pids.
pub fn kill(sys: &mut sysinfo::System, pids: &[u32]) -> Outcome {
    let mut out = Outcome::default();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let mut wsl = false;
    for pid in pids {
        let Some(p) = sys.process(sysinfo::Pid::from_u32(*pid)) else { continue };
        let stem = p.name().to_string_lossy().to_lowercase().trim_end_matches(".exe").to_string();
        if !KNOWN.iter().any(|(_, names)| names.contains(&stem.as_str())) {
            out.failed.push(format!("{stem}: not an app Tessera manages"));
            continue;
        }
        if WSL.contains(&stem.as_str()) {
            wsl = true;
            continue;
        }
        if p.kill() { out.applied += 1 } else { out.failed.push(format!("{stem} ({pid})")) }
    }
    if wsl {
        use std::os::windows::process::CommandExt;
        match std::process::Command::new("wsl").arg("--shutdown").creation_flags(0x0800_0000).status() {
            Ok(s) if s.success() => out.applied += 1,
            _ => out.failed.push("wsl --shutdown".into()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Read-only: lists plans and parses them, changes nothing.
    #[test]
    fn parses_power_plans() {
        let plans = power_plans();
        assert!(!plans.is_empty(), "powercfg /list returned no plans");
        assert!(plans.iter().all(|p| p.guid.len() == 36 && !p.name.is_empty()));
        assert_eq!(plans.iter().filter(|p| p.active).count(), 1);
    }

    #[test]
    fn every_profile_uses_known_tweaks() {
        for (_, _, on) in PROFILES {
            for k in *on {
                let t = TWEAKS.iter().find(|t| t.key == *k).unwrap_or_else(|| panic!("unknown tweak {k}"));
                assert!(t.profile, "{k} is used by a profile but not marked as a profile tweak");
            }
        }
        let keys: std::collections::HashSet<_> = TWEAKS.iter().map(|t| t.key).collect();
        assert_eq!(keys.len(), TWEAKS.len(), "duplicate tweak key");
    }

    #[test]
    fn values_round_trip_through_the_backup_format() {
        for v in [Val::Dword(5), Val::Sz("High".into()), Val::Bin(PERF_MASK.to_vec())] {
            let json = serde_json::to_string(&Some(v.clone())).unwrap();
            assert_eq!(serde_json::from_str::<Option<Val>>(&json).unwrap(), Some(v));
        }
    }
}
