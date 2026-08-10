//! Writing the skill to disk — the one place generation touches the
//! filesystem. Persona content is refused here (a hard guard), exactly one
//! skill is ever created (no bulk), and the result is validated immediately.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{Error, Result};
use crate::skillsmith::{validate_skill, ValidationReport};

use super::guards::find_personas;

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct SkillSpec {
    pub name: String,
    pub description: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ScaffoldResult {
    pub skill_dir: String,
    pub report: ValidationReport,
}

pub fn scaffold(parent_dir: &Path, spec: &SkillSpec) -> Result<ScaffoldResult> {
    // Hard guard: no personas anywhere in the generated content.
    let mut personas = find_personas(&spec.description);
    personas.extend(find_personas(&spec.body));
    if !personas.is_empty() {
        return Err(Error::InvalidInput(format!(
            "refusing to write a persona into the skill (adds cost, no information): {}",
            personas.join(", ")
        )));
    }
    if spec.name.trim().is_empty() {
        return Err(Error::InvalidInput("skill name is required".into()));
    }

    let dir = parent_dir.join(&spec.name);
    if dir.exists() {
        return Err(Error::InvalidInput(format!(
            "a folder named '{}' already exists here",
            spec.name
        )));
    }
    std::fs::create_dir_all(&dir)?;

    let desc_scalar = serde_yaml::to_string(&serde_json::Value::String(spec.description.clone()))
        .unwrap_or_else(|_| format!("{}\n", spec.description));
    let body = spec.body.trim_end();
    let content = format!(
        "---\nname: {}\ndescription: {}\n---\n\n{}\n",
        spec.name,
        desc_scalar.trim_end(),
        body
    );
    std::fs::write(dir.join("SKILL.md"), content)?;

    let report = validate_skill(&dir);
    Ok(ScaffoldResult {
        skill_dir: dir.to_string_lossy().to_string(),
        report,
    })
}

/// Ensure a bundled subdir exists (references/scripts/assets) for the author
/// to drop files into. Never creates files (an empty file would be dead).
pub fn ensure_bundle_dir(skill_dir: &Path, name: &str) -> Result<PathBuf> {
    if !["references", "scripts", "assets"].contains(&name) {
        return Err(Error::InvalidInput(format!("not a bundle dir: {name}")));
    }
    let dir = skill_dir.join(name);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}
