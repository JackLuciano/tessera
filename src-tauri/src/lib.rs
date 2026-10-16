mod dupes;
mod history;
#[cfg(windows)]
mod mft;
mod scanner;

use parking_lot::Mutex;
use scanner::{Cat, Kind, Progress, Tree, CAT_COUNT, NO_PARENT};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
struct AppState {
    tree: Mutex<Option<Tree>>,
    progress: Mutex<Option<Arc<Progress>>>,
    dup_progress: Mutex<Option<Arc<dupes::DupProgress>>>,
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
    Ok(s)
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

fn dto(t: &Tree, id: u32, depth: u32, max_children: usize) -> NodeDto {
    let n = &t.nodes[id as usize];
    let children = if depth > 0 {
        n.children
            .iter()
            .filter(|c| t.nodes[**c as usize].size > 0)
            .take(max_children)
            .map(|c| dto(t, *c, depth - 1, max_children))
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
fn get_node(id: u32, depth: u32, max_children: usize, state: State<'_, AppState>) -> Result<NodeDto, String> {
    let g = state.tree.lock();
    let t = g.as_ref().ok_or("no scan yet")?;
    if id as usize >= t.nodes.len() {
        return Err("unknown node".into());
    }
    Ok(dto(t, id, depth.min(3), max_children.clamp(1, 400)))
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
}

/// Delete nodes by id. `permanent` skips the Recycle Bin / Trash.
#[tauri::command]
async fn delete_nodes(ids: Vec<u32>, permanent: bool, state: State<'_, AppState>) -> Result<DeleteResult, String> {
    let targets: Vec<(u32, PathBuf, Kind, u64)> = {
        let g = state.tree.lock();
        let t = g.as_ref().ok_or("no scan yet")?;
        ids.iter()
            .filter(|id| (**id as usize) < t.nodes.len() && **id != 0)
            .map(|id| { let n = &t.nodes[*id as usize]; (*id, t.path_of(*id), n.kind, n.size) })
            .collect()
    };
    let mut ok = Vec::new();
    let mut failed = Vec::new();
    for (id, path, kind, _) in &targets {
        if *kind == Kind::Bucket {
            failed.push(format!("{}: small-file groups can't be deleted as one item", path.display()));
            continue;
        }
        if is_protected(path) {
            failed.push(format!("{}: protected system location", path.display()));
            continue;
        }
        let res = if permanent {
            if *kind == Kind::Dir { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) }.map_err(|e| e.to_string())
        } else {
            trash::delete(path).map_err(|e| e.to_string())
        };
        match res {
            Ok(()) => ok.push(*id),
            Err(e) => failed.push(format!("{}: {e}", path.display())),
        }
    }
    let mut freed = 0;
    {
        let mut g = state.tree.lock();
        if let Some(t) = g.as_mut() {
            for id in &ok {
                freed += t.nodes[*id as usize].size;
                t.remove(*id);
            }
        }
    }
    Ok(DeleteResult { freed, deleted: ok.len(), failed })
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
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            list_drives, scan, cancel_scan, get_node, get_ancestors, largest_files, find_junk, delete_nodes, reveal,
            is_elevated, relaunch_as_admin, find_duplicates, cancel_duplicates, history_list, history_diff
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
