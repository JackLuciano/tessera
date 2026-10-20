//! Software updater (Windows) on top of winget, the Windows Package Manager. winget has no JSON
//! output for `upgrade`, so its table is parsed by column position (taken from the header line
//! above the `-----` rule, which works whatever language the column titles are in).

use serde::Serialize;

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Upgrade {
    pub name: String,
    pub id: String,
    pub version: String,
    pub available: String,
    pub source: String,
}

pub fn available() -> bool {
    crate::sysutil::run("winget", &["--version"]).is_ok()
}

pub fn list() -> Result<Vec<Upgrade>, String> {
    let out = crate::sysutil::command("winget")
        .args(["upgrade", "--include-unknown", "--accept-source-agreements", "--disable-interactivity"])
        .output()
        .map_err(|e| format!("winget: {e}"))?;
    // A non-zero exit also just means "nothing to upgrade", so the table decides
    Ok(parse(&crate::sysutil::text(&out.stdout)))
}

/// Parse `winget upgrade` output.
pub fn parse(text: &str) -> Vec<Upgrade> {
    // Progress spinners are redrawn with \r on the same line: keep only what was drawn last
    let lines: Vec<Vec<char>> = text.lines().map(|l| l.rsplit('\r').next().unwrap_or("").chars().collect()).collect();
    let Some(rule) = lines.iter().position(|l| l.len() > 20 && l.iter().all(|c| *c == '-')) else { return Vec::new() };
    let Some(header) = rule.checked_sub(1).map(|i| &lines[i]) else { return Vec::new() };
    // Column starts: every place a word begins in the header
    let starts: Vec<usize> = (0..header.len()).filter(|i| header[*i] != ' ' && (*i == 0 || header[i - 1] == ' ')).collect();
    if starts.len() < 4 {
        return Vec::new();
    }
    let cell = |l: &[char], c: usize| -> String {
        let from = starts[c].min(l.len());
        let to = starts.get(c + 1).copied().unwrap_or(l.len()).min(l.len());
        l[from..to].iter().collect::<String>().trim().to_string()
    };
    let mut out = Vec::new();
    for l in &lines[rule + 1..] {
        if l.iter().all(|c| c.is_whitespace()) || l.len() < starts[3] {
            break;
        }
        let id = cell(l, 1);
        if id.is_empty() || id.contains(' ') {
            break;
        }
        out.push(Upgrade {
            name: cell(l, 0),
            id,
            version: cell(l, 2),
            available: cell(l, 3),
            source: if starts.len() > 4 { cell(l, 4) } else { String::new() },
        });
    }
    out
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() < 200 && id.chars().all(|c| c.is_ascii_alphanumeric() || "._-+".contains(c))
}

