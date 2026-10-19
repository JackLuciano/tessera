mod cache;
mod care;
mod delete;
mod diskhealth;
mod dupes;
mod history;
#[cfg(windows)]
mod internet;
mod junk;
#[cfg(windows)]
mod mft;
#[cfg(windows)]
mod optimizer;
mod privacy;
#[cfg(windows)]
mod programs;
#[cfg(windows)]
mod registry;
mod scanner;
mod shortcuts;
mod shred;
mod sysutil;
mod tray;
mod watch;
#[cfg(windows)]
mod winget;

use parking_lot::Mutex;
use scanner::{Cat, Kind, Progress, Tree, CAT_COUNT, NO_PARENT};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

pub(crate) struct AppState {
    tree: Mutex<Option<Tree>>,
    progress: Mutex<Option<Arc<Progress>>>,
    dup_progress: Mutex<Option<Arc<dupes::DupProgress>>>,
    watch: Mutex<Option<watch::LiveWatch>>,
    live: AtomicBool,
    /// Bumped whenever the tree is replaced, so background work started on an older tree can tell.
    generation: AtomicU64,
    /// Kept between calls: CPU usage is measured as the difference between two refreshes.
    sys: Mutex<sysinfo::System>,
    delete_cancel: AtomicBool,
    pub(crate) prefs: Mutex<tray::Prefs>,
    pub(crate) labels: Mutex<tray::Labels>,
    pub(crate) tray: Mutex<Option<tray::TrayItems>>,
    /// Started by a scheduled Care run (`--care`): the window stays hidden until opened.
    hidden_start: AtomicBool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            tree: Mutex::default(),
            progress: Mutex::default(),
            dup_progress: Mutex::default(),
            watch: Mutex::default(),
            live: AtomicBool::new(true),
            generation: AtomicU64::new(0),
            sys: Mutex::new(sysinfo::System::new()),
            delete_cancel: AtomicBool::new(false),
            prefs: Mutex::default(),
            labels: Mutex::default(),
            tray: Mutex::default(),
            hidden_start: AtomicBool::new(false),
        }
    }
}

#[derive(Serialize, Clone)]
struct LiveEvent {
    folders: usize,
    delta: i64,
    size: u64,
    files: u64,
}

/// Watch the scanned root and fold changes into the tree as they happen.
fn start_watch(app: &AppHandle, state: &AppState) {
    *state.watch.lock() = None;
    if !state.live.load(Ordering::Relaxed) {
        return;
    }
    let Some(root) = state.tree.lock().as_ref().map(|t| t.root_path.clone()) else { return };
    let handle = app.clone();
    let res = watch::start(&root, move |dirs| {
        let st = handle.state::<AppState>();
        let ev = {
            let mut g = st.tree.lock();
            let Some(t) = g.as_mut() else { return };
            let mut ids: Vec<u32> = dirs.iter().map(|d| t.find_dir(d)).collect();
            ids.sort_unstable();
            ids.dedup();
            let delta: i64 = ids.iter().map(|id| t.refresh_dir(*id)).sum();
            LiveEvent { folders: ids.len(), delta, size: t.nodes[0].size, files: t.nodes[0].files }
        };
        let _ = handle.emit("live-change", ev);
    });
    match res {
        Ok(w) => *state.watch.lock() = Some(w),
        Err(e) => eprintln!("live watch unavailable for {}: {e}", root.display()),
    }
}

/// Turn live change tracking on or off (Settings).
#[tauri::command]
fn set_live_watch(enabled: bool, app: AppHandle, state: State<'_, AppState>) {
    state.live.store(enabled, Ordering::Relaxed);
    if enabled {
        if state.watch.lock().is_none() {
            start_watch(&app, &state);
        }
    } else {
        *state.watch.lock() = None;
    }
}

/// Current totals, for views to refresh after live changes.
#[tauri::command]
fn get_summary(state: State<'_, AppState>) -> Result<ScanSummary, String> {
    let g = state.tree.lock();
    let t = g.as_ref().ok_or("no scan yet")?;
    Ok(summary(t, "live"))
}

#[derive(Serialize)]
struct DriveInfo {
    name: String,
    mount: String,
    fs: String,
    total: u64,
    available: u64,
    removable: bool,
    kind: String,
}

#[tauri::command]
fn list_drives() -> Vec<DriveInfo> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut out: Vec<DriveInfo> = disks
        .list()
        .iter()
        .filter(|d| d.total_space() > 0)
        .map(|d| DriveInfo {
            name: d.name().to_string_lossy().into_owned(),
            mount: d.mount_point().to_string_lossy().into_owned(),
            fs: d.file_system().to_string_lossy().into_owned(),
            total: d.total_space(),
            available: d.available_space(),
            removable: d.is_removable(),
            kind: format!("{:?}", d.kind()),
        })
        .collect();
    out.sort_by(|a, b| a.mount.cmp(&b.mount));
    out.dedup_by(|a, b| a.mount == b.mount);
    out
}

#[derive(Serialize, Clone)]
struct ProgressEvent {
    files: u64,
    dirs: u64,
    bytes: u64,
}

#[derive(Serialize)]
struct ScanSummary {
    engine: &'static str,
    root: String,
    size: u64,
    files: u64,
    dirs: u64,
    elapsed_ms: u64,
    errors: u64,
    cats: Vec<CatTotal>,
}

#[derive(Serialize)]
struct CatTotal {
    cat: Cat,
    size: u64,
}

fn summary(t: &Tree, engine: &'static str) -> ScanSummary {
    let r = &t.nodes[0];
    let mut cats: Vec<CatTotal> = (0..CAT_COUNT)
        .map(|i| CatTotal { cat: Cat::ALL[i], size: r.cats[i] })
        .filter(|c| c.size > 0)
        .collect();
    cats.sort_by_key(|c| std::cmp::Reverse(c.size));
    ScanSummary {
        engine,
        root: t.root_path.to_string_lossy().into_owned(),
        size: r.size,
        files: r.files,
        dirs: r.dirs,
        elapsed_ms: t.elapsed_ms,
        errors: t.errors,
        cats,
    }
}

