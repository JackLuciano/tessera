//! Physical drive health (SMART status, temperature, wear) and drive optimization: TRIM for SSDs,
//! defragmentation for hard drives, both through the operating system's own tools.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Clone, Default)]
pub struct PhysDisk {
    pub id: String,
    pub name: String,
    /// ssd, hdd or unknown
    pub media: String,
    pub bus: String,
    /// healthy, warning, unhealthy or unknown
    pub health: String,
    pub size: u64,
    /// Degrees Celsius.
    pub temperature: Option<i64>,
    /// Percent of rated endurance used (SSDs).
    pub wear: Option<i64>,
    pub power_on_hours: Option<i64>,
    pub read_errors: Option<i64>,
    /// Drive letters / mount points on this disk.
    pub volumes: Vec<String>,
}

#[derive(Serialize)]
pub struct HealthReport {
    pub disks: Vec<PhysDisk>,
    /// Temperature and wear need administrator rights on Windows.
    pub counters_need_admin: bool,
    /// Optimize (TRIM/defrag) can be started from Tessera on this system.
    pub can_optimize: bool,
}

fn norm_media(s: &str) -> String {
    match s.to_lowercase().as_str() {
        "ssd" | "4" => "ssd".into(),
        "hdd" | "3" => "hdd".into(),
        _ => "unknown".into(),
    }
}

#[cfg(windows)]
pub fn report() -> HealthReport {
    #[derive(Deserialize)]
    struct Raw {
        id: i64,
        name: Option<String>,
        media: Option<String>,
        bus: Option<String>,
        health: Option<String>,
        size: Option<i64>,
        temp: Option<i64>,
        wear: Option<i64>,
        hours: Option<i64>,
        errors: Option<i64>,
        letters: Option<Vec<String>>,
    }
    // Enum values are turned into strings explicitly: Windows PowerShell serializes them as numbers
    let script = r#"
$parts = @(Get-Partition | Where-Object DriveLetter | ForEach-Object { [pscustomobject]@{ disk = [int]$_.DiskNumber; letter = "$($_.DriveLetter):" } })
$out = @(Get-PhysicalDisk | ForEach-Object {
  $r = $null
  try { $r = $_ | Get-StorageReliabilityCounter -ErrorAction Stop } catch {}
  $id = [int]$_.DeviceId
  [pscustomobject]@{
    id = $id; name = "$($_.FriendlyName)"; media = "$($_.MediaType)"; bus = "$($_.BusType)"; health = "$($_.HealthStatus)"; size = [int64]$_.Size
    temp = if ($r -and $r.Temperature) { [int64]$r.Temperature } else { $null }
    wear = if ($r -and $r.Wear -ne $null) { [int64]$r.Wear } else { $null }
    hours = if ($r -and $r.PowerOnHours) { [int64]$r.PowerOnHours } else { $null }
    errors = if ($r -and $r.ReadErrorsTotal -ne $null) { [int64]$r.ReadErrorsTotal } else { $null }
    letters = @($parts | Where-Object disk -eq $id | ForEach-Object letter)
  }
})
ConvertTo-Json -Compress -Depth 4 -InputObject $out
"#;
    let raw: Vec<Raw> = crate::sysutil::powershell(script).unwrap_or_else(|e| {
        eprintln!("disk health: {e}");
        Vec::new()
    });
    let elevated = crate::mft::is_elevated();
    let disks = raw
        .into_iter()
        .map(|r| PhysDisk {
            id: r.id.to_string(),
            name: r.name.unwrap_or_default().trim().to_string(),
            media: norm_media(r.media.as_deref().unwrap_or("")),
            bus: r.bus.unwrap_or_default(),
            health: match r.health.as_deref().map(str::to_lowercase).as_deref() {
                Some("healthy" | "0") => "healthy",
                Some("warning" | "1") => "warning",
                Some("unhealthy" | "2") => "unhealthy",
                _ => "unknown",
            }
            .into(),
            size: r.size.unwrap_or(0).max(0) as u64,
            temperature: r.temp.filter(|t| *t > 0 && *t < 150),
            wear: r.wear.filter(|w| (0..=100).contains(w)),
            power_on_hours: r.hours,
            read_errors: r.errors,
            volumes: r.letters.unwrap_or_default(),
        })
        .collect();
    HealthReport { disks, counters_need_admin: !elevated, can_optimize: elevated }
}

