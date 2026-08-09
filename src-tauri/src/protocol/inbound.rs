//! Frames the CLI writes to stdout (newline-delimited JSON).
//!
//! Parsing is two-stage and tolerant by construction: every line first parses
//! into `serde_json::Value`, then dispatches on `type`/`subtype` into typed
//! structs. Anything unrecognized becomes `Frame::Unknown` — a newer CLI must
//! never crash the app. Shapes verified against claude 2.1.226 fixtures in
//! `tests/fixtures/*.ndjson`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    Init(InitFrame),
    /// `system` frames other than `init` (status, thinking_tokens, ...).
    System(SystemFrame),
    RateLimit(Value),
    User(UserFrame),
    Assistant(AssistantFrame),
    Stream(StreamFrame),
    Result(ResultFrame),
    /// CLI → app control request (permission prompts).
    ControlRequest(ControlRequestFrame),
    /// CLI's reply to one of our control requests.
    ControlResponse(ControlResponsePayload),
    /// CLI cancels a control request it previously sent us.
    ControlCancel { request_id: Option<String> },
    Unknown(Value),
}

/// Parse one stdout line. `Err` only when the line is not JSON at all.
pub fn parse_line(line: &str) -> Result<Frame, serde_json::Error> {
    let v: Value = serde_json::from_str(line)?;
    Ok(parse_value(v))
}

pub fn parse_value(v: Value) -> Frame {
    let ty = v["type"].as_str().unwrap_or_default();
    match ty {
        "system" => {
            if v["subtype"] == "init" {
                match serde_json::from_value::<InitFrame>(v.clone()) {
                    Ok(f) => Frame::Init(f),
                    Err(_) => Frame::Unknown(v),
                }
            } else {
                match serde_json::from_value::<SystemFrame>(v.clone()) {
                    Ok(f) => Frame::System(f),
                    Err(_) => Frame::Unknown(v),
                }
            }
        }
        "rate_limit_event" => Frame::RateLimit(v),
        "user" => match serde_json::from_value::<UserFrame>(v.clone()) {
            Ok(f) => Frame::User(f),
            Err(_) => Frame::Unknown(v),
        },
        "assistant" => match serde_json::from_value::<AssistantFrame>(v.clone()) {
            Ok(f) => Frame::Assistant(f),
            Err(_) => Frame::Unknown(v),
        },
        "stream_event" => match serde_json::from_value::<StreamFrame>(v.clone()) {
            Ok(f) => Frame::Stream(f),
            Err(_) => Frame::Unknown(v),
        },
        "result" => match serde_json::from_value::<ResultFrame>(v.clone()) {
            Ok(f) => Frame::Result(f),
            Err(_) => Frame::Unknown(v),
        },
        "control_request" => match serde_json::from_value::<ControlRequestFrame>(v.clone()) {
            Ok(f) => Frame::ControlRequest(f),
            Err(_) => Frame::Unknown(v),
        },
        "control_response" => {
            match serde_json::from_value::<ControlResponseEnvelope>(v.clone()) {
                Ok(f) => Frame::ControlResponse(f.response),
                Err(_) => Frame::Unknown(v),
            }
        }
        "control_cancel_request" => Frame::ControlCancel {
            request_id: v["request_id"].as_str().map(String::from),
        },
        _ => Frame::Unknown(v),
    }
}

// ---------------------------------------------------------------------------
// system

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InitFrame {
    pub session_id: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub model: String,
    #[serde(rename = "permissionMode", default)]
    pub permission_mode: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub slash_commands: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub mcp_servers: Vec<Value>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub output_style: String,
    #[serde(default)]
    pub claude_code_version: String,
    #[serde(rename = "apiKeySource", default)]
    pub api_key_source: String,
    #[serde(default)]
    pub uuid: String,
}

/// Non-init `system` frames: `status`, `thinking_tokens`, and whatever future
/// subtypes appear — kept loose on purpose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemFrame {
    pub subtype: String,
    /// `status` subtype: "requesting" | "compacting" | null (observed live).
    #[serde(default)]
    pub status: Option<String>,
    /// Set on the status frame that ends a compaction: "success" | "failed".
    #[serde(default)]
    pub compact_result: Option<String>,
    #[serde(default)]
    pub compact_error: Option<String>,
    #[serde(default)]
    pub estimated_tokens: Option<u64>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(flatten)]
    pub rest: Value,
}

// ---------------------------------------------------------------------------
// conversation

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserFrame {
    pub message: ApiMessage,
    #[serde(rename = "isReplay", default)]
    pub is_replay: bool,
    #[serde(default)]
    pub parent_tool_use_id: Option<String>,
    /// Rich structured tool output (e.g. Bash stdout/stderr breakdown) that
    /// accompanies tool_result echo frames.
    #[serde(default)]
    pub tool_use_result: Option<Value>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub uuid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssistantFrame {
    pub message: ApiMessage,
    #[serde(default)]
    pub parent_tool_use_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub uuid: Option<String>,
}

/// An Anthropic API message as embedded in user/assistant frames. Content is
/// typed; everything else we might need later stays raw.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiMessage {
    #[serde(default)]
    pub id: Option<String>,
    pub role: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default, deserialize_with = "content_blocks")]
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub usage: Option<Value>,
}

/// User message content may be a bare string; normalize to blocks.
fn content_blocks<'de, D>(de: D) -> Result<Vec<ContentBlock>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Value::deserialize(de)?;
    match v {
        Value::String(s) => Ok(vec![ContentBlock::Text {
            text: s,
        }]),
        Value::Array(items) => Ok(items
            .into_iter()
            .map(|item| {
                serde_json::from_value::<ContentBlock>(item.clone())
                    .unwrap_or(ContentBlock::Other(item))
            })
            .collect()),
        _ => Ok(vec![]),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Thinking {
        #[serde(default)]
        thinking: String,
    },
    ToolUse {
        id: String,
        name: String,
        #[serde(default)]
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        #[serde(default)]
        content: Value,
        #[serde(default)]
        is_error: Option<bool>,
    },
    #[serde(untagged)]
    Other(Value),
}