#[tauri::command]
async fn scan(path: String, app: AppHandle, state: State<'_, AppState>) -> Result<ScanSummary, String> {
    let root = PathBuf::from(&path);
    if !root.exists() {
        return Err(format!("{path} does not exist"));
    }
    // The old watcher would keep refreshing a tree that is about to be replaced
    *state.watch.lock() = None;
    let progress = Progress::new();
    *state.progress.lock() = Some(progress.clone());

    // Emit progress ~10× per second until the scan finishes
    let ticker = {
        let p = progress.clone();
        let app = app.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(100));
            let ev = ProgressEvent {
                files: p.files.load(Ordering::Relaxed),
                dirs: p.dirs.load(Ordering::Relaxed),
                bytes: p.bytes.load(Ordering::Relaxed),
            };
            let _ = app.emit("scan-progress", ev);
            if p.cancel.load(Ordering::Relaxed) {
                break;
            }
        })
    };

    // Journal position before reading anything, so the next start can catch up on changes from here on
    let journal = journal_position(&root);
    let started_at = cache::now();

    let p = progress.clone();
    let (tree, engine) = tauri::async_runtime::spawn_blocking(move || {
        // Whole-volume scans on NTFS try the MFT fast path first (needs admin),
        // and fall back to the parallel directory walk on any error.
        #[cfg(windows)]
        if mft::is_volume_root(&root) {
            match mft::scan(&root, &p) {
                Ok(t) => return (t, "mft"),
                Err(e) => eprintln!("MFT fast path unavailable: {e}"),
            }
        }
        (scanner::scan(&root, &p), "walk")
    })
    .await
    .map_err(|e| e.to_string())?;
    let cancelled = progress.cancel.load(Ordering::Relaxed);
    progress.cancel.store(true, Ordering::Relaxed);
    let _ = ticker.join();
    *state.progress.lock() = None;
    if cancelled {
        return Err("cancelled".into());
    }
    let s = summary(&tree, engine);
    if let Ok(base) = app.path().app_data_dir()
        && let Err(e) = history::save(&base, &tree)
    {
        eprintln!("could not save history snapshot: {e}");
    }
    *state.tree.lock() = Some(tree);
    state.generation.fetch_add(1, Ordering::Relaxed);
    start_watch(&app, &state);
    save_cache(&app, cache::Meta { saved_at: started_at, journal });
    Ok(s)
}

/// (journal id, next USN) of the volume holding `root`; NTFS on Windows with admin rights only.
fn journal_position(root: &std::path::Path) -> Option<(u64, i64)> {
    #[cfg(windows)]
    return cache::usn::position(root);
    #[cfg(not(windows))]
    {
        let _ = root;
        None
    }
}

/// Folders changed since a journal position, or `None` when the journal can't answer that.
fn journal_changes(root: &std::path::Path, journal: u64, from: i64) -> Option<Vec<PathBuf>> {
    #[cfg(windows)]
    return cache::usn::changed_dirs(root, journal, from);
    #[cfg(not(windows))]
    {
        let _ = (root, journal, from);
        None
    }
}

/// Write the current tree to the scan cache in the background (copy under the lock, I/O without it).
fn save_cache(app: &AppHandle, meta: cache::Meta) {
    let app = app.clone();
    std::thread::spawn(move || {
        let Ok(base) = app.path().app_data_dir() else { return };
        let (root, raw) = {
            let st = app.state::<AppState>();
            let g = st.tree.lock();
            let Some(t) = g.as_ref() else { return };
            (t.root_path.to_string_lossy().into_owned(), cache::encode(t, &meta))
        };
        if let Err(e) = cache::write(&base, &root, &raw) {
            eprintln!("could not save the scan cache: {e}");
        }
    });
}

#[derive(Serialize)]
struct CachedSummary {
    #[serde(flatten)]
    summary: ScanSummary,
    cached_at: u64,
}

/// Open the saved scan of `root` instantly, then catch up on changes in the background
/// (`catchup-done` event). Returns `None` when there is no usable saved scan.
#[tauri::command]
async fn load_cached(root: String, app: AppHandle, state: State<'_, AppState>) -> Result<Option<CachedSummary>, String> {
    let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let r = root.clone();
    let loaded = tauri::async_runtime::spawn_blocking(move || cache::load(&base, &r)).await.map_err(|e| e.to_string())?;
    let Ok((tree, meta)) = loaded else { return Ok(None) };
    if !tree.root_path.exists() {
        return Ok(None);
    }
    let s = CachedSummary { summary: summary(&tree, "cache"), cached_at: meta.saved_at };
    *state.tree.lock() = Some(tree);
    let generation = state.generation.fetch_add(1, Ordering::Relaxed) + 1;

    let handle = app.clone();
    std::thread::spawn(move || {
        let st = handle.state::<AppState>();
        let root = PathBuf::from(&root);
        let journal_now = journal_position(&root);
        let started_at = cache::now();

        // Prefer the USN journal (exact, cheap); otherwise stat every folder against the save time
        let from_journal = meta.journal.and_then(|(id, usn)| journal_changes(&root, id, usn));
        let ids: Vec<u32> = match from_journal {
            Some(paths) => {
                let g = st.tree.lock();
                let Some(t) = g.as_ref() else { return };
                paths.iter().filter(|p| p.starts_with(&t.root_path)).map(|p| t.find_dir(p)).collect()
            }
            None => {
                let dirs = {
                    let g = st.tree.lock();
                    let Some(t) = g.as_ref() else { return };
                    cache::dir_paths(t)
                };
                cache::changed_by_mtime(dirs, meta.saved_at)
            }
        };
        let ev = {
            let mut g = st.tree.lock();
            let Some(t) = g.as_mut() else { return };
            // A rescan replaced the tree meanwhile: these ids belong to the old one
            if st.generation.load(Ordering::Relaxed) != generation {
                return;
            }
            let mut ids = ids;
            ids.sort_unstable();
            ids.dedup();
            let delta: i64 = ids.iter().map(|id| t.refresh_dir(*id)).sum();
            LiveEvent { folders: ids.len(), delta, size: t.nodes[0].size, files: t.nodes[0].files }
        };
        let _ = handle.emit("catchup-done", &ev);
        start_watch(&handle, &st);
        // Save the caught-up tree so the next start has less to do
        if ev.folders > 0 {
            save_cache(&handle, cache::Meta { saved_at: started_at, journal: journal_now });
        }
    });
    Ok(Some(s))
}

#[tauri::command]
fn is_elevated() -> bool {
    #[cfg(windows)]
    {
        mft::is_elevated()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Restart Tessera as administrator so the MFT fast path can be used.
#[tauri::command]
fn relaunch_as_admin(app: AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &format!("Start-Process -FilePath '{}' -Verb RunAs", exe.display())])
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            app.exit(0);
            return Ok(());
        }
        Err("elevation was cancelled".into())
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("only supported on Windows".into())
    }
}

#[derive(Serialize, Clone)]
struct DupProgressEvent {
    hashed: u64,
    total: u64,
}

#[tauri::command]
async fn find_duplicates(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<dupes::DupGroup>, String> {
    let cands = {
        let g = state.tree.lock();
        dupes::candidates(g.as_ref().ok_or("no scan yet")?)
    };
    let progress = Arc::new(dupes::DupProgress { hashed_bytes: AtomicU64::new(0), total_bytes: AtomicU64::new(0), cancel: AtomicBool::new(false) });
    *state.dup_progress.lock() = Some(progress.clone());
    let done = Arc::new(AtomicBool::new(false));
    let ticker = {
        let (p, d, app) = (progress.clone(), done.clone(), app.clone());
        std::thread::spawn(move || {
            while !d.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(150));
                let _ = app.emit("dup-progress", DupProgressEvent { hashed: p.hashed_bytes.load(Ordering::Relaxed), total: p.total_bytes.load(Ordering::Relaxed) });
            }
        })
    };
    let p = progress.clone();
    let res = tauri::async_runtime::spawn_blocking(move || dupes::find(cands, &p)).await.map_err(|e| e.to_string());
    done.store(true, Ordering::Relaxed);
    let _ = ticker.join();
    *state.dup_progress.lock() = None;
    if progress.cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    res
}

