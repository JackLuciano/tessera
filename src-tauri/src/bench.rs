//! Quick benchmark: CPU (single core and all cores), memory bandwidth and disk speed in about half
//! a minute, so tweaks and driver updates can be compared before and after. The GPU part runs in
//! the webview (WebGL) and its score is passed in. Results are saved with the date, the GPU driver
//! version and the active optimizer profile. Nothing runs unless the user starts it.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Run {
    /// Unix seconds.
    pub at: u64,
    /// Points; higher is better.
    pub cpu_single: f64,
    pub cpu_multi: f64,
    /// GB/s
    pub memory: f64,
    /// MB/s
    pub disk_read: f64,
    pub disk_write: f64,
    /// Random 4K reads per second.
    pub disk_iops: f64,
    /// WebGL frames per second at 1080p (None when the GPU test was skipped).
    pub gpu: Option<f64>,
    pub driver: String,
    pub profile: String,
    /// The drive the disk test ran on.
    pub drive: String,
    pub threads: usize,
}

fn path(base: &Path) -> PathBuf {
    base.join("benchmarks.json")
}

pub fn history(base: &Path) -> Vec<Run> {
    std::fs::read(path(base)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn delete(base: &Path, at: u64) {
    let mut all = history(base);
    all.retain(|r| r.at != at);
    let _ = std::fs::write(path(base), serde_json::to_vec(&all).unwrap_or_default());
}

/// A semi-compressible buffer (text-like), the same on every run.
fn workload(len: usize) -> Vec<u8> {
    let words: [&[u8]; 8] = [b"tessera ", b"disk ", b"space ", b"cleaner ", b"benchmark ", b"0123456789 ", b"lorem ipsum ", b"\n"];
    let mut x: u32 = 0x9e37_79b9;
    let mut out = Vec::with_capacity(len + 16);
    while out.len() < len {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        out.extend_from_slice(words[(x % 8) as usize]);
    }
    out.truncate(len);
    out
}

/// One unit of CPU work: hash and compress a 4 MB block. Returns bytes processed.
fn cpu_unit(data: &[u8]) -> u64 {
    let h = blake3::hash(data);
    let c = lz4_flex::compress(data);
    std::hint::black_box((h, c.len()));
    data.len() as u64 * 2
}

/// Megabytes per second through `cpu_unit`, on one thread or all of them, for `secs`.
fn cpu(secs: f64, all: bool) -> f64 {
    let block = workload(4 << 20);
    let start = Instant::now();
    let limit = Duration::from_secs_f64(secs);
    let threads = if all { rayon::current_num_threads() } else { 1 };
    let total: u64 = (0..threads)
        .into_par_iter()
        .map(|_| {
            let mut n = 0;
            while start.elapsed() < limit {
                n += cpu_unit(&block);
            }
            n
        })
        .sum();
    total as f64 / start.elapsed().as_secs_f64() / 1e6
}

/// Memory copy bandwidth in GB/s (read + write counted).
fn memory() -> f64 {
    let size = 256 << 20;
    let src = vec![1u8; size];
    let mut dst = vec![0u8; size];
    let start = Instant::now();
    let mut copied = 0u64;
    while start.elapsed() < Duration::from_secs(2) {
        dst.copy_from_slice(&src);
        std::hint::black_box(&dst);
        copied += 2 * size as u64;
    }
    copied as f64 / start.elapsed().as_secs_f64() / 1e9
}

/// A 4096-aligned buffer, needed for unbuffered I/O on Windows.
struct Aligned {
    ptr: *mut u8,
    layout: std::alloc::Layout,
}

impl Aligned {
    fn new(len: usize) -> Self {
        let layout = std::alloc::Layout::from_size_align(len, 4096).expect("layout");
        // SAFETY: non-zero size, valid alignment; freed in Drop with the same layout
        let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
        assert!(!ptr.is_null(), "out of memory");
        Self { ptr, layout }
    }
    fn slice(&mut self) -> &mut [u8] {
        // SAFETY: ptr points to layout.size() initialised bytes owned by self
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.layout.size()) }
    }
}

impl Drop for Aligned {
    fn drop(&mut self) {
        // SAFETY: allocated in new() with this layout
        unsafe { std::alloc::dealloc(self.ptr, self.layout) }
    }
}

