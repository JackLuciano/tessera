//! Tray icon: open / Care scan / quit, a live CPU and memory readout (tooltip, menu line and an
//! optional meter drawn into the icon), close-to-tray, and the notifications Care sends.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_notification::NotificationExt;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Prefs {
    /// The window's close button hides Tessera to the tray instead of quitting.
    pub close_to_tray: bool,
    /// Draw live CPU / memory bars into the tray icon.
    pub tray_meter: bool,
    /// "Still running in the tray" was shown once.
    pub tray_hint_shown: bool,
    /// The floating desktop widget is open.
    pub widget: bool,
    /// The widget stays above other windows.
    pub widget_on_top: bool,
    pub hotkeys_enabled: bool,
    /// Action → shortcut ("Ctrl+Shift+F"): fps, boost, care, widget.
    pub hotkeys: std::collections::BTreeMap<String, String>,
    /// Send the weekly summary notification.
    pub weekly: bool,
    /// Look of the in-game FPS window.
    pub overlay: OverlayCfg,
}

impl Default for Prefs {
    fn default() -> Self {
        let hotkeys = [("fps", "Ctrl+Shift+F"), ("boost", "Ctrl+Shift+B"), ("care", "Ctrl+Shift+C"), ("widget", "Ctrl+Shift+W")].iter().map(|(a, k)| (a.to_string(), k.to_string())).collect();
        Self { close_to_tray: true, tray_meter: true, tray_hint_shown: false, widget: false, widget_on_top: true, hotkeys_enabled: true, hotkeys, weekly: true, overlay: OverlayCfg::default() }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct OverlayCfg {
    /// tl, tr, bl, br
    pub corner: String,
    /// Size factor, 0.7–1.6.
    pub scale: f32,
    /// Background opacity, 0.2–1.
    pub opacity: f32,
    /// fps, low1, frametime, graph, cpu, gpu, temps, ping
    pub fields: Vec<String>,
    /// Everything on one line.
    pub compact: bool,
}

impl Default for OverlayCfg {
    fn default() -> Self {
        let fields = ["fps", "low1", "frametime", "graph", "cpu", "gpu", "ping"].iter().map(|s| s.to_string()).collect();
        Self { corner: "tl".into(), scale: 1.0, opacity: 0.9, fields, compact: false }
    }
}

/// UI strings for the tray and notifications, sent by the frontend in the current language.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Labels {
    pub open: String,
    pub care: String,
    pub quit: String,
    pub cpu: String,
    pub ram: String,
    pub hidden_title: String,
    pub hidden_body: String,
    pub care_title: String,
    /// {score} {size} {n}
    pub care_body: String,
    pub care_clean: String,
    /// {size}
    pub care_cleaned: String,
    pub widget: String,
    pub boost_title: String,
    /// {game}
    pub boost_body: String,
    pub weekly_title: String,
    /// {size} {games} {score}
    pub weekly_body: String,
    /// {game}
    pub update_title: String,
    /// {size}
    pub update_body: String,
    pub backup_title: String,
    /// {name} {size}
    pub backup_body: String,
}

impl Default for Labels {
    fn default() -> Self {
        Self {
            open: "Open Tessera".into(),
            care: "Run Care scan".into(),
            quit: "Quit".into(),
            cpu: "CPU".into(),
            ram: "RAM".into(),
            hidden_title: "Tessera is still running".into(),
            hidden_body: "It keeps working in the tray. Right-click the icon to quit.".into(),
            care_title: "Tessera Care".into(),
            care_body: "Health score {score}/100 · {size} to clean, {n} issues to fix".into(),
            care_clean: "Health score {score}/100 · nothing to clean".into(),
            care_cleaned: "Cleaned {size} automatically".into(),
            widget: "Desktop widget".into(),
            boost_title: "Game boost on".into(),
            boost_body: "{game} is running with the boost settings".into(),
            weekly_title: "Your week with Tessera".into(),
            weekly_body: "{size} freed · {games} games played · health {score}".into(),
            update_title: "{game} was updated".into(),
            update_body: "The update left {size} behind. Open the Game library to clean it.".into(),
            backup_title: "Backup done".into(),
            backup_body: "{name}: {size} of new or changed files saved".into(),
        }
    }
}

pub struct TrayItems {
    open: MenuItem<Wry>,
    care: MenuItem<Wry>,
    widget: MenuItem<Wry>,
    stats: MenuItem<Wry>,
    quit: MenuItem<Wry>,
}

fn prefs_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("prefs.json"))
}

pub fn load_prefs(app: &AppHandle) -> Prefs {
    prefs_path(app).and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save_prefs(app: &AppHandle, p: &Prefs) {
    if let Some(path) = prefs_path(app) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, serde_json::to_vec_pretty(p).unwrap_or_default());
    }
}

