//! Saved scans: the last tree for each scanned root is kept on disk (LZ4-compressed), so Tessera
//! opens instantly next time instead of rescanning. Changes made while it was closed are caught up
//! afterwards: from the NTFS USN change journal when it can be read (administrator), otherwise by
//! re-reading every folder whose modification time is newer than the save.
//!
//! Format (inside an LZ4 frame, little-endian): header, then reachable nodes in breadth-first order
//! so a parent always precedes its children. Folder totals are not stored; they are rolled up from
//! the leaves on load.

use crate::scanner::{Kind, Node, Tree, CAT_COUNT, NO_PARENT};
use rayon::prelude::*;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"TSC1";
/// Saved scans kept (one per root); the least recently written ones are dropped.
const KEEP: usize = 6;

pub struct Meta {
    pub saved_at: u64,
    /// (journal id, next USN) of the volume when the scan started, for USN catch-up.
    pub journal: Option<(u64, i64)>,
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn dir(base: &Path) -> PathBuf {
    base.join("scans")
}
fn file_for(base: &Path, root: &str) -> PathBuf {
    let h = blake3::hash(root.to_lowercase().as_bytes());
    dir(base).join(format!("{}.tsc", &h.to_hex()[..16]))
}

// ---------- encoding ----------

fn put_str(out: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    let n = b.len().min(u16::MAX as usize);
    out.extend_from_slice(&(n as u16).to_le_bytes());
    out.extend_from_slice(&b[..n]);
}

/// Serialize the reachable tree. Done while the caller holds the tree lock, so it only copies;
/// compression and disk I/O happen in `write` without the lock.
pub fn encode(t: &Tree, meta: &Meta) -> Vec<u8> {
    let mut out = Vec::with_capacity(t.nodes.len() * 24);
    out.extend_from_slice(MAGIC);
    put_str(&mut out, &t.root_path.to_string_lossy());
    for v in [meta.saved_at, t.elapsed_ms, t.errors] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    match meta.journal {
        Some((id, usn)) => {
            out.push(1);
            out.extend_from_slice(&id.to_le_bytes());
            out.extend_from_slice(&usn.to_le_bytes());
        }
        None => out.push(0),
    }
    // Breadth-first over live nodes, remapping ids so the file has no holes from deleted nodes
    let mut order: Vec<u32> = vec![0];
    let mut new_id = vec![u32::MAX; t.nodes.len()];
    new_id[0] = 0;
    let mut i = 0;
    while i < order.len() {
        let id = order[i];
        for c in &t.nodes[id as usize].children {
            new_id[*c as usize] = order.len() as u32;
            order.push(*c);
        }
        i += 1;
    }
    out.extend_from_slice(&(order.len() as u32).to_le_bytes());
    for id in order {
        let n = &t.nodes[id as usize];
        let parent = if id == 0 { NO_PARENT } else { new_id[n.parent as usize] };
        out.extend_from_slice(&parent.to_le_bytes());
        out.push(match n.kind {
            Kind::Dir => 0,
            Kind::File => 1,
            Kind::Bucket => 2,
        });
        put_str(&mut out, &n.name);
        if n.kind == Kind::Dir {
            continue;
        }
        out.extend_from_slice(&n.size.to_le_bytes());
        out.extend_from_slice(&n.modified.to_le_bytes());
        out.extend_from_slice(&n.files.to_le_bytes());
        // Leaves usually have one or two categories: store only the non-zero ones
        let cats: Vec<(usize, u64)> = n.cats.iter().copied().enumerate().filter(|(_, v)| *v > 0).collect();
        out.push(cats.len() as u8);
        for (c, v) in cats {
            out.push(c as u8);
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out
}

pub fn write(base: &Path, root: &str, raw: &[u8]) -> io::Result<()> {
    std::fs::create_dir_all(dir(base))?;
    let path = file_for(base, root);
    let tmp = path.with_extension("tmp");
    {
        let f = BufWriter::new(std::fs::File::create(&tmp)?);
        let mut enc = lz4_flex::frame::FrameEncoder::new(f);
        enc.write_all(raw)?;
        enc.finish().map_err(io::Error::other)?.flush()?;
    }
    std::fs::rename(&tmp, &path)?;
    prune(base);
    Ok(())
}

fn prune(base: &Path) {
    let Ok(rd) = std::fs::read_dir(dir(base)) else { return };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = rd
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "tsc"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    for (_, p) in files.into_iter().skip(KEEP) {
        let _ = std::fs::remove_file(p);
    }
}

// ---------- decoding ----------

struct Cur<'a> {
    b: &'a [u8],
    o: usize,
}
impl Cur<'_> {
    fn take(&mut self, n: usize) -> io::Result<&[u8]> {
        let s = self.b.get(self.o..self.o + n).ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "truncated scan cache"))?;
        self.o += n;
        Ok(s)
    }
    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> io::Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn str(&mut self) -> io::Result<String> {
        let n = self.u16()? as usize;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
}

