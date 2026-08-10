//! Skillsmith: author, validate and eval Claude Code Agent Skills (SKILL.md)
//! from inside Lighter. The validator is fully deterministic (this module);
//! the trigger eval and generator build on top of it.

pub mod diagnostics;
pub mod eval;
pub mod generate;
pub mod library;
pub mod validate;

pub use diagnostics::{Diagnostic, Severity, ValidationReport};
pub use library::{list_local, LocalSkill};
pub use validate::{validate_skill, validate_skill_with, ValidateOptions};
