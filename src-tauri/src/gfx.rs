//! Per-game graphics presets (Windows), through the game's own settings file only, never the
//! driver. Unreal Engine games keep their quality levels in
//! `%LOCALAPPDATA%\<Project>\Saved\Config\Windows*\GameUserSettings.ini` under
//! `[ScalabilityGroups]` (0 low … 3 epic, 4 cinematic), plus V-Sync and the frame cap. Every
//! change backs the file up first and can be restored with one click.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The quality groups shown, in order. ResolutionQuality is a percentage, the rest 0–4.
pub const GROUPS: [&str; 11] = ["ViewDistance", "AntiAliasing", "Shadow", "GlobalIllumination", "Reflection", "PostProcess", "Texture", "Effects", "Foliage", "Shading", "Landscape"];

#[derive(Serialize, Clone, Debug)]
pub struct GameConfig {
    /// Unreal project folder name, e.g. FortniteGame.
    pub project: String,
    /// Library name when the game was recognised, otherwise the project name.
    pub name: String,
    pub path: String,
    /// sg.<Group>Quality values found in the file.
    pub groups: BTreeMap<String, i32>,
    pub resolution: Option<i32>,
    pub vsync: Option<bool>,
    pub frame_limit: Option<f32>,
    pub modified: u64,
    pub backups: Vec<Backup>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Backup {
    pub file: String,
    /// Unix seconds.
    pub at: u64,
}

/// What the UI sends to save: groups to set, and optionally V-Sync / frame cap.
#[derive(Deserialize, Clone, Debug, Default)]
pub struct Change {
    pub groups: BTreeMap<String, i32>,
    pub resolution: Option<i32>,
    pub vsync: Option<bool>,
    pub frame_limit: Option<f32>,
}

const SKIP: &[&str] = &["epicgameslauncher", "unrealengine", "unrealenginelauncher", "crashreportclient"];

fn local() -> Option<PathBuf> {
    crate::sysutil::env_path("LOCALAPPDATA")
}

/// GameUserSettings.ini files with quality groups, one level of per-account folders deep.
fn find_files() -> Vec<(String, PathBuf)> {
    let Some(local) = local() else { return Vec::new() };
    let mut out = Vec::new();
    for project in std::fs::read_dir(&local).into_iter().flatten().flatten() {
        let name = project.file_name().to_string_lossy().into_owned();
        if SKIP.contains(&name.to_lowercase().as_str()) {
            continue;
        }
        let config = project.path().join(r"Saved\Config");
        if !config.is_dir() {
            continue;
        }
        let mut dirs = vec![config.clone()];
        dirs.extend(std::fs::read_dir(&config).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir() && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("Windows"))));
        for d in dirs {
            for platform in ["WindowsClient", "WindowsNoEditor", "Windows"] {
                let f = d.join(platform).join("GameUserSettings.ini");
                if std::fs::read_to_string(&f).is_ok_and(|t| t.contains("[ScalabilityGroups]")) {
                    out.push((name.clone(), f));
                }
            }
        }
    }
    out
}

fn backups_dir(base: &Path) -> PathBuf {
    base.join("gfx-backups")
}

/// Backup files are named <project>-<hash of the path>-<unix time>.ini.
fn backup_prefix(project: &str, path: &Path) -> String {
    let hash = blake3::hash(path.to_string_lossy().to_lowercase().as_bytes()).to_hex();
    format!("{project}-{}-", &hash[..8])
}

fn backups(base: &Path, project: &str, path: &Path) -> Vec<Backup> {
    let prefix = backup_prefix(project, path);
    let mut out: Vec<Backup> = std::fs::read_dir(backups_dir(base))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            let at = n.strip_prefix(&prefix)?.strip_suffix(".ini")?.parse().ok()?;
            Some(Backup { file: n, at })
        })
        .collect();
    out.sort_by_key(|b| std::cmp::Reverse(b.at));
    out
}

/// Read `key=value` lines anywhere in the file (games put V-Sync in their own section).
pub fn read_values(text: &str) -> (BTreeMap<String, i32>, Option<i32>, Option<bool>, Option<f32>) {
    let mut groups = BTreeMap::new();
    let (mut res, mut vsync, mut limit) = (None, None, None);
    for line in text.lines().map(str::trim) {
        let Some((k, v)) = line.split_once('=') else { continue };
        if let Some(g) = k.strip_prefix("sg.").and_then(|g| g.strip_suffix("Quality")) {
            if g == "Resolution" {
                res = v.trim().parse().ok();
            } else if let Ok(n) = v.trim().parse() {
                groups.insert(g.to_string(), n);
            }
        } else if k == "bUseVSync" {
            vsync = Some(v.trim().eq_ignore_ascii_case("true"));
        } else if k == "FrameRateLimit" {
            limit = v.trim().parse().ok();
        }
    }
    (groups, res, vsync, limit)
}

