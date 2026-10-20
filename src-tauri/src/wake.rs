//! Wake & sleep inspector (Windows): what woke the PC, which devices and scheduled tasks may wake
//! it, and what is keeping it awake right now.
//!
//! Wakes come from the Power-Troubleshooter event (id 1), read by field name so the localized
//! message text doesn't matter. Wake-armed devices and sleep blockers come from powercfg.

use crate::sysutil::{powershell, powershell_raw, ps_quote};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Wake {
    /// Unix seconds.
    pub at: u64,
    /// How long it had slept, seconds (0 when unknown).
    pub slept: u64,
    /// "Device -USB Composite Device", "Timer - ...", "Power Button"...
    pub source: String,
    /// Owner of the wake timer, when a timer did it.
    pub timer_owner: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct WakeTask {
    pub path: String,
    pub name: String,
    /// Windows' own tasks are listed but not changed.
    pub system: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct Blocker {
    /// DISPLAY, SYSTEM, AWAYMODE, EXECUTION, PERFBOOST, ACTIVELOCKSCREEN
    pub kind: String,
    /// [PROCESS] / [DRIVER] / [SERVICE] plus its path or name.
    pub who: String,
    pub reason: String,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct WakeInfo {
    pub wakes: Vec<Wake>,
    pub devices: Vec<String>,
    pub tasks: Vec<WakeTask>,
    pub blockers: Vec<Blocker>,
    /// powercfg /requests needs administrator rights.
    pub blockers_need_admin: bool,
}

pub fn info() -> WakeInfo {
    let wakes: Vec<Wake> = powershell(
        r#"$out = @()
try {
  $ev = Get-WinEvent -FilterHashtable @{LogName='System'; ProviderName='Microsoft-Windows-Power-Troubleshooter'; Id=1} -MaxEvents 30 -ErrorAction Stop
  foreach ($e in $ev) { $x = [xml]$e.ToXml(); $d = @{}; $x.Event.EventData.Data | ForEach-Object { $d[$_.Name] = $_.'#text' }
    $slept = 0; try { $slept = [int64](([datetime]$d['WakeTime']) - ([datetime]$d['SleepTime'])).TotalSeconds } catch {}
    $out += [pscustomobject]@{ at = [int64]([DateTimeOffset]$e.TimeCreated).ToUnixTimeSeconds(); slept = [math]::Max(0, $slept); source = "$($d['WakeSourceText'])"; timer_owner = "$($d['WakeTimerOwner'])" } }
} catch {}
ConvertTo-Json -Compress -InputObject @($out)"#,
    )
    .unwrap_or_default();

    let devices: Vec<String> = powershell_raw("powercfg /devicequery wake_armed")
        .map(|o| o.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty() && !l.eq_ignore_ascii_case("NONE")).collect())
        .unwrap_or_default();
    // A single line without any letters means "none" in whatever language powercfg speaks
    let devices = if devices.len() == 1 && devices[0].len() < 8 { Vec::new() } else { devices };

    #[derive(Deserialize)]
    struct RawTask {
        path: String,
        name: String,
    }
    let tasks: Vec<WakeTask> = powershell::<Vec<RawTask>>(
        "ConvertTo-Json -Compress -InputObject @(Get-ScheduledTask | Where-Object { $_.Settings.WakeToRun -and $_.State -ne 'Disabled' } | ForEach-Object { [pscustomobject]@{ path = \"$($_.TaskPath)\"; name = \"$($_.TaskName)\" } })",
    )
    .unwrap_or_default()
    .into_iter()
    .map(|t| WakeTask { system: t.path.to_lowercase().starts_with("\\microsoft\\"), path: t.path, name: t.name })
    .collect();

    let (blockers, need_admin) = match powershell_raw("$o = powercfg /requests; if ($LASTEXITCODE) { throw 'denied' }; $o -join \"`n\"") {
        Ok(out) => (parse_requests(&out), false),
        Err(_) => (Vec::new(), !crate::mft::is_elevated()),
    };
    WakeInfo { wakes, devices, tasks, blockers, blockers_need_admin: need_admin }
}

/// `powercfg /requests`: category headers ("DISPLAY:") are not translated; under each, an entry
/// line ("[PROCESS] \Device\...\chrome.exe") is followed by its reason. Anything else ("None.")
/// is skipped.
pub fn parse_requests(out: &str) -> Vec<Blocker> {
    let mut kind = String::new();
    let mut list: Vec<Blocker> = Vec::new();
    for line in out.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if line.ends_with(':') && line[..line.len() - 1].chars().all(|c| c.is_ascii_uppercase()) {
            kind = line.trim_end_matches(':').to_string();
        } else if line.starts_with('[') {
            let tag = line.split(']').next().unwrap_or("").trim_start_matches('[').to_string();
            let rest = line.split_once(']').map(|(_, r)| r.trim()).unwrap_or(line);
            // Processes are shown by file name; drivers and services by their own name
            let who = if tag == "PROCESS" { rest.rsplit(['\\', '/']).next().unwrap_or(rest) } else { rest }.to_string();
            list.push(Blocker { kind: kind.clone(), who: format!("{tag}: {who}"), reason: String::new() });
        } else if let Some(last) = list.last_mut().filter(|b| b.reason.is_empty() && b.kind == kind) {
            last.reason = line.to_string();
        }
    }
    list
}

/// Allow or stop a device waking the PC (needs administrator rights).
pub fn set_device(name: &str, wake: bool) -> Result<(), String> {
    let flag = if wake { "/deviceenablewake" } else { "/devicedisablewake" };
    powershell_raw(&format!("powercfg {flag} {}; if ($LASTEXITCODE) {{ throw 'powercfg failed' }}", ps_quote(name))).map(|_| ())
}

/// Stop a non-Windows scheduled task from waking the PC (it still runs when the PC is awake).
pub fn disable_task_wake(path: &str, name: &str) -> Result<(), String> {
    if path.to_lowercase().starts_with("\\microsoft\\") {
        return Err("Windows tasks are not changed".into());
    }
    powershell_raw(&format!(
        "$t = Get-ScheduledTask -TaskPath {} -TaskName {} -ErrorAction Stop; $t.Settings.WakeToRun = $false; Set-ScheduledTask -InputObject $t -ErrorAction Stop | Out-Null",
        ps_quote(path),
        ps_quote(name)
    ))
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_power_requests() {
        let out = "DISPLAY:\r\n[PROCESS] \\Device\\HarddiskVolume3\\Program Files\\Google\\Chrome\\Application\\chrome.exe\r\nVideo Wake Lock\r\n\r\nSYSTEM:\r\n[DRIVER] Realtek High Definition Audio (HDAUDIO\\FUNC_01)\r\nAn audio stream is currently in use.\r\n\r\nAWAYMODE:\r\nNincs.\r\n";
        let b = parse_requests(out);
        assert_eq!(b.len(), 2);
        assert_eq!((b[0].kind.as_str(), b[0].who.as_str(), b[0].reason.as_str()), ("DISPLAY", "PROCESS: chrome.exe", "Video Wake Lock"));
        assert_eq!(b[1].kind, "SYSTEM");
        assert!(b[1].reason.starts_with("An audio stream"));
    }

    /// Reads this PC (run with --ignored --nocapture).
    #[test]
    #[ignore]
    fn smoke_this_pc() {
        let i = info();
        println!("wakes: {:?}", i.wakes.iter().take(5).collect::<Vec<_>>());
        println!("devices: {:?}", i.devices);
        println!("tasks: {:?}", i.tasks.iter().map(|t| (&t.name, t.system)).collect::<Vec<_>>());
        println!("blockers: {:?} need_admin {}", i.blockers, i.blockers_need_admin);
    }
}
