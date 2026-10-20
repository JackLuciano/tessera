//! Windows memory helpers: purge the standby list (cached file data Windows keeps "just in case")
//! and request a finer system timer while a game runs. Both need administrator rights for the
//! standby list; the timer request is released automatically when Tessera stops asking.

use std::sync::atomic::{AtomicBool, Ordering};

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtSetSystemInformation(class: i32, info: *mut core::ffi::c_void, len: u32) -> i32;
    fn NtSetTimerResolution(desired: u32, set: u8, current: *mut u32) -> i32;
    fn NtQueryTimerResolution(max: *mut u32, min: *mut u32, current: *mut u32) -> i32;
}

const SYSTEM_MEMORY_LIST_INFORMATION: i32 = 80;
const MEMORY_PURGE_STANDBY_LIST: u32 = 4;

/// Enable a privilege on this process token (SeProfileSingleProcessPrivilege for memory lists).
fn enable_privilege(name: &str) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LUID};
    use windows_sys::Win32::Security::{AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
    // SAFETY: plain Win32 calls with valid pointers; the token handle is closed before returning.
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut luid = LUID { LowPart: 0, HighPart: 0 };
        let ok = LookupPrivilegeValueW(std::ptr::null(), wide.as_ptr(), &mut luid) != 0 && {
            let tp = TOKEN_PRIVILEGES { PrivilegeCount: 1, Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }] };
            AdjustTokenPrivileges(token, 0, &tp, 0, std::ptr::null_mut(), std::ptr::null_mut()) != 0 && windows_sys::Win32::Foundation::GetLastError() == 0
        };
        CloseHandle(token);
        ok
    }
}

/// Drop the standby list. Returns an error without administrator rights.
pub fn purge_standby() -> Result<(), String> {
    if !enable_privilege("SeProfileSingleProcessPrivilege") {
        return Err("administrator rights needed".into());
    }
    let mut cmd = MEMORY_PURGE_STANDBY_LIST;
    // SAFETY: documented information class with a 4-byte command buffer.
    let st = unsafe { NtSetSystemInformation(SYSTEM_MEMORY_LIST_INFORMATION, (&mut cmd as *mut u32).cast(), 4) };
    if st >= 0 { Ok(()) } else { Err(format!("NTSTATUS {:#x}", st as u32)) }
}

static TIMER_HELD: AtomicBool = AtomicBool::new(false);

/// (current, best possible) timer resolution in milliseconds.
pub fn timer_resolution() -> (f32, f32) {
    let (mut max, mut min, mut cur) = (0u32, 0u32, 0u32);
    // SAFETY: three valid out pointers; values are in 100 ns units.
    unsafe { NtQueryTimerResolution(&mut max, &mut min, &mut cur) };
    (cur as f32 / 10_000.0, min as f32 / 10_000.0)
}

/// Ask for the finest timer resolution (usually 0.5 ms) until `release_timer`.
pub fn hold_timer() {
    if TIMER_HELD.swap(true, Ordering::Relaxed) {
        return;
    }
    let (_, best) = timer_resolution();
    let mut cur = 0u32;
    // SAFETY: valid out pointer; the request is per process and released in `release_timer`.
    unsafe { NtSetTimerResolution((best * 10_000.0) as u32, 1, &mut cur) };
}

pub fn release_timer() {
    if !TIMER_HELD.swap(false, Ordering::Relaxed) {
        return;
    }
    let mut cur = 0u32;
    // SAFETY: as above; `set = 0` withdraws this process's request.
    unsafe { NtSetTimerResolution(0, 0, &mut cur) };
}

// ---------- core parking (power plan setting) ----------

const SUB_PROCESSOR: &str = "54533251-82be-4824-96c1-47b60b740d00";
const CPMINCORES: &str = "0cc5b647-c1df-4637-891a-dec35c318583";

/// Minimum unparked cores (%) on AC power for the active plan.
pub fn core_parking_min() -> Option<u32> {
    let out = crate::sysutil::run("powercfg", &["/query", "scheme_current", SUB_PROCESSOR, CPMINCORES]).ok()?;
    // The labels are localized, but the two indexes are the last two hex numbers (AC, then DC)
    let hex: Vec<u32> = out.split_whitespace().filter_map(|w| w.strip_prefix("0x")).filter_map(|h| u32::from_str_radix(h, 16).ok()).collect();
    hex.len().checked_sub(2).map(|i| hex[i])
}

/// Set the minimum unparked cores (%) on AC power and re-apply the plan.
pub fn set_core_parking_min(percent: u32) -> Result<(), String> {
    let v = percent.min(100).to_string();
    crate::sysutil::run("powercfg", &["/setacvalueindex", "scheme_current", SUB_PROCESSOR, CPMINCORES, &v])?;
    crate::sysutil::run("powercfg", &["/setactive", "scheme_current"]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_timer_resolution() {
        let (cur, best) = timer_resolution();
        assert!(cur > 0.0 && best > 0.0 && best <= cur);
    }
}