/// Open the test file bypassing the Windows file cache, so the disk itself is measured.
fn open(p: &Path, write: bool) -> std::io::Result<std::fs::File> {
    let mut o = std::fs::OpenOptions::new();
    o.read(true);
    if write {
        o.write(true).create(true).truncate(true);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_NO_BUFFERING, FILE_FLAG_WRITE_THROUGH};
        o.custom_flags(FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH);
    }
    o.open(p)
}

/// Sequential write and read of 512 MB, then random 4K reads; MB/s, MB/s, IOPS.
fn disk(dir: &Path) -> Result<(f64, f64, f64), String> {
    let file = dir.join("tessera-benchmark.tmp");
    let block = 8 << 20;
    let blocks = 64;
    let mut buf = Aligned::new(block);
    buf.slice().copy_from_slice(&workload(block));
    let result = (|| -> std::io::Result<(f64, f64, f64)> {
        let mut f = open(&file, true)?;
        let t = Instant::now();
        for _ in 0..blocks {
            f.write_all(buf.slice())?;
        }
        f.sync_all()?;
        let write = (block * blocks) as f64 / t.elapsed().as_secs_f64() / 1e6;
        drop(f);
        let mut f = open(&file, false)?;
        let t = Instant::now();
        for _ in 0..blocks {
            f.read_exact(buf.slice())?;
        }
        let read = (block * blocks) as f64 / t.elapsed().as_secs_f64() / 1e6;
        let mut small = Aligned::new(4096);
        let mut x: u64 = 0x2545_f491_4f6c_dd1d;
        let pages = (block * blocks / 4096) as u64;
        let t = Instant::now();
        let mut n = 0u64;
        while t.elapsed() < Duration::from_secs(3) {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            f.seek(SeekFrom::Start((x % pages) * 4096))?;
            f.read_exact(small.slice())?;
            n += 1;
        }
        Ok((read, write, n as f64 / t.elapsed().as_secs_f64()))
    })();
    let _ = std::fs::remove_file(&file);
    result.map_err(|e| format!("disk test: {e}"))
}

/// Run the CPU, memory and disk parts (about 25 s), save and return the result. `progress` gets
/// the part that is starting: cpu_single, cpu_multi, memory, disk.
pub fn run(base: &Path, disk_dir: &Path, gpu: Option<f64>, driver: String, progress: impl Fn(&str)) -> Result<Run, String> {
    progress("cpu_single");
    let cpu_single = cpu(5.0, false);
    progress("cpu_multi");
    let cpu_multi = cpu(5.0, true);
    progress("memory");
    let memory = memory();
    progress("disk");
    let (disk_read, disk_write, disk_iops) = disk(disk_dir)?;
    let r = Run {
        at: crate::sysutil::now(),
        // Points: MB/s scaled so a typical desktop core lands in the low thousands
        cpu_single: (cpu_single * 2.0).round(),
        cpu_multi: (cpu_multi * 2.0).round(),
        memory: (memory * 10.0).round() / 10.0,
        disk_read: disk_read.round(),
        disk_write: disk_write.round(),
        disk_iops: disk_iops.round(),
        gpu: gpu.map(|g| (g * 10.0).round() / 10.0),
        driver,
        profile: profile(base),
        drive: disk_dir.ancestors().last().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
        threads: rayon::current_num_threads(),
    };
    let mut all = history(base);
    all.insert(0, r.clone());
    all.truncate(100);
    std::fs::write(path(base), serde_json::to_vec(&all).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(r)
}

#[cfg(windows)]
fn profile(base: &Path) -> String {
    crate::optimizer::active_profile(base).unwrap_or_default()
}

#[cfg(not(windows))]
fn profile(_: &Path) -> String {
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workload_is_stable_and_compressible() {
        let a = workload(1 << 16);
        assert_eq!(a, workload(1 << 16));
        assert!(lz4_flex::compress(&a).len() < a.len() / 2);
        assert!(cpu(0.05, false) > 0.0);
    }

    #[test]
    fn disk_test_cleans_up() {
        let dir = std::env::temp_dir().join(format!("tessera-bench-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (r, w, iops) = disk(&dir).unwrap();
        assert!(r > 0.0 && w > 0.0 && iops > 0.0);
        assert!(!dir.join("tessera-benchmark.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