pub fn load(base: &Path, root: &str) -> io::Result<(Tree, Meta)> {
    let f = std::fs::File::open(file_for(base, root))?;
    let mut raw = Vec::new();
    lz4_flex::frame::FrameDecoder::new(BufReader::new(f)).read_to_end(&mut raw)?;
    decode(&raw)
}

fn decode(raw: &[u8]) -> io::Result<(Tree, Meta)> {
    let mut c = Cur { b: raw, o: 0 };
    if c.take(4)? != MAGIC {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "not a Tessera scan cache"));
    }
    let root = c.str()?;
    let (saved_at, elapsed_ms, errors) = (c.u64()?, c.u64()?, c.u64()?);
    let journal = if c.u8()? == 1 { Some((c.u64()?, c.u64()? as i64)) } else { None };
    let count = c.u32()? as usize;
    let mut nodes: Vec<Node> = Vec::with_capacity(count);
    for i in 0..count {
        let parent = c.u32()?;
        if i > 0 && parent as usize >= i {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "scan cache is out of order"));
        }
        let kind = match c.u8()? {
            0 => Kind::Dir,
            1 => Kind::File,
            _ => Kind::Bucket,
        };
        let name = c.str()?;
        let mut n = Node { name, parent, size: 0, files: 0, dirs: 0, kind, children: Vec::new(), cats: [0; CAT_COUNT], modified: 0 };
        if kind != Kind::Dir {
            n.size = c.u64()?;
            n.modified = c.u64()?;
            n.files = c.u64()?;
            for _ in 0..c.u8()? {
                let idx = c.u8()? as usize;
                let v = c.u64()?;
                if idx < CAT_COUNT {
                    n.cats[idx] = v;
                }
            }
        }
        nodes.push(n);
    }
    // Children lists and folder totals: every node comes after its parent, so one reverse pass rolls up
    for i in (1..nodes.len()).rev() {
        let (size, files, dirs, cats, modified, is_dir, parent) = {
            let n = &nodes[i];
            (n.size, n.files, n.dirs, n.cats, n.modified, n.kind == Kind::Dir, n.parent as usize)
        };
        let p = &mut nodes[parent];
        p.children.push(i as u32);
        p.size += size;
        p.files += files;
        p.dirs += dirs + u64::from(is_dir);
        p.modified = p.modified.max(modified);
        for (t, v) in p.cats.iter_mut().zip(cats) {
            *t += v;
        }
    }
    let sizes: Vec<u64> = nodes.iter().map(|n| n.size).collect();
    for n in nodes.iter_mut() {
        n.children.sort_by_key(|c| std::cmp::Reverse(sizes[*c as usize]));
    }
    Ok((Tree { nodes, root_path: PathBuf::from(root), elapsed_ms, errors }, Meta { saved_at, journal }))
}

// ---------- catch-up ----------

/// Every live folder with its path. Cheap enough to run under the tree lock; the slow part
/// (`changed_by_mtime`) then runs without it.
pub fn dir_paths(t: &Tree) -> Vec<(u32, PathBuf)> {
    (0..t.nodes.len() as u32).filter(|i| t.nodes[*i as usize].kind == Kind::Dir && t.is_live(*i)).map(|i| (i, t.path_of(i))).collect()
}

