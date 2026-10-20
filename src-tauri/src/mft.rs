//! NTFS fast path: read the Master File Table straight from the raw volume
//! (`\\.\C:`), the same technique WizTree uses. One sequential read of the MFT
//! yields every file's name, parent, size and timestamp without touching a
//! single directory. Requires administrator rights; callers fall back to the
//! directory walk on any error.

use crate::scanner::{Cat, Kind, Node, Progress, Tree, CAT_COUNT, FILE_NODE_MIN, NO_PARENT};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::Ordering;

const ROOT_RECORD: usize = 5;
const CHUNK: usize = 8 << 20;

type Res<T> = Result<T, String>;

fn u16le(b: &[u8], o: usize) -> u16 { u16::from_le_bytes([b[o], b[o + 1]]) }
fn u32le(b: &[u8], o: usize) -> u32 { u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) }
fn u64le(b: &[u8], o: usize) -> u64 { u64::from_le_bytes(b[o..o + 8].try_into().unwrap()) }

/// `C:\` style paths only: the MFT always describes a whole volume.
pub fn is_volume_root(p: &Path) -> bool {
    let s = p.to_string_lossy();
    let b = s.as_bytes();
    (b.len() == 3 || b.len() == 2) && b[0].is_ascii_alphabetic() && b[1] == b':' && (b.len() == 2 || b[2] == b'\\' || b[2] == b'/')
}

/// Opening a raw volume only succeeds with administrator rights.
pub fn is_elevated() -> bool {
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    File::open(format!("\\\\.\\{drive}")).is_ok()
}

/// One MFT record, kept small: a volume has millions of them. The name lives in a shared
/// `names` buffer (`name_off..name_off + name_len`) instead of one heap string per record.
struct Rec {
    size: u64,
    parent: u32,
    name_off: u32,
    /// Unix seconds; fits a u32 until 2106.
    modified: u32,
    name_len: u16,
    is_dir: bool,
    used: bool,
}

impl Rec {
    fn name<'a>(&self, names: &'a str) -> &'a str {
        let start = self.name_off as usize;
        &names[start..start + self.name_len as usize]
    }
}

/// Apply the NTFS update-sequence fixups; returns false for a torn/corrupt record.
fn fixup(rec: &mut [u8]) -> bool {
    let off = u16le(rec, 4) as usize;
    let cnt = u16le(rec, 6) as usize;
    if cnt == 0 || off + cnt * 2 > rec.len() {
        return false;
    }
    let usn = [rec[off], rec[off + 1]];
    for i in 1..cnt {
        let end = i * 512 - 2;
        if end + 2 > rec.len() || rec[end] != usn[0] || rec[end + 1] != usn[1] {
            return false;
        }
        rec[end] = rec[off + i * 2];
        rec[end + 1] = rec[off + i * 2 + 1];
    }
    true
}

/// Decode a non-resident attribute's data runs into (absolute LCN, cluster count).
fn data_runs(attr: &[u8]) -> Vec<(i64, u64)> {
    let mut out = Vec::new();
    let mut p = u16le(attr, 0x20) as usize;
    let mut lcn: i64 = 0;
    while p < attr.len() && attr[p] != 0 {
        let h = attr[p];
        let (ln, on) = ((h & 0x0F) as usize, (h >> 4) as usize);
        p += 1;
        if p + ln + on > attr.len() {
            break;
        }
        let mut len: u64 = 0;
        for i in 0..ln {
            len |= (attr[p + i] as u64) << (8 * i);
        }
        p += ln;
        if on == 0 {
            // Sparse run: no clusters on disk
            out.push((-1, len));
            continue;
        }
        let mut off: i64 = 0;
        for i in 0..on {
            off |= (attr[p + i] as i64) << (8 * i);
        }
        // Sign-extend the relative offset
        if attr[p + on - 1] & 0x80 != 0 {
            off |= -1i64 << (8 * on);
        }
        p += on;
        lcn += off;
        out.push((lcn, len));
    }
    out
}

fn filetime_to_unix(ft: u64) -> u64 {
    (ft / 10_000_000).saturating_sub(11_644_473_600)
}