/// Replace duplicate copies with hard links to the kept file: every path keeps working, the space
/// is freed. `pairs` are (keep, duplicate) node ids. Needs both files on the same volume.
#[tauri::command]
async fn hardlink_dupes(pairs: Vec<(u32, u32)>, state: State<'_, AppState>) -> Result<DeleteResult, String> {
    let jobs: Vec<(PathBuf, PathBuf, u64)> = {
        let g = state.tree.lock();
        let t = g.as_ref().ok_or("no scan yet")?;
        pairs
            .iter()
            .filter(|(k, d)| (*k as usize) < t.nodes.len() && (*d as usize) < t.nodes.len() && k != d)
            .map(|(k, d)| (t.path_of(*k), t.path_of(*d), t.nodes[*d as usize].size))
            .collect()
    };
    tauri::async_runtime::spawn_blocking(move || {
        let (mut freed, mut deleted, mut failed) = (0, 0, Vec::new());
        for (keep, dup, size) in jobs {
            if is_protected(&dup) {
                failed.push(format!("{}: protected system location", dup.display()));
                continue;
            }
            match dupes::hardlink(&keep, &dup) {
                Ok(()) => {
                    freed += size;
                    deleted += 1;
                }
                Err(e) => failed.push(format!("{}: {e}", dup.display())),
            }
        }
        DeleteResult { freed, deleted, failed, cancelled: false }
    })
    .await
    .map_err(|e| e.to_string())
}

// ---------- optimizer (Windows only; other platforms report `supported: false`) ----------

#[derive(Serialize)]
// Only filled on Windows; elsewhere the optimizer reports `supported: false` with an empty list
#[cfg_attr(not(windows), allow(dead_code))]
struct TweakState {
    key: &'static str,
    on: bool,
    admin: bool,
    reboot: bool,
}

#[derive(Serialize)]
struct Memory {
    total: u64,
    used: u64,
}

#[derive(Serialize)]
struct OptStatus {
    supported: bool,
    elevated: bool,
    active: Option<String>,
    #[cfg(windows)]
    power_plans: Vec<optimizer::PowerPlan>,
    #[cfg(not(windows))]
    power_plans: Vec<()>,
    tweaks: Vec<TweakState>,
    has_backup: bool,
    memory: Memory,
    cpu: f32,
}

