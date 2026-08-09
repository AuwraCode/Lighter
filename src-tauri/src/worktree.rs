//! Git worktree isolation: when two sessions would collide on one repository,
//! the newcomer gets its own worktree on a dedicated `lighter/<slug>` branch
//! under ~/.lighter/worktrees. All git operations shell out to the git CLI —
//! that inherits the user's real config, credential helpers and worktree
//! semantics, exactly like claude itself sees them.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorktreeMeta {
    pub path: String,
    pub branch: String,
    pub repo_root: String,
}

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let mut cmd = std::process::Command::new("git");
    cmd.arg("-C").arg(dir).args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let output = cmd
        .output()
        .map_err(|e| Error::Control(format!("failed to run git: {e}")))?;
    if !output.status.success() {
        return Err(Error::Control(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Canonical repo root for a directory, or None when it isn't in a git repo.
pub fn repo_root(cwd: &str) -> Option<PathBuf> {
    let dir = Path::new(cwd);
    let inside = git(dir, &["rev-parse", "--is-inside-work-tree"]).ok()?;
    if inside != "true" {
        return None;
    }
    let top = git(dir, &["rev-parse", "--show-toplevel"]).ok()?;
    let path = PathBuf::from(top);
    Some(dunce::canonicalize(&path).unwrap_or(path))
}

/// Case-insensitive identity key for a repo root (Windows filesystems).
pub fn repo_key(root: &Path) -> String {
    root.to_string_lossy().to_lowercase()
}

pub fn is_dirty(root: &Path) -> bool {
    git(root, &["status", "--porcelain"])
        .map(|out| !out.is_empty())
        .unwrap_or(false)
}

static BASE_OVERRIDE: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

/// Settings-provided worktree base dir (None = ~/.lighter/worktrees).
pub fn set_base_override(path: Option<String>) {
    *BASE_OVERRIDE.write().unwrap() = path.map(PathBuf::from);
}

fn worktree_base(root: &Path) -> PathBuf {
    let base = BASE_OVERRIDE.read().unwrap().clone().unwrap_or_else(|| {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(".lighter")
            .join("worktrees")
    });
    let name = root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "repo".into());
    let mut hasher = DefaultHasher::new();
    repo_key(root).hash(&mut hasher);
    base.join(format!("{name}-{:08x}", hasher.finish() as u32))
}

pub fn slugify(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let mut slug: String = slug.chars().take(20).collect();
    if slug.is_empty() {
        slug = "session".into();
    }
    slug.trim_end_matches('-').to_string()
}

/// Create `lighter/<slug>` worktree from the repo's current HEAD.
pub fn create(root: &Path, slug: &str, id_suffix: &str) -> Result<WorktreeMeta> {
    // Clean up stale worktree registrations first (deleted dirs etc.).
    let _ = git(root, &["worktree", "prune"]);

    let base = worktree_base(root);
    std::fs::create_dir_all(&base)?;

    // Bump the name on collision (existing dir or branch).
    for attempt in 0..10 {
        let name = if attempt == 0 {
            format!("{slug}-{id_suffix}")
        } else {
            format!("{slug}-{id_suffix}-{attempt}")
        };
        let path = base.join(&name);
        let branch = format!("lighter/{name}");
        if path.exists() {
            continue;
        }
        let branch_exists = git(
            root,
            &["rev-parse", "--verify", &format!("refs/heads/{branch}")],
        )
        .is_ok();
        if branch_exists {
            continue;
        }
        let path_str = path.to_string_lossy().to_string();
        git(root, &["worktree", "add", "-b", &branch, &path_str])?;
        return Ok(WorktreeMeta {
            path: path_str,
            branch,
            repo_root: root.to_string_lossy().to_string(),
        });
    }
    Err(Error::Control("could not find a free worktree name".into()))
}

/// Remove a worktree; refuses when dirty unless `force`. Deletes the branch
/// only if it is fully merged (otherwise it stays for later review).
pub fn remove(meta: &WorktreeMeta, force: bool) -> Result<()> {
    let root = Path::new(&meta.repo_root);
    let path = Path::new(&meta.path);
    if !force && path.exists() && is_dirty(path) {
        return Err(Error::Control(format!(
            "worktree has uncommitted changes: {}",
            meta.path
        )));
    }
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(&meta.path);
    git(root, &args)?;
    // Best effort: drop the branch when merged; keep it otherwise.
    let _ = git(root, &["branch", "-d", &meta.branch]);
    Ok(())
}

/// Isolation decision for a new session.
pub fn should_isolate(policy: &str, active_sessions_on_repo: usize) -> bool {
    match policy {
        "always" => true,
        "never" => false,
        // "auto" and anything unknown
        _ => active_sessions_on_repo > 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(dir: &Path, args: &[&str]) {
        assert!(git(dir, args).is_ok(), "git {args:?} failed");
    }

    #[test]
    fn isolation_policy() {
        assert!(should_isolate("always", 0));
        assert!(!should_isolate("never", 5));
        assert!(!should_isolate("auto", 0));
        assert!(should_isolate("auto", 1));
    }

    #[test]
    fn slugs() {
        assert_eq!(slugify("My Cool Project!"), "my-cool-project");
        assert_eq!(slugify("///"), "session");
        assert!(slugify("a-very-long-name-that-exceeds-the-limit").len() <= 20);
    }

    #[test]
    fn worktree_lifecycle_on_real_repo() {
        let repo = std::env::temp_dir().join(format!("lighter-wt-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&repo).unwrap();
        run(&repo, &["init", "-q", "-b", "main"]);
        run(&repo, &["config", "user.email", "t@t.local"]);
        run(&repo, &["config", "user.name", "t"]);
        std::fs::write(repo.join("a.txt"), "hello").unwrap();
        run(&repo, &["add", "."]);
        run(&repo, &["commit", "-q", "-m", "init"]);

        let root = repo_root(repo.to_str().unwrap()).expect("repo root");
        assert!(!is_dirty(&root));

        let meta = create(&root, "test", "abcd").expect("create worktree");
        assert!(Path::new(&meta.path).join("a.txt").exists());
        assert_eq!(meta.branch, "lighter/test-abcd");

        // Same slug collides → suffixed name.
        let meta2 = create(&root, "test", "abcd").expect("second worktree");
        assert_ne!(meta.path, meta2.path);

        // Dirty worktree refuses removal without force.
        std::fs::write(Path::new(&meta.path).join("b.txt"), "dirty").unwrap();
        run(Path::new(&meta.path), &["add", "."]);
        assert!(remove(&meta, false).is_err());
        assert!(remove(&meta, true).is_ok());
        assert!(remove(&meta2, false).is_ok());

        let list = git(&root, &["worktree", "list", "--porcelain"]).unwrap();
        let entries = list
            .lines()
            .filter(|l| l.starts_with("worktree "))
            .count();
        assert_eq!(entries, 1, "only the main worktree should remain: {list}");

        let _ = std::fs::remove_dir_all(&repo);
    }
}
