//! PC health readouts: battery wear, crashes and blue screens, security status, Windows Update,
//! boot time, GPU driver age, and Windows component store cleanup. Everything here only reads,
//! except the component cleanup and the Windows Update cache reset, which the user starts.
// Only the battery readout exists outside Windows; the shared types are unused there
#![cfg_attr(not(windows), allow(dead_code))]

use serde::{Deserialize, Serialize};
#[cfg(windows)]
use crate::sysutil::powershell;

// ---------- battery ----------

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct Battery {
    pub present: bool,
    pub name: String,
    /// mWh
    pub design: u64,
    pub full: u64,
    pub cycles: Option<u64>,
    /// Percent of the design capacity that is gone.
    pub wear: Option<f32>,
    pub charge: Option<f32>,
    pub charging: bool,
}

#[cfg(windows)]
pub fn battery() -> Battery {
    // powercfg writes the report as XML; it's the only no-driver source for design capacity and cycles
    let path = std::env::temp_dir().join(format!("tessera-battery-{}.xml", std::process::id()));
    let ok = crate::sysutil::run("powercfg", &["/batteryreport", "/xml", "/output", &path.to_string_lossy()]).is_ok();
    let xml = if ok { std::fs::read_to_string(&path).unwrap_or_default() } else { String::new() };
    let _ = std::fs::remove_file(&path);
    let tag = |name: &str| -> Option<String> {
        let start = xml.find(&format!("<{name}>"))? + name.len() + 2;
        let end = xml[start..].find(&format!("</{name}>"))? + start;
        Some(xml[start..end].trim().to_string())
    };
    let mut b = Battery::default();
    if let (Some(design), Some(full)) = (tag("DesignCapacity").and_then(|v| v.parse::<u64>().ok()), tag("FullChargeCapacity").and_then(|v| v.parse::<u64>().ok())) {
        b.present = design > 0;
        b.design = design;
        b.full = full;
        b.cycles = tag("CycleCount").and_then(|v| v.parse().ok()).filter(|c| *c > 0);
        b.name = [tag("Manufacturer"), tag("Id")].into_iter().flatten().collect::<Vec<_>>().join(" ");
        b.wear = (design > 0).then(|| (1.0 - full as f32 / design as f32).max(0.0) * 100.0);
    }
    #[derive(Deserialize)]
    struct Now {
        charge: Option<f32>,
        status: Option<u32>,
    }
    if b.present
        && let Ok(v) = powershell::<Vec<Now>>("ConvertTo-Json -Compress -InputObject @(Get-CimInstance Win32_Battery | Select-Object @{n='charge';e={[double]$_.EstimatedChargeRemaining}},@{n='status';e={[int]$_.BatteryStatus}})")
        && let Some(n) = v.first()
    {
        b.charge = n.charge;
        // 2 = on AC, 6-9 = charging states
        b.charging = matches!(n.status, Some(2 | 6..=9));
    }
    b
}

#[cfg(target_os = "linux")]
pub fn battery() -> Battery {
    let base = std::path::Path::new("/sys/class/power_supply");
    let read = |p: &std::path::Path, f: &str| std::fs::read_to_string(p.join(f)).ok().map(|s| s.trim().to_string());
    for e in std::fs::read_dir(base).into_iter().flatten().flatten() {
        let p = e.path();
        if read(&p, "type").as_deref() != Some("Battery") {
            continue;
        }
        // energy_* in µWh, or charge_* in µAh on some laptops (wear is still the same ratio)
        let num = |f: &str| read(&p, f).and_then(|v| v.parse::<u64>().ok());
        let design = num("energy_full_design").or_else(|| num("charge_full_design")).unwrap_or(0) / 1000;
        let full = num("energy_full").or_else(|| num("charge_full")).unwrap_or(0) / 1000;
        return Battery {
            present: design > 0,
            name: [read(&p, "manufacturer"), read(&p, "model_name")].into_iter().flatten().collect::<Vec<_>>().join(" "),
            design,
            full,
            cycles: num("cycle_count").filter(|c| *c > 0),
            wear: (design > 0).then(|| (1.0 - full as f32 / design as f32).max(0.0) * 100.0),
            charge: num("capacity").map(|c| c as f32),
            charging: read(&p, "status").as_deref() == Some("Charging"),
        };
    }
    Battery::default()
}

