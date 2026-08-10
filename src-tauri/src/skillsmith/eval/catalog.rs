//! The skill catalog: the ONLY thing the router model ever sees. At session
//! start Claude Code loads exactly each skill's `name` + `description` (~100
//! tokens each) — body/references/scripts are invisible until a skill fires.
//! So the eval must route against this catalog and NOTHING else; feeding the
//! model any body text would measure something that never happens in reality.

use std::path::Path;

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
}

/// Enumerate every skill directly under `skills_dir` (each is a folder with a
/// SKILL.md). Only name + description are read.
pub fn build_catalog(skills_dir: &Path) -> Vec<SkillMeta> {
    let mut catalog = Vec::new();
    let Ok(entries) = std::fs::read_dir(skills_dir) else {
        // Maybe skills_dir is itself a single skill.
        if let Some((name, description)) = crate::skillsmith::validate::parse_meta(skills_dir) {
            catalog.push(SkillMeta { name, description });
        }
        return catalog;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Some((name, description)) = crate::skillsmith::validate::parse_meta(&path) {
            catalog.push(SkillMeta { name, description });
        }
    }
    // Also allow skills_dir itself being one skill.
    if catalog.is_empty() {
        if let Some((name, description)) = crate::skillsmith::validate::parse_meta(skills_dir) {
            catalog.push(SkillMeta { name, description });
        }
    }
    catalog.sort_by(|a, b| a.name.cmp(&b.name));
    catalog
}

/// System prompt for the router: the catalog and the decision rules. Contains
/// only names + descriptions — never any body text (enforced by construction
/// and asserted in tests).
pub fn routing_system_prompt(catalog: &[SkillMeta]) -> String {
    let mut s = String::from(
        "You are the skill router inside Claude Code. At the start of a session you can see ONLY \
each skill's name and description (below) — not their contents. For the user query, decide which \
SINGLE skill you would consult, if any.\n\n\
Rules:\n\
- Pick a skill only when the task genuinely benefits from it. Simple, one-step tasks you can do \
directly need NO skill.\n\
- If no skill clearly fits, choose none.\n\
- `also_plausible` lists any OTHER skills whose description also plausibly matches — this exposes \
overlap between skills.\n\n\
Available skills:\n",
    );
    for skill in catalog {
        s.push_str("- ");
        s.push_str(&skill.name);
        s.push_str(": ");
        s.push_str(&skill.description);
        s.push('\n');
    }
    s.push_str(
        "\nRespond by calling the `route` tool with the chosen skill name (or null) and any \
also_plausible skill names.",
    );
    s
}

/// The names in the catalog (routing labels), for building tools/matrices.
pub fn skill_names(catalog: &[SkillMeta]) -> Vec<String> {
    catalog.iter().map(|s| s.name.clone()).collect()
}
