//! Per-session materialized state + the frame→event normalizer.
//!
//! The router task owns exactly one `SessionState` and feeds every parsed
//! frame through [`SessionState::apply_frame`]. The returned events are what
//! the frontend consumes; the mutated state is what an (re)attaching webview
//! receives as a snapshot. Both views are produced by the same code path, so
//! they can never disagree.

use std::collections::{HashMap, VecDeque};

use serde_json::Value;
use uuid::Uuid;

use crate::protocol::inbound::{
    AssistantFrame, ContentBlock, Delta, Frame, InboundControlRequest, InitFrame, ResultFrame,
    StreamEvent, SystemFrame, UserFrame,
};

use super::events::{
    DeltaKind, ExitInfo, HandshakeInfo, PendingPermission, PermissionOutcome, SessionEvent,
    SessionMeta, SessionSnapshot, SessionStats, SessionStatus, SessionSummary, StreamingTail,
    ToolOutput, TranscriptItem, TurnStats,
};

const TOOL_OUTPUT_CAP_BYTES: usize = 128 * 1024;
const MAX_ITEMS: usize = 2000;

#[derive(Debug)]
pub struct SessionState {
    pub meta: SessionMeta,
    pub status: SessionStatus,
    pub items: Vec<TranscriptItem>,
    item_index: HashMap<String, usize>,
    /// Blocks currently streaming: (lane, block index) -> (item_id, kind).
    active_blocks: HashMap<(String, usize), (String, ActiveKind)>,
    /// item_id -> accumulated partial tool-input JSON.
    partial_inputs: HashMap<String, String>,
    /// item_ids whose text is still streaming (for snapshot/UI).
    streaming: HashMap<String, DeltaKind>,
    /// lane -> current API message id.
    lane_msg: HashMap<String, String>,
    /// message id -> count of blocks already finalized from assistant frames.
    finalized_blocks: HashMap<String, usize>,
    pub pending: Vec<PendingPermission>,
    pub stats: SessionStats,
    pub handshake: Option<HandshakeInfo>,
    pub exited: Option<ExitInfo>,
    /// Items dropped from the head to respect MAX_ITEMS.
    pub truncated_head: bool,
    /// `result.total_cost_usd` is cumulative per CLI process; this tracks the
    /// previous value so turn cost can be derived as a delta.
    last_process_cost: f64,
    /// Cost carried over from previous processes of this session (resume).
    base_cost: f64,
    created_at_ms: u64,
    /// Optimistic local user echoes awaiting their CLI replay (id, text).
    pending_local_user: VecDeque<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ActiveKind {
    Text,
    Thinking,
    Tool,
}

impl SessionState {
    pub fn new(
        session_id: Uuid,
        title: String,
        cwd: String,
        worktree: Option<crate::worktree::WorktreeMeta>,
        claude_config_dir: Option<String>,
    ) -> Self {
        SessionState {
            meta: SessionMeta {
                session_id,
                title,
                cwd,
                model: String::new(),
                permission_mode: String::new(),
                slash_commands: Vec::new(),
                tools: Vec::new(),
                claude_version: String::new(),
                worktree,
                claude_config_dir,
            },
            status: SessionStatus::Starting,
            items: Vec::new(),
            item_index: HashMap::new(),
            active_blocks: HashMap::new(),
            partial_inputs: HashMap::new(),
            streaming: HashMap::new(),
            lane_msg: HashMap::new(),
            finalized_blocks: HashMap::new(),
            pending: Vec::new(),
            stats: SessionStats::default(),
            handshake: None,
            exited: None,
            truncated_head: false,
            last_process_cost: 0.0,
            base_cost: 0.0,
            pending_local_user: VecDeque::new(),
            created_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        }
    }