#[cfg(target_os = "macos")]
pub fn battery() -> Battery {
    let out = crate::sysutil::run("ioreg", &["-r", "-c", "AppleSmartBattery"]).unwrap_or_default();
    let val = |k: &str| out.lines().find(|l| l.trim_start().starts_with(&format!("\"{k}\" ="))).and_then(|l| l.split('=').nth(1)).map(|v| v.trim().to_string());
    let num = |k: &str| val(k).and_then(|v| v.parse::<u64>().ok());
    let design = num("DesignCapacity").unwrap_or(0);
    let full = num("AppleRawMaxCapacity").or_else(|| num("MaxCapacity")).unwrap_or(0);
    Battery {
        present: design > 0,
        name: "Battery".into(),
        design,
        full,
        cycles: num("CycleCount"),
        wear: (design > 0).then(|| (1.0 - full as f32 / design as f32).max(0.0) * 100.0),
        charge: None,
        charging: val("IsCharging").as_deref() == Some("Yes"),
    }
}

/// Daily battery snapshots for the wear chart.
pub fn battery_history(base: &std::path::Path, now: &Battery) -> Vec<(u64, u64, u64)> {
    let path = base.join("battery-history.json");
    let mut h: Vec<(u64, u64, u64)> = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    if now.present {
        let today = crate::sysutil::now() / 86_400;
        if h.last().is_none_or(|l| l.0 / 86_400 != today) {
            h.push((crate::sysutil::now(), now.full, now.design));
            h.truncate(1000);
            let _ = std::fs::write(&path, serde_json::to_vec(&h).unwrap_or_default());
        }
    }
    h
}

// ---------- crashes and blue screens ----------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Crash {
    pub at: u64,
    /// app_crash, app_hang, bsod, power_loss
    pub kind: String,
    pub app: String,
    pub module: String,
    /// Plain-language culprit ("NVIDIA graphics driver"), when known.
    pub culprit: Option<String>,
    pub code: String,
}

/// Well-known modules that show up in crashes, in plain words.
pub fn explain(module: &str) -> Option<&'static str> {
    let m = module.to_lowercase();
    const KNOWN: &[(&str, &str)] = &[
        ("nvlddmkm", "NVIDIA graphics driver"),
        ("nvwgf2um", "NVIDIA graphics driver"),
        ("nvoglv", "NVIDIA graphics driver (OpenGL)"),
        ("amdkmdag", "AMD graphics driver"),
        ("atikmdag", "AMD graphics driver"),
        ("amdxx64", "AMD graphics driver"),
        ("igdkmd", "Intel graphics driver"),
        ("igd10", "Intel graphics driver"),
        ("dxgkrnl", "DirectX graphics kernel"),
        ("d3d11", "Direct3D 11"),
        ("d3d12", "Direct3D 12"),
        ("dxgi", "DirectX graphics"),
        ("vulkan-1", "Vulkan runtime"),
        ("ntoskrnl", "Windows kernel (often a driver or RAM problem)"),
        ("ntdll", "Windows core library (usually the app itself)"),
        ("kernelbase", "Windows core library (usually the app itself)"),
        ("ucrtbase", "C runtime (usually the app itself)"),
        ("msvcp140", "Visual C++ runtime"),
        ("vcruntime140", "Visual C++ runtime"),
        ("clr.dll", ".NET runtime"),
        ("coreclr", ".NET runtime"),
        ("rtkvhd64", "Realtek audio driver"),
        ("tcpip", "Windows networking"),
        ("ndis", "network adapter driver"),
        ("wdf01000", "a hardware driver (driver framework)"),
        ("storport", "storage driver"),
        ("stornvme", "NVMe storage driver"),
        ("easyanticheat", "Easy Anti-Cheat"),
        ("bedaisy", "BattlEye anti-cheat"),
        ("vgk", "Riot Vanguard anti-cheat"),
    ];
    KNOWN.iter().find(|(k, _)| m.contains(k)).map(|(_, v)| *v)
}

