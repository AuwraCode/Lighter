//! Guarded skill generator: a discriminator gate first (skill vs CLAUDE.md /
//! slash command / subagent), a mandatory "when NOT to trigger" interview, and
//! code-level refusals of personas and bulk generation.

pub mod draft;
pub mod guards;
pub mod scaffold;

pub use guards::{find_personas, slugify_name, IntentKind};
pub use scaffold::{scaffold, ScaffoldResult, SkillSpec};
