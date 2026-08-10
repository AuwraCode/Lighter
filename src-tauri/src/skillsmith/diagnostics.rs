//! Frozen diagnostic codes for SKILL.md validation. Codes are stable strings
//! (the UI and tests key on them); messages may evolve. Severity separates
//! *load-breaking* errors (the skill silently fails to load) from *quality*
//! warnings (it loads but degrades).

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    /// Skill-relative file the diagnostic concerns (e.g. "references/x.md").
    pub file: Option<String>,
}

impl Diagnostic {
    pub fn error(code: &str, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            code: code.into(),
            severity: Severity::Error,
            message: message.into(),
            file: None,
        }
    }

    pub fn warning(code: &str, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            code: code.into(),
            severity: Severity::Warning,
            message: message.into(),
            file: None,
        }
    }

    pub fn at(mut self, file: impl Into<String>) -> Diagnostic {
        self.file = Some(file.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ValidationReport {
    pub skill_dir: String,
    /// Parsed name (best effort; None if unparseable/missing).
    pub name: Option<String>,
    pub diagnostics: Vec<Diagnostic>,
    /// True when there are no Error-severity diagnostics (warnings are ok).
    pub ok: bool,
}

impl ValidationReport {
    pub fn new(skill_dir: String) -> ValidationReport {
        ValidationReport {
            skill_dir,
            name: None,
            diagnostics: Vec::new(),
            ok: true,
        }
    }

    pub fn push(&mut self, d: Diagnostic) {
        if d.severity == Severity::Error {
            self.ok = false;
        }
        self.diagnostics.push(d);
    }

    pub fn codes(&self) -> Vec<String> {
        self.diagnostics.iter().map(|d| d.code.clone()).collect()
    }
}

// ---------------------------------------------------------------------------
// Frozen code constants — do not rename; tests and the UI depend on them.

// frontmatter / format (all load-breaking)
pub const FRONT_MISSING: &str = "FRONT_MISSING";
pub const FRONT_BOM: &str = "FRONT_BOM";
pub const FRONT_NOT_BYTE0: &str = "FRONT_NOT_BYTE0";
pub const FRONT_UNCLOSED: &str = "FRONT_UNCLOSED";
pub const YAML_INVALID: &str = "YAML_INVALID";
pub const YAML_NOT_MAP: &str = "YAML_NOT_MAP";
pub const YAML_DUP_KEY: &str = "YAML_DUP_KEY";

// schema / keys
pub const KEY_UNKNOWN: &str = "KEY_UNKNOWN";
pub const KEY_MISSING_NAME: &str = "KEY_MISSING_NAME";
pub const KEY_MISSING_DESCRIPTION: &str = "KEY_MISSING_DESCRIPTION";

// name
pub const NAME_EMPTY: &str = "NAME_EMPTY";
pub const NAME_TOO_LONG: &str = "NAME_TOO_LONG";
pub const NAME_CHARSET: &str = "NAME_CHARSET";
pub const NAME_HYPHEN_EDGE: &str = "NAME_HYPHEN_EDGE";
pub const NAME_HYPHEN_DOUBLE: &str = "NAME_HYPHEN_DOUBLE";
pub const NAME_RESERVED: &str = "NAME_RESERVED";
pub const NAME_FOLDER_MISMATCH: &str = "NAME_FOLDER_MISMATCH";

// description / compatibility / metadata
pub const DESC_EMPTY: &str = "DESC_EMPTY";
pub const DESC_TOO_LONG: &str = "DESC_TOO_LONG";
pub const COMPAT_TOO_LONG: &str = "COMPAT_TOO_LONG";
pub const METADATA_NOT_STRING_MAP: &str = "METADATA_NOT_STRING_MAP";

// body (quality warnings — the skill still loads)
pub const BODY_TOO_MANY_LINES: &str = "BODY_TOO_MANY_LINES";
pub const BODY_TOO_MANY_TOKENS: &str = "BODY_TOO_MANY_TOKENS";

// files / references
pub const FILE_TOO_DEEP: &str = "FILE_TOO_DEEP";
pub const REF_ABS_PATH: &str = "REF_ABS_PATH";
pub const REF_BACKSLASH: &str = "REF_BACKSLASH";
pub const REF_DEAD: &str = "REF_DEAD";
pub const REF_UNREFERENCED: &str = "REF_UNREFERENCED";

// filename
pub const FILENAME_MISSING: &str = "FILENAME_MISSING";
pub const FILENAME_LOWERCASE: &str = "FILENAME_LOWERCASE";

/// The only frontmatter keys the spec allows.
pub const ALLOWED_KEYS: &[&str] = &[
    "name",
    "description",
    "license",
    "allowed-tools",
    "metadata",
    "compatibility",
];