/// Write the change into the file text, keeping everything else as it was. Quality groups the file
/// doesn't have yet are added to [ScalabilityGroups]; V-Sync and the cap only where they exist.
pub fn apply(text: &str, c: &Change) -> String {
    let mut done: Vec<String> = Vec::new();
    let mut out: Vec<String> = Vec::new();
    let mut in_groups = false;
    let crlf = text.contains("\r\n");
    let flush_missing = |out: &mut Vec<String>, done: &[String]| {
        for (g, v) in c.groups.iter().filter(|(g, _)| !done.contains(g)) {
            out.push(format!("sg.{g}Quality={v}"));
        }
    };
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            if in_groups {
                flush_missing(&mut out, &done);
                done.extend(c.groups.keys().cloned());
            }
            in_groups = t == "[ScalabilityGroups]";
        }
        if let Some((k, _)) = t.split_once('=') {
            if let Some(g) = k.strip_prefix("sg.").and_then(|g| g.strip_suffix("Quality")) {
                if g == "Resolution"
                    && let Some(r) = c.resolution
                {
                    out.push(format!("sg.ResolutionQuality={r}"));
                    continue;
                }
                if let Some(v) = c.groups.get(g) {
                    out.push(format!("sg.{g}Quality={v}"));
                    done.push(g.to_string());
                    continue;
                }
            } else if k == "bUseVSync"
                && let Some(v) = c.vsync
            {
                out.push(format!("bUseVSync={}", if v { "True" } else { "False" }));
                continue;
            } else if k == "FrameRateLimit"
                && let Some(v) = c.frame_limit
            {
                out.push(format!("FrameRateLimit={v:.6}"));
                continue;
            }
        }
        out.push(line.to_string());
    }
    if in_groups {
        flush_missing(&mut out, &done);
    }
    let mut s = out.join(if crlf { "\r\n" } else { "\n" });
    if text.ends_with('\n') {
        s.push_str(if crlf { "\r\n" } else { "\n" });
    }
    s
}

pub fn list(base: &Path, library: &[crate::games::Game]) -> Vec<GameConfig> {
    find_files()
        .into_iter()
        .filter_map(|(project, path)| {
            let text = std::fs::read_to_string(&path).ok()?;
            let (groups, resolution, vsync, frame_limit) = read_values(&text);
            // The install folder of an Unreal game contains a folder named after the project
            let name = library.iter().find(|g| !g.dir.is_empty() && Path::new(&g.dir).join(&project).is_dir()).map(|g| g.name.clone()).unwrap_or_else(|| project.clone());
            let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
            Some(GameConfig { backups: backups(base, &project, &path), project, name, path: path.to_string_lossy().into_owned(), groups, resolution, vsync, frame_limit, modified })
        })
        .collect()
}

fn known_file(path: &str) -> Option<(String, PathBuf)> {
    find_files().into_iter().find(|(_, p)| p.to_string_lossy().eq_ignore_ascii_case(path))
}

/// Back the file up, then write the change.
pub fn save(base: &Path, path: &str, c: &Change) -> Result<(), String> {
    let (project, p) = known_file(path).ok_or("not a game settings file")?;
    if c.groups.keys().any(|g| !GROUPS.contains(&g.as_str())) || c.groups.values().any(|v| !(0..=4).contains(v)) || c.resolution.is_some_and(|r| !(25..=200).contains(&r)) {
        return Err("invalid value".into());
    }
    let text = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(backups_dir(base)).map_err(|e| e.to_string())?;
    let backup = backups_dir(base).join(format!("{}{}.ini", backup_prefix(&project, &p), crate::sysutil::now()));
    std::fs::write(&backup, &text).map_err(|e| e.to_string())?;
    // Some games mark the file read-only to stop it being overwritten; respect that only for us
    let mut perms = std::fs::metadata(&p).map_err(|e| e.to_string())?.permissions();
    let was_ro = perms.readonly();
    if was_ro {
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&p, perms.clone()).map_err(|e| e.to_string())?;
    }
    let r = std::fs::write(&p, apply(&text, c)).map_err(|e| e.to_string());
    if was_ro {
        perms.set_readonly(true);
        let _ = std::fs::set_permissions(&p, perms);
    }
    r
}

/// Put a backup back (the current file is backed up first, so this can be undone too).
pub fn restore(base: &Path, path: &str, file: &str) -> Result<(), String> {
    let (project, p) = known_file(path).ok_or("not a game settings file")?;
    if !file.starts_with(&backup_prefix(&project, &p)) || file.contains(['/', '\\']) {
        return Err("backup does not belong to this game".into());
    }
    let old = std::fs::read_to_string(backups_dir(base).join(file)).map_err(|e| e.to_string())?;
    let now = std::fs::read_to_string(&p).unwrap_or_default();
    let _ = std::fs::write(backups_dir(base).join(format!("{}{}.ini", backup_prefix(&project, &p), crate::sysutil::now())), now);
    std::fs::write(&p, old).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const INI: &str = "[ScalabilityGroups]\r\nsg.ResolutionQuality=100\r\nsg.ShadowQuality=3\r\nsg.TextureQuality=3\r\n\r\n[/Script/Engine.GameUserSettings]\r\nbUseVSync=True\r\nFrameRateLimit=0.000000\r\nLastConfirmedFullscreenMode=1\r\n";

    #[test]
    fn reads_and_writes_quality_groups() {
        let (g, res, vsync, limit) = read_values(INI);
        assert_eq!((g.get("Shadow"), res, vsync, limit), (Some(&3), Some(100), Some(true), Some(0.0)));
        let mut c = Change { vsync: Some(false), frame_limit: Some(141.0), ..Default::default() };
        c.groups.insert("Shadow".into(), 0);
        c.groups.insert("Effects".into(), 1);
        let out = apply(INI, &c);
        assert!(out.contains("sg.ShadowQuality=0\r\n"));
        assert!(out.contains("sg.TextureQuality=3"), "untouched groups stay");
        assert!(out.contains("sg.EffectsQuality=1"), "missing groups are added to their section");
        assert!(out.contains("bUseVSync=False") && out.contains("FrameRateLimit=141.000000"));
        assert!(out.contains("LastConfirmedFullscreenMode=1\r\n"), "other lines and line endings are kept");
        let effects = out.find("sg.EffectsQuality").unwrap();
        assert!(effects < out.find("[/Script").unwrap(), "added inside [ScalabilityGroups]");
    }
}
