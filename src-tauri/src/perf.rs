//! Windows performance counters (PDH) without any driver: CPU clock and performance limits,
//! thermal zones, GPU engine load, memory lists. Counter paths use the English names
//! (`PdhAddEnglishCounterW`), so they work on every Windows language.

use std::collections::HashMap;
use windows_sys::Win32::System::Performance::*;

pub struct Query {
    q: PDH_HQUERY,
    counters: Vec<(String, PDH_HCOUNTER)>,
}

// SAFETY: a PDH query handle can be used from any thread as long as calls don't overlap; the
// owner keeps it behind a mutex.
unsafe impl Send for Query {}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

impl Query {
    /// Open a query with the given counters (wildcards like `\Thermal Zone Information(*)\...` allowed).
    /// Counters that don't exist on this machine are skipped.
    pub fn new(paths: &[&str]) -> Option<Self> {
        let mut q: PDH_HQUERY = std::ptr::null_mut();
        // SAFETY: valid out pointer; a null data source means live data.
        if unsafe { PdhOpenQueryW(std::ptr::null(), 0, &mut q) } != 0 {
            return None;
        }
        let mut counters = Vec::new();
        for p in paths {
            let mut c: PDH_HCOUNTER = std::ptr::null_mut();
            let w = wide(p);
            // SAFETY: `w` is NUL-terminated and outlives the call; `c` is a valid out pointer.
            if unsafe { PdhAddEnglishCounterW(q, w.as_ptr(), 0, &mut c) } == 0 {
                counters.push((p.to_string(), c));
            }
        }
        let query = Query { q, counters };
        // Rate counters need two samples: take the first one now
        query.collect();
        Some(query)
    }

    pub fn collect(&self) {
        // SAFETY: the query handle is open for the lifetime of `self`.
        unsafe { PdhCollectQueryData(self.q) };
    }

    /// Values of every counter after the last `collect`, keyed by path; wildcard counters yield one
    /// entry per instance as `path|instance`.
    pub fn values(&self) -> HashMap<String, f64> {
        let mut out = HashMap::new();
        for (path, c) in &self.counters {
            if path.contains('*') {
                for (inst, v) in array(*c) {
                    out.insert(format!("{path}|{inst}"), v);
                }
            } else {
                let mut v = PDH_FMT_COUNTERVALUE::default();
                // SAFETY: valid counter handle and out struct.
                let st = unsafe { PdhGetFormattedCounterValue(*c, PDH_FMT_DOUBLE, std::ptr::null_mut(), &mut v) };
                if st == 0 && (v.CStatus == 0 || v.CStatus == 1) {
                    // SAFETY: PDH_FMT_DOUBLE fills the double member of the union.
                    out.insert(path.clone(), unsafe { v.Anonymous.doubleValue });
                }
            }
        }
        out
    }
}

fn array(c: PDH_HCOUNTER) -> Vec<(String, f64)> {
    let mut size = 0u32;
    let mut count = 0u32;
    // SAFETY: first call with a null buffer only asks for the size.
    let st = unsafe { PdhGetFormattedCounterArrayW(c, PDH_FMT_DOUBLE, &mut size, &mut count, std::ptr::null_mut()) };
    if st != PDH_MORE_DATA || size == 0 {
        return Vec::new();
    }
    // The buffer holds the item structs followed by their name strings; u64 keeps it aligned
    let mut buf = vec![0u64; (size as usize).div_ceil(8)];
    let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
    // SAFETY: the buffer is at least `size` bytes and suitably aligned.
    if unsafe { PdhGetFormattedCounterArrayW(c, PDH_FMT_DOUBLE, &mut size, &mut count, items) } != 0 {
        return Vec::new();
    }
    (0..count as usize)
        .filter_map(|i| {
            // SAFETY: PDH wrote `count` items into the buffer; names point inside it.
            let it = unsafe { &*items.add(i) };
            if it.FmtValue.CStatus != 0 && it.FmtValue.CStatus != 1 {
                return None;
            }
            let name = unsafe { wstr(it.szName) };
            Some((name, unsafe { it.FmtValue.Anonymous.doubleValue }))
        })
        .collect()
}

/// SAFETY: `p` must be null or point to a NUL-terminated UTF-16 string.
unsafe fn wstr(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut n = 0;
    while unsafe { *p.add(n) } != 0 {
        n += 1;
    }
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, n) })
}

impl Drop for Query {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by PdhOpenQueryW and is closed once.
        unsafe { PdhCloseQuery(self.q) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_processor_counters() {
        let q = Query::new(&[r"\Processor Information(_Total)\% Processor Utility", r"\Memory\Available Bytes", r"\Nope\Missing"]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        q.collect();
        let v = q.values();
        assert!(v.get(r"\Memory\Available Bytes").copied().unwrap_or(0.0) > 0.0);
        assert!(!v.contains_key(r"\Nope\Missing"));
    }
}