/// Parse one fixed-up record into `recs[idx]`, or add an extension record's data size to its base record.
/// Names are appended to `names`; a better name found later in the same record is appended again.
fn parse_record(rec: &[u8], idx: usize, recs: &mut [Rec], names: &mut String) {
    if &rec[0..4] != b"FILE" {
        return;
    }
    let flags = u16le(rec, 0x16);
    if flags & 0x01 == 0 {
        return;
    }
    let base = (u64le(rec, 0x20) & 0x0000_FFFF_FFFF_FFFF) as usize;
    let mut p = u16le(rec, 0x14) as usize;
    let mut best_ns = u8::MAX;
    let (mut size, mut modified, mut name) = (0u64, None, None);
    while p + 16 <= rec.len() {
        let ty = u32le(rec, p);
        if ty == 0xFFFF_FFFF {
            break;
        }
        let len = u32le(rec, p + 4) as usize;
        if len < 16 || p + len > rec.len() {
            break;
        }
        let a = &rec[p..p + len];
        let non_res = a[8] != 0;
        let name_len = a[9];
        match ty {
            0x10 if !non_res && base == 0 => {
                let vo = u16le(a, 0x14) as usize;
                if vo + 0x10 <= a.len() {
                    modified = Some(filetime_to_unix(u64le(a, vo + 0x08)).min(u32::MAX as u64) as u32);
                }
            }
            0x30 if !non_res && base == 0 => {
                let vo = u16le(a, 0x14) as usize;
                if vo + 66 <= a.len() {
                    let v = &a[vo..];
                    let nlen = v[64] as usize;
                    let ns = v[65];
                    // Namespace 2 is the 8.3 DOS alias; prefer the long name
                    let rank = if ns == 2 { 3 } else { ns };
                    if rank < best_ns && 66 + nlen * 2 <= v.len() && names.len() < u32::MAX as usize - 1024 {
                        best_ns = rank;
                        let start = names.len();
                        let units = (0..nlen).map(|i| u16le(v, 66 + i * 2));
                        names.extend(char::decode_utf16(units).map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER)));
                        let parent = (u64le(v, 0) & 0x0000_FFFF_FFFF_FFFF) as u32;
                        name = Some((start as u32, (names.len() - start) as u16, parent));
                    }
                }
            }
            // Unnamed $DATA stream = the file's content
            0x80 if name_len == 0 => {
                size += if non_res {
                    // Only the first segment (starting VCN 0) carries the real size
                    if u64le(a, 0x10) == 0 { u64le(a, 0x30) } else { 0 }
                } else {
                    u32le(a, 0x10) as u64
                };
            }
            _ => {}
        }
        p += len;
    }
    if base != 0 {
        // Extension record: only its data size matters, and it belongs to the base record
        if let Some(b) = recs.get_mut(base) {
            b.size += size;
        }
        return;
    }
    let r = &mut recs[idx];
    r.size += size;
    if let Some(m) = modified {
        r.modified = m;
    }
    if let Some((off, len, parent)) = name {
        (r.name_off, r.name_len, r.parent) = (off, len, parent);
    }
    r.used = true;
    r.is_dir = flags & 0x02 != 0;
}

