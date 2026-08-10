//! The trigger test set: queries that should/shouldn't route to a skill, and
//! queries aimed at OTHER skills (cross-talk). It is hand-editable and must
//! never be silently clobbered — regeneration replaces only the machine
//! `generated` cases and leaves `manual`/`locked` ones untouched.

use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum CaseSource {
    Generated,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TriggerCase {
    pub id: u32,
    pub query: String,
    /// Skill this query is meant for; None = should trigger no skill.
    pub intended: Option<String>,
    pub source: CaseSource,
    /// A locked case is never touched by regeneration (implies hand-authored).
    #[serde(default)]
    pub locked: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TriggerSet {
    #[serde(default = "one")]
    pub version: u32,
    pub cases: Vec<TriggerCase>,
}

fn one() -> u32 {
    1
}

impl TriggerSet {
    pub fn load(path: &Path) -> TriggerSet {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_yaml::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let yaml = serde_yaml::to_string(self).unwrap_or_default();
        let tmp = path.with_extension("yaml.tmp");
        std::fs::write(&tmp, yaml)?;
        std::fs::rename(&tmp, path)
    }

    /// Merge freshly generated cases in without disturbing hand-authored ones.
    /// Preserved: every `Manual` case and every `locked` case. Dropped: prior
    /// non-locked `Generated` cases (they are being regenerated). Added: the
    /// incoming generated cases whose query text isn't already preserved.
    pub fn merge_generated(&self, generated: Vec<TriggerCase>) -> TriggerSet {
        let mut out: Vec<TriggerCase> = self
            .cases
            .iter()
            .filter(|c| c.locked || c.source == CaseSource::Manual)
            .cloned()
            .collect();

        let preserved_queries: Vec<String> =
            out.iter().map(|c| normalize(&c.query)).collect();

        for mut gen in generated {
            if preserved_queries.contains(&normalize(&gen.query)) {
                continue;
            }
            gen.source = CaseSource::Generated;
            gen.locked = false;
            out.push(gen);
        }

        // Stable, sequential ids.
        for (i, case) in out.iter_mut().enumerate() {
            case.id = i as u32 + 1;
        }
        TriggerSet {
            version: 1,
            cases: out,
        }
    }
}

fn normalize(q: &str) -> String {
    q.trim().to_lowercase()
}
