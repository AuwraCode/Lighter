//! Account profiles: a profile is a named Claude config directory
//! (CLAUDE_CONFIG_DIR). Each directory holds its own OAuth credentials, so
//! two profiles = two accounts. `config_dir: None` means the system default
//! (~/.claude, or whatever CLAUDE_CONFIG_DIR the parent environment sets).

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::persistence::Store;

const FILE: &str = "profiles.json";

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Profile {
    pub id: Uuid,
    pub name: String,
    /// None = system default config dir.
    pub config_dir: Option<String>,
    #[serde(default)]
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ProfilesInfo {
    pub profiles: Vec<Profile>,
    pub default_profile_id: Option<Uuid>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ProfilesFile {
    version: u32,
    default_profile_id: Option<Uuid>,
    profiles: Vec<Profile>,
}

pub struct Profiles {
    store: Store,
    inner: Mutex<ProfilesFile>,
}

impl Profiles {
    pub fn load(store: Store) -> Profiles {
        let mut file: ProfilesFile = store.load_or_default(FILE);
        if file.profiles.is_empty() {
            let default = Profile {
                id: Uuid::new_v4(),
                name: "Default".into(),
                config_dir: None,
                created_at_ms: now_ms(),
            };
            file.default_profile_id = Some(default.id);
            file.profiles.push(default);
            let _ = store.save(FILE, &file);
        }
        Profiles {
            store,
            inner: Mutex::new(file),
        }
    }

    pub fn info(&self) -> ProfilesInfo {
        let inner = self.inner.lock().unwrap();
        let mut profiles = inner.profiles.clone();
        profiles.sort_by_key(|p| p.created_at_ms);
        ProfilesInfo {
            profiles,
            default_profile_id: inner.default_profile_id,
        }
    }

    pub fn get(&self, id: Uuid) -> Option<Profile> {
        self.inner
            .lock()
            .unwrap()
            .profiles
            .iter()
            .find(|p| p.id == id)
            .cloned()
    }

    pub fn save(&self, mut profile: Profile) -> Result<Profile> {
        if profile.name.trim().is_empty() {
            return Err(Error::InvalidInput("profile name is required".into()));
        }
        if let Some(dir) = &profile.config_dir {
            if dir.trim().is_empty() {
                profile.config_dir = None;
            }
        }
        if profile.created_at_ms == 0 {
            profile.created_at_ms = now_ms();
        }
        let mut inner = self.inner.lock().unwrap();
        match inner.profiles.iter_mut().find(|p| p.id == profile.id) {
            Some(slot) => *slot = profile.clone(),
            None => inner.profiles.push(profile.clone()),
        }
        if inner.default_profile_id.is_none() {
            inner.default_profile_id = Some(profile.id);
        }
        self.persist(&inner)?;
        Ok(profile)
    }

    pub fn delete(&self, id: Uuid) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if inner.profiles.len() <= 1 {
            return Err(Error::InvalidInput(
                "at least one profile must remain".into(),
            ));
        }
        inner.profiles.retain(|p| p.id != id);
        if inner.default_profile_id == Some(id) {
            inner.default_profile_id = inner.profiles.first().map(|p| p.id);
        }
        self.persist(&inner)
    }

    pub fn set_default(&self, id: Uuid) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if !inner.profiles.iter().any(|p| p.id == id) {
            return Err(Error::InvalidInput("unknown profile".into()));
        }
        inner.default_profile_id = Some(id);
        self.persist(&inner)
    }

    /// Scan the home directory for `.claude*` config dirs with credentials
    /// that are not registered as a profile yet.
    pub fn discover(&self) -> Vec<Profile> {
        let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) else {
            return Vec::new();
        };
        let registered: Vec<String> = {
            let inner = self.inner.lock().unwrap();
            inner
                .profiles
                .iter()
                .filter_map(|p| p.config_dir.clone())
                .map(|d| d.to_lowercase())
                .collect()
        };
        let default_dir = home.join(".claude").to_string_lossy().to_lowercase();
        let Ok(entries) = std::fs::read_dir(&home) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.starts_with(".claude") || !path.is_dir() {
                continue;
            }
            if !path.join(".credentials.json").is_file() {
                continue;
            }
            let dir = path.to_string_lossy().to_string();
            let key = dir.to_lowercase();
            // The bare default dir is covered by the "Default" profile.
            if key == default_dir || registered.contains(&key) {
                continue;
            }
            found.push(Profile {
                id: Uuid::new_v4(),
                name: name.trim_start_matches('.').to_string(),
                config_dir: Some(dir),
                created_at_ms: now_ms(),
            });
        }
        found
    }

    fn persist(&self, file: &ProfilesFile) -> Result<()> {
        self.store.save(
            FILE,
            &ProfilesFile {
                version: 1,
                default_profile_id: file.default_profile_id,
                profiles: file.profiles.clone(),
            },
        )
    }
}

/// `claude auth status --json` for a given config dir (None = system default).
pub fn auth_status(config_dir: Option<String>) -> Result<Value> {
    let bin = crate::session::spawn::resolve_claude_bin()?;
    let mut cmd = std::process::Command::new(bin);
    cmd.args(["auth", "status", "--json"]);
    if let Some(dir) = &config_dir {
        cmd.env("CLAUDE_CONFIG_DIR", dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let output = cmd
        .output()
        .map_err(|e| Error::Control(format!("failed to run claude auth status: {e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim())
        .map_err(|_| Error::Control(format!("unexpected auth status output: {}", stdout.trim())))
}

/// Open a console window running `claude auth login` for the given profile.
/// The OAuth browser flow completes there; Lighter picks the account up on
/// the next session launch.
pub fn open_login_terminal(config_dir: Option<String>) -> Result<()> {
    let mut cmd = std::process::Command::new("cmd.exe");
    let set_part = config_dir
        .map(|d| format!("set CLAUDE_CONFIG_DIR={d}&& "))
        .unwrap_or_default();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.raw_arg(format!(
            "/c start \"Claude sign-in\" cmd /k \"{set_part}claude auth login\""
        ));
    }
    cmd.spawn()
        .map_err(|e| Error::Control(format!("failed to open sign-in terminal: {e}")))?;
    Ok(())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_default_and_crud() {
        let dir = std::env::temp_dir().join(format!("lighter-profiles-{}", Uuid::new_v4()));
        let profiles = Profiles::load(Store::new(dir.clone()));

        // Fresh store seeds a "Default" profile and marks it default.
        let info = profiles.info();
        assert_eq!(info.profiles.len(), 1);
        assert_eq!(info.default_profile_id, Some(info.profiles[0].id));
        assert!(info.profiles[0].config_dir.is_none());

        // Add a second profile, make it default, survive reload.
        let work = profiles
            .save(Profile {
                id: Uuid::new_v4(),
                name: "Work".into(),
                config_dir: Some("C:/Users/x/.claude-work".into()),
                created_at_ms: 0,
            })
            .unwrap();
        profiles.set_default(work.id).unwrap();

        let reloaded = Profiles::load(Store::new(dir.clone()));
        let info = reloaded.info();
        assert_eq!(info.profiles.len(), 2);
        assert_eq!(info.default_profile_id, Some(work.id));

        // Cannot delete the last profile; deleting default reassigns it.
        reloaded.delete(work.id).unwrap();
        let info = reloaded.info();
        assert_eq!(info.profiles.len(), 1);
        assert_eq!(info.default_profile_id, Some(info.profiles[0].id));
        assert!(reloaded.delete(info.profiles[0].id).is_err());

        let _ = std::fs::remove_dir_all(dir);
    }
}