/// Common stop codes in plain words.
pub fn bugcheck_name(code: u32) -> Option<&'static str> {
    Some(match code {
        0x0A => "IRQL_NOT_LESS_OR_EQUAL (a driver accessed bad memory)",
        0x1A => "MEMORY_MANAGEMENT (often faulty RAM or overclock)",
        0x3B => "SYSTEM_SERVICE_EXCEPTION (a driver crashed)",
        0x50 => "PAGE_FAULT_IN_NONPAGED_AREA (driver or RAM)",
        0x7E => "SYSTEM_THREAD_EXCEPTION_NOT_HANDLED (a driver crashed)",
        0x9F => "DRIVER_POWER_STATE_FAILURE (a driver failed during sleep/wake)",
        0xD1 => "DRIVER_IRQL_NOT_LESS_OR_EQUAL (a driver accessed bad memory)",
        0xEF => "CRITICAL_PROCESS_DIED",
        0x116 => "VIDEO_TDR_FAILURE (graphics driver stopped responding)",
        0x117 => "VIDEO_TDR_TIMEOUT_DETECTED (graphics driver hang)",
        0x124 => "WHEA_UNCORRECTABLE_ERROR (hardware error: CPU, RAM, overclock or heat)",
        0x133 => "DPC_WATCHDOG_VIOLATION (a driver took too long, often storage)",
        0x139 => "KERNEL_SECURITY_CHECK_FAILURE",
        0x13A => "KERNEL_MODE_HEAP_CORRUPTION (a driver corrupted memory)",
        0x1E => "KMODE_EXCEPTION_NOT_HANDLED (a driver crashed)",
        _ => return None,
    })
}

#[cfg(windows)]
pub fn crashes(days: u32) -> Vec<Crash> {
    #[derive(Deserialize)]
    struct Raw {
        t: i64,
        id: u32,
        p: Vec<String>,
    }
    // Application Error 1000 / Hang 1002 (app, module), BugCheck 1001 (stop code), Kernel-Power 41
    let script = format!(
        r#"$s = (Get-Date).AddDays(-{days})
$ev = @()
$ev += Get-WinEvent -FilterHashtable @{{LogName='Application'; Id=1000,1002; StartTime=$s}} -MaxEvents 200 -ErrorAction SilentlyContinue
$ev += Get-WinEvent -FilterHashtable @{{LogName='System'; ProviderName='Microsoft-Windows-WER-SystemErrorReporting'; Id=1001; StartTime=$s}} -MaxEvents 50 -ErrorAction SilentlyContinue
$ev += Get-WinEvent -FilterHashtable @{{LogName='System'; ProviderName='Microsoft-Windows-Kernel-Power'; Id=41; StartTime=$s}} -MaxEvents 50 -ErrorAction SilentlyContinue
$out = @($ev | Where-Object {{ $_ }} | ForEach-Object {{ [pscustomobject]@{{ t = [int64]([DateTimeOffset]$_.TimeCreated).ToUnixTimeSeconds(); id = [int]$_.Id; p = @($_.Properties | ForEach-Object {{ "$($_.Value)" }}) }} }})
ConvertTo-Json -Compress -Depth 4 -InputObject $out"#
    );
    let raw: Vec<Raw> = powershell(&script).unwrap_or_default();
    let mut out: Vec<Crash> = raw
        .into_iter()
        .map(|r| {
            let p = |i: usize| r.p.get(i).cloned().unwrap_or_default();
            match r.id {
                1000 => {
                    let module = p(3);
                    Crash { at: r.t as u64, kind: "app_crash".into(), culprit: explain(&module).map(String::from), app: p(0), module, code: p(6) }
                }
                1002 => Crash { at: r.t as u64, kind: "app_hang".into(), app: p(0), module: String::new(), culprit: None, code: String::new() },
                1001 => {
                    // First property is the stop code, e.g. "0x00000116 (0x..., ...)"
                    let first = p(0);
                    let code = u32::from_str_radix(first.trim_start_matches("0x").split([' ', '(']).next().unwrap_or("").trim(), 16).unwrap_or(0);
                    Crash { at: r.t as u64, kind: "bsod".into(), app: String::new(), module: String::new(), culprit: bugcheck_name(code).map(String::from), code: format!("0x{code:X}") }
                }
                _ => Crash { at: r.t as u64, kind: "power_loss".into(), app: String::new(), module: String::new(), culprit: None, code: String::new() },
            }
        })
        .collect();
    out.sort_by_key(|c| std::cmp::Reverse(c.at));
    out
}

