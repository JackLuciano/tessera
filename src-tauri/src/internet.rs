//! Internet booster (Windows): switch DNS servers per network adapter, measure which DNS answers
//! fastest from here, flush the DNS cache, and a few TCP settings. Every change keeps the original
//! value in `internet-backup.json` so it can be restored.

use crate::sysutil::{self, powershell, powershell_raw};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::{SocketAddr, UdpSocket};
use std::path::Path;
use std::time::{Duration, Instant};

pub struct Preset {
    pub key: &'static str,
    pub servers: &'static [&'static str],
}

pub const PRESETS: &[Preset] = &[
    Preset { key: "cloudflare", servers: &["1.1.1.1", "1.0.0.1", "2606:4700:4700::1111", "2606:4700:4700::1001"] },
    Preset { key: "google", servers: &["8.8.8.8", "8.8.4.4", "2001:4860:4860::8888", "2001:4860:4860::8844"] },
    Preset { key: "quad9", servers: &["9.9.9.9", "149.112.112.112", "2620:fe::fe", "2620:fe::9"] },
    Preset { key: "opendns", servers: &["208.67.222.222", "208.67.220.220", "2620:119:35::35", "2620:119:53::53"] },
];

#[derive(Serialize, Deserialize, Clone)]
pub struct Adapter {
    pub index: u32,
    pub name: String,
    pub description: String,
    pub guid: String,
    pub servers: Vec<String>,
    /// DNS servers set by hand (otherwise they come from the router via DHCP).
    pub manual: bool,
    /// Matching preset key, "auto" for DHCP, "custom" otherwise.
    pub preset: String,
}

pub fn adapters() -> Result<Vec<Adapter>, String> {
    #[derive(Deserialize)]
    struct Raw {
        index: u32,
        name: String,
        description: Option<String>,
        guid: String,
        servers: Option<Vec<String>>,
    }
    let raw: Vec<Raw> = powershell(
        r#"$out = @(Get-NetAdapter -Physical | Where-Object Status -eq 'Up' | ForEach-Object {
  $i = $_.ifIndex
  [pscustomobject]@{ index = [int]$i; name = "$($_.Name)"; description = "$($_.InterfaceDescription)"; guid = "$($_.InterfaceGuid)";
    servers = @(Get-DnsClientServerAddress -InterfaceIndex $i -ErrorAction SilentlyContinue | ForEach-Object { $_.ServerAddresses }) }
})
ConvertTo-Json -Compress -Depth 4 -InputObject $out"#,
    )?;
    Ok(raw
        .into_iter()
        .map(|r| {
            let servers = r.servers.unwrap_or_default();
            let manual = manual_dns(&r.guid);
            let preset = if !manual {
                "auto".to_string()
            } else {
                PRESETS
                    .iter()
                    .find(|p| servers.first().is_some_and(|s| p.servers.contains(&s.as_str())))
                    .map(|p| p.key.to_string())
                    .unwrap_or_else(|| "custom".into())
            };
            Adapter { index: r.index, name: r.name, description: r.description.unwrap_or_default(), guid: r.guid, servers, manual, preset }
        })
        .collect())
}

/// Static DNS servers live in the interface's `NameServer` value (empty means DHCP).
fn manual_dns(guid: &str) -> bool {
    use winreg::{enums::HKEY_LOCAL_MACHINE, RegKey};
    let hk = RegKey::predef(HKEY_LOCAL_MACHINE);
    [r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces", r"SYSTEM\CurrentControlSet\Services\Tcpip6\Parameters\Interfaces"].iter().any(|base| {
        hk.open_subkey(format!(r"{base}\{guid}")).and_then(|k| k.get_value::<String, _>("NameServer")).is_ok_and(|v| !v.trim().is_empty())
    })
}

// ---------- backup ----------

#[derive(Serialize, Deserialize, Default)]
struct Backup {
    /// Adapter GUID → original servers (empty = automatic).
    dns: BTreeMap<String, Vec<String>>,
    /// TCP setting key → original value.
    tcp: BTreeMap<String, String>,
    /// Interface GUID → original (TcpAckFrequency, TCPNoDelay); `None` = not set.
    nagle: BTreeMap<String, (Option<u32>, Option<u32>)>,
}

