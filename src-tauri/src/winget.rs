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

#[cfg(test)]
mod tests {
    use super::*;

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