    /// Digest for the dashboard tile / sidebar row.
    pub fn summary(&self) -> SessionSummary {
        let last_snippet = self
            .items
            .iter()
            .rev()
            .find_map(|item| match item {
                TranscriptItem::AssistantText { text, .. } if !text.trim().is_empty() => {
                    Some(truncate_chars(text.trim(), 140))
                }
                TranscriptItem::ToolUse { name, .. } => Some(format!("⚙ {name}")),
                TranscriptItem::UserText { text, injected, .. }
                    if !injected && !text.trim().is_empty() =>
                {
                    Some(format!("› {}", truncate_chars(text.trim(), 140)))
                }
                _ => None,
            })
            .unwrap_or_default();
        SessionSummary {
            id: self.meta.session_id,
            title: self.meta.title.clone(),
            cwd: self.meta.cwd.clone(),
            status: self.status,
            model: self.meta.model.clone(),
            permission_mode: self.meta.permission_mode.clone(),
            total_cost_usd: self.stats.total_cost_usd,
            turns: self.stats.turns,
            pending_permissions: self.pending.len() as u32,
            last_snippet,
            context_used_tokens: self.stats.context_used_tokens,
            context_window: self.stats.context_window,
            exited_code: self.exited.as_ref().and_then(|e| e.code),
            created_at_ms: self.created_at_ms,
            worktree_branch: self.meta.worktree.as_ref().map(|w| w.branch.clone()),
            claude_config_dir: self.meta.claude_config_dir.clone(),
        }
    }

    pub fn snapshot(&self, last_seq: u64) -> SessionSnapshot {
        SessionSnapshot {
            meta: self.meta.clone(),
            status: self.status,
            items: self.items.clone(),
            streaming: self
                .streaming
                .iter()
                .map(|(id, kind)| StreamingTail {
                    item_id: id.clone(),
                    kind: *kind,
                    text: String::new(),
                    parent_tool_use_id: None,
                })
                .collect(),
            pending_permissions: self.pending.clone(),
            stats: self.stats.clone(),
            handshake: self.handshake.clone(),
            last_seq,
            exited: self.exited.clone(),
        }
    }

    // -----------------------------------------------------------------------
    // frame application

    pub fn apply_frame(&mut self, frame: Frame) -> Vec<SessionEvent> {
        match frame {
            Frame::Init(init) => self.apply_init(init),
            Frame::System(sys) => self.apply_system(sys),
            Frame::RateLimit(v) => vec![SessionEvent::RateLimit {
                info: v.get("rate_limit_info").cloned().unwrap_or(Value::Null),
            }],
            Frame::User(user) => self.apply_user(user),
            Frame::Assistant(a) => self.apply_assistant(a),
            Frame::Stream(s) => {
                let lane = s.parent_tool_use_id.clone().unwrap_or_default();
                self.apply_stream(lane, s.parent_tool_use_id, s.event)
            }
            Frame::Result(r) => self.apply_result(r),
            Frame::ControlRequest(req) => match req.request {
                InboundControlRequest::CanUseTool {
                    tool_name,
                    display_name,
                    description,
                    input,
                    permission_suggestions,
                    tool_use_id,
                    ..
                } => {
                    let pending = PendingPermission {
                        request_id: req.request_id,
                        tool_name,
                        display_name,
                        description,
                        input,
                        suggestions: permission_suggestions.unwrap_or(Value::Null),
                        tool_use_id,
                    };
                    self.pending.push(pending.clone());
                    let mut evs = vec![SessionEvent::PermissionRequested { request: pending }];
                    evs.extend(self.set_status(SessionStatus::AwaitingApproval));
                    evs
                }
                _ => vec![],
            },
            Frame::ControlCancel { request_id } => match request_id {
                Some(id) => self.resolve_permission(&id, PermissionOutcome::Cancelled),
                None => vec![],
            },
            // Handled by the router (request/response correlation).
            Frame::ControlResponse(_) => vec![],
            Frame::Unknown(v) => {
                tracing::debug!(frame = %v, "unknown frame");
                vec![]
            }
        }
    }

    fn apply_init(&mut self, init: InitFrame) -> Vec<SessionEvent> {
        if let Ok(id) = Uuid::parse_str(&init.session_id) {
            self.meta.session_id = id;
        }
        self.meta.cwd = init.cwd;
        self.meta.model = init.model;
        self.meta.permission_mode = init.permission_mode;
        self.meta.slash_commands = init.slash_commands;
        self.meta.tools = init.tools;
        self.meta.claude_version = init.claude_code_version;
        vec![SessionEvent::Ready {
            meta: self.meta.clone(),
        }]
    }