// ---------- security ----------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Security {
    pub defender: Option<bool>,
    pub realtime: Option<bool>,
    /// Days since the antivirus signatures were updated.
    pub signature_age: Option<u32>,
    pub firewall: Option<bool>,
    pub bitlocker: Option<bool>,
    pub secure_boot: Option<bool>,
    pub uac: Option<bool>,
}

#[cfg(windows)]
pub fn security() -> Security {
    use winreg::{enums::HKEY_LOCAL_MACHINE, RegKey};
    #[derive(Deserialize, Default)]
    struct Raw {
        av: Option<bool>,
        rt: Option<bool>,
        age: Option<u32>,
        fw: Option<bool>,
        bl: Option<bool>,
    }
    // Each probe fails quietly (third-party antivirus, no admin for BitLocker), leaving `null`
    let raw: Vec<Raw> = powershell(
        r#"$m = $null; try { $m = Get-MpComputerStatus -ErrorAction Stop } catch {}
$fw = $null; try { $fw = @(Get-NetFirewallProfile -ErrorAction Stop | Where-Object { -not $_.Enabled }).Count -eq 0 } catch {}
$bl = $null; try { $v = Get-BitLockerVolume -MountPoint $env:SystemDrive -ErrorAction Stop; $bl = "$($v.ProtectionStatus)" -eq 'On' } catch {}
ConvertTo-Json -Compress -InputObject @([pscustomobject]@{ av = if ($m) { [bool]$m.AntivirusEnabled } else { $null }; rt = if ($m) { [bool]$m.RealTimeProtectionEnabled } else { $null }; age = if ($m) { [int]$m.AntivirusSignatureAge } else { $null }; fw = $fw; bl = $bl })"#,
    )
    .unwrap_or_default();
    let r = raw.into_iter().next().unwrap_or_default();
    let hk = RegKey::predef(HKEY_LOCAL_MACHINE);
    let secure_boot = hk.open_subkey(r"SYSTEM\CurrentControlSet\Control\SecureBoot\State").and_then(|k| k.get_value::<u32, _>("UEFISecureBootEnabled")).ok().map(|v| v == 1);
    let uac = hk.open_subkey(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System").and_then(|k| k.get_value::<u32, _>("EnableLUA")).ok().map(|v| v == 1);
    Security { defender: r.av, realtime: r.rt, signature_age: r.age, firewall: r.fw, bitlocker: r.bl, secure_boot, uac }
}

// ---------- Windows Update ----------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct UpdateStatus {
    /// Unix seconds of the newest installed update.
    pub last_install: Option<u64>,
    pub last_title: Option<String>,
    pub reboot_pending: bool,
    pub service_running: Option<bool>,
}

#[cfg(windows)]
pub fn update_status() -> UpdateStatus {
    use winreg::{enums::HKEY_LOCAL_MACHINE, RegKey};
    #[derive(Deserialize, Default)]
    struct Raw {
        t: Option<i64>,
        title: Option<String>,
        svc: Option<bool>,
    }
    let raw: Vec<Raw> = powershell(
        r#"$h = Get-HotFix -ErrorAction SilentlyContinue | Where-Object InstalledOn | Sort-Object InstalledOn -Descending | Select-Object -First 1
$s = Get-Service wuauserv -ErrorAction SilentlyContinue
ConvertTo-Json -Compress -InputObject @([pscustomobject]@{ t = if ($h) { [int64]([DateTimeOffset]$h.InstalledOn).ToUnixTimeSeconds() } else { $null }; title = if ($h) { "$($h.HotFixID) $($h.Description)" } else { $null }; svc = if ($s) { "$($s.Status)" -eq 'Running' } else { $null } })"#,
    )
    .unwrap_or_default();
    let r = raw.into_iter().next().unwrap_or_default();
    let hk = RegKey::predef(HKEY_LOCAL_MACHINE);
    let reboot_pending = hk.open_subkey(r"SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired").is_ok()
        || hk.open_subkey(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending").is_ok();
    UpdateStatus { last_install: r.t.map(|t| t as u64), last_title: r.title, reboot_pending, service_running: r.svc }
}

/// Pending updates through the Windows Update agent (slow: it asks Microsoft's servers).
#[cfg(windows)]
pub fn pending_updates() -> Result<Vec<String>, String> {
    powershell::<Vec<String>>(
        "$s = (New-Object -ComObject Microsoft.Update.Session).CreateUpdateSearcher(); $r = $s.Search(\"IsInstalled=0 and IsHidden=0\"); ConvertTo-Json -Compress -InputObject @($r.Updates | ForEach-Object { \"$($_.Title)\" })",
    )
}

/// The classic fix for stuck updates: stop the services, set the download cache aside, restart.
#[cfg(windows)]
pub fn reset_update_cache() -> Result<(), String> {
    crate::sysutil::powershell_raw(
        r#"Stop-Service wuauserv, bits, cryptsvc -Force
$sd = Join-Path $env:SystemRoot 'SoftwareDistribution'
if (Test-Path "$sd.old") { Remove-Item "$sd.old" -Recurse -Force }
Rename-Item $sd 'SoftwareDistribution.old'
Start-Service wuauserv, bits, cryptsvc"#,
    )
    .map(|_| ())
    .map_err(|e| if e.contains("denied") || e.contains("PermissionDenied") { "administrator rights needed".into() } else { e })
}

// ---------- boot time ----------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct BootInfo {
    pub last_boot: Option<u64>,
    /// (unix seconds, total boot ms, main path ms, post boot ms), newest first.
    pub history: Vec<(u64, u64, u64, u64)>,
    /// Apps Windows flagged as slowing boot: (name, delay ms).
    pub slow: Vec<(String, u64)>,
    /// The boot performance log needs administrator rights.
    pub needs_admin: bool,
}