#[tauri::command]
async fn opt_status(app: AppHandle, state: State<'_, AppState>) -> Result<OptStatus, String> {
    let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let (memory, cpu) = {
        let mut sys = state.sys.lock();
        sys.refresh_memory();
        sys.refresh_cpu_usage();
        (Memory { total: sys.total_memory(), used: sys.used_memory() }, sys.global_cpu_usage())
    };
    #[cfg(windows)]
    {
        let (plans, tweaks) = tauri::async_runtime::spawn_blocking(|| {
            let tweaks = optimizer::TWEAKS
                .iter()
                .map(|t| TweakState { key: t.key, on: optimizer::tweak_on(t), admin: t.machine(), reboot: t.reboot })
                .collect::<Vec<_>>();
            (optimizer::power_plans(), tweaks)
        })
        .await
        .map_err(|e| e.to_string())?;
        Ok(OptStatus {
            supported: true,
            elevated: mft::is_elevated(),
            active: optimizer::active_profile(&base),
            power_plans: plans,
            tweaks,
            has_backup: optimizer::has_backup(&base),
            memory,
            cpu,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = base;
        Ok(OptStatus { supported: false, elevated: false, active: None, power_plans: vec![], tweaks: vec![], has_backup: false, memory, cpu })
    }
}

#[derive(Serialize)]
struct SysStats {
    memory: Memory,
    cpu: f32,
}

/// CPU and memory only, cheap enough to poll every couple of seconds.
#[tauri::command]
fn sys_stats(state: State<'_, AppState>) -> SysStats {
    let mut sys = state.sys.lock();
    sys.refresh_memory();
    sys.refresh_cpu_usage();
    SysStats { memory: Memory { total: sys.total_memory(), used: sys.used_memory() }, cpu: sys.global_cpu_usage() }
}

#[cfg(windows)]
type OptResult = Result<optimizer::Outcome, String>;
#[cfg(not(windows))]
type OptResult = Result<(), String>;

#[tauri::command]
async fn opt_apply_profile(profile: String, app: AppHandle) -> OptResult {
    #[cfg(windows)]
    {
        let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
        tauri::async_runtime::spawn_blocking(move || {
            restore_point_for(&app, "Tessera optimizer profile");
            optimizer::apply_profile(&base, &profile, mft::is_elevated())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = (profile, app);
        Err("the optimizer is only available on Windows".into())
    }
}

#[tauri::command]
async fn opt_set_tweak(key: String, on: bool, app: AppHandle) -> OptResult {
    #[cfg(windows)]
    {
        let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
        tauri::async_runtime::spawn_blocking(move || {
            restore_point_for(&app, "Tessera optimizer tweak");
            optimizer::set_tweak(&base, &key, on)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = (key, on, app);
        Err("the optimizer is only available on Windows".into())
    }
}

#[tauri::command]
async fn opt_set_power_plan(guid: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || optimizer::set_power_plan(&guid)).await.map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = guid;
        Err("the optimizer is only available on Windows".into())
    }
}

#[tauri::command]
async fn opt_restore(app: AppHandle) -> OptResult {
    #[cfg(windows)]
    {
        let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
        tauri::async_runtime::spawn_blocking(move || optimizer::restore(&base)).await.map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("the optimizer is only available on Windows".into())
    }
}

#[tauri::command]
#[cfg(windows)]
fn opt_startup_list() -> Vec<optimizer::StartupApp> {
    optimizer::startup_list()
}
#[tauri::command]
#[cfg(not(windows))]
fn opt_startup_list() -> Vec<()> {
    Vec::new()
}

#[tauri::command]
fn opt_startup_set(id: String, enabled: bool, app: AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
        optimizer::set_startup_tracked(&base, &id, enabled)
    }
    #[cfg(not(windows))]
    {
        let _ = (id, enabled, app);
        Err("the optimizer is only available on Windows".into())
    }
}

#[tauri::command]
#[cfg(windows)]
async fn opt_processes(state: State<'_, AppState>) -> Result<Vec<optimizer::AppProcess>, String> {
    Ok(optimizer::processes(&mut state.sys.lock()))
}
#[tauri::command]
#[cfg(not(windows))]
async fn opt_processes() -> Result<Vec<()>, String> {
    Ok(Vec::new())
}

#[tauri::command]
fn opt_kill(pids: Vec<u32>, state: State<'_, AppState>) -> OptResult {
    #[cfg(windows)]
    {
        Ok(optimizer::kill(&mut state.sys.lock(), &pids))
    }
    #[cfg(not(windows))]
    {
        let _ = (pids, state);
        Err("the optimizer is only available on Windows".into())
    }
}

#[tauri::command]
fn cancel_duplicates(state: State<'_, AppState>) {
    if let Some(p) = state.dup_progress.lock().as_ref() {
        p.cancel.store(true, Ordering::Relaxed);
    }
}

#[tauri::command]
fn history_list(app: AppHandle, root: String) -> Result<Vec<history::SnapshotInfo>, String> {
    let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(history::list(&base, &root))
}

#[tauri::command]
fn history_diff(ts: u64, app: AppHandle, state: State<'_, AppState>) -> Result<Vec<history::DiffRow>, String> {
    let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let g = state.tree.lock();
    let t = g.as_ref().ok_or("no scan yet")?;
    history::diff(&base, t, ts).ok_or_else(|| "snapshot not found".into())
}

#[tauri::command]
fn cancel_scan(state: State<'_, AppState>) {
    if let Some(p) = state.progress.lock().as_ref() {
        p.cancel.store(true, Ordering::Relaxed);
    }
}

#[derive(Serialize)]
struct NodeDto {
    id: u32,
    name: String,
    path: String,
    size: u64,
    files: u64,
    dirs: u64,
    kind: Kind,
    cat: Cat,
    modified: u64,
    child_count: usize,
    children: Vec<NodeDto>,
}

/// `min_size` prunes grandchildren and deeper levels that would be too small to see in the treemap;
/// direct children are always listed (up to `max_children`) for the side panel.
fn dto(t: &Tree, id: u32, depth: u32, max_children: usize, min_size: u64, top: bool) -> NodeDto {
    let n = &t.nodes[id as usize];
    let floor = if top { 1 } else { min_size.max(1) };
    let children = if depth > 0 {
        n.children
            .iter()
            .filter(|c| t.nodes[**c as usize].size >= floor)
            .take(max_children)
            .map(|c| dto(t, *c, depth - 1, max_children, min_size, false))
            .collect()
    } else {
        Vec::new()
    };
    NodeDto {
        id,
        name: if n.parent == NO_PARENT && id == 0 { t.root_path.to_string_lossy().into_owned() } else { n.name.clone() },
        path: t.path_of(id).to_string_lossy().into_owned(),
        size: n.size,
        files: n.files,
        dirs: n.dirs,
        kind: n.kind,
        cat: t.dominant_cat(id),
        modified: n.modified,
        child_count: n.children.len(),
        children,
    }
}

/// A node with `depth` levels of children (largest first, capped per level).
#[tauri::command]
fn get_node(id: u32, depth: u32, max_children: usize, min_frac: Option<f64>, state: State<'_, AppState>) -> Result<NodeDto, String> {
    let g = state.tree.lock();
    let t = g.as_ref().ok_or("no scan yet")?;
    if id as usize >= t.nodes.len() {
        return Err("unknown node".into());
    }
    // Without a size floor deep requests could serialize the whole drive, so cap depth harder then
    let (max_depth, min_size) = match min_frac {
        Some(f) if f > 0.0 => (6, (t.nodes[id as usize].size as f64 * f.min(1.0)) as u64),
        _ => (3, 0),
    };
    Ok(dto(t, id, depth.min(max_depth), max_children.clamp(1, 400), min_size, true))
}

/// Ancestor chain (root first) for breadcrumbs.
#[tauri::command]
fn get_ancestors(id: u32, state: State<'_, AppState>) -> Result<Vec<(u32, String)>, String> {
    let g = state.tree.lock();
    let t = g.as_ref().ok_or("no scan yet")?;
    let mut out = Vec::new();
    let mut cur = id;
    while cur != NO_PARENT && (cur as usize) < t.nodes.len() {
        let n = &t.nodes[cur as usize];
        out.push((cur, if cur == 0 { t.root_path.to_string_lossy().into_owned() } else { n.name.clone() }));
        cur = n.parent;
    }
    out.reverse();
    Ok(out)
}

#[derive(Serialize)]
struct FileDto {
    id: u32,
    name: String,
    path: String,
    size: u64,
    cat: Cat,
    modified: u64,
}

#[tauri::command]
fn largest_files(limit: usize, state: State<'_, AppState>) -> Result<Vec<FileDto>, String> {
    let g = state.tree.lock();
    let t = g.as_ref().ok_or("no scan yet")?;
    let mut ids: Vec<u32> = (0..t.nodes.len() as u32)
        .filter(|i| { let n = &t.nodes[*i as usize]; n.kind == Kind::File && n.size > 0 })
        .collect();
    ids.sort_unstable_by(|a, b| t.nodes[*b as usize].size.cmp(&t.nodes[*a as usize].size));
    ids.truncate(limit.min(2000));
    Ok(ids
        .into_iter()
        .map(|id| {
            let n = &t.nodes[id as usize];
            FileDto { id, name: n.name.clone(), path: t.path_of(id).to_string_lossy().into_owned(), size: n.size, cat: t.dominant_cat(id), modified: n.modified }
        })
        .collect())
}

#[derive(Serialize)]
struct JunkGroup {
    key: String,
    label: String,
    hint: String,
    safe: bool,
    size: u64,
    ids: Vec<u32>,
    sample: Vec<String>,
}

/// Find well-known regenerable folders (dependency dirs, build caches, temp dirs).
#[tauri::command]
fn find_junk(state: State<'_, AppState>) -> Result<Vec<JunkGroup>, String> {
    let g = state.tree.lock();
    let t = g.as_ref().ok_or("no scan yet")?;
    // (key, label, hint, safe, matcher on lowercase dir name + lowercase parent path)
    type M = fn(&str, &str) -> bool;
    let rules: [(&str, &str, &str, bool, M); 9] = [
        ("node_modules", "node_modules folders", "Reinstall with npm/pnpm install", true, |n, _| n == "node_modules"),
        ("next", "Next.js / Nuxt build caches", ".next, .nuxt, .svelte-kit — rebuilt on next build", true, |n, _| n == ".next" || n == ".nuxt" || n == ".svelte-kit"),
        ("pycache", "Python caches", "__pycache__ and .pytest_cache", true, |n, _| n == "__pycache__" || n == ".pytest_cache" || n == ".mypy_cache"),
        ("cargo_target", "Rust target folders", "Rebuilt by cargo build", true, |n, p| n == "target" && p.len() > 3),
        ("gradle", "Gradle caches", "Rebuilt by Gradle", true, |n, _| n == ".gradle"),
        ("temp", "Temp folders", "Temporary files left behind by apps", true, |n, p| n == "temp" && (p.ends_with("\\appdata\\local") || p.ends_with("\\windows")) || n == "tmp" && p.len() <= 4),
        ("pkg_cache", "Package manager caches", "npm, pnpm, pip, NuGet, yarn — re-downloaded on demand", true,
            |n, p| n == "npm-cache" || (matches!(n, "pnpm" | "pip" | "yarn") && p.ends_with("\\appdata\\local")) || (n == "packages" && p.ends_with("\\.nuget"))),
        ("browser", "Browser caches", "Chrome / Edge / Firefox cache, logins are not touched", true, |n, p| (n == "cache" || n == "code cache" || n == "cache2") && (p.contains("chrome") || p.contains("edge") || p.contains("firefox") || p.contains("brave"))),
        ("wu", "Windows Update downloads", "Already-installed update packages", false, |n, p| n == "download" && p.ends_with("\\softwaredistribution")),
    ];
    let mut groups: Vec<JunkGroup> = rules
        .iter()
        .map(|(k, l, h, s, _)| JunkGroup { key: k.to_string(), label: l.to_string(), hint: h.to_string(), safe: *s, size: 0, ids: vec![], sample: vec![] })
        .collect();

    // Walk top-down; once a folder matches, its subtree is not searched again.
    let mut stack: Vec<u32> = vec![0];
    while let Some(id) = stack.pop() {
        let n = &t.nodes[id as usize];
        if n.kind != Kind::Dir || n.size == 0 {
            continue;
        }
        let name = n.name.to_lowercase();
        let parent_path = if n.parent == NO_PARENT { String::new() } else { t.path_of(n.parent).to_string_lossy().to_lowercase() };
        if id != 0
            && let Some(i) = rules.iter().position(|r| (r.4)(&name, &parent_path))
        {
            let g = &mut groups[i];
            g.size += n.size;
            g.ids.push(id);
            if g.sample.len() < 4 {
                g.sample.push(t.path_of(id).to_string_lossy().into_owned());
            }
            continue;
        }
        stack.extend(n.children.iter().copied());
    }
    groups.retain(|g| g.size > 0);
    groups.sort_by_key(|g| std::cmp::Reverse(g.size));
    Ok(groups)
}

#[derive(Serialize)]
struct DeleteResult {
    freed: u64,
    deleted: usize,
    failed: Vec<String>,
    cancelled: bool,
}

/// Delete nodes by id. `permanent` skips the Recycle Bin / Trash; `shred` overwrites files first.
#[tauri::command]
async fn delete_nodes(ids: Vec<u32>, permanent: bool, shred: Option<bool>, app: AppHandle, state: State<'_, AppState>) -> Result<DeleteResult, String> {
    let shred = shred.unwrap_or(false);
    let (targets, generation): (Vec<(u32, PathBuf, Kind, u64)>, u64) = {
        let g = state.tree.lock();
        let t = g.as_ref().ok_or("no scan yet")?;
        let targets = ids
            .iter()
            .filter(|id| (**id as usize) < t.nodes.len() && **id != 0)
            .map(|id| { let n = &t.nodes[*id as usize]; (*id, t.path_of(*id), n.kind, n.size) })
            .collect();
        (targets, state.generation.load(Ordering::Relaxed))
    };
    state.delete_cancel.store(false, Ordering::Relaxed);

    // Deleting thousands of folders takes a while: run off the async runtime and report progress
    let handle = app.clone();
    let (ok, failed, freed, cancelled) = tauri::async_runtime::spawn_blocking(move || {
        let st = handle.state::<AppState>();
        let mut failed = Vec::new();
        let mut items: Vec<(u32, PathBuf, bool, u64)> = Vec::new();
        for (id, path, kind, size) in targets {
            if kind == Kind::Bucket {
                failed.push(format!("{}: small-file groups can't be deleted as one item", path.display()));
            } else if is_protected(&path) {
                failed.push(format!("{}: protected system location", path.display()));
            } else {
                items.push((id, path, kind == Kind::Dir, size));
            }
        }
        let total = items.len();
        let total_bytes: u64 = items.iter().map(|i| i.3).sum();
        let progress = Arc::new(delete::Progress::new());

        // ~8 updates a second, plus the cancel flag mirrored into the deleter
        let finished = Arc::new(AtomicBool::new(false));
        let ticker = {
            let (p, fin, h) = (progress.clone(), finished.clone(), handle.clone());
            std::thread::spawn(move || {
                while !fin.load(Ordering::Relaxed) {
                    let st = h.state::<AppState>();
                    if st.delete_cancel.load(Ordering::Relaxed) {
                        p.cancel.store(true, Ordering::Relaxed);
                    }
                    let _ = h.emit("delete-progress", DeleteProgress {
                        done: p.done.load(Ordering::Relaxed),
                        total,
                        bytes: p.bytes.load(Ordering::Relaxed).min(total_bytes),
                        total_bytes,
                    });
                    std::thread::sleep(Duration::from_millis(120));
                }
            })
        };

        let jobs: Vec<(PathBuf, bool, u64)> = items.iter().map(|i| (i.1.clone(), i.2, i.3)).collect();
        let outcome = if shred {
            let paths: Vec<PathBuf> = jobs.iter().map(|j| j.0.clone()).collect();
            shred::shred_all(&paths, 1, &progress).into_iter().map(|r| r.map(|_| ())).collect()
        } else if permanent {
            delete::permanent_all(&jobs, &progress)
        } else {
            delete::trash_all(&jobs, &progress)
        };
        finished.store(true, Ordering::Relaxed);
        let _ = ticker.join();

        let mut ok = Vec::new();
        let mut freed = 0u64;
        let mut cancelled = false;
        for ((id, path, _, size), r) in items.iter().zip(outcome) {
            match r {
                Ok(()) => {
                    ok.push(*id);
                    freed += size;
                }
                Err(e) if e == "cancelled" || e.contains("cancelled") => cancelled = true,
                Err(e) => failed.push(format!("{}: {e}", path.display())),
            }
        }
        let _ = handle.emit("delete-progress", DeleteProgress { done: total, total, bytes: freed, total_bytes });
        cancelled |= st.delete_cancel.load(Ordering::Relaxed);
        (ok, failed, freed, cancelled)
    })
    .await
    .map_err(|e| e.to_string())?;

    {
        let mut g = state.tree.lock();
        // Ids belong to the tree the delete started on; a rescan meanwhile makes them meaningless
        if let Some(t) = g.as_mut()
            && state.generation.load(Ordering::Relaxed) == generation
        {
            for id in &ok {
                t.remove(*id);
            }
        }
    }
    Ok(DeleteResult { freed, deleted: ok.len(), failed, cancelled })
}

#[derive(Serialize, Clone)]
struct DeleteProgress {
    done: usize,
    total: usize,
    bytes: u64,
    total_bytes: u64,
}

/// Stop a running delete after the item in progress.
#[tauri::command]
fn cancel_delete(state: State<'_, AppState>) {
    state.delete_cancel.store(true, Ordering::Relaxed);
}

// ---------- system tools (Care, cleaners, uninstaller, updater, internet, disk health) ----------

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

/// Run blocking work off the async runtime.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

#[cfg_attr(windows, allow(dead_code))]
const WINDOWS_ONLY: &str = "only available on Windows";

/// Create a restore point before a system change, telling the UI first (it can take a while).
fn restore_point_for(app: &AppHandle, what: &str) -> sysutil::RestorePoint {
    #[cfg(windows)]
    if mft::is_elevated() {
        let _ = app.emit("restore-point", "creating");
    }
    #[cfg(not(windows))]
    let _ = app;
    sysutil::restore_point(what)
}

#[tauri::command]
async fn junk_scan() -> Result<Vec<junk::JunkItem>, String> {
    blocking(junk::scan).await
}

#[tauri::command]
async fn junk_clean(keys: Vec<String>) -> Result<junk::CleanOutcome, String> {
    blocking(move || junk::clean(&keys)).await
}

#[tauri::command]
async fn privacy_scan() -> Result<Vec<privacy::PrivacyItem>, String> {
    blocking(privacy::scan).await
}

#[tauri::command]
async fn privacy_clean(ids: Vec<String>) -> Result<privacy::PrivacyOutcome, String> {
    blocking(move || privacy::clean(&ids)).await
}

#[tauri::command]
async fn shortcuts_scan() -> Result<Vec<shortcuts::BrokenShortcut>, String> {
    blocking(shortcuts::find_broken).await
}

#[derive(Serialize)]
struct CountOutcome {
    fixed: usize,
    failed: Vec<String>,
}

#[tauri::command]
async fn shortcuts_fix(paths: Vec<String>) -> Result<CountOutcome, String> {
    blocking(move || {
        let (fixed, failed) = shortcuts::remove(&paths);
        CountOutcome { fixed, failed }
    })
    .await
}

/// Empty folders in the scanned tree (needs a scan).
#[tauri::command]
fn empty_folders(state: State<'_, AppState>) -> Result<Vec<shortcuts::EmptyFolder>, String> {
    let g = state.tree.lock();
    let t = g.as_ref().ok_or("no scan yet")?;
    Ok(shortcuts::empty_folders(t, 2000))
}

#[tauri::command]
async fn shred_inspect(paths: Vec<String>) -> Result<Vec<shred::ShredTarget>, String> {
    blocking(move || shred::inspect(&paths)).await
}

/// Shred files and folders picked in the Shredder (paths from the file dialog or drag and drop).
#[tauri::command]
async fn shred_paths(paths: Vec<String>, passes: u8, app: AppHandle, state: State<'_, AppState>) -> Result<DeleteResult, String> {
    state.delete_cancel.store(false, Ordering::Relaxed);
    let handle = app.clone();
    let res = blocking(move || {
        let targets: Vec<PathBuf> = paths.iter().map(PathBuf::from).filter(|p| !is_protected(p)).collect();
        let info = shred::inspect(&paths);
        let total = targets.len();
        let total_bytes: u64 = info.iter().map(|i| i.size).sum();
        let progress = Arc::new(delete::Progress::new());
        let finished = Arc::new(AtomicBool::new(false));
        let ticker = {
            let (p, fin, h) = (progress.clone(), finished.clone(), handle.clone());
            std::thread::spawn(move || {
                while !fin.load(Ordering::Relaxed) {
                    if h.state::<AppState>().delete_cancel.load(Ordering::Relaxed) {
                        p.cancel.store(true, Ordering::Relaxed);
                    }
                    let _ = h.emit("delete-progress", DeleteProgress { done: p.done.load(Ordering::Relaxed), total, bytes: p.bytes.load(Ordering::Relaxed).min(total_bytes), total_bytes });
                    std::thread::sleep(Duration::from_millis(120));
                }
            })
        };
        let results = shred::shred_all(&targets, passes.clamp(1, 3), &progress);
        finished.store(true, Ordering::Relaxed);
        let _ = ticker.join();
        let mut out = DeleteResult { freed: 0, deleted: 0, failed: vec![], cancelled: false };
        out.failed.extend(paths.iter().filter(|p| is_protected(std::path::Path::new(p))).map(|p| format!("{p}: protected system location")));
        for (p, r) in targets.iter().zip(results) {
            match r {
                Ok(n) => {
                    out.freed += n;
                    out.deleted += 1;
                }
                Err(e) if e == "cancelled" => out.cancelled = true,
                Err(e) => out.failed.push(format!("{}: {e}", p.display())),
            }
        }
        out
    })
    .await?;
    Ok(res)
}

#[tauri::command]
async fn disk_health() -> Result<diskhealth::HealthReport, String> {
    blocking(diskhealth::report).await
}

#[tauri::command]
async fn disk_optimize(volume: String) -> Result<String, String> {
    blocking(move || diskhealth::optimize(&volume)).await?
}

#[cfg(windows)]
#[derive(Serialize)]
struct RegistryFix {
    #[serde(flatten)]
    out: registry::FixOutcome,
    restore_point: sysutil::RestorePoint,
}

#[tauri::command]
#[cfg(windows)]
async fn registry_scan() -> Result<Vec<registry::Issue>, String> {
    blocking(registry::scan).await
}
#[tauri::command]
#[cfg(not(windows))]
async fn registry_scan() -> Result<Vec<()>, String> {
    Ok(Vec::new())
}

#[tauri::command]
#[cfg(windows)]
async fn registry_fix(ids: Vec<String>, app: AppHandle) -> Result<RegistryFix, String> {
    let base = data_dir(&app)?;
    blocking(move || {
        let restore_point = restore_point_for(&app, "Tessera registry cleanup");
        registry::fix(&base, &ids).map(|out| RegistryFix { out, restore_point })
    })
    .await?
}
#[tauri::command]
#[cfg(not(windows))]
async fn registry_fix(ids: Vec<String>) -> Result<(), String> {
    let _ = ids;
    Err(WINDOWS_ONLY.into())
}

#[tauri::command]
#[cfg(windows)]
fn registry_backups(app: AppHandle) -> Result<Vec<registry::BackupInfo>, String> {
    Ok(registry::backups(&data_dir(&app)?))
}
#[tauri::command]
#[cfg(not(windows))]
fn registry_backups() -> Vec<()> {
    Vec::new()
}

#[tauri::command]
#[cfg(windows)]
async fn registry_undo(name: String, app: AppHandle) -> Result<usize, String> {
    let base = data_dir(&app)?;
    blocking(move || registry::undo(&base, &name)).await?
}
#[tauri::command]
#[cfg(not(windows))]
async fn registry_undo(name: String) -> Result<usize, String> {
    let _ = name;
    Err(WINDOWS_ONLY.into())
}

#[tauri::command]
#[cfg(windows)]
async fn programs_list() -> Result<Vec<programs::Program>, String> {
    blocking(programs::list).await
}
#[tauri::command]
#[cfg(not(windows))]
async fn programs_list() -> Result<Vec<()>, String> {
    Ok(Vec::new())
}

/// Run a program's uninstaller and wait for it; true when its entry is gone afterwards.
#[tauri::command]
#[cfg(windows)]
async fn program_uninstall(id: String) -> Result<bool, String> {
    blocking(move || programs::uninstall(&id)).await?
}
#[tauri::command]
#[cfg(not(windows))]
async fn program_uninstall(id: String) -> Result<bool, String> {
    let _ = id;
    Err(WINDOWS_ONLY.into())
}

#[tauri::command]
#[cfg(windows)]
async fn program_leftovers(program: programs::Program) -> Result<Vec<programs::Leftover>, String> {
    blocking(move || programs::leftovers(&program)).await
}
#[tauri::command]
#[cfg(not(windows))]
async fn program_leftovers(program: serde_json::Value) -> Result<Vec<()>, String> {
    let _ = program;
    Ok(Vec::new())
}

#[cfg(windows)]
#[derive(Serialize)]
struct LeftoverFix {
    #[serde(flatten)]
    out: programs::LeftoverOutcome,
    restore_point: Option<sysutil::RestorePoint>,
}

#[tauri::command]
#[cfg(windows)]
async fn leftovers_remove(items: Vec<programs::Leftover>, app: AppHandle) -> Result<LeftoverFix, String> {
    let base = data_dir(&app)?;
    blocking(move || {
        let restore_point = items.iter().any(|l| l.kind == "registry").then(|| restore_point_for(&app, "Tessera uninstall leftovers"));
        LeftoverFix { out: programs::remove_leftovers(&base, &items), restore_point }
    })
    .await
}
#[tauri::command]
#[cfg(not(windows))]
async fn leftovers_remove(items: Vec<serde_json::Value>) -> Result<(), String> {
    let _ = items;
    Err(WINDOWS_ONLY.into())
}

#[derive(Serialize)]
struct UpdatesList {
    /// winget is installed.
    available: bool,
    #[cfg(windows)]
    items: Vec<winget::Upgrade>,
    #[cfg(not(windows))]
    items: Vec<()>,
}

#[tauri::command]
async fn updates_list() -> Result<UpdatesList, String> {
    #[cfg(windows)]
    {
        blocking(|| {
            if !winget::available() {
                return Ok(UpdatesList { available: false, items: vec![] });
            }
            winget::list().map(|items| UpdatesList { available: true, items })
        })
        .await?
    }
    #[cfg(not(windows))]
    Ok(UpdatesList { available: false, items: vec![] })
}

#[tauri::command]
async fn update_install(id: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        blocking(move || winget::upgrade(&id)).await?
    }
    #[cfg(not(windows))]
    {
        let _ = id;
        Err(WINDOWS_ONLY.into())
    }
}

#[cfg(windows)]
#[derive(Serialize)]
struct NetStatus {
    adapters: Vec<internet::Adapter>,
    tcp: Vec<internet::TcpSetting>,
    has_backup: bool,
    elevated: bool,
}

#[tauri::command]
#[cfg(windows)]
async fn net_status(app: AppHandle) -> Result<NetStatus, String> {
    let base = data_dir(&app)?;
    blocking(move || {
        Ok(NetStatus { adapters: internet::adapters()?, tcp: internet::tcp_settings().unwrap_or_default(), has_backup: internet::has_backup(&base), elevated: mft::is_elevated() })
    })
    .await?
}
#[tauri::command]
#[cfg(not(windows))]
async fn net_status() -> Result<(), String> {
    Err(WINDOWS_ONLY.into())
}

#[tauri::command]
#[cfg(windows)]
async fn net_set_dns(preset: String, app: AppHandle) -> Result<usize, String> {
    let base = data_dir(&app)?;
    blocking(move || {
        restore_point_for(&app, "Tessera DNS change");
        internet::set_dns(&base, &preset)
    })
    .await?
}
#[tauri::command]
#[cfg(not(windows))]
async fn net_set_dns(preset: String) -> Result<usize, String> {
    let _ = preset;
    Err(WINDOWS_ONLY.into())
}

#[tauri::command]
#[cfg(windows)]
async fn net_latency(current: Option<String>) -> Result<Vec<internet::Latency>, String> {
    blocking(move || internet::latency(current)).await
}
#[tauri::command]
#[cfg(not(windows))]
async fn net_latency(current: Option<String>) -> Result<Vec<()>, String> {
    let _ = current;
    Ok(Vec::new())
}

#[tauri::command]
async fn net_flush() -> Result<(), String> {
    #[cfg(windows)]
    {
        blocking(internet::flush_dns).await?
    }
    #[cfg(not(windows))]
    Err(WINDOWS_ONLY.into())
}

#[tauri::command]
#[cfg(windows)]
async fn net_set_tcp(key: String, on: bool, app: AppHandle) -> Result<(), String> {
    let base = data_dir(&app)?;
    blocking(move || {
        restore_point_for(&app, "Tessera network settings");
        internet::set_tcp(&base, &key, on)
    })
    .await?
}
#[tauri::command]
#[cfg(not(windows))]
async fn net_set_tcp(key: String, on: bool) -> Result<(), String> {
    let _ = (key, on);
    Err(WINDOWS_ONLY.into())
}

#[tauri::command]
async fn net_restore(app: AppHandle) -> Result<usize, String> {
    #[cfg(windows)]
    {
        let base = data_dir(&app)?;
        blocking(move || internet::restore(&base)).await?
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err(WINDOWS_ONLY.into())
    }
}

#[tauri::command]
async fn net_repair() -> Result<(), String> {
    #[cfg(windows)]
    {
        blocking(internet::repair).await?
    }
    #[cfg(not(windows))]
    Err(WINDOWS_ONLY.into())
}

// ---------- Care ----------

#[derive(Serialize)]
struct CareState {
    #[serde(flatten)]
    config: care::CareConfig,
    /// A scan started from the tray or the schedule is running right now.
    running: bool,
    /// Scheduling is available (Windows Task Scheduler).
    can_schedule: bool,
}

#[tauri::command]
fn care_config(app: AppHandle) -> Result<CareState, String> {
    Ok(CareState { config: care::load_config(&data_dir(&app)?), running: tray::care_running(), can_schedule: cfg!(windows) })
}

/// Save Care settings; the schedule is (re)registered with Task Scheduler.
#[tauri::command]
async fn care_set_config(config: care::CareConfig, app: AppHandle) -> Result<(), String> {
    let base = data_dir(&app)?;
    blocking(move || {
        let mut c = care::load_config(&base);
        let schedule_changed = c.schedule != config.schedule || c.time != config.time;
        c.modules = config.modules;
        c.privacy_kinds = config.privacy_kinds;
        c.auto_clean = config.auto_clean;
        c.schedule = config.schedule;
        c.time = config.time;
        if schedule_changed {
            care::apply_schedule(&c)?;
        }
        care::save_config(&base, &c)
    })
    .await?
}

#[derive(Serialize, Clone)]
struct CareScan {
    #[serde(flatten)]
    report: care::CareReport,
    plan: care::FixPlan,
}

#[tauri::command]
async fn care_scan(app: AppHandle) -> Result<CareScan, String> {
    let base = data_dir(&app)?;
    blocking(move || {
        let cfg = care::load_config(&base);
        let report = care::scan(&cfg);
        care::remember(&base, &report);
        let plan = care::default_plan(&report, &cfg);
        CareScan { report, plan }
    })
    .await
}

#[tauri::command]
async fn care_fix(plan: care::FixPlan, app: AppHandle) -> Result<care::FixOutcome, String> {
    let base = data_dir(&app)?;
    blocking(move || {
        if !plan.registry.is_empty() {
            let _ = restore_point_for(&app, "Tessera Care");
        }
        care::fix(&base, &plan)
    })
    .await
}

/// "1.4 GB", same units as the UI.
fn fmt_bytes(b: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let (mut v, mut i) = (b as f64, 0);
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if v >= 100.0 || i == 0 { format!("{v:.0} {}", units[i]) } else { format!("{v:.1} {}", units[i]) }
}

/// Care in the background (tray menu, or `--care` from the schedule): scan, optionally clean the
/// safe part, remember the result, tell the UI and send a notification.
pub(crate) fn run_care_background(app: &AppHandle, scheduled: bool) {
    if tray::CARE_RUNNING.swap(true, Ordering::Relaxed) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let Ok(base) = data_dir(&app) else {
            tray::CARE_RUNNING.store(false, Ordering::Relaxed);
            return;
        };
        let _ = app.emit("care-started", ());
        let cfg = care::load_config(&base);
        let mut report = care::scan(&cfg);
        let st = app.state::<AppState>();
        let labels = st.labels.lock().clone();
        let body = if scheduled && cfg.auto_clean && report.fixable_bytes > 0 {
            // Unattended runs never touch the registry or shortcuts: only junk and privacy items
            let full = care::default_plan(&report, &cfg);
            let out = care::fix(&base, &care::FixPlan { junk: full.junk, privacy: full.privacy, ..Default::default() });
            report = care::scan(&cfg);
            labels.care_cleaned.replace("{size}", &fmt_bytes(out.freed))
        } else if report.fixable_bytes == 0 && report.fixable_issues == 0 {
            labels.care_clean.replace("{score}", &report.score.to_string())
        } else {
            labels.care_body.replace("{score}", &report.score.to_string()).replace("{size}", &fmt_bytes(report.fixable_bytes)).replace("{n}", &report.fixable_issues.to_string())
        };
        care::remember(&base, &report);
        let plan = care::default_plan(&report, &cfg);
        let _ = app.emit("care-report", CareScan { report, plan });
        tray::notify(&app, &labels.care_title, &body);
        tray::CARE_RUNNING.store(false, Ordering::Relaxed);
        // A scheduled run that started hidden ends here unless Tessera is meant to stay in the tray
        let visible = app.get_webview_window("main").and_then(|w| w.is_visible().ok()).unwrap_or(false);
        if scheduled && st.hidden_start.load(Ordering::Relaxed) && !visible && !st.prefs.lock().close_to_tray {
            std::thread::sleep(Duration::from_secs(3));
            app.exit(0);
        }
    });
}