/// Upgrade one package silently. Installers that need administrator rights show the UAC prompt.
pub fn upgrade(id: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err("invalid package id".into());
    }
    let out = crate::sysutil::command("winget")
        .args(["upgrade", "--id", id, "--exact", "--silent", "--accept-package-agreements", "--accept-source-agreements", "--disable-interactivity"])
        .output()
        .map_err(|e| format!("winget: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let text = crate::sysutil::text(&out.stdout);
    // The last meaningful line is winget's own error message
    let last = text.lines().map(|l| l.rsplit('\r').next().unwrap_or("").trim()).rfind(|l| l.len() > 3 && !l.chars().all(|c| "-\\|/ █▒".contains(c))).unwrap_or("");
    Err(format!("{id}: {} (exit {:#x})", if last.is_empty() { "upgrade failed" } else { last }, out.status.code().unwrap_or(-1) as u32))
}

// ---------- update history and rollback ----------

/// One update done through Tessera, kept so it can be undone.
#[derive(Serialize, serde::Deserialize, Clone, Debug)]
pub struct Done {
    pub id: String,
    pub name: String,
    pub from: String,
    pub to: String,
    /// Unix seconds.
    pub at: u64,
    /// Set once the previous version was put back.
    #[serde(default)]
    pub rolled_back: bool,
}

fn history_path(base: &std::path::Path) -> std::path::PathBuf {
    base.join("update-history.json")
}

pub fn history(base: &std::path::Path) -> Vec<Done> {
    std::fs::read(history_path(base)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_history(base: &std::path::Path, all: &[Done]) {
    let _ = std::fs::write(history_path(base), serde_json::to_vec(all).unwrap_or_default());
}

pub fn remember(base: &std::path::Path, d: Done) {
    let mut all = history(base);
    all.insert(0, d);
    all.truncate(200);
    save_history(base, &all);
}

/// Versions winget can still install for a package, newest first.
pub fn versions(id: &str) -> Result<Vec<String>, String> {
    if !valid_id(id) {
        return Err("invalid package id".into());
    }
    let out = crate::sysutil::command("winget")
        .args(["show", "--id", id, "--exact", "--versions", "--accept-source-agreements", "--disable-interactivity"])
        .output()
        .map_err(|e| format!("winget: {e}"))?;
    Ok(parse_versions(&crate::sysutil::text(&out.stdout)))
}

/// `winget show --versions`: a "Version" title, a rule of dashes, then one version per line.
pub fn parse_versions(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().map(|l| l.rsplit('\r').next().unwrap_or("").trim()).collect();
    let Some(rule) = lines.iter().position(|l| l.len() >= 3 && l.chars().all(|c| c == '-')) else { return Vec::new() };
    lines[rule + 1..].iter().take_while(|l| !l.is_empty() && !l.contains(' ')).map(|l| l.to_string()).collect()
}

/// Put an earlier version back. The installer runs over the newer one, like a normal install.
pub fn install_version(id: &str, version: &str) -> Result<(), String> {
    if !valid_id(id) || version.is_empty() || version.len() > 64 || !version.chars().all(|c| c.is_ascii_alphanumeric() || "._-+".contains(c)) {
        return Err("invalid package id or version".into());
    }
    let out = crate::sysutil::command("winget")
        .args(["install", "--id", id, "--exact", "--version", version, "--force", "--silent", "--accept-package-agreements", "--accept-source-agreements", "--disable-interactivity"])
        .output()
        .map_err(|e| format!("winget: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let text = crate::sysutil::text(&out.stdout);
    let last = text.lines().map(|l| l.rsplit('\r').next().unwrap_or("").trim()).rfind(|l| l.len() > 3 && !l.chars().all(|c| "-\\|/ █▒".contains(c))).unwrap_or("");
    Err(format!("{id} {version}: {}", if last.is_empty() { "install failed" } else { last }))
}

/// Roll the update recorded at `at` back to its previous version, optionally pinning the package
/// so the next "update all" skips it.
pub fn rollback(base: &std::path::Path, at: u64, pin: bool) -> Result<(), String> {
    let mut all = history(base);
    let Some(d) = all.iter_mut().find(|d| d.at == at) else { return Err("update not found".into()) };
    // winget lists "< 1.2" or "Unknown" when it couldn't read the installed version
    let from = d.from.trim_start_matches(['<', '>', ' ']).to_string();
    if !from.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return Err("the previous version is not known".into());
    }
    install_version(&d.id, &from)?;
    if pin {
        let _ = set_pin(&d.id, true);
    }
    d.rolled_back = true;
    save_history(base, &all);
    Ok(())
}

/// Pinned packages are left out of "upgrade all" (they can still be updated one by one).
pub fn set_pin(id: &str, on: bool) -> Result<(), String> {
    if !valid_id(id) {
        return Err("invalid package id".into());
    }
    let args: &[&str] = if on { &["pin", "add", "--id", id, "--exact", "--force", "--accept-source-agreements", "--disable-interactivity"] } else { &["pin", "remove", "--id", id, "--exact", "--accept-source-agreements", "--disable-interactivity"] };
    let out = crate::sysutil::command("winget").args(args).output().map_err(|e| format!("winget: {e}"))?;
    if out.status.success() { Ok(()) } else { Err(format!("winget pin failed (exit {:#x})", out.status.code().unwrap_or(-1) as u32)) }
}

/// Ids of pinned packages.
pub fn pins() -> Vec<String> {
    let Ok(out) = crate::sysutil::command("winget").args(["pin", "list", "--accept-source-agreements", "--disable-interactivity"]).output() else { return Vec::new() };
    parse(&crate::sysutil::text(&out.stdout)).into_iter().map(|u| u.id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_version_lists() {
        let text = "Found Git [Git.Git]\r\nVersion\r\n--------\r\n2.55.0.5\r\n2.54.0\r\n2.53.1\r\n";
        assert_eq!(parse_versions(text), vec!["2.55.0.5", "2.54.0", "2.53.1"]);
        assert!(parse_versions("No package found matching input criteria.").is_empty());
    }

    const SAMPLE: &str = "   - \r   \\ \rName                         Id                         Version      Available    Source\n\
---------------------------------------------------------------------------------------------\n\
Git                          Git.Git                    2.54.0       2.55.0.5     winget\n\
Windows terminál             Microsoft.WindowsTerminal  1.24.12741.0 1.25.2733.0  winget\n\
Python Launcher              Python.Launcher            < 3.14.7     3.14.7       winget\n\
3 upgrades available.\n";

    #[test]
    fn parses_upgrade_tables() {
        let u = parse(SAMPLE);
        assert_eq!(u.len(), 3);
        assert_eq!(u[0], Upgrade { name: "Git".into(), id: "Git.Git".into(), version: "2.54.0".into(), available: "2.55.0.5".into(), source: "winget".into() });
        assert_eq!(u[1].name, "Windows terminál");
        assert_eq!(u[1].id, "Microsoft.WindowsTerminal");
        assert_eq!(u[2].version, "< 3.14.7");
        assert!(parse("No installed package found matching input criteria.").is_empty());
    }

    #[test]
    fn validates_ids() {
        assert!(valid_id("Microsoft.VisualStudioCode"));
        assert!(!valid_id("x; rm -rf"));
        assert!(!valid_id(""));
    }
}