    fn apply_system(&mut self, sys: SystemFrame) -> Vec<SessionEvent> {
        if sys.subtype != "status" {
            // thinking_tokens, task_started/updated/notification, ...
            return vec![];
        }
        let mut evs = Vec::new();
        match sys.status.as_deref() {
            Some("requesting") => evs.extend(self.set_status(SessionStatus::Working)),
            Some("compacting") => evs.extend(self.set_status(SessionStatus::Compacting)),
            _ => {}
        }
        if let Some(result) = sys.compact_result {
            let ok = result == "success";
            if ok {
                let marker = TranscriptItem::CompactMarker {
                    id: format!("compact:{}", self.items.len()),
                };
                evs.extend(self.push_item(marker));
            }
            evs.push(SessionEvent::CompactResult {
                ok,
                error: sys.compact_error,
            });
            evs.extend(self.set_status(SessionStatus::Working));
        }
        evs
    }

    fn apply_user(&mut self, user: UserFrame) -> Vec<SessionEvent> {
        let mut evs = Vec::new();
        let uuid = user
            .uuid
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        for (i, block) in user.message.content.iter().enumerate() {
            match block {
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => {
                    let output = ToolOutput::from_content(
                        content,
                        is_error.unwrap_or(false),
                        user.tool_use_result.clone(),
                    );
                    if let Some(ix) = self.item_index.get(tool_use_id).copied() {
                        if let TranscriptItem::ToolUse { output: slot, .. } = &mut self.items[ix] {
                            *slot = Some(output);
                            evs.push(SessionEvent::ItemUpdated {
                                item: self.items[ix].clone(),
                            });
                        }
                    }
                }
                ContentBlock::Text { text } => {
                    // Replay echoes confirm messages we already showed
                    // optimistically — never render them twice.
                    if user.is_replay && self.consume_local_echo(text) {
                        continue;
                    }
                    let item = TranscriptItem::UserText {
                        id: format!("{uuid}:{i}"),
                        text: text.clone(),
                        injected: !user.is_replay,
                    };
                    evs.extend(self.push_item(item));
                }
                _ => {}
            }
        }
        evs
    }

    /// Instant local echo for a message we just wrote to stdin. The CLI's
    /// replay frame later confirms it (and is deduped against this item).
    pub fn push_local_user(&mut self, text: &str) -> Vec<SessionEvent> {
        let id = format!("local:{}", Uuid::new_v4());
        self.pending_local_user.push_back((id.clone(), text.to_string()));
        if self.pending_local_user.len() > 64 {
            self.pending_local_user.pop_front();
        }
        self.push_item(TranscriptItem::UserText {
            id,
            text: text.to_string(),
            injected: false,
        })
    }

    fn consume_local_echo(&mut self, text: &str) -> bool {
        if let Some(pos) = self
            .pending_local_user
            .iter()
            .position(|(_, t)| t == text)
        {
            self.pending_local_user.remove(pos);
            true
        } else {
            false
        }
    }

    fn apply_assistant(&mut self, frame: AssistantFrame) -> Vec<SessionEvent> {
        let mut evs = Vec::new();
        let lane = frame.parent_tool_use_id.clone().unwrap_or_default();
        let msg_id = frame
            .message
            .id
            .clone()
            .unwrap_or_else(|| format!("lane:{lane}"));
        for block in frame.message.content {
            let ordinal = {
                let n = self.finalized_blocks.entry(msg_id.clone()).or_insert(0);
                let v = *n;
                *n += 1;
                v
            };
            let item = match block {
                ContentBlock::Text { text } => TranscriptItem::AssistantText {
                    id: self.block_item_id(&lane, ordinal, &msg_id),
                    text,
                    parent_tool_use_id: frame.parent_tool_use_id.clone(),
                },
                ContentBlock::Thinking { thinking } => TranscriptItem::Thinking {
                    id: self.block_item_id(&lane, ordinal, &msg_id),
                    text: thinking,
                    parent_tool_use_id: frame.parent_tool_use_id.clone(),
                },
                ContentBlock::ToolUse { id, name, input } => TranscriptItem::ToolUse {
                    id,
                    name,
                    input,
                    output: None,
                    parent_tool_use_id: frame.parent_tool_use_id.clone(),
                },
                _ => continue,
            };
            let id = item.id().to_string();
            self.active_blocks.remove(&(lane.clone(), ordinal));
            self.streaming.remove(&id);
            self.partial_inputs.remove(&id);
            evs.push(self.upsert_item(item));
        }
        evs
    }