#[tauri::command]
fn prefs_get(state: State<'_, AppState>) -> tray::Prefs {
    state.prefs.lock().clone()
}

#[tauri::command]
fn prefs_set(prefs: tray::Prefs, app: AppHandle, state: State<'_, AppState>) {
    *state.prefs.lock() = prefs.clone();
    tray::save_prefs(&app, &prefs);
}

/// Tray menu and notification texts in the UI language.
#[tauri::command]
fn tray_labels(labels: tray::Labels, state: State<'_, AppState>) {
    if let Some(items) = state.tray.lock().as_ref() {
        tray::relabel(items, &labels);
    }
    *state.labels.lock() = labels;
}

#[derive(Serialize)]
struct LaunchState {
    /// Started hidden by a scheduled Care run; the window hasn't been opened yet.
    hidden: bool,
}

#[tauri::command]
fn launch_state(app: AppHandle, state: State<'_, AppState>) -> LaunchState {
    let visible = app.get_webview_window("main").and_then(|w| w.is_visible().ok()).unwrap_or(true);
    LaunchState { hidden: state.hidden_start.load(Ordering::Relaxed) && !visible }
}

fn is_protected(p: &std::path::Path) -> bool {
    let s = p.to_string_lossy().to_lowercase().replace('/', "\\");
    let roots = ["c:\\windows", "c:\\program files", "c:\\program files (x86)", "c:\\programdata\\microsoft"];
    let files = ["pagefile.sys", "hiberfil.sys", "swapfile.sys"];
    roots.iter().any(|r| s == *r || s.starts_with(&format!("{r}\\")))
        || files.iter().any(|f| s.ends_with(f))
        || s.len() <= 3
        || s == "/" || s.starts_with("/system") || s.starts_with("/usr") || s.starts_with("/bin")
}

