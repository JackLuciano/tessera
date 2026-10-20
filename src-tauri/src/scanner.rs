//! Parallel filesystem scanner that builds a size tree in an arena.
//!
//! Directories are walked recursively with rayon; every directory becomes a node,
//! files of at least `FILE_NODE_MIN` bytes become their own node, and smaller files
//! are folded into one "small files" bucket per directory to keep memory bounded.

use rayon::prelude::*;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

pub const FILE_NODE_MIN: u64 = 512 * 1024;
pub const CAT_COUNT: usize = 9;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Cat {
    Video,
    Games,
    Dev,
    Apps,
    System,
    Images,
    Cache,
    Archives,
    Docs,
}

impl Cat {
    pub const ALL: [Cat; CAT_COUNT] = [
        Cat::Video, Cat::Games, Cat::Dev, Cat::Apps, Cat::System,
        Cat::Images, Cat::Cache, Cat::Archives, Cat::Docs,
    ];

    fn idx(self) -> usize {
        self as usize
    }

    /// Classify by extension first, then by well-known folder names in the path.
    pub fn classify(name: &str, path_lower: &str) -> Cat {
        if path_lower.contains("\\cache") || path_lower.contains("/cache") || path_lower.contains("\\temp\\")
            || path_lower.contains("/.cache/") || path_lower.contains("node_modules\\.cache")
        {
            return Cat::Cache;
        }
        if path_lower.contains("node_modules") || path_lower.contains("\\.git\\") || path_lower.contains("/.git/") {
            return Cat::Dev;
        }
        if path_lower.contains("steamapps") || path_lower.contains("\\games\\") || path_lower.contains("epic games") {
            return Cat::Games;
        }
        let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
        match ext.as_str() {
            "mp4" | "mkv" | "mov" | "avi" | "webm" | "wmv" | "flv" | "m4v" | "ts" => Cat::Video,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "raw" | "dng" | "psd" | "psb" | "tif" | "tiff" | "bmp" | "svg" => Cat::Images,
            "zip" | "7z" | "rar" | "tar" | "gz" | "xz" | "zst" | "iso" | "img" | "dmg" | "vhd" | "vhdx" | "bak" => Cat::Archives,
            "pak" | "archive" | "bundle" | "upk" | "uasset" | "vpk" | "bsa" | "ba2" => Cat::Games,
            "exe" | "dll" | "msi" | "app" | "so" | "dylib" | "appx" | "msix" | "deb" | "rpm" | "appimage" => Cat::Apps,
            "rs" | "js" | "mjs" | "ts_" | "tsx" | "jsx" | "py" | "c" | "cpp" | "h" | "hpp" | "cs" | "java" | "go"
            | "lua" | "json" | "map" | "pdb" | "lib" | "obj" | "o" | "rlib" | "rmeta" | "wasm" | "node" => Cat::Dev,
            "sys" | "log" | "etl" | "evtx" | "cab" | "manifest" | "mui" | "cat" => Cat::System,
            "tmp" | "temp" | "cache" | "db-wal" => Cat::Cache,
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "md" | "csv" | "odt" | "epub" => Cat::Docs,
            _ => {
                if path_lower.contains("\\windows\\") || path_lower.starts_with("c:\\windows") {
                    Cat::System
                } else if path_lower.contains("program files") {
                    Cat::Apps
                } else {
                    Cat::Docs
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct Node {
    pub name: String,
    pub parent: u32,
    pub size: u64,
    pub files: u64,
    pub dirs: u64,
    pub kind: Kind,
    pub children: Vec<u32>,
    pub cats: [u64; CAT_COUNT],
    pub modified: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Dir,
    File,
    /// Aggregate of all small files directly inside one directory.
    Bucket,
}

pub struct Tree {
    pub nodes: Vec<Node>,
    pub root_path: PathBuf,
    pub elapsed_ms: u64,
    pub errors: u64,
}

pub const NO_PARENT: u32 = u32::MAX;

pub struct Progress {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub bytes: AtomicU64,
    pub errors: AtomicU64,
    pub cancel: AtomicBool,
}

impl Progress {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            files: AtomicU64::new(0),
            dirs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            cancel: AtomicBool::new(false),
        })
    }
}

/// Owned intermediate tree produced by the parallel walk, flattened afterwards.
struct Scanned {
    name: String,
    kind: Kind,
    size: u64,
    files: u64,
    dirs: u64,
    cats: [u64; CAT_COUNT],
    modified: u64,
    children: Vec<Scanned>,
}

fn mtime(md: &fs::Metadata) -> u64 {
    md.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Windows "size on disk" is not exposed cheaply; apparent length is used everywhere.
fn walk(path: &Path, name: String, progress: &Progress) -> Scanned {
    let mut node = Scanned {
        name, kind: Kind::Dir, size: 0, files: 0, dirs: 0, cats: [0; CAT_COUNT], modified: 0, children: Vec::new(),
    };
    progress.dirs.fetch_add(1, Ordering::Relaxed);
    if progress.cancel.load(Ordering::Relaxed) {
        return node;
    }
    let rd = match fs::read_dir(path) {
        Ok(rd) => rd,
        Err(_) => {
            progress.errors.fetch_add(1, Ordering::Relaxed);
            return node;
        }
    };

    let mut subdirs: Vec<(PathBuf, String)> = Vec::new();
    let mut bucket = Scanned {
        name: String::new(), kind: Kind::Bucket, size: 0, files: 0, dirs: 0, cats: [0; CAT_COUNT], modified: 0, children: Vec::new(),
    };
    let path_lower = path.to_string_lossy().to_lowercase();

    for entry in rd.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        // Junctions and symlinks are skipped so the same data is never counted twice.
        if ft.is_symlink() {
            continue;
        }
        let fname = entry.file_name().to_string_lossy().into_owned();
        if ft.is_dir() {
            subdirs.push((entry.path(), fname));
            continue;
        }
        let Ok(md) = entry.metadata() else {
            progress.errors.fetch_add(1, Ordering::Relaxed);
            continue;
        };
        let len = md.len();
        let cat = Cat::classify(&fname, &path_lower);
        progress.files.fetch_add(1, Ordering::Relaxed);
        progress.bytes.fetch_add(len, Ordering::Relaxed);
        let modified = mtime(&md);
        if len >= FILE_NODE_MIN {
            let mut cats = [0; CAT_COUNT];
            cats[cat.idx()] = len;
            node.children.push(Scanned {
                name: fname, kind: Kind::File, size: len, files: 1, dirs: 0, cats, modified, children: Vec::new(),
            });
        } else {
            bucket.size += len;
            bucket.files += 1;
            bucket.cats[cat.idx()] += len;
            bucket.modified = bucket.modified.max(modified);
        }
    }

    let sub: Vec<Scanned> = subdirs
        .into_par_iter()
        .map(|(p, n)| walk(&p, n, progress))
        .collect();
    node.children.extend(sub);
    if bucket.files > 0 {
        bucket.name = format!("{} small files", bucket.files);
        node.children.push(bucket);
    }
    for c in &node.children {
        node.size += c.size;
        node.files += c.files;
        node.dirs += c.dirs + u64::from(c.kind == Kind::Dir);
        node.modified = node.modified.max(c.modified);
        for i in 0..CAT_COUNT {
            node.cats[i] += c.cats[i];
        }
    }
    node
}

fn flatten(s: Scanned, parent: u32, out: &mut Vec<Node>) -> u32 {
    let id = out.len() as u32;
    out.push(Node {
        name: s.name, parent, size: s.size, files: s.files, dirs: s.dirs, kind: s.kind,
        children: Vec::with_capacity(s.children.len()), cats: s.cats, modified: s.modified,
    });
    let mut kids: Vec<u32> = s.children.into_iter().map(|c| flatten(c, id, out)).collect();
    kids.sort_by(|a, b| out[*b as usize].size.cmp(&out[*a as usize].size));
    out[id as usize].children = kids;
    id
}

pub fn scan(root: &Path, progress: &Progress) -> Tree {
    let started = std::time::Instant::now();
    let root_name = root.to_string_lossy().into_owned();
    let scanned = walk(root, root_name, progress);
    let mut nodes = Vec::with_capacity(1 << 16);
    flatten(scanned, NO_PARENT, &mut nodes);
    nodes.shrink_to_fit();
    Tree {
        nodes,
        root_path: root.to_path_buf(),
        elapsed_ms: started.elapsed().as_millis() as u64,
        errors: progress.errors.load(Ordering::Relaxed),
    }
}

impl Tree {
    pub fn path_of(&self, id: u32) -> PathBuf {
        let mut parts = Vec::new();
        let mut cur = id;
        while cur != NO_PARENT {
            let n = &self.nodes[cur as usize];
            if n.parent == NO_PARENT {
                break;
            }
            parts.push(n.name.as_str());
            cur = n.parent;
        }
        let mut p = self.root_path.clone();
        for part in parts.iter().rev() {
            p.push(part);
        }
        p
    }

    pub fn dominant_cat(&self, id: u32) -> Cat {
        let c = &self.nodes[id as usize].cats;
        let (i, _) = c.iter().enumerate().max_by_key(|(_, v)| **v).unwrap_or((0, &0));
        Cat::ALL[i]
    }

    /// Subtract a removed node from all ancestors and detach it from its parent.
    pub fn remove(&mut self, id: u32) {
        let (size, files, cats, parent, is_dir) = {
            let n = &self.nodes[id as usize];
            (n.size, n.files, n.cats, n.parent, n.kind == Kind::Dir)
        };
        if parent == NO_PARENT {
            return;
        }
        self.nodes[parent as usize].children.retain(|c| *c != id);
        let mut cur = parent;
        while cur != NO_PARENT {
            let n = &mut self.nodes[cur as usize];
            n.size = n.size.saturating_sub(size);
            n.files = n.files.saturating_sub(files);
            if is_dir {
                n.dirs = n.dirs.saturating_sub(1);
            }
            for (c, sub) in n.cats.iter_mut().zip(cats) {
                *c = c.saturating_sub(sub);
            }
            cur = n.parent;
        }
        // Keep the nodes in the arena (ids stay stable) but zero the whole subtree, so arena-wide
        // queries (largest files, duplicates) that filter on size never return detached nodes.
        self.nodes[id as usize].parent = NO_PARENT;
        self.zero_subtree(id);
    }

    fn zero_subtree(&mut self, id: u32) {
        let mut stack = vec![id];
        while let Some(i) = stack.pop() {
            let n = &mut self.nodes[i as usize];
            n.size = 0;
            stack.append(&mut n.children);
        }
    }

    /// False for nodes detached by `remove` (or below one): their parent chain never reaches the root.
    pub fn is_live(&self, id: u32) -> bool {
        let mut cur = id;
        while cur != 0 {
            let p = self.nodes[cur as usize].parent;
            if p == NO_PARENT {
                return false;
            }
            cur = p;
        }
        true
    }

    /// Deepest existing directory node on the way to `path` (the root if `path` is outside the tree).
    pub fn find_dir(&self, path: &Path) -> u32 {
        let Ok(rel) = path.strip_prefix(&self.root_path) else { return 0 };
        let mut cur = 0u32;
        for part in rel.components() {
            let part = part.as_os_str().to_string_lossy();
            let next = self.nodes[cur as usize].children.iter().copied().find(|c| {
                let n = &self.nodes[*c as usize];
                n.kind == Kind::Dir && names_eq(&n.name, &part)
            });
            match next {
                Some(c) => cur = c,
                None => break,
            }
        }
        cur
    }

    /// Re-read one directory after a change on disk, without rescanning what is below it:
    /// existing sub-folders keep their nodes, new sub-folders are walked, files are re-listed.
    /// The size difference is propagated to every ancestor. Returns the change in bytes.
    pub fn refresh_dir(&mut self, id: u32) -> i64 {
        // A detached node's path would resolve to the root; there is nothing left to refresh
        if !self.is_live(id) {
            return 0;
        }
        let path = self.path_of(id);
        let Ok(rd) = fs::read_dir(&path) else {
            // The folder itself is gone: drop it (the root is never removed)
            if id == 0 {
                return 0;
            }
            let before = self.nodes[id as usize].size as i64;
            self.remove(id);
            return -before;
        };
        let progress = Progress::new();
        let path_lower = path.to_string_lossy().to_lowercase();
        let old_children = std::mem::take(&mut self.nodes[id as usize].children);
        let mut keep: Vec<u32> = Vec::new();
        let mut new_dirs: Vec<(PathBuf, String)> = Vec::new();
        let mut fresh: Vec<Scanned> = Vec::new();
        let mut bucket = Scanned {
            name: String::new(), kind: Kind::Bucket, size: 0, files: 0, dirs: 0, cats: [0; CAT_COUNT], modified: 0, children: Vec::new(),
        };
        for entry in rd.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            let fname = entry.file_name().to_string_lossy().into_owned();
            if ft.is_dir() {
                let existing = old_children.iter().copied().find(|c| {
                    let n = &self.nodes[*c as usize];
                    n.kind == Kind::Dir && n.name == fname
                });
                match existing {
                    Some(c) => keep.push(c),
                    None => new_dirs.push((entry.path(), fname)),
                }
                continue;
            }
            let Ok(md) = entry.metadata() else { continue };
            let len = md.len();
            let cat = Cat::classify(&fname, &path_lower);
            let modified = mtime(&md);
            if len >= FILE_NODE_MIN {
                let mut cats = [0; CAT_COUNT];
                cats[cat.idx()] = len;
                fresh.push(Scanned { name: fname, kind: Kind::File, size: len, files: 1, dirs: 0, cats, modified, children: Vec::new() });
            } else {
                bucket.size += len;
                bucket.files += 1;
                bucket.cats[cat.idx()] += len;
                bucket.modified = bucket.modified.max(modified);
            }
        }
        let walked: Vec<Scanned> = new_dirs.into_par_iter().map(|(p, n)| walk(&p, n, &progress)).collect();
        fresh.extend(walked);
        if bucket.files > 0 {
            bucket.name = format!("{} small files", bucket.files);
            fresh.push(bucket);
        }

        // Detach everything that is not carried over (old file nodes, bucket, vanished folders)
        for c in old_children.iter().copied().filter(|c| !keep.contains(c)) {
            self.nodes[c as usize].parent = NO_PARENT;
            self.zero_subtree(c);
        }
        let mut kids = keep;
        for s in fresh {
            kids.push(flatten(s, id, &mut self.nodes));
        }
        kids.sort_by_key(|c| std::cmp::Reverse(self.nodes[*c as usize].size));

        // New totals for this folder, then push the difference up the ancestor chain
        let (mut size, mut files, mut dirs, mut cats, mut modified) = (0u64, 0u64, 0u64, [0u64; CAT_COUNT], 0u64);
        for c in &kids {
            let n = &self.nodes[*c as usize];
            size += n.size;
            files += n.files;
            dirs += n.dirs + u64::from(n.kind == Kind::Dir);
            modified = modified.max(n.modified);
            for (t, s) in cats.iter_mut().zip(n.cats) {
                *t += s;
            }
        }
        let n = &mut self.nodes[id as usize];
        let (old_size, old_files, old_dirs, old_cats) = (n.size, n.files, n.dirs, n.cats);
        n.children = kids;
        (n.size, n.files, n.dirs, n.cats, n.modified) = (size, files, dirs, cats, modified.max(n.modified));
        let mut cur = n.parent;
        while cur != NO_PARENT {
            let a = &mut self.nodes[cur as usize];
            a.size = (a.size + size).saturating_sub(old_size);
            a.files = (a.files + files).saturating_sub(old_files);
            a.dirs = (a.dirs + dirs).saturating_sub(old_dirs);
            for ((t, new), old) in a.cats.iter_mut().zip(cats).zip(old_cats) {
                *t = (*t + new).saturating_sub(old);
            }
            cur = a.parent;
        }
        // Keep siblings ordered largest first after the size change
        let parent = self.nodes[id as usize].parent;
        if parent != NO_PARENT {
            let mut sib = std::mem::take(&mut self.nodes[parent as usize].children);
            sib.sort_by_key(|c| std::cmp::Reverse(self.nodes[*c as usize].size));
            self.nodes[parent as usize].children = sib;
        }
        size as i64 - old_size as i64
    }
}

/// File names compare case-insensitively on Windows and macOS (default APFS), exactly elsewhere.
fn names_eq(a: &str, b: &str) -> bool {
    if cfg!(any(windows, target_os = "macos")) { a.to_lowercase() == b.to_lowercase() } else { a == b }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_extensions() {
        assert_eq!(Cat::classify("movie.MKV", "d:\\videos"), Cat::Video);
        assert_eq!(Cat::classify("setup.exe", "d:\\downloads"), Cat::Apps);
        assert_eq!(Cat::classify("index.js", "c:\\proj\\node_modules\\x"), Cat::Dev);
        assert_eq!(Cat::classify("a.bin", "c:\\users\\me\\appdata\\local\\temp\\x"), Cat::Cache);
    }

    #[test]
    fn scans_and_removes() {
        let dir = std::env::temp_dir().join(format!("tessera-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("big")).unwrap();
        fs::write(dir.join("big/blob.iso"), vec![0u8; (FILE_NODE_MIN + 10) as usize]).unwrap();
        fs::write(dir.join("small.txt"), b"hello").unwrap();

        let mut tree = scan(&dir, &Progress::new());
        let root = &tree.nodes[0];
        assert_eq!(root.files, 2);
        assert_eq!(root.size, FILE_NODE_MIN + 15);

        // Largest child first: the "big" folder, holding the file node
        let big = tree.nodes[0].children[0];
        assert_eq!(tree.nodes[big as usize].name, "big");
        let blob = tree.nodes[big as usize].children[0];
        assert_eq!(tree.path_of(blob), dir.join("big").join("blob.iso"));

        tree.remove(big);
        assert_eq!(tree.nodes[0].size, 5);
        assert_eq!(tree.nodes[0].files, 1);
        // Files below a removed folder no longer count anywhere
        assert_eq!(tree.nodes[blob as usize].size, 0);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn refreshes_one_folder_incrementally() {
        let dir = std::env::temp_dir().join(format!("tessera-refresh-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("a/deep")).unwrap();
        fs::write(dir.join("a/deep/x.bin"), vec![0u8; 100]).unwrap();
        fs::write(dir.join("a/one.txt"), b"12345").unwrap();
        let mut tree = scan(&dir, &Progress::new());
        assert_eq!(tree.nodes[0].size, 105);

        // Grow a file, add a new folder with a file, then refresh only "a"
        fs::write(dir.join("a/one.txt"), b"1234567890").unwrap();
        fs::create_dir_all(dir.join("a/new")).unwrap();
        fs::write(dir.join("a/new/y.bin"), vec![0u8; 50]).unwrap();
        let a = tree.find_dir(&dir.join("a").join("new"));
        assert_eq!(tree.nodes[a as usize].name, "a", "new folder is not in the tree yet");
        let delta = tree.refresh_dir(a);
        assert_eq!(delta, 55);
        assert_eq!(tree.nodes[0].size, 160);
        assert_eq!(tree.nodes[0].files, 3);
        assert_ne!(tree.find_dir(&dir.join("a").join("new")), a);

        // Deleting a folder on disk and refreshing it drops it from the totals
        fs::remove_dir_all(dir.join("a/deep")).unwrap();
        let deep = tree.find_dir(&dir.join("a").join("deep"));
        assert_eq!(tree.refresh_dir(deep), -100);
        assert_eq!(tree.nodes[0].size, 60);
        fs::remove_dir_all(&dir).unwrap();
    }
}
