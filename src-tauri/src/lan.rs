//! Multi-PC view over the local network. A PC that turns sharing on answers status requests
//! (Care score, temperatures, load, disk space) on TCP port 47615, and answers "who's there"
//! broadcasts on UDP 47616 so others can find it. Nothing leaves the LAN: requests from public
//! addresses are refused and nothing is sent to the internet.
//!
//! Pairing uses the 6-digit code the sharing PC shows. The code itself never goes over the
//! network: each request carries a timestamp and a BLAKE3 keyed hash of it, with the key derived
//! from the code. Old timestamps are refused, and repeated wrong signatures lock the PC out for a
//! minute. Everything is read-only.

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub const PORT: u16 = 47615;
const DISCOVERY_PORT: u16 = 47616;
const HELLO: &[u8] = b"TESSERA?";

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Sharing {
    pub enabled: bool,
    pub code: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Peer {
    pub name: String,
    /// host:port
    pub addr: String,
    pub code: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Status {
    pub name: String,
    pub version: String,
    pub os: String,
    pub uptime: u64,
    pub care_score: Option<u32>,
    pub care_at: u64,
    pub cpu_load: f32,
    pub cpu_temp: Option<f32>,
    pub gpu_name: String,
    pub gpu_load: Option<f32>,
    pub gpu_temp: Option<f32>,
    pub ram_used: u64,
    pub ram_total: u64,
    /// (mount, total, free)
    pub disks: Vec<(String, u64, u64)>,
    pub at: u64,
}

fn sharing_path(base: &Path) -> PathBuf {
    base.join("lan-sharing.json")
}
fn peers_path(base: &Path) -> PathBuf {
    base.join("lan-peers.json")
}

pub fn sharing(base: &Path) -> Sharing {
    std::fs::read(sharing_path(base)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save_sharing(base: &Path, s: &Sharing) -> Result<(), String> {
    std::fs::write(sharing_path(base), serde_json::to_vec(s).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// A fresh random 6-digit pairing code.
pub fn new_code() -> String {
    let seed = format!("{:?}{}{}", Instant::now(), std::process::id(), crate::sysutil::now());
    let h = blake3::hash(seed.as_bytes());
    let n = u32::from_le_bytes(h.as_bytes()[..4].try_into().unwrap_or_default()) % 1_000_000;
    format!("{n:06}")
}

pub fn peers(base: &Path) -> Vec<Peer> {
    std::fs::read(peers_path(base)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save_peers(base: &Path, p: &[Peer]) -> Result<(), String> {
    std::fs::write(peers_path(base), serde_json::to_vec(p).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn sign(code: &str, t: u64) -> String {
    let key = blake3::derive_key("tessera lan v1", code.trim().as_bytes());
    blake3::keyed_hash(&key, t.to_string().as_bytes()).to_hex().to_string()
}

/// Private, loopback and link-local addresses only.
pub fn is_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v.is_private() || v.is_loopback() || v.is_link_local(),
        IpAddr::V6(v) => v.is_loopback() || (v.segments()[0] & 0xfe00) == 0xfc00 || (v.segments()[0] & 0xffc0) == 0xfe80 || v.to_ipv4_mapped().is_some_and(|m| m.is_private() || m.is_loopback()),
    }
}

pub fn host_name() -> String {
    sysinfo::System::host_name().unwrap_or_else(|| "PC".into())
}

/// What this PC tells paired PCs.
pub fn local_status(base: &Path) -> Status {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let snap = crate::sensors::latest().unwrap_or_default();
    let gpu = snap.gpus.first();
    let care = crate::care::load_config(base);
    let disks = sysinfo::Disks::new_with_refreshed_list().iter().filter(|d| d.total_space() > 0).map(|d| (d.mount_point().to_string_lossy().into_owned(), d.total_space(), d.available_space())).collect();
    Status {
        name: host_name(),
        version: env!("CARGO_PKG_VERSION").into(),
        os: sysinfo::System::long_os_version().unwrap_or_default(),
        uptime: sysinfo::System::uptime(),
        care_score: care.last_score,
        care_at: care.last_run,
        cpu_load: snap.cpu_load,
        cpu_temp: snap.cpu_temp,
        gpu_name: gpu.map(|g| g.name.clone()).unwrap_or_default(),
        gpu_load: gpu.and_then(|g| g.load),
        gpu_temp: gpu.and_then(|g| g.temp),
        ram_used: sys.used_memory(),
        ram_total: sys.total_memory(),
        disks,
        at: crate::sysutil::now(),
    }
}

static SERVING: AtomicBool = AtomicBool::new(false);

/// Answer status requests and discovery broadcasts while sharing is on. Safe to call again: the
/// threads start once and check the setting on every request.
pub fn serve(base: PathBuf) {
    if SERVING.swap(true, Ordering::SeqCst) {
        return;
    }
    let b = base.clone();
    std::thread::spawn(move || {
        let Ok(listener) = TcpListener::bind(("0.0.0.0", PORT)) else {
            SERVING.store(false, Ordering::SeqCst);
            return;
        };
        let mut failures: Vec<(IpAddr, Instant)> = Vec::new();
        for stream in listener.incoming().flatten() {
            let Ok(peer) = stream.peer_addr() else { continue };
            failures.retain(|f| f.1.elapsed() < Duration::from_secs(60));
            if !is_local(peer.ip()) || failures.iter().filter(|f| f.0 == peer.ip()).count() >= 10 {
                continue;
            }
            if !answer(stream, &b) {
                failures.push((peer.ip(), Instant::now()));
            }
        }
    });
    std::thread::spawn(move || {
        let Ok(sock) = UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)) else { return };
        let mut buf = [0u8; 64];
        while let Ok((n, from)) = sock.recv_from(&mut buf) {
            if &buf[..n] == HELLO && is_local(from.ip()) && sharing(&base).enabled {
                let reply = serde_json::json!({ "name": host_name(), "port": PORT }).to_string();
                let _ = sock.send_to(reply.as_bytes(), from);
            }
        }
    });
}

/// One request: `GET /status?t=<unix>&sig=<hex>`. Returns false on a bad signature.
fn answer(mut s: TcpStream, base: &Path) -> bool {
    let _ = s.set_read_timeout(Some(Duration::from_secs(3)));
    // Read the whole request head: closing with unread bytes makes Windows reset the connection
    let mut buf = [0u8; 2048];
    let mut n = 0;
    while n < buf.len() {
        match s.read(&mut buf[n..]) {
            Ok(0) | Err(_) => break,
            Ok(k) => n += k,
        }
        if buf[..n].windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let req = String::from_utf8_lossy(&buf[..n]);
    let line = req.lines().next().unwrap_or("");
    let share = sharing(base);
    let reply = |s: &mut TcpStream, code: &str, body: &str| {
        let msg = format!("HTTP/1.1 {code}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        let _ = s.write_all(msg.as_bytes());
        let _ = s.shutdown(std::net::Shutdown::Write);
    };
    if !share.enabled || share.code.is_empty() {
        reply(&mut s, "403 Forbidden", "{\"error\":\"sharing is off\"}");
        return true;
    }
    let query = line.split_whitespace().nth(1).and_then(|p| p.strip_prefix("/status?")).unwrap_or("");
    let get = |k: &str| query.split('&').find_map(|kv| kv.strip_prefix(k).and_then(|v| v.strip_prefix('=')));
    let t: u64 = get("t").and_then(|v| v.parse().ok()).unwrap_or(0);
    let fresh = crate::sysutil::now().abs_diff(t) <= 120;
    if !fresh || get("sig") != Some(sign(&share.code, t).as_str()) {
        reply(&mut s, "401 Unauthorized", "{\"error\":\"wrong code\"}");
        return false;
    }
    let body = serde_json::to_string(&local_status(base)).unwrap_or_default();
    reply(&mut s, "200 OK", &body);
    true
}

/// Ask a paired PC for its status.
pub fn fetch(addr: &str, code: &str) -> Result<Status, String> {
    let sa: SocketAddr = addr.parse().or_else(|_| format!("{addr}:{PORT}").parse()).map_err(|_| format!("{addr} is not an address"))?;
    if !is_local(sa.ip()) {
        return Err("only PCs on the local network can be added".into());
    }
    let mut s = TcpStream::connect_timeout(&sa, Duration::from_secs(2)).map_err(|_| "not reachable (is Tessera running there with sharing on?)".to_string())?;
    let _ = s.set_read_timeout(Some(Duration::from_secs(4)));
    let t = crate::sysutil::now();
    let req = format!("GET /status?t={t}&sig={} HTTP/1.1\r\nHost: {sa}\r\nConnection: close\r\n\r\n", sign(code, t));
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut resp = String::new();
    s.read_to_string(&mut resp).map_err(|e| e.to_string())?;
    let (head, body) = resp.split_once("\r\n\r\n").ok_or("bad answer")?;
    if head.starts_with("HTTP/1.1 401") {
        return Err("wrong pairing code".into());
    }
    if head.starts_with("HTTP/1.1 403") {
        return Err("sharing is turned off on that PC".into());
    }
    serde_json::from_str(body).map_err(|e| format!("bad answer: {e}"))
}

/// Broadcast "who's there" and collect the PCs that answer within a second: (name, host:port).
pub fn discover() -> Vec<(String, String)> {
    let Ok(sock) = UdpSocket::bind(("0.0.0.0", 0)) else { return Vec::new() };
    let _ = sock.set_broadcast(true);
    let _ = sock.set_read_timeout(Some(Duration::from_millis(250)));
    let _ = sock.send_to(HELLO, ("255.255.255.255", DISCOVERY_PORT));
    let start = Instant::now();
    let mut out: Vec<(String, String)> = Vec::new();
    let mut buf = [0u8; 512];
    while start.elapsed() < Duration::from_millis(1200) {
        let Ok((n, from)) = sock.recv_from(&mut buf) else { continue };
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&buf[..n]) else { continue };
        let name = v["name"].as_str().unwrap_or("PC").to_string();
        let port = v["port"].as_u64().unwrap_or(PORT as u64);
        let addr = format!("{}:{port}", from.ip());
        if is_local(from.ip()) && !out.iter().any(|o| o.1 == addr) {
            out.push((name, addr));
        }
    }
    out
}

/// This PC's LAN addresses, to show next to the pairing code.
pub fn local_addresses() -> Vec<String> {
    // Connecting a UDP socket sends nothing; it only picks the outgoing interface
    let mut out = Vec::new();
    if let Ok(s) = UdpSocket::bind(("0.0.0.0", 0))
        && s.connect(("10.255.255.255", 1)).is_ok()
        && let Ok(a) = s.local_addr()
        && is_local(a.ip())
    {
        out.push(a.ip().to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_and_checks_addresses() {
        assert_eq!(sign("123456", 100), sign("123456", 100));
        assert_ne!(sign("123456", 100), sign("123457", 100));
        assert_ne!(sign("123456", 100), sign("123456", 101));
        assert!(is_local("192.168.0.12".parse().unwrap()));
        assert!(is_local("10.1.2.3".parse().unwrap()));
        assert!(!is_local("8.8.8.8".parse().unwrap()));
        assert!(is_local("fe80::1".parse().unwrap()));
        assert!(!is_local("2001:4860::8888".parse().unwrap()));
        let c = new_code();
        assert!(c.len() == 6 && c.chars().all(|d| d.is_ascii_digit()));
    }

    #[test]
    fn answers_only_with_the_right_code() {
        let base = std::env::temp_dir().join(format!("tessera-lan-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        save_sharing(&base, &Sharing { enabled: true, code: "246810".into() }).unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let b = base.clone();
        let server = std::thread::spawn(move || {
            for s in listener.incoming().take(2).flatten() {
                answer(s, &b);
            }
        });
        assert_eq!(fetch(&addr, "000000").unwrap_err(), "wrong pairing code");
        let st = fetch(&addr, "246810").unwrap();
        assert_eq!(st.name, host_name());
        server.join().unwrap();
        let _ = std::fs::remove_dir_all(&base);
    }
}
