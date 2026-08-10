//! Deterministic SKILL.md validator. Every check is offline and reproducible;
//! model-based checks live in the eval, never here.
//!
//! Design notes:
//! - Frontmatter is located at the BYTE level (byte 0, BOM, `---` fences)
//!   before any YAML parsing — a real parser can't see a BOM or a misplaced
//!   fence, and those are the silent-failure cases.
//! - The YAML itself is parsed with a real parser (`serde_yaml`). Duplicate
//!   top-level keys are a known parser blind spot (last-wins), so they get a
//!   targeted structural scan on top-level `key:` lines.
//! - Body length limits are WARNINGS: an over-long body still loads, it just
//!   wastes context. Only load-breaking issues are errors.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use unicode_normalization::UnicodeNormalization;

use super::diagnostics::*;

const NAME_MAX: usize = 64;
const DESC_MAX: usize = 1024;
const COMPAT_MAX: usize = 500;
const BODY_MAX_LINES: usize = 500;
const BODY_MAX_TOKENS: usize = 5000;
const REF_DIRS: [&str; 3] = ["references", "scripts", "assets"];

pub fn validate_skill(skill_dir: &Path) -> ValidationReport {
    let mut report = ValidationReport::new(skill_dir.to_string_lossy().to_string());

    // 1. Locate the skill file (SKILL.md preferred, skill.md tolerated).
    let (skill_file, filename) = match locate_skill_file(skill_dir) {
        Some(v) => v,
        None => {
            report.push(Diagnostic::error(
                FILENAME_MISSING,
                "no SKILL.md (or skill.md) in the skill directory",
            ));
            return report;
        }
    };
    if filename == "skill.md" {
        report.push(Diagnostic::warning(
            FILENAME_LOWERCASE,
            "file is named skill.md; prefer SKILL.md",
        ));
    }

    let bytes = match std::fs::read(&skill_file) {
        Ok(b) => b,
        Err(e) => {
            report.push(Diagnostic::error(FILENAME_MISSING, format!("cannot read: {e}")));
            return report;
        }
    };

    // 2. Frontmatter at the byte level.
    let (yaml_text, body) = match split_frontmatter(&bytes, &mut report) {
        Some(v) => v,
        None => return report, // fatal frontmatter problem already recorded
    };

    // 3. Parse YAML (real parser) + duplicate top-level key scan. Duplicate
    //    keys are fatal (parsers keep last-wins, so the value you see is not
    //    the one you wrote) — stop before schema checks read a phantom value.
    if scan_duplicate_keys(&yaml_text, &mut report) {
        return report;
    }
    let value: serde_yaml::Value = match serde_yaml::from_str(&yaml_text) {
        Ok(v) => v,
        Err(e) => {
            report.push(Diagnostic::error(
                YAML_INVALID,
                format!("frontmatter is not valid YAML: {e}"),
            ));
            return report;
        }
    };
    let map = match value {
        serde_yaml::Value::Mapping(m) => m,
        // An empty frontmatter parses to Null; treat as an empty map so the
        // required-key diagnostics fire instead of a confusing type error.
        serde_yaml::Value::Null => serde_yaml::Mapping::new(),
        _ => {
            report.push(Diagnostic::error(
                YAML_NOT_MAP,
                "frontmatter must be a key/value mapping",
            ));
            return report;
        }
    };

    // 4. Key whitelist + required keys.
    for key in map.keys() {
        if let Some(k) = key.as_str() {
            if !ALLOWED_KEYS.contains(&k) {
                report.push(Diagnostic::error(
                    KEY_UNKNOWN,
                    format!("unknown frontmatter key: {k}"),
                ));
            }
        }
    }

    let name = str_field(&map, "name");
    let description = str_field(&map, "description");

    // 5. name.
    match &name {
        None => report.push(Diagnostic::error(KEY_MISSING_NAME, "missing required key: name")),
        Some(n) => {
            report.name = Some(n.clone());
            validate_name(n, skill_dir, &mut report);
        }
    }

    // 6. description.
    match &description {
        None => report.push(Diagnostic::error(
            KEY_MISSING_DESCRIPTION,
            "missing required key: description",
        )),
        Some(d) if d.trim().is_empty() => {
            report.push(Diagnostic::error(DESC_EMPTY, "description is empty"))
        }
        Some(d) if d.chars().count() > DESC_MAX => report.push(Diagnostic::error(
            DESC_TOO_LONG,
            format!("description is {} chars (max {DESC_MAX})", d.chars().count()),
        )),
        Some(_) => {}
    }

    // 7. compatibility / metadata.
    if let Some(compat) = str_field(&map, "compatibility") {
        if compat.chars().count() > COMPAT_MAX {
            report.push(Diagnostic::error(
                COMPAT_TOO_LONG,
                format!("compatibility is {} chars (max {COMPAT_MAX})", compat.chars().count()),
            ));
        }
    }
    if let Some(meta) = map.get("metadata") {
        if !is_string_map(meta) {
            report.push(Diagnostic::warning(
                METADATA_NOT_STRING_MAP,
                "metadata should be a map of string keys to string values",
            ));
        }
    }

    // 8. body length (warnings).
    let line_count = body.lines().count();
    if line_count > BODY_MAX_LINES {
        report.push(Diagnostic::warning(
            BODY_TOO_MANY_LINES,
            format!("body is {line_count} lines (keep under {BODY_MAX_LINES})"),
        ));
    }
    let tokens = estimate_tokens(&body);
    if tokens > BODY_MAX_TOKENS {
        report.push(Diagnostic::warning(
            BODY_TOO_MANY_TOKENS,
            format!("body is ~{tokens} tokens (keep under ~{BODY_MAX_TOKENS})"),
        ));
    }

    // 9. bundled files: depth + references.
    validate_files(skill_dir, &body, &mut report);

    report
}

