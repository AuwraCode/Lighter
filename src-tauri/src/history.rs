//! Best-effort transcript backfill for resumed sessions: parse the CLI's own
//! JSONL transcript (undocumented location/format, but the line shapes match
//! the stream-json frames) through the same normalizer the live path uses.

use std::path::PathBuf;

use serde_json::Value;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::protocol::inbound::parse_value;
use crate::session::events::TranscriptItem;
use crate::session::state::SessionState;

fn claude_config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(".claude")
}

/// `C:\Users\me\proj` → `C--Users-me-proj` (observed convention).
fn cwd_slug(cwd: &str) -> String {
    cwd.replace([':', '\\', '/'], "-")
}

fn transcript_path(cwd: &str, session_id: Uuid) -> Option<PathBuf> {
    let projects = claude_config_dir().join("projects");
    let direct = projects
        .join(cwd_slug(cwd))
        .join(format!("{session_id}.jsonl"));
    if direct.is_file() {
        return Some(direct);
    }
    // Slug conventions may drift between CLI versions — fall back to a scan.
    let needle = format!("{session_id}.jsonl");
    for entry in std::fs::read_dir(&projects).ok()?.flatten() {
        let candidate = entry.path().join(&needle);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn load_history(cwd: &str, session_id: Uuid) -> Result<Vec<TranscriptItem>> {
    let path = transcript_path(cwd, session_id).ok_or_else(|| {
        Error::Control("no transcript found for this session on disk".into())
    })?;
    let content = std::fs::read_to_string(&path)?;

    let mut state = SessionState::new(session_id, String::new(), cwd.to_string(), None);
    for line in content.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(mut value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        // Transcript user lines carry no isReplay flag; without it the
        // normalizer would style them as CLI-injected. They are real input.
        if value["type"] == "user" && value.get("isReplay").is_none() {
            value["isReplay"] = Value::Bool(true);
        }
        let _ = state.apply_frame(parse_value(value));
    }
    Ok(state.items)
}
