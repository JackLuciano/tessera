//! Real CPU temperature through PawnIO, the signed kernel driver LibreHardwareMonitor also uses.
//! The user installs it once from Tessera (opt-in, needs admin). Tessera opens its device with
//! `DeviceIoControl` and loads one of PawnIO's own signed modules: IntelMSR (package thermal status
//! MSR) or AMDFamily17 (Zen thermal register over SMN). Without the driver `sensors` falls back to
//! ACPI thermal zones, which many boards don't have at all.
//!
//! `pawnio/*.bin` are the unmodified modules from PawnIO.Modules 0.2.11 (LGPL-2.1, `pawnio/COPYING`).

use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};
use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, WAIT_ABANDONED, WAIT_OBJECT_0};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};

const SETUP_VERSION: &str = "2.2.0";
const SETUP_URL: &str = "https://github.com/namazso/PawnIO.Setup/releases/download/2.2.0/PawnIO_setup.exe";
const SETUP_SHA256: &str = "1F519A22E47187F70A1379A48CA604981C4FCF694F4E65B734AAA74A9FBA3032";

const INTEL_MSR: &[u8] = include_bytes!("../pawnio/IntelMSR.bin");
const AMD_FAMILY_17: &[u8] = include_bytes!("../pawnio/AMDFamily17.bin");

const DEVICE_TYPE: u32 = 41394 << 16;
const IOCTL_LOAD_BINARY: u32 = DEVICE_TYPE | (0x821 << 2);
const IOCTL_EXECUTE: u32 = DEVICE_TYPE | (0x841 << 2);
/// Function names are passed in a fixed 32-byte, NUL-padded field ahead of the arguments.
const FN_NAME_LEN: usize = 32;

const MSR_TEMPERATURE_TARGET: u64 = 0x1A2;
const MSR_THERM_STATUS: u64 = 0x19C;
const MSR_PACKAGE_THERM_STATUS: u64 = 0x1B1;
/// Zen THM_TCON_CUR_TMP.
const SMN_CUR_TEMP: u64 = 0x0005_9800;

/// Set after an install, so the sampler tries the driver again right away.
pub static RETRY: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, PartialEq)]
enum Vendor {
    Intel,
    /// Family 17h and newer (Zen).
    AmdZen,
}