// ---------------------------------------------------------------------------

fn locate_skill_file(dir: &Path) -> Option<(PathBuf, String)> {
    // Scan real directory entries so the case is authoritative — a constructed
    // `dir.join("SKILL.md")` matches skill.md on case-insensitive filesystems
    // (Windows/macOS) and would hide the FILENAME_LOWERCASE warning.
    let mut lower: Option<(PathBuf, String)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "SKILL.md" {
            return Some((entry.path(), name));
        }
        if name == "skill.md" {
            lower = Some((entry.path(), name));
        }
    }
    lower
}

/// Returns (yaml_text, body) or None after recording a fatal frontmatter error.
fn split_frontmatter(bytes: &[u8], report: &mut ValidationReport) -> Option<(String, String)> {
    let mut content = bytes;
    if content.starts_with(&[0xEF, 0xBB, 0xBF]) {
        report.push(Diagnostic::error(
            FRONT_BOM,
            "file begins with a UTF-8 BOM; the frontmatter will not be recognized",
        ));
        content = &content[3..];
    }
    let text = String::from_utf8_lossy(content).replace('\r', "");
    let lines: Vec<&str> = text.split('\n').collect();

    let first = lines.first().copied().unwrap_or("");
    if first != "---" {
        if lines.iter().any(|l| *l == "---") {
            report.push(Diagnostic::error(
                FRONT_NOT_BYTE0,
                "frontmatter `---` must be on the very first line (no leading blank lines or text)",
            ));
        } else {
            report.push(Diagnostic::error(
                FRONT_MISSING,
                "no YAML frontmatter (a `---` fenced block at the top of the file)",
            ));
        }
        return None;
    }

    let close = lines.iter().enumerate().skip(1).find(|(_, l)| **l == "---");
    let Some((close_idx, _)) = close else {
        report.push(Diagnostic::error(
            FRONT_UNCLOSED,
            "frontmatter is missing its closing `---`",
        ));
        return None;
    };

    let yaml_text = lines[1..close_idx].join("\n");
    let body = lines[close_idx + 1..].join("\n");
    Some((yaml_text, body))
}