// ---------------------------------------------------------------------------
// stream events

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamFrame {
    pub event: StreamEvent,
    #[serde(default)]
    pub parent_tool_use_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    MessageStart {
        #[serde(default)]
        message: Value,
    },
    ContentBlockStart {
        index: usize,
        content_block: ContentBlock,
    },
    ContentBlockDelta {
        index: usize,
        delta: Delta,
    },
    ContentBlockStop {
        index: usize,
    },
    MessageDelta {
        #[serde(default)]
        delta: Value,
        #[serde(default)]
        usage: Value,
    },
    MessageStop,
    #[serde(untagged)]
    Other(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Delta {
    TextDelta {
        text: String,
    },
    ThinkingDelta {
        thinking: String,
    },
    SignatureDelta {
        #[serde(default)]
        signature: String,
    },
    InputJsonDelta {
        #[serde(default)]
        partial_json: String,
    },
    #[serde(untagged)]
    Other(Value),
}

// ---------------------------------------------------------------------------
// result

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultFrame {
    pub subtype: String,
    #[serde(default)]
    pub is_error: bool,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub total_cost_usd: Option<f64>,
    #[serde(default)]
    pub usage: Option<Value>,
    #[serde(rename = "modelUsage", default)]
    pub model_usage: Option<Value>,
    #[serde(default)]
    pub num_turns: Option<u32>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub duration_api_ms: Option<u64>,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub terminal_reason: Option<String>,
    #[serde(default)]
    pub permission_denials: Vec<Value>,
    #[serde(default)]
    pub session_id: Option<String>,
}

// ---------------------------------------------------------------------------
// control protocol (CLI → app)

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlRequestFrame {
    pub request_id: String,
    pub request: InboundControlRequest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
pub enum InboundControlRequest {
    CanUseTool {
        tool_name: String,
        /// Human-friendly tool name for UI ("Write").
        #[serde(default)]
        display_name: Option<String>,
        /// Short human-friendly summary of the call ("probe.txt").
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        input: Value,
        #[serde(default)]
        permission_suggestions: Option<Value>,
        #[serde(default)]
        tool_use_id: Option<String>,
        #[serde(default)]
        blocked_path: Option<String>,
    },
    HookCallback {
        #[serde(default)]
        callback_id: Option<String>,
        #[serde(flatten)]
        rest: Value,
    },
    #[serde(untagged)]
    Other(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ControlResponseEnvelope {
    response: ControlResponsePayload,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlResponsePayload {
    pub subtype: String,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub response: Option<Value>,
    #[serde(default)]
    pub error: Option<String>,
}

impl Frame {
    /// Compact descriptor used by snapshot tests and debug logging.
    pub fn descriptor(&self) -> String {
        match self {
            Frame::Init(_) => "system/init".into(),
            Frame::System(s) => format!("system/{}", s.subtype),
            Frame::RateLimit(_) => "rate_limit_event".into(),
            Frame::User(u) => {
                let kinds = block_kinds(&u.message.content);
                if u.is_replay {
                    format!("user(replay)[{kinds}]")
                } else {
                    format!("user[{kinds}]")
                }
            }
            Frame::Assistant(a) => format!("assistant[{}]", block_kinds(&a.message.content)),
            Frame::Stream(s) => match &s.event {
                StreamEvent::MessageStart { .. } => "stream/message_start".into(),
                StreamEvent::ContentBlockStart { content_block, .. } => {
                    format!("stream/block_start({})", block_kind(content_block))
                }
                StreamEvent::ContentBlockDelta { delta, .. } => format!(
                    "stream/delta({})",
                    match delta {
                        Delta::TextDelta { .. } => "text",
                        Delta::ThinkingDelta { .. } => "thinking",
                        Delta::SignatureDelta { .. } => "signature",
                        Delta::InputJsonDelta { .. } => "input_json",
                        Delta::Other(_) => "other",
                    }
                ),
                StreamEvent::ContentBlockStop { .. } => "stream/block_stop".into(),
                StreamEvent::MessageDelta { .. } => "stream/message_delta".into(),
                StreamEvent::MessageStop => "stream/message_stop".into(),
                StreamEvent::Other(_) => "stream/other".into(),
            },
            Frame::Result(r) => format!("result/{}", r.subtype),
            Frame::ControlRequest(c) => match &c.request {
                InboundControlRequest::CanUseTool { tool_name, .. } => {
                    format!("control_request/can_use_tool({tool_name})")
                }
                InboundControlRequest::HookCallback { .. } => {
                    "control_request/hook_callback".into()
                }
                InboundControlRequest::Other(_) => "control_request/other".into(),
            },
            Frame::ControlResponse(r) => format!("control_response/{}", r.subtype),
            Frame::ControlCancel { .. } => "control_cancel_request".into(),
            Frame::Unknown(v) => format!(
                "UNKNOWN({})",
                v["type"].as_str().unwrap_or("?")
            ),
        }
    }
}

fn block_kind(b: &ContentBlock) -> &'static str {
    match b {
        ContentBlock::Text { .. } => "text",
        ContentBlock::Thinking { .. } => "thinking",
        ContentBlock::ToolUse { .. } => "tool_use",
        ContentBlock::ToolResult { .. } => "tool_result",
        ContentBlock::Other(_) => "other",
    }
}

fn block_kinds(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .map(block_kind)
        .collect::<Vec<_>>()
        .join(",")
}
