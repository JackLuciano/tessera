//! Driver store cleanup (Windows): every driver update leaves the previous package in
//! `System32\DriverStore\FileRepository`, often hundreds of MB each for graphics drivers.
//!
//! A package can go when no device uses it and a newer package of the same driver (same original
//! .inf name and provider) is installed. Removal goes through `pnputil /delete-driver` without
//! `/force`, so Windows itself refuses anything still in use.

use crate::sysutil::{powershell, powershell_raw, ps_quote};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Clone, Debug)]
pub struct Package {
    /// Published name, e.g. oem42.inf.
    pub inf: String,
    /// The driver's own .inf name, e.g. nv_dispi.inf.
    pub original: String,
    pub provider: String,
    pub class: String,
    pub version: String,
    /// Unix seconds of the driver date.
    pub date: u64,
    pub size: u64,
    pub in_use: bool,
    /// The newest package of this driver.
    pub newest: bool,
    /// Old and unused: safe to remove.
    pub removable: bool,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Store {
    pub packages: Vec<Package>,
    pub removable_bytes: u64,
    /// Listing the store needs administrator rights.
    pub needs_admin: bool,
}

#[derive(Deserialize)]
struct Raw {
    inf: String,
    original: String,
    provider: Option<String>,
    class: Option<String>,
    version: Option<String>,
    date: Option<i64>,
    dir: Option<String>,
    /// A device uses this package right now (unknown counts as used).
    used: Option<bool>,
}

pub fn scan() -> Store {
    if !crate::mft::is_elevated() {
        return Store { needs_admin: true, ..Default::default() };
    }
    let raw: Vec<Raw> = powershell(
        r#"$used = @{}; Get-CimInstance Win32_PnPSignedDriver | ForEach-Object { if ($_.InfName) { $used[$_.InfName.ToLower()] = $true } }
$out = @(Get-WindowsDriver -Online | ForEach-Object {
  $dir = Split-Path -Parent $_.OriginalFileName
  [pscustomobject]@{ inf = "$($_.Driver)"; original = (Split-Path -Leaf $_.OriginalFileName); provider = "$($_.ProviderName)"; class = "$($_.ClassName)";
    version = "$($_.Version)"; date = if ($_.Date) { [int64]([DateTimeOffset]$_.Date).ToUnixTimeSeconds() } else { 0 }; dir = $dir; used = [bool]$used[$_.Driver.ToLower()] } })
ConvertTo-Json -Compress -InputObject $out"#,
    )
    .unwrap_or_default();
    let mut packages: Vec<Package> = raw
        .into_iter()
        .map(|r| Package {
            in_use: r.used.unwrap_or(true),
            size: r.dir.as_deref().map(|d| dir_size(Path::new(d))).unwrap_or(0),
            inf: r.inf,
            original: r.original.to_lowercase(),
            provider: r.provider.unwrap_or_default(),
            class: r.class.unwrap_or_default(),
            version: r.version.unwrap_or_default(),
            date: r.date.unwrap_or(0).max(0) as u64,
            newest: false,
            removable: false,
        })
        .collect();
    mark(&mut packages);
    packages.sort_by(|a, b| b.removable.cmp(&a.removable).then(b.size.cmp(&a.size)));
    let removable_bytes = packages.iter().filter(|p| p.removable).map(|p| p.size).sum();
    Store { packages, removable_bytes, needs_admin: false }
}

/// Mark the newest package of each driver, and the old unused ones as removable.
pub fn mark(packages: &mut [Package]) {
    let key = |p: &Package| format!("{}|{}", p.original, p.provider.to_lowercase());
    for i in 0..packages.len() {
        let k = key(&packages[i]);
        let newest = packages.iter().filter(|o| key(o) == k).all(|o| cmp_version(&packages[i].version, &o.version).is_ge());
        packages[i].newest = newest;
    }
    for i in 0..packages.len() {
        let k = key(&packages[i]);
        let has_newer = packages.iter().any(|o| key(o) == k && o.newest && o.inf != packages[i].inf);
        packages[i].removable = !packages[i].in_use && !packages[i].newest && has_newer;
    }
}

/// Compare dotted versions numerically (32.0.15.6094 < 32.0.15.6636).
pub fn cmp_version(a: &str, b: &str) -> std::cmp::Ordering {
    let parts = |s: &str| s.split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    parts(a).cmp(&parts(b))
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    rd.flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(_) => e.metadata().map(|m| m.len()).unwrap_or(0),
            Err(_) => 0,
        })
        .sum()
}

/// Remove driver packages by published name (oemNN.inf). Returns (removed, failed).
pub fn remove(infs: &[String]) -> (Vec<String>, Vec<String>) {
    let mut ok = Vec::new();
    let mut failed = Vec::new();
    for inf in infs {
        let valid = inf.len() < 20 && inf.to_lowercase().starts_with("oem") && inf.to_lowercase().ends_with(".inf") && inf[3..inf.len() - 4].chars().all(|c| c.is_ascii_digit());
        if !valid {
            failed.push(inf.clone());
            continue;
        }
        match powershell_raw(&format!("pnputil /delete-driver {} | Out-Null; if ($LASTEXITCODE) {{ throw 'in use' }}", ps_quote(inf))) {
            Ok(_) => ok.push(inf.clone()),
            Err(_) => failed.push(inf.clone()),
        }
    }
    (ok, failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(inf: &str, original: &str, version: &str, in_use: bool) -> Package {
        Package { inf: inf.into(), original: original.into(), provider: "NVIDIA".into(), class: "Display".into(), version: version.into(), date: 0, size: 900, in_use, newest: false, removable: false }
    }

    #[test]
    fn keeps_newest_and_in_use_packages() {
        let mut p = vec![
            pkg("oem10.inf", "nv_dispi.inf", "32.0.15.6094", false),
            pkg("oem22.inf", "nv_dispi.inf", "32.0.15.6636", true),
            pkg("oem7.inf", "nv_dispi.inf", "31.0.15.5222", true),
            pkg("oem3.inf", "rtkaudio.inf", "6.0.9600.1", false),
        ];
        mark(&mut p);
        assert!(p[0].removable, "old and unused");
        assert!(!p[1].removable && p[1].newest);
        assert!(!p[2].removable, "still used by a device");
        assert!(!p[3].removable, "the only package of its driver");
        assert!(cmp_version("10.1", "9.9").is_gt());
    }
}
