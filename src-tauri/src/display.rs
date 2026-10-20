//! Displays (Windows): refresh rate per monitor and whether it runs below its maximum (a very
//! common reason games feel choppy), plus the two Windows graphics switches for games:
//! optimizations for windowed games and variable refresh rate.

use serde::Serialize;
use windows_sys::Win32::Graphics::Gdi::*;

#[derive(Serialize, Clone, Debug)]
pub struct Monitor {
    /// \\.\DISPLAY1
    pub device: String,
    pub name: String,
    pub adapter: String,
    pub width: u32,
    pub height: u32,
    pub hz: u32,
    /// Highest refresh rate available at the current resolution.
    pub max_hz: u32,
    pub primary: bool,
}

fn from_wide(w: &[u16]) -> String {
    String::from_utf16_lossy(&w[..w.iter().position(|c| *c == 0).unwrap_or(w.len())])
}

fn devmode() -> DEVMODEW {
    DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() }
}

pub fn monitors() -> Vec<Monitor> {
    let mut out = Vec::new();
    for i in 0.. {
        let mut dd = DISPLAY_DEVICEW { cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32, ..Default::default() };
        // SAFETY: valid, sized out struct; enumeration stops when the call fails.
        if unsafe { EnumDisplayDevicesW(std::ptr::null(), i, &mut dd, 0) } == 0 {
            break;
        }
        if dd.StateFlags & DISPLAY_DEVICE_ACTIVE == 0 {
            continue;
        }
        let device = dd.DeviceName;
        let mut cur = devmode();
        // SAFETY: device name is NUL-terminated (fixed buffer from the API); out struct sized.
        if unsafe { EnumDisplaySettingsW(device.as_ptr(), ENUM_CURRENT_SETTINGS, &mut cur) } == 0 {
            continue;
        }
        let mut max_hz = cur.dmDisplayFrequency;
        for m in 0.. {
            let mut dm = devmode();
            // SAFETY: as above; mode index enumeration ends with a failed call.
            if unsafe { EnumDisplaySettingsW(device.as_ptr(), m, &mut dm) } == 0 {
                break;
            }
            if dm.dmPelsWidth == cur.dmPelsWidth && dm.dmPelsHeight == cur.dmPelsHeight && dm.dmBitsPerPel == cur.dmBitsPerPel {
                max_hz = max_hz.max(dm.dmDisplayFrequency);
            }
        }
        // The monitor itself is the first child device of the adapter output
        let mut mon = DISPLAY_DEVICEW { cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32, ..Default::default() };
        // SAFETY: as above.
        let name = if unsafe { EnumDisplayDevicesW(device.as_ptr(), 0, &mut mon, 0) } != 0 { from_wide(&mon.DeviceString) } else { String::new() };
        out.push(Monitor {
            device: from_wide(&device),
            name,
            adapter: from_wide(&dd.DeviceString),
            width: cur.dmPelsWidth,
            height: cur.dmPelsHeight,
            hz: cur.dmDisplayFrequency,
            max_hz,
            primary: dd.StateFlags & 4 != 0,
        });
    }
    out
}

/// Switch a monitor to the given refresh rate at its current resolution (tested first).
pub fn set_refresh(device: &str, hz: u32) -> Result<(), String> {
    let wide: Vec<u16> = device.encode_utf16().chain([0]).collect();
    let mut dm = devmode();
    // SAFETY: NUL-terminated device name, sized out struct.
    if unsafe { EnumDisplaySettingsW(wide.as_ptr(), ENUM_CURRENT_SETTINGS, &mut dm) } == 0 {
        return Err("display not found".into());
    }
    dm.dmDisplayFrequency = hz;
    dm.dmFields = DM_DISPLAYFREQUENCY;
    // SAFETY: valid device name and mode; CDS_TEST only checks, CDS_UPDATEREGISTRY applies and keeps it.
    unsafe {
        if ChangeDisplaySettingsExW(wide.as_ptr(), &dm, std::ptr::null_mut(), CDS_TEST, std::ptr::null()) != DISP_CHANGE_SUCCESSFUL {
            return Err(format!("{hz} Hz is not supported by this display"));
        }
        if ChangeDisplaySettingsExW(wide.as_ptr(), &dm, std::ptr::null_mut(), CDS_UPDATEREGISTRY, std::ptr::null()) != DISP_CHANGE_SUCCESSFUL {
            return Err("Windows refused the change".into());
        }
    }
    Ok(())
}

// ---------- Windows graphics settings (Settings › Display › Graphics) ----------

const PREFS: &str = r"Software\Microsoft\DirectX\UserGpuPreferences";
const GLOBAL: &str = "DirectXUserGlobalSettings";

fn global() -> String {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    RegKey::predef(HKEY_CURRENT_USER).open_subkey(PREFS).and_then(|k| k.get_value::<String, _>(GLOBAL)).unwrap_or_default()
}

/// Read a `Key=1;` switch from the global DirectX settings string.
pub fn graphics_switch(key: &str) -> bool {
    global().split(';').any(|kv| kv.trim() == format!("{key}=1"))
}

/// SwapEffectUpgradeEnable (optimizations for windowed games) or VRROptimizeEnable (variable refresh rate).
pub fn set_graphics_switch(key: &str, on: bool) -> Result<(), String> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    if key != "SwapEffectUpgradeEnable" && key != "VRROptimizeEnable" {
        return Err("unknown setting".into());
    }
    let mut parts: Vec<String> = global().split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty() && !s.starts_with(&format!("{key}="))).collect();
    parts.push(format!("{key}={}", u8::from(on)));
    let (k, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(PREFS).map_err(|e| e.to_string())?;
    k.set_value(GLOBAL, &format!("{};", parts.join(";"))).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_monitors() {
        let m = monitors();
        assert!(m.iter().all(|x| x.hz > 0 && x.max_hz >= x.hz));
    }
}