    /// The id a streaming block was registered under, falling back to a
    /// deterministic synthetic id when the block never streamed.
    fn block_item_id(&self, lane: &str, ordinal: usize, msg_id: &str) -> String {
        self.active_blocks
            .get(&(lane.to_string(), ordinal))
            .map(|(id, _)| id.clone())
            .unwrap_or_else(|| format!("{msg_id}:{ordinal}"))
    }

    fn apply_stream(
        &mut self,
        lane: String,
        parent_tool_use_id: Option<String>,
        event: StreamEvent,
    ) -> Vec<SessionEvent> {
        match event {
            StreamEvent::MessageStart { message } => {
                if let Some(id) = message["id"].as_str() {
                    self.lane_msg.insert(lane, id.to_string());
                }
                vec![]
            }
            StreamEvent::ContentBlockStart {
                index,
                content_block,
            } => {
                let msg_id = self
                    .lane_msg
                    .get(&lane)
                    .cloned()
                    .unwrap_or_else(|| format!("lane:{lane}"));
                match content_block {
                    ContentBlock::Text { text } => {
                        let id = format!("{msg_id}:{index}");
                        self.active_blocks
                            .insert((lane, index), (id.clone(), ActiveKind::Text));
                        self.streaming.insert(id.clone(), DeltaKind::Text);
                        let item = TranscriptItem::AssistantText {
                            id,
                            text,
                            parent_tool_use_id,
                        };
                        self.insert_started(item)
                    }
                    ContentBlock::Thinking { thinking } => {
                        let id = format!("{msg_id}:{index}");
                        self.active_blocks
                            .insert((lane, index), (id.clone(), ActiveKind::Thinking));
                        self.streaming.insert(id.clone(), DeltaKind::Thinking);
                        let item = TranscriptItem::Thinking {
                            id,
                            text: thinking,
                            parent_tool_use_id,
                        };
                        self.insert_started(item)
                    }
                    ContentBlock::ToolUse { id, name, input } => {
                        self.active_blocks
                            .insert((lane, index), (id.clone(), ActiveKind::Tool));
                        let item = TranscriptItem::ToolUse {
                            id,
                            name,
                            input,
                            output: None,
                            parent_tool_use_id,
                        };
                        self.insert_started(item)
                    }
                    _ => vec![],
                }
            }
            StreamEvent::ContentBlockDelta { index, delta } => {
                let Some((item_id, kind)) = self.active_blocks.get(&(lane, index)).cloned() else {
                    return vec![];
                };
                match (kind, delta) {
                    (ActiveKind::Text, Delta::TextDelta { text }) => {
                        self.append_text(&item_id, &text);
                        vec![SessionEvent::ItemDelta {
                            item_id,
                            kind: DeltaKind::Text,
                            delta: text,
                        }]
                    }
                    (ActiveKind::Thinking, Delta::ThinkingDelta { thinking }) => {
                        self.append_text(&item_id, &thinking);
                        vec![SessionEvent::ItemDelta {
                            item_id,
                            kind: DeltaKind::Thinking,
                            delta: thinking,
                        }]
                    }
                    (ActiveKind::Tool, Delta::InputJsonDelta { partial_json }) => {
                        self.partial_inputs
                            .entry(item_id)
                            .or_default()
                            .push_str(&partial_json);
                        vec![]
                    }
                    _ => vec![],
                }
            }
            StreamEvent::ContentBlockStop { index } => {
                // Authoritative content lands via the assistant re-emit, which
                // normally precedes this. If a tool block never got one, try to
                // finalize its input from the accumulated partial JSON.
                if let Some((item_id, ActiveKind::Tool)) =
                    self.active_blocks.get(&(lane.clone(), index)).cloned()
                {
                    if let Some(buf) = self.partial_inputs.remove(&item_id) {
                        if let (Some(ix), Ok(input)) = (
                            self.item_index.get(&item_id).copied(),
                            serde_json::from_str::<Value>(&buf),
                        ) {
                            if let TranscriptItem::ToolUse { input: slot, .. } =
                                &mut self.items[ix]
                            {
                                if slot.is_null()
                                    || slot.as_object().is_some_and(|o| o.is_empty())
                                {
                                    *slot = input;
                                    return vec![SessionEvent::ItemUpdated {
                                        item: self.items[ix].clone(),
                                    }];
                                }
                            }
                        }
                    }
                }
                vec![]
            }
            StreamEvent::MessageDelta { usage, .. } => {
                if !lane.is_empty() {
                    return vec![];
                }
                let used = ["input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens", "output_tokens"]
                    .iter()
                    .filter_map(|k| usage.get(*k).and_then(Value::as_u64))
                    .sum::<u64>();
                if used > 0 {
                    self.stats.context_used_tokens = Some(used);
                    vec![SessionEvent::StatsUpdated {
                        stats: self.stats.clone(),
                    }]
                } else {
                    vec![]
                }
            }
            StreamEvent::MessageStop => vec![],
            StreamEvent::Other(_) => vec![],
        }
    }