#[cfg(windows)]
pub fn boot_info() -> BootInfo {
    #[derive(Deserialize)]
    struct Raw {
        last: Option<i64>,
        boots: Option<Vec<Vec<i64>>>,
        slow: Option<Vec<(String, i64)>>,
        denied: Option<bool>,
    }
    let script = r#"$last = $null; try { $last = [int64]([DateTimeOffset](Get-CimInstance Win32_OperatingSystem).LastBootUpTime).ToUnixTimeSeconds() } catch {}
$boots = @(); $slow = @(); $denied = $false
try {
  $ev = Get-WinEvent -FilterHashtable @{LogName='Microsoft-Windows-Diagnostics-Performance/Operational'; Id=100} -MaxEvents 30 -ErrorAction Stop
  foreach ($e in $ev) { $x = [xml]$e.ToXml(); $d = @{}; $x.Event.EventData.Data | ForEach-Object { $d[$_.Name] = $_.'#text' }
    $boots += ,@([int64]([DateTimeOffset]$e.TimeCreated).ToUnixTimeSeconds(), [int64]$d['BootTime'], [int64]$d['MainPathBootTime'], [int64]$d['BootPostBootTime']) }
  $apps = Get-WinEvent -FilterHashtable @{LogName='Microsoft-Windows-Diagnostics-Performance/Operational'; Id=101,102,103,106,109} -MaxEvents 40 -ErrorAction SilentlyContinue
  foreach ($e in $apps) { $x = [xml]$e.ToXml(); $d = @{}; $x.Event.EventData.Data | ForEach-Object { $d[$_.Name] = $_.'#text' }
    $name = if ($d['FriendlyName']) { $d['FriendlyName'] } elseif ($d['Name']) { $d['Name'] } else { $d['FileName'] }
    if ($name) { $slow += ,@("$name", [int64]$d['TotalTime']) } }
} catch [System.UnauthorizedAccessException] { $denied = $true } catch { if ("$_" -match 'denied|unauthorized') { $denied = $true } }
ConvertTo-Json -Compress -Depth 4 -InputObject @([pscustomobject]@{ last = $last; boots = $boots; slow = $slow; denied = $denied })"#;
    let r = powershell::<Vec<Raw>>(script).ok().and_then(|v| v.into_iter().next());
    let Some(r) = r else { return BootInfo::default() };
    let mut slow: Vec<(String, u64)> = Vec::new();
    for (n, ms) in r.slow.unwrap_or_default() {
        match slow.iter_mut().find(|s| s.0 == n) {
            Some(s) => s.1 = s.1.max(ms.max(0) as u64),
            None => slow.push((n, ms.max(0) as u64)),
        }
    }
    slow.sort_by_key(|s| std::cmp::Reverse(s.1));
    slow.truncate(10);
    let history_empty = r.boots.as_ref().is_none_or(|b| b.is_empty());
    BootInfo {
        last_boot: r.last.map(|t| t as u64),
        history: r.boots.unwrap_or_default().into_iter().filter(|b| b.len() == 4).map(|b| (b[0] as u64, b[1].max(0) as u64, b[2].max(0) as u64, b[3].max(0) as u64)).collect(),
        slow,
        // The boot log is admin-only; its "access denied" message is localized, so an empty result
        // without admin rights is treated the same way
        needs_admin: !crate::mft::is_elevated() && (r.denied.unwrap_or(false) || history_empty),
    }
}

