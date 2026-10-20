//! Ping monitor: round-trip time, jitter and packet loss to a host, sampled once a second while
//! something (the FPS window, the page) wants it. ICMP echo on Windows (no admin needed through
//! IcmpSendEcho), a TCP connect to port 443 elsewhere.

use parking_lot::Mutex;
use serde::Serialize;
use std::collections::VecDeque;
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[derive(Serialize, Clone, Default)]
pub struct PingState {
    pub host: String,
    pub running: bool,
    /// Last results in ms, `None` = lost.
    pub samples: Vec<Option<f32>>,
    pub avg: Option<f32>,
    pub jitter: Option<f32>,
    /// Percent of the last samples that got no answer.
    pub loss: f32,
}

struct State {
    host: String,
    samples: VecDeque<Option<f32>>,
    /// Last time someone asked for results; the pinger stops after a minute without interest.
    wanted: Instant,
    running: bool,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(State { host: "1.1.1.1".into(), samples: VecDeque::new(), wanted: Instant::now(), running: false }))
}

fn resolve(host: &str) -> Option<IpAddr> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Some(ip);
    }
    (host, 0).to_socket_addrs().ok()?.map(|a| a.ip()).find(|ip| ip.is_ipv4())
}

#[cfg(windows)]
fn ping_once(ip: IpAddr) -> Option<f32> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho, ICMP_ECHO_REPLY};
    let IpAddr::V4(v4) = ip else { return tcp_once(ip) };
    let data = [0x54u8; 32];
    let mut reply = vec![0u8; std::mem::size_of::<ICMP_ECHO_REPLY>() + data.len() + 8];
    // SAFETY: handle is checked and closed; buffers are valid for the sizes passed. The address is
    // in network byte order as a u32.
    unsafe {
        let h = IcmpCreateFile();
        if h.is_null() || h as isize == -1 {
            return None;
        }
        let n = IcmpSendEcho(h, u32::from_ne_bytes(v4.octets()), data.as_ptr().cast(), data.len() as u16, std::ptr::null(), reply.as_mut_ptr().cast(), reply.len() as u32, 1000);
        IcmpCloseHandle(h);
        if n == 0 {
            return None;
        }
        let r = &*(reply.as_ptr() as *const ICMP_ECHO_REPLY);
        (r.Status == 0).then_some(r.RoundTripTime as f32)
    }
}

#[cfg(not(windows))]
fn ping_once(ip: IpAddr) -> Option<f32> {
    tcp_once(ip)
}

fn tcp_once(ip: IpAddr) -> Option<f32> {
    let start = Instant::now();
    TcpStream::connect_timeout(&SocketAddr::new(ip, 443), Duration::from_millis(1000)).ok()?;
    Some(start.elapsed().as_secs_f32() * 1000.0)
}

pub fn stats(samples: &[Option<f32>]) -> (Option<f32>, Option<f32>, f32) {
    let ok: Vec<f32> = samples.iter().flatten().copied().collect();
    let avg = (!ok.is_empty()).then(|| ok.iter().sum::<f32>() / ok.len() as f32);
    let jitter = (ok.len() > 1).then(|| ok.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f32>() / (ok.len() - 1) as f32);
    let loss = if samples.is_empty() { 0.0 } else { (samples.len() - ok.len()) as f32 / samples.len() as f32 * 100.0 };
    (avg, jitter, loss)
}

/// Current results; asking keeps the pinger alive (and starts it).
pub fn get(host: Option<String>) -> PingState {
    let mut st = state().lock();
    if let Some(h) = host.filter(|h| !h.trim().is_empty() && *h != st.host) {
        st.host = h.trim().to_string();
        st.samples.clear();
    }
    st.wanted = Instant::now();
    if !st.running {
        st.running = true;
        std::thread::spawn(run);
    }
    let samples: Vec<Option<f32>> = st.samples.iter().copied().collect();
    let (avg, jitter, loss) = stats(&samples);
    PingState { host: st.host.clone(), running: true, samples, avg, jitter, loss }
}

fn run() {
    loop {
        let host = {
            let mut st = state().lock();
            if st.wanted.elapsed() > Duration::from_secs(60) {
                st.running = false;
                return;
            }
            st.host.clone()
        };
        let started = Instant::now();
        let r = resolve(&host).and_then(ping_once);
        {
            let mut st = state().lock();
            if st.host == host {
                st.samples.push_back(r);
                while st.samples.len() > 120 {
                    st.samples.pop_front();
                }
            }
        }
        std::thread::sleep(Duration::from_secs(1).saturating_sub(started.elapsed()));
    }
}

/// The default gateway (router) address, for the "router" preset.
pub fn gateway() -> Option<String> {
    #[cfg(windows)]
    {
        crate::sysutil::powershell::<Vec<String>>("ConvertTo-Json -Compress -InputObject @(Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue | Sort-Object RouteMetric | ForEach-Object { \"$($_.NextHop)\" })")
            .ok()?
            .into_iter()
            .find(|h| h != "0.0.0.0")
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_jitter_and_loss() {
        let (avg, jitter, loss) = stats(&[Some(10.0), Some(20.0), None, Some(10.0)]);
        assert_eq!(avg, Some(40.0 / 3.0));
        assert_eq!(jitter, Some(10.0));
        assert_eq!(loss, 25.0);
    }
}