fn backup_path(base: &Path) -> std::path::PathBuf {
    base.join("internet-backup.json")
}
fn load(base: &Path) -> Backup {
    std::fs::read(backup_path(base)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}
fn save(base: &Path, b: &Backup) -> Result<(), String> {
    std::fs::create_dir_all(base).map_err(|e| e.to_string())?;
    std::fs::write(backup_path(base), serde_json::to_vec_pretty(b).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
pub fn has_backup(base: &Path) -> bool {
    let b = load(base);
    !(b.dns.is_empty() && b.tcp.is_empty() && b.nagle.is_empty())
}

fn apply_dns(index: u32, servers: &[String]) -> Result<(), String> {
    let script = if servers.is_empty() {
        format!("Set-DnsClientServerAddress -InterfaceIndex {index} -ResetServerAddresses")
    } else {
        let list = servers.iter().map(|s| sysutil::ps_quote(s)).collect::<Vec<_>>().join(",");
        format!("Set-DnsClientServerAddress -InterfaceIndex {index} -ServerAddresses @({list})")
    };
    powershell_raw(&script).map(|_| ()).map_err(admin_hint)
}

fn admin_hint(e: String) -> String {
    if e.contains("Access") || e.contains("0x80070005") || e.contains("PermissionDenied") { format!("administrator rights needed ({e})") } else { e }
}

/// Switch every connected adapter to a preset ("auto" = back to DHCP).
pub fn set_dns(base: &Path, preset: &str) -> Result<usize, String> {
    let servers: Vec<String> = if preset == "auto" {
        Vec::new()
    } else {
        PRESETS.iter().find(|p| p.key == preset).ok_or("unknown preset")?.servers.iter().map(|s| s.to_string()).collect()
    };
    let mut backup = load(base);
    let mut changed = 0;
    for a in adapters()? {
        backup.dns.entry(a.guid.clone()).or_insert_with(|| if a.manual { a.servers.clone() } else { Vec::new() });
        apply_dns(a.index, &servers)?;
        changed += 1;
    }
    save(base, &backup)?;
    let _ = flush_dns();
    Ok(changed)
}

pub fn flush_dns() -> Result<(), String> {
    sysutil::run("ipconfig", &["/flushdns"]).map(|_| ())
}

// ---------- latency ----------

#[derive(Serialize, Clone)]
pub struct Latency {
    /// Preset key, or "current" for the adapter's own (router) DNS.
    pub key: String,
    pub server: String,
    /// Median of three queries; `None` when the server did not answer.
    pub ms: Option<f64>,
}

/// A minimal DNS query for `example.com` (A record) with the given transaction id.
fn dns_query(id: u16) -> Vec<u8> {
    let mut q = Vec::with_capacity(29);
    q.extend(id.to_be_bytes());
    q.extend([0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]);
    for label in ["example", "com"] {
        q.push(label.len() as u8);
        q.extend(label.as_bytes());
    }
    q.extend([0, 0, 1, 0, 1]);
    q
}

fn time_server(server: &str) -> Option<f64> {
    let addr: SocketAddr = format!("{server}:53").parse().ok()?;
    let sock = UdpSocket::bind(if addr.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }).ok()?;
    sock.set_read_timeout(Some(Duration::from_millis(1500))).ok()?;
    let mut times = Vec::new();
    let mut buf = [0u8; 512];
    for i in 0..3u16 {
        let id = 0x7e00 + i;
        let start = Instant::now();
        sock.send_to(&dns_query(id), addr).ok()?;
        loop {
            match sock.recv_from(&mut buf) {
                Ok((n, from)) if from == addr && n >= 2 && buf[..2] == id.to_be_bytes() => {
                    times.push(start.elapsed().as_secs_f64() * 1000.0);
                    break;
                }
                Ok(_) => continue,
                Err(_) => break,
            }
        }
    }
    if times.is_empty() {
        return None;
    }
    times.sort_by(|a, b| a.total_cmp(b));
    Some(times[times.len() / 2])
}

/// Query each preset's primary server (and the current DNS) in parallel.
pub fn latency(current: Option<String>) -> Vec<Latency> {
    let mut jobs: Vec<(String, String)> = PRESETS.iter().map(|p| (p.key.to_string(), p.servers[0].to_string())).collect();
    if let Some(c) = current.filter(|c| c.parse::<std::net::IpAddr>().is_ok()) {
        jobs.insert(0, ("current".into(), c));
    }
    let handles: Vec<_> = jobs
        .into_iter()
        .map(|(key, server)| std::thread::spawn(move || Latency { ms: time_server(&server), key, server }))
        .collect();
    handles.into_iter().filter_map(|h| h.join().ok()).collect()
}

// ---------- TCP ----------

#[derive(Serialize)]
pub struct TcpSetting {
    pub key: &'static str,
    pub on: bool,
    /// Raw current value, for the hint line.
    pub value: String,
}

const INTERFACES: &str = r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces";

fn nagle_off(guid: &str) -> bool {
    use winreg::{enums::HKEY_LOCAL_MACHINE, RegKey};
    let Ok(k) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(format!(r"{INTERFACES}\{guid}")) else { return false };
    k.get_value::<u32, _>("TcpAckFrequency").ok() == Some(1) && k.get_value::<u32, _>("TCPNoDelay").ok() == Some(1)
}

pub fn tcp_settings() -> Result<Vec<TcpSetting>, String> {
    #[derive(Deserialize)]
    struct Raw {
        autotuning: Option<String>,
        rss: Option<String>,
    }
    let raw: Vec<Raw> = powershell(
        r#"$t = Get-NetTCPSetting -SettingName Internet -ErrorAction SilentlyContinue | Select-Object -First 1
$o = Get-NetOffloadGlobalSetting -ErrorAction SilentlyContinue
ConvertTo-Json -Compress -InputObject @([pscustomobject]@{ autotuning = "$($t.AutoTuningLevelLocal)"; rss = "$($o.ReceiveSideScaling)" })"#,
    )?;
    let r = raw.into_iter().next().ok_or("no TCP settings")?;
    let at = r.autotuning.unwrap_or_default();
    let rss = r.rss.unwrap_or_default();
    let adapters = adapters().unwrap_or_default();
    let nagle = !adapters.is_empty() && adapters.iter().all(|a| nagle_off(&a.guid));
    Ok(vec![
        TcpSetting { key: "autotuning", on: at.eq_ignore_ascii_case("normal"), value: at },
        TcpSetting { key: "rss", on: rss.eq_ignore_ascii_case("enabled"), value: rss },
        TcpSetting { key: "nagle", on: nagle, value: String::new() },
    ])
}

fn netsh_global(name: &str, value: &str) -> Result<(), String> {
    sysutil::run("netsh", &["int", "tcp", "set", "global", &format!("{name}={value}")]).map(|_| ()).map_err(admin_hint)
}

pub fn set_tcp(base: &Path, key: &str, on: bool) -> Result<(), String> {
    let mut backup = load(base);
    let current = tcp_settings()?;
    let now = current.iter().find(|s| s.key == key).ok_or("unknown setting")?;
    match key {
        "autotuning" | "rss" => {
            let netsh_name = if key == "autotuning" { "autotuninglevel" } else { "rss" };
            backup.tcp.entry(key.into()).or_insert_with(|| now.value.to_lowercase());
            // "On" is the Windows default and the recommended state. "Off" only ever goes back to
            // what the user had before; it never switches the feature off from a good state.
            let value = if on {
                (if key == "autotuning" { "normal" } else { "enabled" }).to_string()
            } else {
                backup.tcp.get(key).cloned().filter(|v| !v.is_empty() && v != "normal" && v != "enabled").ok_or("already at the Windows default")?
            };
            netsh_global(netsh_name, &value)?;
        }
        "nagle" => {
            use winreg::{enums::*, RegKey};
            let hk = RegKey::predef(HKEY_LOCAL_MACHINE);
            for a in adapters()? {
                let path = format!(r"{INTERFACES}\{}", a.guid);
                let (k, _) = hk.create_subkey(&path).map_err(|e| admin_hint(e.to_string()))?;
                backup.nagle.entry(a.guid.clone()).or_insert_with(|| (k.get_value::<u32, _>("TcpAckFrequency").ok(), k.get_value::<u32, _>("TCPNoDelay").ok()));
                let orig = backup.nagle.get(&a.guid).copied().unwrap_or((None, None));
                for (name, original) in [("TcpAckFrequency", orig.0), ("TCPNoDelay", orig.1)] {
                    let r = match (on, original) {
                        (true, _) => k.set_value(name, &1u32),
                        (false, Some(v)) if v != 1 => k.set_value(name, &v),
                        (false, _) => match k.delete_value(name) {
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                            r => r,
                        },
                    };
                    r.map_err(|e| admin_hint(e.to_string()))?;
                }
            }
        }
        _ => return Err("unknown setting".into()),
    }
    save(base, &backup)
}

/// Put back the original DNS servers and TCP settings, then forget the backup.
pub fn restore(base: &Path) -> Result<usize, String> {
    let backup = load(base);
    let mut n = 0;
    let adapters = adapters().unwrap_or_default();
    for (guid, servers) in &backup.dns {
        if let Some(a) = adapters.iter().find(|a| &a.guid == guid) {
            apply_dns(a.index, servers)?;
            n += 1;
        }
    }
    for (key, value) in &backup.tcp {
        let name = if key == "autotuning" { "autotuninglevel" } else { "rss" };
        if !value.is_empty() {
            netsh_global(name, value)?;
            n += 1;
        }
    }
    if !backup.nagle.is_empty() {
        use winreg::{enums::*, RegKey};
        let hk = RegKey::predef(HKEY_LOCAL_MACHINE);
        for (guid, (ack, nodelay)) in &backup.nagle {
            let Ok(k) = hk.open_subkey_with_flags(format!(r"{INTERFACES}\{guid}"), KEY_SET_VALUE) else { continue };
            for (name, v) in [("TcpAckFrequency", ack), ("TCPNoDelay", nodelay)] {
                let _ = match v {
                    Some(v) => k.set_value(name, v),
                    None => k.delete_value(name),
                };
            }
            n += 1;
        }
    }
    let _ = std::fs::remove_file(backup_path(base));
    let _ = flush_dns();
    Ok(n)
}

/// Reset Winsock and flush DNS: the classic fix for "connected but nothing loads". Needs a restart.
pub fn repair() -> Result<(), String> {
    flush_dns()?;
    sysutil::run("netsh", &["winsock", "reset"]).map(|_| ()).map_err(admin_hint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_dns_queries() {
        let q = dns_query(0x1234);
        assert_eq!(&q[..2], &[0x12, 0x34]);
        assert_eq!(q.len(), 12 + 1 + 7 + 1 + 3 + 1 + 4);
        assert_eq!(&q[q.len() - 4..], &[0, 1, 0, 1]);
    }
}