// ---------- GPU drivers ----------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Driver {
    pub name: String,
    /// nvidia, amd, intel, other
    pub vendor: String,
    pub version: String,
    /// Unix seconds of the driver date.
    pub date: Option<u64>,
    /// User-facing version (e.g. NVIDIA 566.36 instead of 32.0.15.6636).
    pub display_version: String,
    pub download: String,
}

/// NVIDIA's public version is the last 5 digits of the Windows driver version: 32.0.15.6636 → 566.36.
pub fn nvidia_version(win: &str) -> Option<String> {
    let digits: String = win.split('.').skip(2).collect::<String>();
    if digits.len() < 5 {
        return None;
    }
    let last5 = &digits[digits.len() - 5..];
    Some(format!("{}.{}", &last5[..3], &last5[3..]))
}

#[cfg(windows)]
pub fn drivers() -> Vec<Driver> {
    #[derive(Deserialize)]
    struct Raw {
        name: String,
        version: Option<String>,
        date: Option<i64>,
    }
    let raw: Vec<Raw> = powershell("ConvertTo-Json -Compress -InputObject @(Get-CimInstance Win32_VideoController | ForEach-Object { [pscustomobject]@{ name = \"$($_.Name)\"; version = \"$($_.DriverVersion)\"; date = if ($_.DriverDate) { [int64]([DateTimeOffset]$_.DriverDate).ToUnixTimeSeconds() } else { $null } } })").unwrap_or_default();
    raw.into_iter()
        .filter(|r| !r.name.to_lowercase().contains("basic display") && !r.name.to_lowercase().contains("remote"))
        .map(|r| {
            let n = r.name.to_lowercase();
            let vendor = if n.contains("nvidia") { "nvidia" } else if n.contains("amd") || n.contains("radeon") { "amd" } else if n.contains("intel") { "intel" } else { "other" };
            let version = r.version.unwrap_or_default();
            let display_version = if vendor == "nvidia" { nvidia_version(&version).unwrap_or_else(|| version.clone()) } else { version.clone() };
            let download = match vendor {
                "nvidia" => "https://www.nvidia.com/en-us/drivers/",
                "amd" => "https://www.amd.com/en/support/download/drivers.html",
                "intel" => "https://www.intel.com/content/www/us/en/support/detect.html",
                _ => "ms-settings:windowsupdate",
            };
            Driver { name: r.name, vendor: vendor.into(), version, date: r.date.map(|d| d.max(0) as u64), display_version, download: download.into() }
        })
        .collect()
}

// ---------- component store ----------

/// `DISM /StartComponentCleanup`: removes superseded Windows component versions. Returns the
/// bytes freed on the system drive (measured, since DISM's own report is localized).
#[cfg(windows)]
pub fn component_cleanup() -> Result<u64, String> {
    if !crate::mft::is_elevated() {
        return Err("administrator rights needed".into());
    }
    let sys = crate::sysutil::env_path("SystemDrive").map(|d| format!("{}\\", d.display())).unwrap_or_else(|| "C:\\".into());
    let free = || sysinfo::Disks::new_with_refreshed_list().list().iter().find(|d| d.mount_point().to_string_lossy().eq_ignore_ascii_case(&sys)).map(|d| d.available_space()).unwrap_or(0);
    let before = free();
    crate::sysutil::run("dism.exe", &["/Online", "/Cleanup-Image", "/StartComponentCleanup", "/Quiet"])?;
    Ok(free().saturating_sub(before))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explains_crash_modules_and_codes() {
        assert_eq!(explain("nvlddmkm.sys"), Some("NVIDIA graphics driver"));
        assert_eq!(explain("game.exe"), None);
        assert!(bugcheck_name(0x116).unwrap().contains("VIDEO_TDR"));
        assert_eq!(nvidia_version("32.0.15.6636").as_deref(), Some("566.36"));
    }
}