    fn apply_result(&mut self, r: ResultFrame) -> Vec<SessionEvent> {
        let process_total = r.total_cost_usd.unwrap_or(self.last_process_cost);
        let turn_cost = (process_total - self.last_process_cost).max(0.0);
        self.last_process_cost = process_total;
        self.stats.total_cost_usd = self.base_cost + process_total;
        self.stats.turns += 1;

        if let Some(usage) = &r.usage {
            let used = ["input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens", "output_tokens"]
                .iter()
                .filter_map(|k| usage.get(*k).and_then(Value::as_u64))
                .sum::<u64>();
            if used > 0 {
                self.stats.context_used_tokens = Some(used);
            }
        }
        if let Some(mu) = &r.model_usage {
            if let Some(window) = mu
                .as_object()
                .and_then(|o| o.values().filter_map(|v| v["contextWindow"].as_u64()).max())
            {
                self.stats.context_window = Some(window);
            }
        }

        // A turn boundary always ends streaming activity.
        self.active_blocks.clear();
        self.streaming.clear();
        self.partial_inputs.clear();

        let mut evs = vec![
            SessionEvent::TurnCompleted {
                stats: TurnStats {
                    turn_cost_usd: turn_cost,
                    total_cost_usd: self.stats.total_cost_usd,
                    duration_ms: r.duration_ms,
                    is_error: r.is_error,
                    terminal_reason: r.terminal_reason.clone(),
                    result_text: r.result.clone(),
                },
            },
            SessionEvent::StatsUpdated {
                stats: self.stats.clone(),
            },
        ];
        evs.extend(self.set_status(SessionStatus::Idle));
        evs
    }

    // -----------------------------------------------------------------------
    // mutations shared with the router

    /// Set the cost carried over from previous processes (resume).
    pub fn set_base_cost(&mut self, base: f64) {
        self.base_cost = base;
        self.stats.total_cost_usd = base;
    }

    /// Items whose text is still streaming (focus-time catch-up sync).
    pub fn streaming_items(&self) -> Vec<TranscriptItem> {
        self.streaming
            .keys()
            .filter_map(|id| self.item_index.get(id).map(|ix| self.items[*ix].clone()))
            .collect()
    }