/// Structural scan for repeated top-level keys (parsers keep last-wins).
/// Returns true if any duplicate was found.
fn scan_duplicate_keys(yaml_text: &str, report: &mut ValidationReport) -> bool {
    let mut seen: Vec<String> = Vec::new();
    let mut reported: Vec<String> = Vec::new();
    for line in yaml_text.split('\n') {
        // Top-level key: starts at column 0, `key:` with an allowed key charset.
        if line.starts_with(|c: char| c.is_whitespace()) {
            continue;
        }
        let Some(colon) = line.find(':') else { continue };
        let key = line[..colon].trim();
        if key.is_empty()
            || !key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            continue;
        }
        if seen.iter().any(|k| k == key) {
            if !reported.iter().any(|k| k == key) {
                report.push(Diagnostic::error(
                    YAML_DUP_KEY,
                    format!("duplicate frontmatter key: {key}"),
                ));
                reported.push(key.to_string());
            }
        } else {
            seen.push(key.to_string());
        }
    }
    !reported.is_empty()
}

fn validate_name(name: &str, skill_dir: &Path, report: &mut ValidationReport) {
    if name.is_empty() {
        report.push(Diagnostic::error(NAME_EMPTY, "name is empty"));
        return;
    }
    if name.chars().count() > NAME_MAX {
        report.push(Diagnostic::error(
            NAME_TOO_LONG,
            format!("name is {} chars (max {NAME_MAX})", name.chars().count()),
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        report.push(Diagnostic::error(
            NAME_CHARSET,
            "name may contain only lowercase letters, digits and hyphens",
        ));
    }
    if name.starts_with('-') || name.ends_with('-') {
        report.push(Diagnostic::error(
            NAME_HYPHEN_EDGE,
            "name must not start or end with a hyphen",
        ));
    }
    if name.contains("--") {
        report.push(Diagnostic::error(
            NAME_HYPHEN_DOUBLE,
            "name must not contain consecutive hyphens",
        ));
    }
    let lower = name.to_lowercase();
    if lower.contains("anthropic") || lower.contains("claude") {
        report.push(Diagnostic::error(
            NAME_RESERVED,
            "name must not contain 'anthropic' or 'claude'",
        ));
    }
    if let Some(folder) = skill_dir.file_name().and_then(|f| f.to_str()) {
        let name_nfkc: String = name.nfkc().collect();
        let folder_nfkc: String = folder.nfkc().collect();
        if name_nfkc != folder_nfkc {
            report.push(Diagnostic::error(
                NAME_FOLDER_MISMATCH,
                format!("name '{name}' must match the folder name '{folder}'"),
            ));
        }
    }
}

fn validate_files(skill_dir: &Path, body: &str, report: &mut ValidationReport) {
    let refs = extract_references(body, report);

    for dir_name in REF_DIRS {
        let dir = skill_dir.join(dir_name);
        if !dir.is_dir() {
            continue;
        }
        let mut depth1: Vec<String> = Vec::new();
        collect_files(&dir, dir_name, 1, &mut depth1, report);
        for rel in depth1 {
            // __init__.py is a Python package marker — never referenced, not dead.
            if rel.ends_with("/__init__.py") {
                continue;
            }
            if refs.coverage.iter().any(|r| r == &rel) {
                continue;
            }
            // An unreferenced doc in references/ is genuinely dead (docs are
            // only reachable via explicit SKILL.md pointers). Scripts and
            // assets are commonly reached indirectly (module imports, output
            // templates), so those are warnings, not errors — verified against
            // the reference skill-creator, which has such helper scripts.
            if dir_name == "references" {
                report.push(
                    Diagnostic::error(
                        REF_UNREFERENCED,
                        format!("{rel} is never referenced from the body, so it is dead"),
                    )
                    .at(rel),
                );
            } else {
                report.push(
                    Diagnostic::warning(
                        REF_UNREFERENCED,
                        format!("{rel} is never referenced from the body"),
                    )
                    .at(rel),
                );
            }
        }
    }

    // Dead references: a path is named in the body but the file is absent.
    for rel in &refs.paths {
        let path = rel_to_path(skill_dir, rel);
        if !path.exists() {
            report.push(
                Diagnostic::error(REF_DEAD, format!("{rel} is referenced but does not exist"))
                    .at(rel.clone()),
            );
        }
    }
}

struct References {
    /// Explicit forward-slash paths named in the body (checked for existence).
    paths: Vec<String>,
    /// Everything that counts as "referenced" for coverage, including files
    /// reached via Python module notation (`python -m scripts.run_loop`).
    coverage: Vec<String>,
}

/// Collect files under `dir`; anything deeper than one level → FILE_TOO_DEEP.
/// `depth1` receives the skill-relative paths of the valid depth-1 files.
fn collect_files(
    dir: &Path,
    prefix: &str,
    depth: usize,
    depth1: &mut Vec<String>,
    report: &mut ValidationReport,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let rel = format!("{prefix}/{name}");
        if path.is_dir() {
            // Recurse to report each too-deep file individually.
            collect_deep(&path, &rel, report);
        } else if depth == 1 {
            depth1.push(rel);
        }
    }
}

fn collect_deep(dir: &Path, prefix: &str, report: &mut ValidationReport) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let rel = format!("{prefix}/{name}");
        if path.is_dir() {
            collect_deep(&path, &rel, report);
        } else {
            report.push(
                Diagnostic::error(
                    FILE_TOO_DEEP,
                    format!("{rel} is nested too deep; keep bundled files exactly one level deep"),
                )
                .at(rel),
            );
        }
    }
}

