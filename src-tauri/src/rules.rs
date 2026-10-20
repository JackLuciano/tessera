//! Custom junk-cleaning rules: the user's own additions to Smart cleanup. A rule matches folders
//! in the scanned tree by name or by full path (`*` and `?` wildcards, case-insensitive),
//! optionally only when nothing in them changed for some days. "Safe" rules are picked by default
//! in the cleanup list; the others are shown for review.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Rule {
    pub id: String,
    pub name: String,
    /// "name": the folder's own name; "path": its full path.
    pub kind: String,
    pub pattern: String,
    /// Only folders not modified for this many days (0 = any age).
    pub min_age_days: u32,
    pub safe: bool,
    pub enabled: bool,
}

impl Default for Rule {
    fn default() -> Self {
        Self { id: String::new(), name: String::new(), kind: "name".into(), pattern: String::new(), min_age_days: 0, safe: false, enabled: true }
    }
}

#[derive(Serialize, Deserialize, Default)]
struct File {
    rules: Vec<Rule>,
}

pub const FILE: &str = "junk-rules.json";

fn path(base: &Path) -> PathBuf {
    base.join(FILE)
}

pub fn load(base: &Path) -> Vec<Rule> {
    std::fs::read(path(base)).ok().and_then(|b| serde_json::from_slice::<File>(&b).ok()).map(|f| f.rules).unwrap_or_default()
}

pub fn save(base: &Path, rules: &[Rule]) -> Result<(), String> {
    for r in rules {
        validate(r)?;
    }
    std::fs::create_dir_all(base).map_err(|e| e.to_string())?;
    std::fs::write(path(base), serde_json::to_vec_pretty(&File { rules: rules.to_vec() }).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// A rule must not be able to match everything (or whole drives).
pub fn validate(r: &Rule) -> Result<(), String> {
    let p = r.pattern.trim();
    let literal: String = p.chars().filter(|c| !matches!(c, '*' | '?' | '\\' | '/' | ':' | '.')).collect();
    if literal.len() < 2 {
        return Err(format!("\"{p}\" is too broad"));
    }
    if r.kind != "name" && r.kind != "path" {
        return Err("unknown rule kind".into());
    }
    if r.kind == "name" && p.contains(['\\', '/']) {
        return Err("a folder name can't contain \\ or /".into());
    }
    Ok(())
}

/// Wildcard match: `*` any run of characters (path separators included), `?` one character.
pub fn glob(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().replace('/', "\\").chars().collect();
    let t: Vec<char> = text.to_lowercase().replace('/', "\\").chars().collect();
    let (mut pi, mut ti, mut star, mut mark) = (0, 0, None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

/// Does the rule match a folder with this name, full path and last change (unix seconds)?
pub fn matches(r: &Rule, name: &str, full: &str, modified: u64, now: u64) -> bool {
    if !r.enabled {
        return false;
    }
    let hit = if r.kind == "path" { glob(r.pattern.trim(), full) } else { glob(r.pattern.trim(), name) };
    hit && (r.min_age_days == 0 || (modified > 0 && now.saturating_sub(modified) >= r.min_age_days as u64 * 86400))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(kind: &str, pattern: &str, age: u32) -> Rule {
        Rule { id: "r".into(), name: "r".into(), kind: kind.into(), pattern: pattern.into(), min_age_days: age, safe: true, enabled: true }
    }

    #[test]
    fn matches_names_paths_and_age() {
        assert!(glob("*.egg-info", "mypkg.egg-info"));
        assert!(glob("build?", "build2"));
        assert!(!glob("build?", "build"));
        assert!(glob(r"C:\Users\*\Downloads\*.tmp", r"c:\users\pat\downloads\big.tmp"));
        let now = 100 * 86400;
        assert!(matches(&rule("name", "logs", 0), "Logs", r"C:\app\Logs", 0, now));
        assert!(matches(&rule("name", "logs", 30), "logs", "x", now - 40 * 86400, now));
        assert!(!matches(&rule("name", "logs", 30), "logs", "x", now - 5 * 86400, now), "changed too recently");
        assert!(matches(&rule("path", r"*\unity\cache\*", 0), "x", r"D:\Unity\Cache\abc", 0, now));
        assert!(validate(&rule("name", "*", 0)).is_err());
        assert!(validate(&rule("path", r"C:\*", 0)).is_err());
        assert!(validate(&rule("name", "a/b", 0)).is_err());
        assert!(validate(&rule("name", "obj", 0)).is_ok());
    }
}