/// Bring the main window back from the tray (or from being minimized).
pub fn show_main(app: &AppHandle) {
    // Cancels a pending tree release
    app.state::<crate::AppState>().hide_epoch.fetch_add(1, Ordering::Relaxed);
    if let Some(w) = app.get_webview_window("main") {
        low_memory(&w, false);
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        let _ = app.emit("window-shown", ());
    }
}

pub fn build(app: &AppHandle) -> tauri::Result<TrayItems> {
    let l = Labels::default();
    let items = TrayItems {
        open: MenuItem::with_id(app, "open", &l.open, true, None::<&str>)?,
        care: MenuItem::with_id(app, "care", &l.care, true, None::<&str>)?,
        widget: MenuItem::with_id(app, "widget", &l.widget, true, None::<&str>)?,
        stats: MenuItem::with_id(app, "stats", "…", false, None::<&str>)?,
        quit: MenuItem::with_id(app, "quit", &l.quit, true, None::<&str>)?,
    };
    let menu = Menu::with_items(
        app,
        &[&items.open, &items.care, &items.widget, &PredefinedMenuItem::separator(app)?, &items.stats, &PredefinedMenuItem::separator(app)?, &items.quit],
    )?;
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Tessera")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "open" => show_main(app),
            "care" => crate::run_care_background(app, false),
            "widget" => crate::toggle_widget(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(items)
}

pub fn relabel(items: &TrayItems, l: &Labels) {
    let _ = items.open.set_text(&l.open);
    let _ = items.care.set_text(&l.care);
    let _ = items.widget.set_text(&l.widget);
    let _ = items.quit.set_text(&l.quit);
}

/// Refresh the tooltip, the stats line and (if enabled) the meter icon every two seconds.
pub fn start_meter(app: AppHandle) {
    std::thread::spawn(move || {
        let mut sys = sysinfo::System::new();
        let mut meter_was_on = false;
        loop {
            std::thread::sleep(Duration::from_secs(2));
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            let cpu = sys.global_cpu_usage().clamp(0.0, 100.0);
            let ram = sys.used_memory() as f32 / sys.total_memory().max(1) as f32 * 100.0;
            let st = app.state::<crate::AppState>();
            let (cpu_l, ram_l) = {
                let l = st.labels.lock();
                (l.cpu.clone(), l.ram.clone())
            };
            let line = format!("{cpu_l} {cpu:.0}% · {ram_l} {ram:.0}%");
            if let Some(items) = st.tray.lock().as_ref() {
                let _ = items.stats.set_text(&line);
            }
            let Some(tray) = app.tray_by_id("main") else { continue };
            let _ = tray.set_tooltip(Some(format!("Tessera · {line}")));
            let meter = st.prefs.lock().tray_meter;
            if meter {
                let _ = tray.set_icon(Some(meter_icon(cpu, ram)));
            } else if meter_was_on && let Some(icon) = app.default_window_icon() {
                let _ = tray.set_icon(Some(icon.clone()));
            }
            meter_was_on = meter;
        }
    });
}

/// 32×32 icon: two bars (CPU, memory) on a dark rounded tile; a bar turns amber above 85%.
pub fn meter_icon(cpu: f32, ram: f32) -> Image<'static> {
    const S: usize = 32;
    let mut px = vec![0u8; S * S * 4];
    let mut put = |x: usize, y: usize, c: [u8; 4]| {
        let i = (y * S + x) * 4;
        px[i..i + 4].copy_from_slice(&c);
    };
    let r = 7.0f32;
    for y in 0..S {
        for x in 0..S {
            // Rounded-square tile with a 1px soft edge
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let dx = (r - fx).max(fx - (S as f32 - r)).max(0.0);
            let dy = (r - fy).max(fy - (S as f32 - r)).max(0.0);
            let d = (dx * dx + dy * dy).sqrt();
            let a = (r - d).clamp(0.0, 1.0);
            if a > 0.0 {
                put(x, y, [32, 28, 52, (a * 255.0) as u8]);
            }
        }
    }
    let bar = |put: &mut dyn FnMut(usize, usize, [u8; 4]), x0: usize, pct: f32, color: [u8; 4]| {
        let (top, bottom) = (5usize, 27usize);
        let h = bottom - top;
        let filled = ((pct / 100.0) * h as f32).round().clamp(1.0, h as f32) as usize;
        for y in top..bottom {
            for x in x0..x0 + 8 {
                let on = y >= bottom - filled;
                put(x, y, if on { color } else { [255, 255, 255, 38] });
            }
        }
    };
    let warn = [251, 191, 36, 255];
    bar(&mut put, 6, cpu, if cpu > 85.0 { warn } else { [167, 139, 250, 255] });
    bar(&mut put, 18, ram, if ram > 85.0 { warn } else { [103, 232, 249, 255] });
    Image::new_owned(px, S as u32, S as u32)
}

pub fn notify(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        eprintln!("notification failed: {e}");
    }
}

