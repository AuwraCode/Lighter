//! App settings (settings.json): claude binary override, worktree base dir,
//! defaults for new sessions. Overrides are applied process-wide on load and
//! on every save.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{Error, Result};
use crate::persistence::Store;

const FILE: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AppSettings {
    /// Full path to claude.exe; None = resolve from PATH.
    pub claude_bin: Option<String>,
    /// Base directory for isolation worktrees; None = ~/.lighter/worktrees.
    pub worktree_base: Option<String>,
    /// Prefill for the new-session dialog.
    pub default_model: Option<String>,
    pub default_permission_mode: Option<String>,
    /// Skill plugins to auto-provision into each account (see skills.rs).
    #[serde(default = "default_skill_plugins")]
    pub skill_plugins: Vec<String>,
}

fn default_skill_plugins() -> Vec<String> {
    vec!["example-skills".into(), "document-skills".into()]
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            claude_bin: None,
            worktree_base: None,
            default_model: None,
            default_permission_mode: None,
            skill_plugins: default_skill_plugins(),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SettingsFile {
    version: u32,
    settings: AppSettings,
}

pub struct Settings {
    store: Store,
    current: Mutex<AppSettings>,
}

impl Settings {
    pub fn load(store: Store) -> Settings {
        let file: SettingsFile = store.load_or_default(FILE);
        apply_overrides(&file.settings);
        Settings {
            store,
            current: Mutex::new(file.settings),
        }
    }

    pub fn get(&self) -> AppSettings {
        self.current.lock().unwrap().clone()
    }

    pub fn skill_plugins(&self) -> Vec<String> {
        self.current.lock().unwrap().skill_plugins.clone()
    }

    pub fn save(&self, mut settings: AppSettings) -> Result<AppSettings> {
        normalize(&mut settings);
        if let Some(bin) = &settings.claude_bin {
            if !std::path::Path::new(bin).is_file() {
                return Err(Error::InvalidInput(format!(
                    "claude binary not found at: {bin}"
                )));
            }
        }
        self.store.save(
            FILE,
            &SettingsFile {
                version: 1,
                settings: settings.clone(),
            },
        )?;
        apply_overrides(&settings);
        *self.current.lock().unwrap() = settings.clone();
        Ok(settings)
    }
}

fn normalize(s: &mut AppSettings) {
    for field in [
        &mut s.claude_bin,
        &mut s.worktree_base,
        &mut s.default_model,
        &mut s.default_permission_mode,
    ] {
        if field.as_deref().is_some_and(|v| v.trim().is_empty()) {
            *field = None;
        }
    }
    s.skill_plugins
        .retain(|p| crate::skills::AVAILABLE.iter().any(|(id, _)| id == p));
}

fn apply_overrides(s: &AppSettings) {
    crate::session::spawn::set_claude_bin_override(s.claude_bin.clone());
    crate::worktree::set_base_override(s.worktree_base.clone());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_validation() {
        let dir = std::env::temp_dir().join(format!("lighter-settings-{}", uuid::Uuid::new_v4()));
        let settings = Settings::load(Store::new(dir.clone()));
        assert!(settings.get().claude_bin.is_none());

        // Bogus binary path is rejected.
        let bad = AppSettings {
            claude_bin: Some("C:/definitely/not/claude.exe".into()),
            ..Default::default()
        };
        assert!(settings.save(bad).is_err());

        // Empty strings normalize to None; values persist across reload.
        let saved = settings
            .save(AppSettings {
                claude_bin: None,
                worktree_base: Some("C:/tmp/wt".into()),
                default_model: Some("haiku".into()),
                default_permission_mode: Some("".into()),
                ..Default::default()
            })
            .unwrap();
        assert!(saved.default_permission_mode.is_none());

        let reloaded = Settings::load(Store::new(dir.clone()));
        assert_eq!(reloaded.get().default_model.as_deref(), Some("haiku"));
        assert_eq!(reloaded.get().worktree_base.as_deref(), Some("C:/tmp/wt"));

        // Reset the global overrides for other tests.
        apply_overrides(&AppSettings::default());
        let _ = std::fs::remove_dir_all(dir);
    }
}