pub fn scan(root: &Path, progress: &Progress) -> Res<Tree> {
    let started = std::time::Instant::now();
    let letter = root.to_string_lossy().chars().next().ok_or("bad path")?;
    let mut vol = File::open(format!("\\\\.\\{letter}:")).map_err(|e| format!("open volume: {e}"))?;

    let mut boot = [0u8; 512];
    vol.read_exact(&mut boot).map_err(|e| e.to_string())?;
    if &boot[3..11] != b"NTFS    " {
        return Err("not an NTFS volume".into());
    }
    let bps = u16le(&boot, 0x0B) as u64;
    let spc = boot[0x0D] as u64;
    let cluster = bps * spc;
    let mft_lcn = u64le(&boot, 0x30);
    let cpr = boot[0x40] as i8;
    let rec_size = if cpr < 0 { 1u64 << (-cpr) } else { cpr as u64 * cluster } as usize;
    if cluster == 0 || rec_size < 512 {
        return Err("unexpected NTFS geometry".into());
    }

    // Record 0 is $MFT itself; its $DATA runs say where the whole table lives
    let mut rec0 = vec![0u8; rec_size];
    vol.seek(SeekFrom::Start(mft_lcn * cluster)).map_err(|e| e.to_string())?;
    vol.read_exact(&mut rec0).map_err(|e| e.to_string())?;
    if !fixup(&mut rec0) {
        return Err("corrupt $MFT record".into());
    }
    let (runs, mft_size) = {
        let mut p = u16le(&rec0, 0x14) as usize;
        let mut found = None;
        while p + 16 <= rec0.len() {
            let ty = u32le(&rec0, p);
            let len = u32le(&rec0, p + 4) as usize;
            if ty == 0xFFFF_FFFF || len < 16 || p + len > rec0.len() {
                break;
            }
            if ty == 0x80 && rec0[p + 8] != 0 && rec0[p + 9] == 0 {
                let a = &rec0[p..p + len];
                found = Some((data_runs(a), u64le(a, 0x30)));
                break;
            }
            p += len;
        }
        found.ok_or("no $MFT data attribute")?
    };

    let count = (mft_size / rec_size as u64) as usize;
    let mut recs: Vec<Rec> = (0..count)
        .map(|_| Rec { size: 0, parent: NO_PARENT, name_off: 0, modified: 0, name_len: 0, is_dir: false, used: false })
        .collect();
    // All names back to back; roughly 20 bytes per record on a typical system drive
    let mut names = String::with_capacity(count * 20);

    // Read the MFT run by run in large chunks, parsing records as they stream in
    let mut idx = 0usize;
    let mut buf = vec![0u8; CHUNK];
    'runs: for (lcn, clusters) in runs {
        let mut remaining = clusters * cluster;
        if lcn < 0 {
            idx += (remaining / rec_size as u64) as usize;
            continue;
        }
        vol.seek(SeekFrom::Start(lcn as u64 * cluster)).map_err(|e| e.to_string())?;
        while remaining > 0 {
            if progress.cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            let n = remaining.min(CHUNK as u64) as usize;
            vol.read_exact(&mut buf[..n]).map_err(|e| format!("read MFT: {e}"))?;
            let (mut files, mut dirs, mut bytes) = (0u64, 0u64, 0u64);
            for rec in buf[..n].chunks_exact_mut(rec_size) {
                if idx >= count {
                    break;
                }
                if fixup(rec) {
                    parse_record(rec, idx, &mut recs, &mut names);
                    let r = &recs[idx];
                    if r.used {
                        if r.is_dir { dirs += 1 } else { files += 1; bytes += r.size }
                    }
                }
                idx += 1;
            }
            remaining -= n as u64;
            // Live counters for the scan dialog: real files, folders and bytes, not raw MFT records
            progress.files.fetch_add(files, Ordering::Relaxed);
            progress.dirs.fetch_add(dirs, Ordering::Relaxed);
            progress.bytes.fetch_add(bytes, Ordering::Relaxed);
            if idx >= count {
                break 'runs;
            }
        }
    }
    drop(buf);

    build_tree(root, recs, &names, progress, started)
}

