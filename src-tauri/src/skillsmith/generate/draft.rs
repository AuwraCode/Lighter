//! Model-assisted drafting for the interview. Every prompt forbids personas
//! and general-knowledge filler; the caller still runs the persona guard on
//! the output. The redundancy check operationalizes "if the model can do the
//! task from the description alone, the skill is pointless".

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::Result;
use crate::skillsmith::eval::model::Model;

/// Answers gathered in the interview — note the mandatory "when NOT" field.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct DraftInput {
    pub name: String,
    /// What the skill enables Claude to do.
    pub what: String,
    /// Situations/phrasings that SHOULD trigger it.
    pub when_to: String,
    /// Situations that should NOT trigger it (feeds negative triggers).
    pub when_not: String,
    /// What a new teammate would get wrong doing this — the real knowledge.
    pub teammate_pitfall: String,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Redundancy {
    pub redundant: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ScriptSuggestion {
    pub filename: String,
    pub language: String,
    pub reason: String,
}

fn input_block(i: &DraftInput) -> String {
    format!(
        "What it does: {}\nWhen it SHOULD trigger: {}\nWhen it should NOT trigger: {}\n\
What a new teammate gets wrong: {}\nExpected output: {}",
        i.what, i.when_to, i.when_not, i.teammate_pitfall, i.output
    )
}

pub async fn draft_description(model: &Model, input: &DraftInput) -> Result<String> {
    let system = String::from(
        "You write the `description` for a Claude Code skill — the ONLY text that decides whether \
it triggers. Encode BOTH what it does AND concrete WHEN-to-use contexts (keywords, situations). \
Be slightly pushy to fight under-triggering, but use the 'when NOT' info to avoid false triggers. \
Max 1024 characters. Never write a persona ('you are a…'). Return ONLY {\"description\": \"...\"}.",
    );
    let user = format!("Skill name: {}\n{}", input.name, input_block(input));
    let value = model.complete_json(&system, &user).await?;
    Ok(value["description"].as_str().unwrap_or_default().trim().to_string())
}

pub async fn draft_body(model: &Model, input: &DraftInput) -> Result<String> {
    let system = String::from(
        "You write the body of a Claude Code SKILL.md: imperative, concrete instructions that \
capture the project-specific procedure and the pitfalls a newcomer hits. Explain WHY where it \
matters. Do NOT restate general knowledge the model already has, and do NOT write a persona. Keep \
it well under 500 lines. Return ONLY {\"body\": \"...\"} with markdown in the body string.",
    );
    let user = format!("Skill name: {}\n{}", input.name, input_block(input));
    let value = model.complete_json(&system, &user).await?;
    Ok(value["body"].as_str().unwrap_or_default().trim().to_string())
}

/// If the model could do the task well from the description alone (no body),
/// the skill is redundant general knowledge.
pub async fn redundancy_check(model: &Model, name: &str, description: &str) -> Result<Redundancy> {
    let system = String::from(
        "You judge whether a Claude Code skill is redundant. A skill is REDUNDANT if a capable \
model could already do the task well from its description alone, with no special project \
knowledge — the body would add nothing. It is NOT redundant if doing it right needs \
project-specific steps, conventions, or gotchas. Return ONLY {\"redundant\": true|false, \
\"note\": \"one sentence why\"}.",
    );
    let user = format!("Skill: {name}\nDescription: {description}");
    let value = model.complete_json(&system, &user).await?;
    Ok(Redundancy {
        redundant: value["redundant"].as_bool().unwrap_or(false),
        note: value["note"].as_str().unwrap_or_default().to_string(),
    })
}

/// Suggest extracting a deterministic step sequence into a script (scripts are
/// for operations where variance is a bug).
pub async fn suggest_script(model: &Model, body: &str) -> Result<Option<ScriptSuggestion>> {
    let system = String::from(
        "You look for a deterministic, repetitive sequence in a skill body that should be a \
SCRIPT instead of prose — steps where any variation is a bug (fixed transforms, exact command \
sequences, parsing). If there is a clear candidate, return {\"filename\": \"scripts/x.py\", \
\"language\": \"python|bash|node\", \"reason\": \"...\"}. If not, return {}.",
    );
    let value = model.complete_json(&system, body).await?;
    let filename = value["filename"].as_str().unwrap_or_default();
    if filename.is_empty() {
        return Ok(None);
    }
    Ok(Some(ScriptSuggestion {
        filename: filename.to_string(),
        language: value["language"].as_str().unwrap_or("python").to_string(),
        reason: value["reason"].as_str().unwrap_or_default().to_string(),
    }))
}