/// For tools that may work *inside* Program Files and AppData (uninstall leftovers): those folders
/// themselves, the user profile, Windows and Microsoft's own areas stay off limits.
#[cfg_attr(not(windows), allow(dead_code))]
fn is_protected_strict(p: &std::path::Path) -> bool {
    let norm = |s: &str| s.to_lowercase().replace('/', "\\").trim_end_matches('\\').to_string();
    let s = norm(&p.to_string_lossy());
    let var = |v: &str| sysutil::env_path(v).map(|p| norm(&p.to_string_lossy()));
    let exact: Vec<String> = ["ProgramFiles", "ProgramFiles(x86)", "ProgramData", "APPDATA", "LOCALAPPDATA", "USERPROFILE", "PUBLIC"]
        .iter()
        .filter_map(|v| var(v))
        .chain(var("LOCALAPPDATA").map(|l| format!(r"{l}\programs")))
        .collect();
    let mut under: Vec<String> = vec![var("SystemRoot").unwrap_or_else(|| r"c:\windows".into())];
    for (v, subs) in [
        ("ProgramFiles", &["common files", "windowsapps", "windows defender", "windows nt", "windowspowershell", "internet explorer", "microsoft update health tools"][..]),
        ("ProgramFiles(x86)", &["common files", "windows defender", "windows nt", "windowspowershell", "internet explorer", "microsoft"][..]),
        ("ProgramData", &["microsoft", "packages"][..]),
        ("LOCALAPPDATA", &["microsoft", "packages"][..]),
        ("APPDATA", &["microsoft"][..]),
    ] {
        if let Some(base) = var(v) {
            under.extend(subs.iter().map(|s| format!(r"{base}\{s}")));
        }
    }
    s.len() <= 3 || exact.contains(&s) || under.iter().any(|u| s == *u || s.starts_with(&format!(r"{u}\")))
}

/// Show a file or folder selected in the OS file manager.
#[tauri::command]
fn reveal(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer").arg(format!("/select,{path}")).spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").args(["-R", &path]).spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        let dir = std::path::Path::new(&path).parent().map(|p| p.to_path_buf()).unwrap_or_default();
        std::process::Command::new("xdg-open").arg(dir).spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Scheduled Care runs start Tessera with --care: scan in the background, window stays hidden
    let care_mode = std::env::args().any(|a| a == "--care");
    tauri::Builder::default()
        // First: a second launch (Start menu, or the schedule) is handed to the running instance
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if args.iter().any(|a| a == "--care") {
                run_care_background(app, true);
            } else {
                tray::show_main(app);
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .manage(AppState::default())
        .setup(move |app| {
            let handle = app.handle().clone();
            let st = handle.state::<AppState>();
            *st.prefs.lock() = tray::load_prefs(&handle);
            match tray::build(&handle) {
                Ok(items) => *st.tray.lock() = Some(items),
                Err(e) => eprintln!("tray icon unavailable: {e}"),
            }
            tray::start_meter(handle.clone());
            if care_mode {
                st.hidden_start.store(true, Ordering::Relaxed);
                run_care_background(&handle, true);
            } else if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && window.label() == "main"
                && tray::on_close(window.app_handle())
            {
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_drives, scan, cancel_scan, get_node, get_ancestors, largest_files, find_junk, delete_nodes, cancel_delete, reveal,
            is_elevated, relaunch_as_admin, find_duplicates, cancel_duplicates, hardlink_dupes, history_list, history_diff,
            set_live_watch, get_summary, load_cached,
            opt_status, opt_apply_profile, opt_set_tweak, opt_set_power_plan, opt_restore, opt_startup_list, opt_startup_set,
            opt_processes, opt_kill, sys_stats,
            junk_scan, junk_clean, privacy_scan, privacy_clean, shortcuts_scan, shortcuts_fix, empty_folders, shred_inspect, shred_paths,
            disk_health, disk_optimize, registry_scan, registry_fix, registry_backups, registry_undo, programs_list, program_uninstall,
            program_leftovers, leftovers_remove, updates_list, update_install, net_status, net_set_dns, net_latency, net_flush,
            net_set_tcp, net_restore, net_repair, care_config, care_set_config, care_scan, care_fix, prefs_get, prefs_set, tray_labels,
            launch_state
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
