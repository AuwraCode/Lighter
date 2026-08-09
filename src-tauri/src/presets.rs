//! Named launch configurations ("workflows"): one click on the dashboard
//! spawns a session with this shape. Persisted to presets.json in the app
//! config dir with atomic writes.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::error::Result;
use crate::persistence::Store;

pub const WORKTREE_POLICIES: [&str; 3] = ["auto", "always", "never"];

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Preset {
    pub id: Uuid,
    pub name: String,
    pub cwd: String,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    pub effort: Option<String>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub disallowed_tools: Vec<String>,
    pub append_system_prompt: Option<String>,
    pub initial_prompt: Option<String>,
    /// "auto" (isolate when the repo is busy) | "always" | "never".
    #[serde(default = "default_worktree_policy")]
    pub worktree_policy: String,
    #[serde(default)]
    pub created_at_ms: u64,
}

fn default_worktree_policy() -> String {
    "auto".into()
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PresetsFile {
    version: u32,
    presets: Vec<Preset>,
}

pub struct Presets {
    store: Store,
    list: Mutex<Vec<Preset>>,
}

const FILE: &str = "presets.json";

impl Presets {
    pub fn load(store: Store) -> Presets {
        let file: PresetsFile = store.load_or_default(FILE);
        Presets {
            store,
            list: Mutex::new(file.presets),
        }
    }

    pub fn list(&self) -> Vec<Preset> {
        let mut presets = self.list.lock().unwrap().clone();
        presets.sort_by_key(|p| p.created_at_ms);
        presets
    }

    pub fn save(&self, mut preset: Preset) -> Result<Preset> {
        if preset.created_at_ms == 0 {
            preset.created_at_ms = now_ms();
        }
        if !WORKTREE_POLICIES.contains(&preset.worktree_policy.as_str()) {
            preset.worktree_policy = default_worktree_policy();
        }
        let mut list = self.list.lock().unwrap();
        match list.iter_mut().find(|p| p.id == preset.id) {
            Some(slot) => *slot = preset.clone(),
            None => list.push(preset.clone()),
        }
        self.persist(&list)?;
        Ok(preset)
    }

    pub fn delete(&self, id: Uuid) -> Result<()> {
        let mut list = self.list.lock().unwrap();
        list.retain(|p| p.id != id);
        self.persist(&list)
    }

    fn persist(&self, list: &[Preset]) -> Result<()> {
        self.store.save(
            FILE,
            &PresetsFile {
                version: 1,
                presets: list.to_vec(),
            },
        )
    }
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

    fn preset(name: &str) -> Preset {
        Preset {
            id: Uuid::new_v4(),
            name: name.into(),
            cwd: "C:/tmp".into(),
            model: Some("haiku".into()),
            permission_mode: Some("plan".into()),
            effort: None,
            allowed_tools: vec![],
            disallowed_tools: vec![],
            append_system_prompt: None,
            initial_prompt: None,
            worktree_policy: "auto".into(),
            created_at_ms: 0,
        }
    }

    #[test]
    fn crud_roundtrip_survives_reload() {
        let dir = std::env::temp_dir().join(format!("lighter-presets-{}", Uuid::new_v4()));
        let presets = Presets::load(Store::new(dir.clone()));

        let a = presets.save(preset("A")).unwrap();
        let b = presets.save(preset("B")).unwrap();
        assert_eq!(presets.list().len(), 2);

        // Upsert by id.
        let mut a2 = a.clone();
        a2.name = "A2".into();
        presets.save(a2).unwrap();
        assert_eq!(presets.list().len(), 2);

        // Fresh load from disk sees the same data (restart survival).
        let reloaded = Presets::load(Store::new(dir.clone()));
        let names: Vec<String> = reloaded.list().into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["A2".to_string(), "B".to_string()]);

        reloaded.delete(b.id).unwrap();
        assert_eq!(Presets::load(Store::new(dir.clone())).list().len(), 1);

        let _ = std::fs::remove_dir_all(dir);
    }
}
