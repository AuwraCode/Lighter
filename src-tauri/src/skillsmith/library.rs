//! Listing the skills actually installed for an account — user-scope skills
//! under a config dir plus optional project-scope skills — so the Skills hub can
//! show "what's active" next to the marketplace plugins. Read-only; the plugin
//! side lives in `crate::skills`.

use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

use super::validate::parse_meta;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct LocalSkill {
    pub name: String,
    pub description: String,
    /// "user" (`<config_dir>/skills`) or "project" (`<project>/.claude/skills`).
    pub scope: String,
    /// Absolute path to the skill folder — feeds the Validate/Eval shortcuts.
    pub dir: String,
    /// Frontmatter parsed cleanly. False = folder has a SKILL.md we couldn't
    /// read a name/description from (still listed so the hub can offer Validate).
    pub parsed: bool,
}

fn home_claude() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(".claude")
}

/// A config-dir string → its path, defaulting to `~/.claude`.
fn config_root(explicit: Option<&str>) -> PathBuf {
    match explicit.map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) => PathBuf::from(d),
        None => home_claude(),
    }
}

/// Every subfolder of `skills_dir` that holds a SKILL.md, tagged with `scope`.
/// A skill whose frontmatter can't be parsed still appears (folder name, empty
/// description, `parsed = false`) so the hub can surface it for a fix.
fn scan(skills_dir: &Path, scope: &str, out: &mut Vec<LocalSkill>) {
    let Ok(entries) = std::fs::read_dir(skills_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() || !dir.join("SKILL.md").is_file() {
            continue;
        }
        let parsed = parse_meta(&dir);
        let (name, description) = parsed.clone().unwrap_or_else(|| {
            let folder = dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            (folder, String::new())
        });
        out.push(LocalSkill {
            name,
            description,
            scope: scope.to_string(),
            dir: dir.to_string_lossy().to_string(),
            parsed: parsed.is_some(),
        });
    }
}

/// User-scope skills for `config_dir` (default `~/.claude`) plus, when given,
/// project-scope skills under `<project_dir>/.claude/skills`.
pub fn list_local(config_dir: Option<&str>, project_dir: Option<&str>) -> Vec<LocalSkill> {
    let mut out = Vec::new();
    scan(&config_root(config_dir).join("skills"), "user", &mut out);
    if let Some(proj) = project_dir.map(str::trim).filter(|p| !p.is_empty()) {
        scan(
            &Path::new(proj).join(".claude").join("skills"),
            "project",
            &mut out,
        );
    }
    out.sort_by(|a, b| a.scope.cmp(&b.scope).then(a.name.cmp(&b.name)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_skill(skills_root: &Path, name: &str, desc: &str) {
        let d = skills_root.join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {desc}\n---\n\nBody.\n"),
        )
        .unwrap();
    }

    #[test]
    fn lists_user_and_project_skills() {
        let base = std::env::temp_dir().join(format!("sks-lib-{}", uuid::Uuid::new_v4()));
        let cfg = base.join("cfg");
        write_skill(&cfg.join("skills"), "alpha", "Do alpha. Use when alpha.");
        let proj = base.join("repo");
        write_skill(
            &proj.join(".claude").join("skills"),
            "beta",
            "Do beta. Use when beta.",
        );
        // A folder without SKILL.md is ignored.
        std::fs::create_dir_all(cfg.join("skills").join("not-a-skill")).unwrap();

        let list = list_local(Some(cfg.to_str().unwrap()), Some(proj.to_str().unwrap()));
        assert_eq!(list.len(), 2, "{list:?}");
        let alpha = list.iter().find(|s| s.name == "alpha").unwrap();
        assert_eq!(alpha.scope, "user");
        assert!(alpha.parsed);
        let beta = list.iter().find(|s| s.name == "beta").unwrap();
        assert_eq!(beta.scope, "project");

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn unparseable_skill_still_listed() {
        let base = std::env::temp_dir().join(format!("sks-lib-bad-{}", uuid::Uuid::new_v4()));
        let skills = base.join("skills");
        let d = skills.join("broken");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("SKILL.md"), "no frontmatter here").unwrap();

        let list = list_local(Some(base.to_str().unwrap()), None);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "broken");
        assert!(!list[0].parsed);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn missing_dirs_are_empty_not_error() {
        assert!(list_local(Some("C:/nope/does/not/exist"), None).is_empty());
    }
}