    pub fn resolve_permission(
        &mut self,
        request_id: &str,
        outcome: PermissionOutcome,
    ) -> Vec<SessionEvent> {
        let before = self.pending.len();
        self.pending.retain(|p| p.request_id != request_id);
        if self.pending.len() == before {
            return vec![];
        }
        let mut evs = vec![SessionEvent::PermissionResolved {
            request_id: request_id.to_string(),
            outcome,
        }];
        if self.pending.is_empty() && self.status == SessionStatus::AwaitingApproval {
            evs.extend(self.set_status(SessionStatus::Working));
        }
        evs
    }

    pub fn cancel_all_permissions(&mut self) -> Vec<SessionEvent> {
        let ids: Vec<String> = self.pending.iter().map(|p| p.request_id.clone()).collect();
        let mut evs = Vec::new();
        for id in ids {
            evs.extend(self.resolve_permission(&id, PermissionOutcome::Cancelled));
        }
        evs
    }

    /// `expected` — we initiated the stop, so a nonzero code only mirrors the
    /// last turn's error status (e.g. exit 1 after an interrupted turn) and
    /// must not mark the session as failed.
    pub fn apply_exit(
        &mut self,
        code: Option<i32>,
        stderr_tail: String,
        expected: bool,
    ) -> Vec<SessionEvent> {
        let mut evs = self.cancel_all_permissions();
        let failed = !expected && code != Some(0);
        self.exited = Some(ExitInfo {
            code,
            stderr_tail: stderr_tail.clone(),
        });
        evs.extend(self.set_status(if failed {
            SessionStatus::Failed
        } else {
            SessionStatus::Exited
        }));
        evs.push(SessionEvent::Exited { code, stderr_tail });
        evs
    }

    pub fn set_status(&mut self, status: SessionStatus) -> Option<SessionEvent> {
        // Terminal states are sticky.
        if self.status == status
            || matches!(self.status, SessionStatus::Exited | SessionStatus::Failed)
        {
            return None;
        }
        self.status = status;
        Some(SessionEvent::Status { status })
    }

    pub fn mark_working_on_send(&mut self) -> Option<SessionEvent> {
        match self.status {
            SessionStatus::Starting | SessionStatus::Idle => {
                self.set_status(SessionStatus::Working)
            }
            _ => None,
        }
    }

    pub fn apply_handshake(&mut self, response: &Value) -> Vec<SessionEvent> {
        let info = HandshakeInfo {
            commands: response.get("commands").cloned().unwrap_or(Value::Null),
            models: response.get("models").cloned().unwrap_or(Value::Null),
            account: response.get("account").cloned().unwrap_or(Value::Null),
            current_permission_mode: response["current_permission_mode"]
                .as_str()
                .map(String::from),
            output_style: response["output_style"].as_str().map(String::from),
        };
        if let Some(mode) = &info.current_permission_mode {
            self.meta.permission_mode = mode.clone();
        }
        self.handshake = Some(info.clone());
        vec![SessionEvent::Handshake { info }]
    }

    pub fn apply_mode_change(&mut self, mode: &str) -> Vec<SessionEvent> {
        self.meta.permission_mode = mode.to_string();
        vec![SessionEvent::Ready {
            meta: self.meta.clone(),
        }]
    }

    pub fn apply_model_change(&mut self, model: &str) -> Vec<SessionEvent> {
        self.meta.model = model.to_string();
        vec![SessionEvent::Ready {
            meta: self.meta.clone(),
        }]
    }

    // -----------------------------------------------------------------------
    // item plumbing

    fn insert_started(&mut self, item: TranscriptItem) -> Vec<SessionEvent> {
        let id = item.id().to_string();
        if self.item_index.contains_key(&id) {
            return vec![];
        }
        self.item_index.insert(id, self.items.len());
        self.items.push(item.clone());
        self.trim_items();
        vec![SessionEvent::ItemStarted { item }]
    }

    fn push_item(&mut self, item: TranscriptItem) -> Vec<SessionEvent> {
        let id = item.id().to_string();
        self.item_index.insert(id, self.items.len());
        self.items.push(item.clone());
        self.trim_items();
        vec![SessionEvent::ItemCompleted { item }]
    }

