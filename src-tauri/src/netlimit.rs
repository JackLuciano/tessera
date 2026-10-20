//! Network priority while gaming (Windows): bandwidth caps for the apps the user picked, plus a cap
//! on Windows Update's downloads, removed again when the game exits.
//!
//! App caps are Windows QoS policies created in the ActiveStore, which is never written to disk:
//! even if Tessera crashed mid-game they are gone after a restart. The Windows Update cap is a
//! Delivery Optimization policy value; its previous state is saved to a file first, so it is put
//! back at the next start if the session didn't end cleanly. Everything needs administrator rights.

use crate::sysutil::{powershell_raw, ps_quote};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const PREFIX: &str = "Tessera-";
const DO_KEY: &str = r"SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization";
const DO_VALUES: [&str; 2] = ["DOMaxForegroundDownloadBandwidth", "DOMaxBackgroundDownloadBandwidth"];

/// Process names are used inside a PowerShell literal and a policy name: keep them plain.
fn clean(app: &str) -> Option<String> {
    let n = app.trim().to_lowercase();
    let n = n.trim_end_matches(".exe");
    (!n.is_empty() && n.len() < 64 && n.chars().all(|c| c.is_ascii_alphanumeric() || " ._-".contains(c))).then(|| n.to_string())
}

/// Cap each app to `mbps` megabits per second. Returns the apps that got a policy.
pub fn limit(apps: &[String], mbps: u32) -> Vec<String> {
    clear_apps();
    let bits = (mbps.clamp(1, 10_000) as u64) * 1_000_000;
    let mut done = Vec::new();
    for app in apps.iter().filter_map(|a| clean(a)) {
        let script = format!(
            "New-NetQosPolicy -Name {} -AppPathNameMatchCondition {} -ThrottleRateActionBitsPerSecond {bits} -PolicyStore ActiveStore -ErrorAction Stop | Out-Null",
            ps_quote(&format!("{PREFIX}{app}")),
            ps_quote(&format!("{app}.exe"))
        );
        if powershell_raw(&script).is_ok() {
            done.push(app);
        }
    }
    done
}

/// Remove every app cap Tessera created.
pub fn clear_apps() {
    let _ = powershell_raw(&format!("Get-NetQosPolicy -PolicyStore ActiveStore -ErrorAction SilentlyContinue | Where-Object {{ $_.Name -like '{PREFIX}*' }} | Remove-NetQosPolicy -PolicyStore ActiveStore -Confirm:$false -ErrorAction SilentlyContinue"));
}

#[derive(Serialize, Deserialize, Default)]
struct DoBackup {
    /// Previous values of the two policy values; None = they were not set.
    values: Vec<Option<u32>>,
}

fn backup_path(base: &Path) -> PathBuf {
    base.join("netlimit-backup.json")
}

/// Cap Windows Update (Delivery Optimization) downloads to `kbytes` KB/s.
pub fn limit_updates(base: &Path, kbytes: u32) -> bool {
    use winreg::{RegKey, enums::*};
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    // Keep the first backup if one is still there from a session that didn't end cleanly
    if !backup_path(base).exists() {
        let prev: Vec<Option<u32>> = DO_VALUES.iter().map(|v| hklm.open_subkey(DO_KEY).ok().and_then(|k| k.get_value::<u32, _>(v).ok())).collect();
        if std::fs::write(backup_path(base), serde_json::to_vec(&DoBackup { values: prev }).unwrap_or_default()).is_err() {
            return false;
        }
    }
    let Ok((key, _)) = hklm.create_subkey(DO_KEY) else { return false };
    DO_VALUES.iter().all(|v| key.set_value(v, &kbytes.max(64)).is_ok())
}

/// Put the Windows Update download limits back the way they were.
pub fn restore_updates(base: &Path) {
    use winreg::{RegKey, enums::*};
    let Some(b) = std::fs::read(backup_path(base)).ok().and_then(|b| serde_json::from_slice::<DoBackup>(&b).ok()) else { return };
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(key) = hklm.open_subkey_with_flags(DO_KEY, KEY_ALL_ACCESS) {
        for (v, prev) in DO_VALUES.iter().zip(b.values.iter().chain(std::iter::repeat(&None))) {
            let _ = match prev {
                Some(x) => key.set_value(v, x),
                None => key.delete_value(v),
            };
        }
    }
    let _ = std::fs::remove_file(backup_path(base));
}

/// At startup: undo anything a crashed session left behind.
pub fn recover(base: &Path) {
    if backup_path(base).exists() && crate::mft::is_elevated() {
        restore_updates(base);
        clear_apps();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_process_names() {
        assert_eq!(clean("OneDrive.exe").as_deref(), Some("onedrive"));
        assert_eq!(clean("battle.net").as_deref(), Some("battle.net"));
        assert_eq!(clean("x'; Remove-Item"), None);
        assert_eq!(clean(""), None);
    }
}