/// Folders whose own modification time is newer than `since` (an entry was added, removed or
/// renamed in them), plus folders that no longer exist. Stats folders only, in parallel.
pub fn changed_by_mtime(dirs: Vec<(u32, PathBuf)>, since: u64) -> Vec<u32> {
    dirs.into_par_iter()
        .filter(|(_, path)| match std::fs::metadata(path) {
            Ok(md) => md.modified().ok().and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()).is_none_or(|d| d.as_secs() >= since),
            Err(_) => true,
        })
        .map(|(id, _)| id)
        .collect()
}

#[cfg(windows)]
pub mod usn {
    //! Minimal NTFS USN journal reader: which folders had entries change since a given USN.
    use std::path::{Path, PathBuf};
    use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::*;
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::{FSCTL_QUERY_USN_JOURNAL, FSCTL_READ_USN_JOURNAL, READ_USN_JOURNAL_DATA_V0, USN_JOURNAL_DATA_V0};

    /// Beyond this many changed folders a targeted catch-up isn't worth it; callers fall back.
    const MAX_DIRS: usize = 50_000;

    struct H(HANDLE);
    impl Drop for H {
        fn drop(&mut self) {
            // SAFETY: the handle came from CreateFileW/OpenFileById and is closed exactly once.
            unsafe { CloseHandle(self.0) };
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn open_volume(root: &Path) -> Option<H> {
        let s = root.to_string_lossy();
        let letter = s.chars().next().filter(|c| c.is_ascii_alphabetic())?;
        let name = wide(&format!("\\\\.\\{letter}:"));
        // SAFETY: valid NUL-terminated path; the returned handle is checked and owned by `H`.
        let h = unsafe { CreateFileW(name.as_ptr(), GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE, std::ptr::null(), OPEN_EXISTING, 0, std::ptr::null_mut()) };
        (h != INVALID_HANDLE_VALUE).then_some(H(h))
    }

    fn query_on(vol: &H) -> Option<USN_JOURNAL_DATA_V0> {
        // SAFETY: an all-zero USN_JOURNAL_DATA_V0 is a valid output buffer of the right size.
        let mut data: USN_JOURNAL_DATA_V0 = unsafe { std::mem::zeroed() };
        let mut ret = 0u32;
        // SAFETY: output buffer pointer/size describe `data`; no input buffer.
        let ok = unsafe {
            DeviceIoControl(vol.0, FSCTL_QUERY_USN_JOURNAL, std::ptr::null(), 0, (&mut data as *mut USN_JOURNAL_DATA_V0).cast(), size_of::<USN_JOURNAL_DATA_V0>() as u32, &mut ret, std::ptr::null_mut())
        };
        (ok != 0).then_some(data)
    }

    /// Current (journal id, next USN) of the volume holding `root`; needs administrator rights.
    pub fn position(root: &Path) -> Option<(u64, i64)> {
        let vol = open_volume(root)?;
        let d = query_on(&vol)?;
        Some((d.UsnJournalID, d.NextUsn))
    }

    /// Full paths of folders whose entries changed since `from`. `None` when the journal was
    /// recreated or has wrapped past `from`, or too much changed for a targeted catch-up.
    pub fn changed_dirs(root: &Path, journal: u64, from: i64) -> Option<Vec<PathBuf>> {
        let vol = open_volume(root)?;
        let d = query_on(&vol)?;
        if d.UsnJournalID != journal || from < d.FirstUsn {
            return None;
        }
        let mut parents = std::collections::HashSet::<u64>::new();
        let mut buf = vec![0u8; 1 << 20];
        let mut start = from;
        while start < d.NextUsn {
            let req = READ_USN_JOURNAL_DATA_V0 { StartUsn: start, ReasonMask: u32::MAX, ReturnOnlyOnClose: 0, Timeout: 0, BytesToWaitFor: 0, UsnJournalID: journal };
            let mut ret = 0u32;
            // SAFETY: input/output pointers and sizes describe `req` and `buf`.
            let ok = unsafe {
                DeviceIoControl(vol.0, FSCTL_READ_USN_JOURNAL, (&req as *const READ_USN_JOURNAL_DATA_V0).cast(), size_of::<READ_USN_JOURNAL_DATA_V0>() as u32, buf.as_mut_ptr().cast(), buf.len() as u32, &mut ret, std::ptr::null_mut())
            };
            if ok == 0 || ret < 8 {
                return None;
            }
            let next = i64::from_le_bytes(buf[..8].try_into().unwrap());
            let mut o = 8usize;
            while o + 8 <= ret as usize {
                let len = u32::from_le_bytes(buf[o..o + 4].try_into().unwrap()) as usize;
                let major = u16::from_le_bytes(buf[o + 4..o + 6].try_into().unwrap());
                if len == 0 || o + len > ret as usize {
                    break;
                }
                // USN_RECORD_V2: 64-bit parent reference at +16; V3: 128-bit at +24 (low half is the NTFS reference)
                let at = if major >= 3 { o + 24 } else { o + 16 };
                if at + 8 <= o + len {
                    parents.insert(u64::from_le_bytes(buf[at..at + 8].try_into().unwrap()));
                }
                o += len;
            }
            if parents.len() > MAX_DIRS {
                return None;
            }
            if next <= start {
                break;
            }
            start = next;
        }
        Some(parents.into_iter().filter_map(|frn| path_of_id(&vol, frn)).collect())
    }

    /// Resolve an NTFS file reference to a `C:\...` path; deleted folders simply don't resolve.
    fn path_of_id(vol: &H, frn: u64) -> Option<PathBuf> {
        let desc = FILE_ID_DESCRIPTOR { dwSize: size_of::<FILE_ID_DESCRIPTOR>() as u32, Type: FileIdType, Anonymous: FILE_ID_DESCRIPTOR_0 { FileId: frn as i64 } };
        // SAFETY: `desc` is a valid descriptor; zero access rights are enough to query the name.
        let h = unsafe { OpenFileById(vol.0, &desc, 0, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, std::ptr::null(), FILE_FLAG_BACKUP_SEMANTICS) };
        if h == INVALID_HANDLE_VALUE {
            return None;
        }
        let h = H(h);
        let mut buf = vec![0u16; 1024];
        // SAFETY: buffer pointer and length match `buf`.
        let n = unsafe { GetFinalPathNameByHandleW(h.0, buf.as_mut_ptr(), buf.len() as u32, FILE_NAME_NORMALIZED | VOLUME_NAME_DOS) } as usize;
        if n == 0 || n >= buf.len() {
            return None;
        }
        let s = String::from_utf16_lossy(&buf[..n]);
        Some(PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{scan, Progress};

    #[test]
    fn round_trips_a_scan() {
        let d = std::env::temp_dir().join(format!("tessera-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("data/sub")).unwrap();
        std::fs::write(d.join("data/sub/big.iso"), vec![1u8; (crate::scanner::FILE_NODE_MIN + 7) as usize]).unwrap();
        std::fs::write(d.join("data/a.txt"), b"hello").unwrap();
        let mut t = scan(&d.join("data"), &Progress::new());
        // A removed node must not end up in the file
        let sub = t.nodes[0].children.iter().copied().find(|c| t.nodes[*c as usize].name == "sub").unwrap();
        let before = (t.nodes[0].size, t.nodes[0].files);
        let base = d.join("app");
        let raw = encode(&t, &Meta { saved_at: 42, journal: Some((7, 99)) });
        write(&base, &t.root_path.to_string_lossy(), &raw).unwrap();
        let (back, meta) = load(&base, &t.root_path.to_string_lossy()).unwrap();
        assert_eq!((back.nodes[0].size, back.nodes[0].files), before);
        assert_eq!(meta.saved_at, 42);
        assert_eq!(meta.journal, Some((7, 99)));
        assert_eq!(back.nodes.len(), t.nodes.len());
        assert!(back.path_of(back.find_dir(&d.join("data").join("sub"))).ends_with("sub"));

        t.remove(sub);
        let raw = encode(&t, &Meta { saved_at: 1, journal: None });
        let (back, _) = decode(&raw).unwrap();
        assert_eq!(back.nodes[0].size, 5);
        assert!(back.nodes.len() < t.nodes.len());
        std::fs::remove_dir_all(&d).unwrap();
    }
}