    /// Replace by id (or append) and emit ItemCompleted.
    fn upsert_item(&mut self, mut item: TranscriptItem) -> SessionEvent {
        let id = item.id().to_string();
        match self.item_index.get(&id).copied() {
            Some(ix) => {
                // Preserve an already-attached tool output across re-emits.
                let existing_output = match &self.items[ix] {
                    TranscriptItem::ToolUse {
                        output: Some(out), ..
                    } => Some(out.clone()),
                    _ => None,
                };
                if let (Some(out), TranscriptItem::ToolUse { output, .. }) =
                    (existing_output, &mut item)
                {
                    if output.is_none() {
                        *output = Some(out);
                    }
                }
                self.items[ix] = item.clone();
                SessionEvent::ItemCompleted { item }
            }
            None => {
                self.item_index.insert(id, self.items.len());
                self.items.push(item.clone());
                self.trim_items();
                SessionEvent::ItemCompleted { item }
            }
        }
    }

    fn append_text(&mut self, item_id: &str, delta: &str) {
        if let Some(ix) = self.item_index.get(item_id).copied() {
            match &mut self.items[ix] {
                TranscriptItem::AssistantText { text, .. }
                | TranscriptItem::Thinking { text, .. } => text.push_str(delta),
                _ => {}
            }
        }
    }

    fn trim_items(&mut self) {
        if self.items.len() <= MAX_ITEMS {
            return;
        }
        let drop = self.items.len() - MAX_ITEMS;
        self.items.drain(0..drop);
        self.truncated_head = true;
        self.item_index.clear();
        for (ix, item) in self.items.iter().enumerate() {
            self.item_index.insert(item.id().to_string(), ix);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::inbound::{ApiMessage, UserFrame};

    fn replay(text: &str, uuid: &str) -> Frame {
        Frame::User(UserFrame {
            message: ApiMessage {
                id: None,
                role: "user".into(),
                model: None,
                content: vec![ContentBlock::Text { text: text.into() }],
                stop_reason: None,
                usage: None,
            },
            is_replay: true,
            parent_tool_use_id: None,
            tool_use_result: None,
            session_id: None,
            uuid: Some(uuid.into()),
        })
    }

    #[test]
    fn local_echo_shows_instantly_and_dedupes_the_replay() {
        let mut s = SessionState::new(Uuid::nil(), "t".into(), "c".into(), None, None);

        let evs = s.push_local_user("hello");
        assert_eq!(evs.len(), 1, "echo emits one ItemCompleted");
        assert_eq!(s.items.len(), 1);

        // The CLI's replay of the same text must not duplicate the item.
        let evs = s.apply_frame(replay("hello", "u1"));
        assert!(evs.is_empty());
        assert_eq!(s.items.len(), 1);

        // A replay we never locally echoed still appends normally.
        s.apply_frame(replay("other", "u2"));
        assert_eq!(s.items.len(), 2);

        // Two identical sends → two items, each consumed once.
        s.push_local_user("dup");
        s.push_local_user("dup");
        assert_eq!(s.items.len(), 4);
        s.apply_frame(replay("dup", "u3"));
        s.apply_frame(replay("dup", "u4"));
        assert_eq!(s.items.len(), 4, "replays consumed both echoes");
        s.apply_frame(replay("dup", "u5"));
        assert_eq!(s.items.len(), 5, "third replay has no echo left");
    }
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}

impl ToolOutput {
    fn from_content(content: &Value, is_error: bool, raw: Option<Value>) -> ToolOutput {
        let mut text = match content {
            Value::String(s) => s.clone(),
            Value::Array(parts) => parts
                .iter()
                .filter_map(|p| p["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            Value::Null => String::new(),
            other => other.to_string(),
        };
        let truncated = text.len() > TOOL_OUTPUT_CAP_BYTES;
        if truncated {
            let mut cut = TOOL_OUTPUT_CAP_BYTES;
            while !text.is_char_boundary(cut) {
                cut -= 1;
            }
            text.truncate(cut);
        }
        ToolOutput {
            text,
            is_error,
            truncated,
            raw,
        }
    }
}
