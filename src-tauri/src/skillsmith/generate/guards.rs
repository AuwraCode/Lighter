//! Code-level constraints on what the generator will produce — these are
//! guards, not documentation. They refuse the failure modes that make skills
//! worthless: personas, bulk generation, and building a skill for something
//! that belongs in CLAUDE.md / a slash command / a subagent.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The discriminating question asked BEFORE anything is generated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum IntentKind {
    /// A rule that always applies → belongs in CLAUDE.md.
    AlwaysRule,
    /// A procedure Claude should follow when a situation arises → a skill.
    ContextualProcedure,
    /// Something invoked deliberately by name → a slash command.
    DeliberateInvoke,
    /// Something context-heavy that should run in isolation → a subagent.
    ContextHeavy,
}

impl IntentKind {
    /// None means "proceed to build a skill"; Some(_) is the redirect to show
    /// instead of generating.
    pub fn redirect(self) -> Option<&'static str> {
        match self {
            IntentKind::ContextualProcedure => None,
            IntentKind::AlwaysRule => Some(
                "A rule that always applies belongs in CLAUDE.md, not a skill — a skill only \
loads when its description triggers, so an always-on rule would be silently skipped whenever it \
doesn't. Put it in CLAUDE.md.",
            ),
            IntentKind::DeliberateInvoke => Some(
                "Something you invoke deliberately by name should be a slash command, not a \
skill. Skills are for procedures Claude decides to consult on its own; if you always type the \
name, make it a command.",
            ),
            IntentKind::ContextHeavy => Some(
                "Something that eats a lot of context in isolation should be a subagent, not a \
skill. Subagents run in their own context window and report back; a skill would load all that \
weight into the main conversation.",
            ),
        }
    }
}

/// Persona phrases add cost with zero information. Returns the offending
/// snippets found (case-insensitive), empty if clean.
pub fn find_personas(text: &str) -> Vec<String> {
    const PATTERNS: &[&str] = &[
        "you are a ",
        "you are an ",
        "you're a ",
        "you're an ",
        "you are the world",
        "you are an expert",
        "act as ",
        "acting as ",
        "pretend to be",
        "pretend you are",
        "roleplay",
        "role-play",
        "as a senior",
        "as an expert",
        "as a world-class",
        "you are a seasoned",
    ];
    let lower = text.to_lowercase();
    let mut hits = Vec::new();
    for p in PATTERNS {
        if lower.contains(p) && !hits.iter().any(|h| h == p) {
            hits.push(p.to_string());
        }
    }
    hits
}

/// Derive a valid skill name from a free-text title: lowercase, hyphenated,
/// reserved words stripped, clamped to 64 chars, never empty.
pub fn slugify_name(title: &str) -> String {
    let lowered = title.to_lowercase();
    // Drop reserved substrings entirely.
    let cleaned = lowered.replace("anthropic", "").replace("claude", "");
    let mut slug = String::new();
    let mut prev_hyphen = false;
    for ch in cleaned.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            slug.push(ch);
            prev_hyphen = false;
        } else if !prev_hyphen && !slug.is_empty() {
            slug.push('-');
            prev_hyphen = true;
        }
    }
    let slug: String = slug.trim_matches('-').chars().take(64).collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "my-skill".to_string()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_contextual_procedure_proceeds() {
        assert!(IntentKind::ContextualProcedure.redirect().is_none());
        assert!(IntentKind::AlwaysRule.redirect().unwrap().contains("CLAUDE.md"));
        assert!(IntentKind::DeliberateInvoke.redirect().unwrap().contains("slash command"));
        assert!(IntentKind::ContextHeavy.redirect().unwrap().contains("subagent"));
    }

    #[test]
    fn persona_detection() {
        assert!(find_personas("Extract text from PDFs. Use when handling PDFs.").is_empty());
        assert!(find_personas("You are a senior security engineer. Review code.")
            .contains(&"you are a ".to_string()));
        assert!(find_personas("Act as an expert and roleplay a DBA").len() >= 2);
    }

    #[test]
    fn slugify_rules() {
        assert_eq!(slugify_name("PDF Form Filler"), "pdf-form-filler");
        assert_eq!(slugify_name("Claude's Helper!!"), "s-helper");
        assert_eq!(slugify_name("  --- "), "my-skill");
        assert_eq!(slugify_name("Deploy → Prod (v2)"), "deploy-prod-v2");
    }
}