/// Turn the flat record table into the same arena layout the directory walker produces.
fn build_tree(root: &Path, recs: Vec<Rec>, names: &str, progress: &Progress, started: std::time::Instant) -> Res<Tree> {
    if recs.len() <= ROOT_RECORD || !recs[ROOT_RECORD].used {
        return Err("root directory record missing".into());
    }
    // Children of every record as one flat list: kids[start[p]..start[p + 1]] are p's children.
    // One Vec per record would be millions of small allocations.
    let is_child = |i: usize, r: &Rec| {
        i != ROOT_RECORD && r.used && r.name_len > 0 && (r.parent as usize) < recs.len() && recs[r.parent as usize].is_dir
    };
    let mut start = vec![0u32; recs.len() + 1];
    for (i, r) in recs.iter().enumerate() {
        if is_child(i, r) {
            start[r.parent as usize + 1] += 1;
        }
    }
    for i in 1..start.len() {
        start[i] += start[i - 1];
    }
    let mut kids = vec![0u32; start[recs.len()] as usize];
    let mut fill = start.clone();
    for (i, r) in recs.iter().enumerate() {
        if is_child(i, r) {
            let p = r.parent as usize;
            kids[fill[p] as usize] = i as u32;
            fill[p] += 1;
        }
    }
    drop(fill);

    let mut nodes: Vec<Node> = Vec::with_capacity(recs.len() / 3);
    let new_node = |name: String, parent: u32, kind: Kind| Node {
        name, parent, size: 0, files: 0, dirs: 0, kind, children: Vec::new(), cats: [0; CAT_COUNT], modified: 0,
    };
    nodes.push(new_node(root.to_string_lossy().into_owned(), NO_PARENT, Kind::Dir));

    // Depth-first with an explicit stack: a node gets its index when its parent is expanded, so every
    // parent still has a smaller arena index than its children, and only the folders along the
    // current path (plus their pending siblings) hold a path string at any time.
    let mut stack: Vec<(usize, u32, String)> = vec![(ROOT_RECORD, 0, root.to_string_lossy().to_lowercase())];
    let mut visited = vec![false; recs.len()];
    visited[ROOT_RECORD] = true;
    let (mut files, mut bytes) = (0u64, 0u64);
    while let Some((rec_id, node_id, path_lower)) = stack.pop() {
        let mut bucket = new_node(String::new(), node_id, Kind::Bucket);
        for &k in &kids[start[rec_id] as usize..start[rec_id + 1] as usize] {
            let k = k as usize;
            if visited[k] {
                continue;
            }
            visited[k] = true;
            let r = &recs[k];
            let name = r.name(names);
            if r.is_dir {
                let id = nodes.len() as u32;
                let mut n = new_node(name.to_owned(), node_id, Kind::Dir);
                n.modified = r.modified as u64;
                nodes.push(n);
                nodes[node_id as usize].children.push(id);
                stack.push((k, id, format!("{path_lower}\\{}", name.to_lowercase())));
            } else {
                files += 1;
                bytes += r.size;
                let cat = Cat::classify(name, &path_lower) as usize;
                if r.size >= FILE_NODE_MIN {
                    let id = nodes.len() as u32;
                    let mut n = new_node(name.to_owned(), node_id, Kind::File);
                    n.size = r.size;
                    n.files = 1;
                    n.cats[cat] = r.size;
                    n.modified = r.modified as u64;
                    nodes.push(n);
                    nodes[node_id as usize].children.push(id);
                } else {
                    bucket.size += r.size;
                    bucket.files += 1;
                    bucket.cats[cat] += r.size;
                    bucket.modified = bucket.modified.max(r.modified as u64);
                }
            }
        }
        if bucket.files > 0 {
            bucket.name = format!("{} small files", bucket.files);
            let id = nodes.len() as u32;
            nodes.push(bucket);
            nodes[node_id as usize].children.push(id);
        }
    }
    // The record table is the biggest allocation of the scan; free it before the roll-up
    drop((recs, kids, start, visited));
    // Final, reachable-only totals (orphaned records are excluded)
    progress.bytes.store(bytes, Ordering::Relaxed);
    progress.files.store(files, Ordering::Relaxed);
    progress.dirs.store(nodes.iter().filter(|n| n.kind == Kind::Dir).count() as u64, Ordering::Relaxed);

    // Roll sizes up: children always have higher indices than their parent
    for i in (1..nodes.len()).rev() {
        let (size, f, d, cats, m, parent, is_dir) = {
            let n = &nodes[i];
            (n.size, n.files, n.dirs, n.cats, n.modified, n.parent as usize, n.kind == Kind::Dir)
        };
        let p = &mut nodes[parent];
        p.size += size;
        p.files += f;
        p.dirs += d + u64::from(is_dir);
        p.modified = p.modified.max(m);
        for (c, v) in p.cats.iter_mut().zip(cats) {
            *c += v;
        }
    }
    for i in 0..nodes.len() {
        let mut ch = std::mem::take(&mut nodes[i].children);
        ch.sort_by_key(|c| std::cmp::Reverse(nodes[*c as usize].size));
        // Pushing one by one leaves up to half of each list as spare capacity
        ch.shrink_to_fit();
        nodes[i].children = ch;
    }
    nodes.shrink_to_fit();

    Ok(Tree { nodes, root_path: root.to_path_buf(), elapsed_ms: started.elapsed().as_millis() as u64, errors: 0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_data_runs() {
        // Header at 0x20 points to runs: 0x31 len=0x10 off=+0x1000 ; 0x21 len=0x08 off=-0x10 ; end
        let mut a = vec![0u8; 0x40];
        a[0x20] = 0x30;
        a[0x30..0x3A].copy_from_slice(&[0x21, 0x10, 0x00, 0x10, 0x21, 0x08, 0xF0, 0xFF, 0x00, 0x00]);
        assert_eq!(data_runs(&a), vec![(0x1000, 0x10), (0x0FF0, 0x08)]);
    }

    #[test]
    fn volume_root_detection() {
        assert!(is_volume_root(Path::new("C:\\")));
        assert!(is_volume_root(Path::new("d:")));
        assert!(!is_volume_root(Path::new("C:\\Users")));
    }
}
