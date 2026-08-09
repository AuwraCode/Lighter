//! Events Lighter emits to the frontend (per-session IPC channel), plus the
//! snapshot/config types shared across the IPC boundary.
//!
//! Field names stay snake_case on the wire; TypeScript bindings are generated
//! with ts-rs (`pnpm typegen`), so both sides always agree.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Envelope {
    pub seq: u64,
    pub event: SessionEvent,
}

/// One IPC message: a group of events that became visible together.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Batch {
    pub session_id: Uuid,
    pub events: Vec<Envelope>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "type")]
#[ts(export)]
pub enum SessionEvent {
    /// Session metadata (re)established: emitted on every `system/init` and
    /// after successful set_permission_mode/set_model. Reducer overwrites.
    Ready { meta: SessionMeta },
    /// Response to the `initialize` handshake: command palette + model picker
    /// source data (kept raw — display-only).
    Handshake { info: HandshakeInfo },
    Status { status: SessionStatus },

    /// A transcript item began (streaming block or tool call in flight).
    ItemStarted { item: TranscriptItem },
    /// Append streamed text to an in-flight item.
    ItemDelta {
        item_id: String,
        kind: DeltaKind,
        delta: String,
    },
    /// Authoritative final content for an item (upsert by id).
    ItemCompleted { item: TranscriptItem },
    /// Existing item changed after completion (tool result attached).
    ItemUpdated { item: TranscriptItem },

    /// A turn finished (`result` frame).
    TurnCompleted { stats: TurnStats },
    StatsUpdated { stats: SessionStats },

    PermissionRequested { request: PendingPermission },
    PermissionResolved {
        request_id: String,
        outcome: PermissionOutcome,
    },

    CompactResult { ok: bool, error: Option<String> },
    RateLimit { info: Value },

    Exited {
        code: Option<i32>,
        stderr_tail: String,
    },
    ProtocolError { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SessionMeta {
    pub session_id: Uuid,
    pub title: String,
    pub cwd: String,
    pub model: String,
    pub permission_mode: String,
    pub slash_commands: Vec<String>,
    pub tools: Vec<String>,
    pub claude_version: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HandshakeInfo {
    /// Array of {name, description, argumentHint, aliases}.
    pub commands: Value,
    /// Array of {value, displayName, description, supportedEffortLevels, ...}.
    pub models: Value,
    pub account: Value,
    pub current_permission_mode: Option<String>,
    pub output_style: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum SessionStatus {
    Starting,
    Idle,
    Working,
    AwaitingApproval,
    Compacting,
    Exited,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum DeltaKind {
    Text,
    Thinking,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum PermissionOutcome {
    Allowed,
    Denied,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind")]
#[ts(export)]
pub enum TranscriptItem {
    UserText {
        id: String,
        text: String,
        /// True for CLI-synthesized frames ("[Request interrupted by user]").
        injected: bool,
    },
    AssistantText {
        id: String,
        text: String,
        parent_tool_use_id: Option<String>,
    },
    Thinking {
        id: String,
        text: String,
        parent_tool_use_id: Option<String>,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
        /// None while the call is executing (or awaiting approval).
        output: Option<ToolOutput>,
        parent_tool_use_id: Option<String>,
    },
    CompactMarker {
        id: String,
    },
}

impl TranscriptItem {
    pub fn id(&self) -> &str {
        match self {
            TranscriptItem::UserText { id, .. }
            | TranscriptItem::AssistantText { id, .. }
            | TranscriptItem::Thinking { id, .. }
            | TranscriptItem::ToolUse { id, .. }
            | TranscriptItem::CompactMarker { id } => id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolOutput {
    pub text: String,
    pub is_error: bool,
    pub truncated: bool,
    /// Rich structured result (`tool_use_result`) when the CLI provides one.
    pub raw: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PendingPermission {
    pub request_id: String,
    pub tool_name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub input: Value,
    /// `permission_suggestions` passed through verbatim (array or null).
    pub suggestions: Value,
    pub tool_use_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SessionStats {
    /// Cumulative for the lifetime of the session record (incl. resumes).
    pub total_cost_usd: f64,
    pub turns: u32,
    pub context_used_tokens: Option<u64>,
    pub context_window: Option<u64>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TurnStats {
    pub turn_cost_usd: f64,
    pub total_cost_usd: f64,
    pub duration_ms: Option<u64>,
    pub is_error: bool,
    pub terminal_reason: Option<String>,
    pub result_text: Option<String>,
}

// ---------------------------------------------------------------------------
// IPC inputs / snapshots

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SessionConfig {
    pub cwd: String,
    pub title: Option<String>,
    pub model: Option<String>,
    /// CLI-flag permission mode: acceptEdits | auto | bypassPermissions |
    /// manual | dontAsk | plan.
    pub permission_mode: Option<String>,
    pub effort: Option<String>,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    pub append_system_prompt: Option<String>,
    pub initial_prompt: Option<String>,
    /// Resume an existing CLI session id instead of starting fresh.
    pub resume_session_id: Option<String>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        SessionConfig {
            cwd: String::new(),
            title: None,
            model: None,
            permission_mode: None,
            effort: None,
            allowed_tools: Vec::new(),
            disallowed_tools: Vec::new(),
            append_system_prompt: None,
            initial_prompt: None,
            resume_session_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SessionInfo {
    pub id: Uuid,
    pub title: String,
    pub cwd: String,
    pub status: SessionStatus,
}

/// Full state handed to the webview on (re)attach.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SessionSnapshot {
    pub meta: SessionMeta,
    pub status: SessionStatus,
    pub items: Vec<TranscriptItem>,
    pub streaming: Vec<StreamingTail>,
    pub pending_permissions: Vec<PendingPermission>,
    pub stats: SessionStats,
    pub handshake: Option<HandshakeInfo>,
    /// seq of the last event included in this snapshot; events on the channel
    /// with seq <= this are already reflected and must be dropped.
    pub last_seq: u64,
    pub exited: Option<ExitInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExitInfo {
    pub code: Option<i32>,
    pub stderr_tail: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct StreamingTail {
    pub item_id: String,
    pub kind: DeltaKind,
    pub text: String,
    pub parent_tool_use_id: Option<String>,
}

/// UI's answer to a permission prompt.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct PermissionDecisionDto {
    pub allow: bool,
    /// Echo `permission_suggestions` back as updatedPermissions ("always allow").
    pub use_suggestions: bool,
    pub message: Option<String>,
    pub interrupt: bool,
}

// ---------------------------------------------------------------------------
// registry (dashboard tiles)

/// Lightweight per-session digest streamed to the dashboard at ~4 Hz.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SessionSummary {
    pub id: Uuid,
    pub title: String,
    pub cwd: String,
    pub status: SessionStatus,
    pub model: String,
    pub permission_mode: String,
    pub total_cost_usd: f64,
    pub turns: u32,
    pub pending_permissions: u32,
    pub last_snippet: String,
    pub context_used_tokens: Option<u64>,
    pub context_window: Option<u64>,
    pub exited_code: Option<i32>,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RegistryBatch {
    pub updates: Vec<SessionSummary>,
    pub removed: Vec<Uuid>,
}