/// While hidden, ask WebView2 to keep its memory low: it drops caches and lets Windows page out
/// the renderer. Back to normal when the window is shown again.
fn low_memory(w: &tauri::WebviewWindow, low: bool) {
    #[cfg(windows)]
    {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
        };
        use windows_core::Interface;
        let level = if low { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW } else { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL };
        let _ = w.with_webview(move |wv| {
            // SAFETY: plain COM calls on the window's own controller, on the thread that owns it.
            // Older WebView2 runtimes don't have ICoreWebView2_19; then the cast fails and nothing happens.
            unsafe {
                if let Ok(core) = wv.controller().CoreWebView2()
                    && let Ok(core) = core.cast::<ICoreWebView2_19>()
                {
                    let _ = core.SetMemoryUsageTargetLevel(level);
                }
            }
        });
    }
    #[cfg(not(windows))]
    let _ = (w, low);
}

/// Window close: hide to the tray when that's the setting; tell the user the first time.
pub fn on_close(app: &AppHandle) -> bool {
    let st = app.state::<crate::AppState>();
    let mut prefs = st.prefs.lock().clone();
    if !prefs.close_to_tray || app.tray_by_id("main").is_none() {
        return false;
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
        low_memory(&w, true);
    }
    crate::release_when_hidden(app);
    if !prefs.tray_hint_shown {
        prefs.tray_hint_shown = true;
        *st.prefs.lock() = prefs.clone();
        save_prefs(app, &prefs);
        let l = st.labels.lock().clone();
        notify(app, &l.hidden_title, &l.hidden_body);
    }
    true
}

/// Tells the page which small window it is (overlay, widget, prompt) and, for the prompt, the game.
/// A query string can't be used: in a packaged build `index.html?window=…` is looked up as a file
/// name and gives a 404.
pub fn small_window_script(kind: &str, game: &str) -> String {
    format!("window.__TESSERA_WINDOW = {};", serde_json::json!({ "window": kind, "game": game }))
}

/// Top-left position (logical) for a `w`×`h` window in a corner (tl, tr, bl, br) of a monitor.
pub fn corner_pos(m: &tauri::Monitor, corner: &str, w: f64, h: f64) -> (f64, f64) {
    let scale = m.scale_factor();
    let size = m.size().to_logical::<f64>(scale);
    let pos = m.position().to_logical::<f64>(scale);
    let x = if corner.ends_with('r') { pos.x + size.width - w - 16.0 } else { pos.x + 16.0 };
    // Bottom corners stay clear of the taskbar
    let y = if corner.starts_with('b') { pos.y + size.height - h - 64.0 } else { pos.y + 16.0 };
    (x, y)
}

/// Move an open small window to its corner after it changed size.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn fit_small_window(app: &AppHandle, label: &str, corner: &str, w: f64, h: f64) {
    let Some(win) = app.get_webview_window(label) else { return };
    let _ = win.set_size(tauri::LogicalSize::new(w, h));
    if let Ok(Some(m)) = win.current_monitor().or_else(|_| app.primary_monitor()) {
        let (x, y) = corner_pos(&m, corner, w, h);
        let _ = win.set_position(tauri::LogicalPosition::new(x, y));
    }
}

/// A small frameless window (FPS window, desktop widget) in a corner (tl, tr, bl, br) of the
/// primary screen. Opening an open window or closing a closed one does nothing.
pub fn small_window(app: &AppHandle, label: &str, on: bool, w: f64, h: f64, corner: &str, on_top: bool) {
    if let Some(win) = app.get_webview_window(label) {
        if on {
            let _ = win.set_always_on_top(on_top);
        } else {
            let _ = win.close();
        }
        return;
    }
    if !on {
        return;
    }
    // Building a webview window on the event-loop thread (hotkey and tray callbacks, sync commands)
    // deadlocks on Windows, so it is always built from a thread of its own
    let app = app.clone();
    let label = label.to_string();
    let corner = corner.to_string();
    std::thread::spawn(move || {
        let mut b = tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::App("index.html".into()))
            .initialization_script(small_window_script(&label, ""))
            .title("Tessera")
            .inner_size(w, h)
            .decorations(false)
            .resizable(false)
            .always_on_top(on_top)
            .skip_taskbar(true)
            .focused(false)
            .shadow(false);
        // Transparent windows need Tauri's private-API feature on macOS; there the card is drawn on
        // an opaque window instead
        #[cfg(not(target_os = "macos"))]
        {
            b = b.transparent(true);
        }
        if let Ok(Some(m)) = app.primary_monitor() {
            let (x, y) = corner_pos(&m, &corner, w, h);
            b = b.position(x, y);
        }
        if let Err(e) = b.build() {
            eprintln!("{label} window: {e}");
        }
    });
}

/// Set while a Care scan runs in the background, so tray clicks and schedules don't stack up.
pub static CARE_RUNNING: AtomicBool = AtomicBool::new(false);

pub fn care_running() -> bool {
    CARE_RUNNING.load(Ordering::Relaxed)
}