/// Extract skill-relative references mentioned in the body. Flags backslash
/// forms; recognizes both path form (`scripts/run.py`) and Python module form
/// (`python -m scripts.run_loop` → scripts/run_loop.py). Module-derived paths
/// count only for coverage, never for the existence check.
fn extract_references(body: &str, report: &mut ValidationReport) -> References {
    let mut paths: Vec<String> = Vec::new();
    let mut coverage: Vec<String> = Vec::new();
    let add_cov = |p: String, cov: &mut Vec<String>| {
        if !cov.contains(&p) {
            cov.push(p);
        }
    };

    for token in tokenize_paths(body) {
        // Path form: references/x.md, scripts/run.py, assets/t.html
        let is_path = REF_DIRS.iter().any(|d| {
            token.starts_with(&format!("{d}/")) || token.starts_with(&format!("{d}\\"))
        });
        if is_path {
            if token.contains('\\') {
                report.push(Diagnostic::error(
                    REF_BACKSLASH,
                    format!("reference '{token}' uses backslashes; use forward slashes"),
                ));
            }
            let normalized = token.replace('\\', "/").trim_end_matches('.').to_string();
            if !paths.contains(&normalized) {
                paths.push(normalized.clone());
            }
            add_cov(normalized, &mut coverage);
            continue;
        }
        // Python module form for scripts: `scripts.run_loop` → scripts/run_loop.py
        if let Some(rest) = token.strip_prefix("scripts.") {
            if let Some(first) = rest.split('.').next().filter(|s| !s.is_empty()) {
                add_cov(format!("scripts/{first}.py"), &mut coverage);
            }
        }
    }
    References { paths, coverage }
}

/// Maximal runs of path characters — a cheap, dependency-free path scanner.
fn tokenize_paths(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in body.chars() {
        if ch.is_ascii_alphanumeric() || "/_.-\\".contains(ch) {
            cur.push(ch);
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn rel_to_path(skill_dir: &Path, rel: &str) -> PathBuf {
    let mut p = skill_dir.to_path_buf();
    for part in rel.split('/') {
        p.push(part);
    }
    p
}

fn str_field(map: &serde_yaml::Mapping, key: &str) -> Option<String> {
    map.get(key).and_then(|v| v.as_str()).map(String::from)
}

fn is_string_map(v: &serde_yaml::Value) -> bool {
    match v {
        serde_yaml::Value::Mapping(m) => m
            .iter()
            .all(|(k, val)| k.as_str().is_some() && val.as_str().is_some()),
        _ => false,
    }
}

fn estimate_tokens(text: &str) -> usize {
    // cl100k is a proxy for Claude's tokenizer — good enough for the "~5000"
    // heuristic and fully offline/deterministic. The eval can cross-check the
    // exact count via the API when online.
    static BPE: OnceLock<Option<tiktoken_rs::CoreBPE>> = OnceLock::new();
    let bpe = BPE.get_or_init(|| tiktoken_rs::cl100k_base().ok());
    match bpe {
        Some(b) => b.encode_ordinary(text).len(),
        None => text.len() / 4, // fallback heuristic
    }
}