#[cfg(target_os = "linux")]
pub fn report() -> HealthReport {
    #[derive(Deserialize)]
    struct Lsblk {
        blockdevices: Vec<Dev>,
    }
    #[derive(Deserialize)]
    struct Dev {
        name: String,
        model: Option<String>,
        size: Option<serde_json::Value>,
        rota: Option<serde_json::Value>,
        tran: Option<String>,
        #[serde(rename = "type")]
        kind: Option<String>,
        mountpoints: Option<Vec<Option<String>>>,
        children: Option<Vec<Dev>>,
    }
    fn mounts(d: &Dev, out: &mut Vec<String>) {
        out.extend(d.mountpoints.iter().flatten().flatten().cloned());
        for c in d.children.iter().flatten() {
            mounts(c, out);
        }
    }
    let as_u64 = |v: &Option<serde_json::Value>| v.as_ref().and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()));
    let rotational = |v: &Option<serde_json::Value>| v.as_ref().map(|v| v.as_bool().unwrap_or_else(|| v.as_u64() == Some(1) || v.as_str() == Some("1")));
    let json = crate::sysutil::run("lsblk", &["-J", "-b", "-o", "NAME,MODEL,SIZE,ROTA,TRAN,TYPE,MOUNTPOINTS"]).unwrap_or_default();
    let devs = serde_json::from_str::<Lsblk>(&json).map(|l| l.blockdevices).unwrap_or_default();
    let disks = devs
        .into_iter()
        .filter(|d| d.kind.as_deref() == Some("disk") && !d.name.starts_with("loop") && !d.name.starts_with("zram"))
        .map(|d| {
            let mut volumes = Vec::new();
            mounts(&d, &mut volumes);
            volumes.retain(|m| !m.starts_with("[SWAP]"));
            PhysDisk {
                id: d.name.clone(),
                name: d.model.clone().unwrap_or_else(|| d.name.clone()).trim().to_string(),
                media: match rotational(&d.rota) {
                    Some(true) => "hdd".into(),
                    Some(false) => "ssd".into(),
                    None => "unknown".into(),
                },
                bus: d.tran.clone().unwrap_or_default(),
                health: smart_health(&d.name),
                size: as_u64(&d.size).unwrap_or(0),
                temperature: sysfs_temp(&d.name),
                volumes,
                ..Default::default()
            }
        })
        .collect();
    HealthReport { disks, counters_need_admin: false, can_optimize: true }
}

/// `smartctl -H` when smartmontools is installed and allowed to read the drive; else unknown.
#[cfg(target_os = "linux")]
fn smart_health(dev: &str) -> String {
    let Ok(out) = crate::sysutil::command("smartctl").args(["-H", "-j", &format!("/dev/{dev}")]).output() else { return "unknown".into() };
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
    match v.pointer("/smart_status/passed").and_then(|p| p.as_bool()) {
        Some(true) => "healthy".into(),
        Some(false) => "unhealthy".into(),
        None => "unknown".into(),
    }
}

/// Drive temperature from the kernel's hwmon interface (NVMe, and SATA with the drivetemp module).
#[cfg(target_os = "linux")]
fn sysfs_temp(dev: &str) -> Option<i64> {
    let base = std::path::Path::new("/sys/block").join(dev).join("device");
    let mut dirs = vec![base.join("hwmon")];
    if let Ok(rd) = std::fs::read_dir(&base) {
        dirs.extend(rd.flatten().map(|e| e.path().join("hwmon")));
    }
    for d in dirs {
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            if let Ok(s) = std::fs::read_to_string(e.path().join("temp1_input")) {
                return s.trim().parse::<i64>().ok().map(|m| m / 1000);
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
pub fn report() -> HealthReport {
    let json = crate::sysutil::run("system_profiler", &["SPNVMeDataType", "SPSerialATADataType", "-json"]).unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
    let mut disks = Vec::new();
    for key in ["SPNVMeDataType", "SPSerialATADataType"] {
        for ctrl in v.get(key).and_then(|x| x.as_array()).into_iter().flatten() {
            // NVMe lists drives directly; SATA nests them under the controller
            let items: Vec<&serde_json::Value> = match ctrl.get("_items").and_then(|i| i.as_array()) {
                Some(list) => list.iter().collect(),
                None => vec![ctrl],
            };
            for d in items {
                let s = |k: &str| d.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
                let smart = s("smart_status").to_lowercase();
                disks.push(PhysDisk {
                    id: s("bsd_name"),
                    name: s("_name"),
                    media: if key == "SPNVMeDataType" || s("spsata_medium_type").to_lowercase().contains("solid") { "ssd".into() } else { "hdd".into() },
                    bus: if key == "SPNVMeDataType" { "NVMe".into() } else { "SATA".into() },
                    health: if smart == "verified" { "healthy".into() } else if smart.is_empty() { "unknown".into() } else { "unhealthy".into() },
                    size: d.get("size_in_bytes").and_then(|x| x.as_u64()).unwrap_or(0),
                    ..Default::default()
                });
            }
        }
    }
    // macOS trims SSDs by itself; there is nothing to start by hand
    HealthReport { disks, counters_need_admin: false, can_optimize: false }
}

/// TRIM an SSD or defragment a hard drive. Windows picks the right one per drive (`defrag /O`).
pub fn optimize(volume: &str) -> Result<String, String> {
    #[cfg(windows)]
    {
        let v = volume.trim_end_matches(['\\', '/']);
        if v.len() != 2 || !v.ends_with(':') || !v.as_bytes()[0].is_ascii_alphabetic() {
            return Err("invalid drive".into());
        }
        crate::sysutil::run("defrag", &[v, "/O"]).map(|_| v.to_string())
    }
    #[cfg(target_os = "linux")]
    {
        // fstrim needs root; pkexec shows the desktop's password prompt
        let _ = volume;
        crate::sysutil::run("pkexec", &["fstrim", "-av"])
    }
    #[cfg(target_os = "macos")]
    {
        let _ = volume;
        Err("macOS optimizes drives by itself".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_media_types() {
        assert_eq!(norm_media("SSD"), "ssd");
        assert_eq!(norm_media("3"), "hdd");
        assert_eq!(norm_media("Unspecified"), "unknown");
    }
}