#[allow(unused_unsafe)]
fn vendor() -> Option<Vendor> {
    #[cfg(target_arch = "x86_64")]
    {
        use std::arch::x86_64::__cpuid;
        // SAFETY: CPUID is available on every x86_64 CPU
        let (v0, v1) = unsafe { (__cpuid(0), __cpuid(1)) };
        let mut id = [0u8; 12];
        id[..4].copy_from_slice(&v0.ebx.to_le_bytes());
        id[4..8].copy_from_slice(&v0.edx.to_le_bytes());
        id[8..].copy_from_slice(&v0.ecx.to_le_bytes());
        let base = (v1.eax >> 8) & 0xF;
        let family = if base == 0xF { base + ((v1.eax >> 20) & 0xFF) } else { base };
        match &id {
            b"GenuineIntel" => Some(Vendor::Intel),
            b"AuthenticAMD" if family >= 0x17 => Some(Vendor::AmdZen),
            _ => None,
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    None
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Whether this CPU has a PawnIO module Tessera knows how to read.
pub fn supported() -> bool {
    vendor().is_some()
}

/// Installed version of the driver, from its uninstall entry.
pub fn installed_version() -> Option<String> {
    use winreg::{enums::HKEY_LOCAL_MACHINE, RegKey};
    let key = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO").ok()?;
    key.get_value::<String, _>("DisplayVersion").ok().filter(|v| !v.is_empty())
}

/// Download the signed PawnIO installer (checksum-pinned) and run it silently with a UAC prompt.
/// Returns true when Windows needs a restart before the driver works.
pub fn install(base: &Path) -> Result<bool, String> {
    let dir = base.join("tools");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let exe = dir.join(format!("PawnIO_setup-{SETUP_VERSION}.exe"));
    let exe_s = crate::sysutil::ps_quote(&exe.to_string_lossy());
    let hash = crate::sysutil::powershell_raw(&format!(
        "Invoke-WebRequest -UseBasicParsing -Uri {} -OutFile {exe_s}; (Get-FileHash -Algorithm SHA256 -LiteralPath {exe_s}).Hash",
        crate::sysutil::ps_quote(SETUP_URL)
    ))?;
    if !hash.trim().eq_ignore_ascii_case(SETUP_SHA256) {
        let _ = std::fs::remove_file(&exe);
        return Err("the PawnIO download did not match the expected checksum".into());
    }
    let code = crate::sysutil::powershell_raw(&format!(
        // A declined UAC prompt throws; report it as ERROR_CANCELLED
        "try {{ (Start-Process -FilePath {exe_s} -ArgumentList '-install','-silent' -Verb RunAs -Wait -PassThru -ErrorAction Stop).ExitCode }} catch {{ 1223 }}"
    ));
    let _ = std::fs::remove_file(&exe);
    let code: i64 = code?.trim().parse().map_err(|_| "the PawnIO installer did not start".to_string())?;
    RETRY.store(true, Ordering::Relaxed);
    match code {
        0 => Ok(false),
        // ERROR_SUCCESS_REBOOT_REQUIRED
        3010 => Ok(true),
        // ERROR_CANCELLED: the UAC prompt was declined
        1223 => Err("cancelled".into()),
        c => Err(format!("the PawnIO installer failed (code {c})")),
    }
}

/// An open PawnIO handle with the CPU's module loaded.
pub struct Reader {
    handle: HANDLE,
    vendor: Vendor,
    /// Shared with other monitoring tools that touch PCI/SMN (LibreHardwareMonitor, HWiNFO, …).
    pci_mutex: HANDLE,
    /// Intel: the temperature the digital readouts count down from.
    tjmax: f32,
    /// Intel: the package sensor is missing on some CPUs; the core sensor is used then.
    package: bool,
}

// SAFETY: the handles are only used through `&self` by the one sampler thread that owns the reader
unsafe impl Send for Reader {}

impl Reader {
    /// Open the driver and load the module; `None` when the driver is missing, Tessera isn't
    /// running as administrator, or the CPU isn't supported.
    pub fn open() -> Option<Self> {
        let vendor = vendor()?;
        let path = wide(r"\\?\GLOBALROOT\Device\PawnIO");
        // SAFETY: plain Win32 calls with valid, NUL-terminated buffers; every handle is closed in Drop
        unsafe {
            let handle = CreateFileW(path.as_ptr(), GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE, null(), OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, null_mut());
            if handle == INVALID_HANDLE_VALUE || handle.is_null() {
                return None;
            }
            let blob = if vendor == Vendor::Intel { INTEL_MSR } else { AMD_FAMILY_17 };
            if DeviceIoControl(handle, IOCTL_LOAD_BINARY, blob.as_ptr().cast(), blob.len() as u32, null_mut(), 0, null_mut(), null_mut()) == 0 {
                CloseHandle(handle);
                return None;
            }
            let pci_mutex = if vendor == Vendor::AmdZen { CreateMutexW(null(), 0, wide(r"Global\Access_PCI").as_ptr()) } else { null_mut() };
            let mut r = Self { handle, vendor, pci_mutex, tjmax: 100.0, package: true };
            if vendor == Vendor::Intel {
                r.tjmax = r.exec("ioctl_read_msr", MSR_TEMPERATURE_TARGET).map(|v| ((v >> 16) & 0xFF) as f32).filter(|t| (70.0..=120.0).contains(t)).unwrap_or(100.0);
                r.package = r.exec("ioctl_read_msr", MSR_PACKAGE_THERM_STATUS).is_some();
            }
            r.read().is_some().then_some(r)
        }
    }

    /// Run a module function that takes and returns one 64-bit value.
    fn exec(&self, name: &str, arg: u64) -> Option<u64> {
        let mut input = [0u8; FN_NAME_LEN + 8];
        let n = name.len().min(FN_NAME_LEN - 1);
        input[..n].copy_from_slice(&name.as_bytes()[..n]);
        input[FN_NAME_LEN..].copy_from_slice(&arg.to_le_bytes());
        let mut out = 0u64;
        let mut got = 0u32;
        // SAFETY: both buffers outlive the synchronous call and their sizes are passed alongside
        let ok = unsafe {
            DeviceIoControl(self.handle, IOCTL_EXECUTE, input.as_ptr().cast(), input.len() as u32, (&mut out as *mut u64).cast(), 8, &mut got, null_mut())
        };
        (ok != 0 && got >= 8).then_some(out)
    }

    /// CPU temperature in °C: Intel package (or core) temperature, AMD Tctl.
    pub fn read(&self) -> Option<f32> {
        let t = match self.vendor {
            Vendor::Intel if self.package => {
                let v = self.exec("ioctl_read_msr", MSR_PACKAGE_THERM_STATUS)?;
                self.tjmax - ((v >> 16) & 0x7F) as f32
            }
            Vendor::Intel => {
                let v = self.exec("ioctl_read_msr", MSR_THERM_STATUS)?;
                // Bit 31: the readout is valid
                if v & (1 << 31) == 0 {
                    return None;
                }
                self.tjmax - ((v >> 16) & 0x7F) as f32
            }
            Vendor::AmdZen => {
                // SAFETY: the mutex handle is valid (or null, then skipped) for the reader's lifetime
                let locked = !self.pci_mutex.is_null() && matches!(unsafe { WaitForSingleObject(self.pci_mutex, 100) }, WAIT_OBJECT_0 | WAIT_ABANDONED);
                let v = self.exec("ioctl_read_smn", SMN_CUR_TEMP);
                if locked {
                    // SAFETY: released only after a successful wait
                    unsafe { ReleaseMutex(self.pci_mutex) };
                }
                let v = v?;
                let t = ((v >> 21) & 0x7FF) as f32 / 8.0;
                // Range select bit: the value is offset by 49 °C
                if v & (1 << 19) != 0 { t - 49.0 } else { t }
            }
        };
        (t > 5.0 && t < 125.0).then_some(t)
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        // SAFETY: handles opened in `open` and closed exactly once here
        unsafe {
            CloseHandle(self.handle);
            if !self.pci_mutex.is_null() {
                CloseHandle(self.pci_mutex);
            }
        }
    }
}
