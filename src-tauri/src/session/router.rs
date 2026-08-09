//! The per-session actor. One router task exclusively owns a session's state,
//! its child-process pipes and its IPC sink; every frame, command and timeout
//! flows through its inbox, which gives strict per-session event ordering
//! without any shared locks.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use futures_util::StreamExt;
use serde_json::Value;
use tauri::ipc::Channel;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};
use tokio_util::codec::{FramedRead, LinesCodec};
use uuid::Uuid;

use crate::protocol::{inbound, outbound};

use super::events::{
    Batch, Envelope, PermissionDecisionDto, PermissionOutcome, SessionEvent, SessionSnapshot,
    SessionStatus,
};
use super::spawn::Spawned;
use super::state::SessionState;

const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;
const CONTROL_TIMEOUT: Duration = Duration::from_secs(10);
const GRACEFUL_KILL_DEADLINE: Duration = Duration::from_secs(5);
const STDERR_RING_LINES: usize = 200;

pub enum SessionCommand {
    Attach {
        channel: Channel<Batch>,
        reply: oneshot::Sender<SessionSnapshot>,
    },
    SendUser {
        text: String,
    },
    RespondPermission {
        request_id: String,
        decision: PermissionDecisionDto,
        reply: oneshot::Sender<Result<(), String>>,
    },
    SetMode {
        mode: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
    SetModel {
        model: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
    Interrupt {
        reply: oneshot::Sender<Result<(), String>>,
    },
    Stop {
        graceful: bool,
    },
}

enum IoMsg {
    Line(String),
    LineError(String),
    Stderr(String),
    Exited(Option<i32>),
    CtrlTimeout(String),
    KillDeadline,
}

enum PendingCtrl {
    Initialize,
    SetMode {
        mode: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
    SetModel {
        model: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
    Interrupt {
        reply: oneshot::Sender<Result<(), String>>,
    },
}

struct Router {
    session_id: Uuid,
    state: SessionState,
    sink: Option<Channel<Batch>>,
    seq: u64,
    ctrl_tx: Option<mpsc::UnboundedSender<String>>,
    data_tx: Option<mpsc::UnboundedSender<String>>,
    pending_ctrl: HashMap<String, PendingCtrl>,
    req_counter: u64,
    stderr_ring: VecDeque<String>,
    job: Option<win32job::Job>,
    pid: Option<u32>,
    io_tx: mpsc::UnboundedSender<IoMsg>,
    exited: bool,
    kill_scheduled: bool,
    stop_requested: bool,
}

pub fn start(
    session_id: Uuid,
    state: SessionState,
    spawned: Spawned,
    initial_prompt: Option<String>,
    channel: Option<Channel<Batch>>,
) -> mpsc::UnboundedSender<SessionCommand> {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (io_tx, io_rx) = mpsc::unbounded_channel();
    let (ctrl_tx, ctrl_rx) = mpsc::unbounded_channel::<String>();
    let (data_tx, data_rx) = mpsc::unbounded_channel::<String>();

    let Spawned {
        mut child,
        stdin,
        stdout,
        stderr,
        job,
        pid,
        args,
    } = spawned;
    tracing::info!(%session_id, ?pid, ?args, "session spawned");

    // stdout reader: NDJSON lines with a hard length cap.
    {
        let io_tx = io_tx.clone();
        tokio::spawn(async move {
            let mut framed = FramedRead::new(
                stdout,
                LinesCodec::new_with_max_length(MAX_LINE_BYTES),
            );
            while let Some(item) = framed.next().await {
                let msg = match item {
                    Ok(line) => IoMsg::Line(line),
                    Err(e) => IoMsg::LineError(e.to_string()),
                };
                if io_tx.send(msg).is_err() {
                    break;
                }
            }
        });
    }

    // stderr reader.
    {
        let io_tx = io_tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if io_tx.send(IoMsg::Stderr(line)).is_err() {
                    break;
                }
            }
        });
    }

    // stdin writer: control lane beats data lane so interrupt never queues
    // behind a large user message.
    {
        let mut stdin = stdin;
        let mut ctrl_rx = ctrl_rx;
        let mut data_rx = data_rx;
        tokio::spawn(async move {
            loop {
                let line = tokio::select! {
                    biased;
                    Some(l) = ctrl_rx.recv() => l,
                    Some(l) = data_rx.recv() => l,
                    else => break,
                };
                if stdin.write_all(line.as_bytes()).await.is_err() {
                    break;
                }
                if stdin.write_all(b"\n").await.is_err() {
                    break;
                }
                let _ = stdin.flush().await;
            }
            // stdin dropped here → CLI sees EOF and exits gracefully.
        });
    }

    // exit waiter: owns the Child.
    {
        let io_tx = io_tx.clone();
        tokio::spawn(async move {
            let code = child.wait().await.ok().and_then(|s| s.code());
            let _ = io_tx.send(IoMsg::Exited(code));
        });
    }

    let router = Router {
        session_id,
        state,
        sink: channel,
        seq: 0,
        ctrl_tx: Some(ctrl_tx),
        data_tx: Some(data_tx),
        pending_ctrl: HashMap::new(),
        req_counter: 0,
        stderr_ring: VecDeque::new(),
        job,
        pid,
        io_tx,
        exited: false,
        kill_scheduled: false,
        stop_requested: false,
    };

    tokio::spawn(router.run(cmd_rx, io_rx, initial_prompt));
    cmd_tx
}

impl Router {
    async fn run(
        mut self,
        mut cmd_rx: mpsc::UnboundedReceiver<SessionCommand>,
        mut io_rx: mpsc::UnboundedReceiver<IoMsg>,
        initial_prompt: Option<String>,
    ) {
        // Handshake first: its response is our readiness signal and feeds the
        // command palette / model picker.
        self.send_control(
            outbound::ControlRequest::Initialize { hooks: None },
            PendingCtrl::Initialize,
        );
        if let Some(prompt) = initial_prompt {
            self.send_user(prompt);
        }

        loop {
            tokio::select! {
                cmd = cmd_rx.recv() => match cmd {
                    Some(cmd) => self.handle_cmd(cmd),
                    None => break, // session removed from the manager
                },
                Some(msg) = io_rx.recv() => self.handle_io(msg),
            }
        }
        tracing::info!(session_id = %self.session_id, "router stopped");
        // Dropping self drops the Job handle → any survivors are killed.
    }

    fn handle_cmd(&mut self, cmd: SessionCommand) {
        match cmd {
            SessionCommand::Attach { channel, reply } => {
                // Snapshot and sink swap happen between two frames, so the
                // snapshot + subsequent events form a gapless sequence.
                let snapshot = self.state.snapshot(self.seq);
                self.sink = Some(channel);
                let _ = reply.send(snapshot);
            }
            SessionCommand::SendUser { text } => self.send_user(text),
            SessionCommand::RespondPermission {
                request_id,
                decision,
                reply,
            } => {
                let result = self.respond_permission(&request_id, decision);
                let _ = reply.send(result);
            }
            SessionCommand::SetMode { mode, reply } => {
                if self.exited {
                    let _ = reply.send(Err("session has exited".into()));
                    return;
                }
                self.send_control(
                    outbound::ControlRequest::SetPermissionMode { mode: mode.clone() },
                    PendingCtrl::SetMode { mode, reply },
                );
            }
            SessionCommand::SetModel { model, reply } => {
                if self.exited {
                    let _ = reply.send(Err("session has exited".into()));
                    return;
                }
                self.send_control(
                    outbound::ControlRequest::SetModel { model: model.clone() },
                    PendingCtrl::SetModel { model, reply },
                );
            }
            SessionCommand::Interrupt { reply } => {
                if self.exited {
                    let _ = reply.send(Err("session has exited".into()));
                    return;
                }
                self.send_control(
                    outbound::ControlRequest::Interrupt,
                    PendingCtrl::Interrupt { reply },
                );
            }
            SessionCommand::Stop { graceful } => self.stop(graceful),
        }
    }

    fn handle_io(&mut self, msg: IoMsg) {
        match msg {
            IoMsg::Line(line) => match inbound::parse_line(&line) {
                Ok(inbound::Frame::ControlResponse(payload)) => {
                    self.handle_control_response(payload);
                }
                Ok(frame) => {
                    let events = self.state.apply_frame(frame);
                    self.emit(events);
                }
                Err(e) => {
                    self.emit(vec![SessionEvent::ProtocolError {
                        message: format!("unparseable stdout line: {e}"),
                    }]);
                }
            },
            IoMsg::LineError(e) => {
                self.emit(vec![SessionEvent::ProtocolError {
                    message: format!("stdout framing error: {e}"),
                }]);
            }
            IoMsg::Stderr(line) => {
                tracing::debug!(session_id = %self.session_id, stderr = %line);
                if self.stderr_ring.len() >= STDERR_RING_LINES {
                    self.stderr_ring.pop_front();
                }
                self.stderr_ring.push_back(line);
            }
            IoMsg::Exited(code) => {
                if self.exited {
                    return;
                }
                self.exited = true;
                self.job = None;
                self.ctrl_tx = None;
                self.data_tx = None;
                for (_, pending) in self.pending_ctrl.drain() {
                    fail_pending(pending, "session exited");
                }
                let tail: Vec<String> = self.stderr_ring.iter().cloned().collect();
                let events = self
                    .state
                    .apply_exit(code, tail.join("\n"), self.stop_requested);
                self.emit(events);
            }
            IoMsg::CtrlTimeout(request_id) => {
                if let Some(pending) = self.pending_ctrl.remove(&request_id) {
                    fail_pending(pending, "control request timed out");
                }
            }
            IoMsg::KillDeadline => {
                if !self.exited {
                    tracing::warn!(session_id = %self.session_id, "graceful stop deadline hit; killing job");
                    self.kill_now();
                }
            }
        }
    }

    fn handle_control_response(&mut self, payload: inbound::ControlResponsePayload) {
        let Some(request_id) = payload.request_id.clone() else {
            return;
        };
        let Some(pending) = self.pending_ctrl.remove(&request_id) else {
            return;
        };
        let ok = payload.subtype == "success";
        let err = || {
            payload
                .error
                .clone()
                .unwrap_or_else(|| "control request failed".to_string())
        };
        match pending {
            PendingCtrl::Initialize => {
                if ok {
                    let response = payload.response.unwrap_or(Value::Null);
                    let events = self.state.apply_handshake(&response);
                    self.emit(events);
                } else {
                    tracing::warn!(error = %err(), "initialize handshake failed");
                }
            }
            PendingCtrl::SetMode { mode, reply } => {
                if ok {
                    let events = self.state.apply_mode_change(&mode);
                    self.emit(events);
                    let _ = reply.send(Ok(()));
                } else {
                    let _ = reply.send(Err(err()));
                }
            }
            PendingCtrl::SetModel { model, reply } => {
                if ok {
                    let events = self.state.apply_model_change(&model);
                    self.emit(events);
                    let _ = reply.send(Ok(()));
                } else {
                    let _ = reply.send(Err(err()));
                }
            }
            PendingCtrl::Interrupt { reply } => {
                let _ = reply.send(if ok { Ok(()) } else { Err(err()) });
            }
        }
    }

    fn send_user(&mut self, text: String) {
        if self.exited {
            return;
        }
        let frame = outbound::user_message(&text);
        self.send_data_line(frame.to_string());
        let ev = self.state.mark_working_on_send();
        self.emit(ev.into_iter().collect());
    }

    fn respond_permission(
        &mut self,
        request_id: &str,
        decision: PermissionDecisionDto,
    ) -> Result<(), String> {
        let Some(pending) = self
            .state
            .pending
            .iter()
            .find(|p| p.request_id == request_id)
            .cloned()
        else {
            // Already resolved (double click / cancelled) — idempotent no-op.
            return Ok(());
        };
        let outcome = if decision.allow {
            let updated_permissions = if decision.use_suggestions
                && pending.suggestions.is_array()
                && !pending.suggestions.as_array().unwrap().is_empty()
            {
                Some(pending.suggestions.clone())
            } else {
                None
            };
            let frame = outbound::permission_response(
                request_id,
                &outbound::PermissionDecision::Allow {
                    updated_input: pending.input.clone(),
                    updated_permissions,
                },
            );
            self.send_ctrl_line(frame.to_string());
            PermissionOutcome::Allowed
        } else {
            let frame = outbound::permission_response(
                request_id,
                &outbound::PermissionDecision::Deny {
                    message: decision
                        .message
                        .unwrap_or_else(|| "Denied by user".to_string()),
                    interrupt: decision.interrupt,
                },
            );
            self.send_ctrl_line(frame.to_string());
            PermissionOutcome::Denied
        };
        let events = self.state.resolve_permission(request_id, outcome);
        self.emit(events);
        Ok(())
    }

    fn send_control(&mut self, request: outbound::ControlRequest, pending: PendingCtrl) {
        if self.exited || self.ctrl_tx.is_none() {
            fail_pending(pending, "session is not running");
            return;
        }
        self.req_counter += 1;
        let request_id = format!("app_req_{}", self.req_counter);
        let frame = outbound::control_request(&request_id, &request);
        self.send_ctrl_line(frame.to_string());
        self.pending_ctrl.insert(request_id.clone(), pending);

        let io_tx = self.io_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(CONTROL_TIMEOUT).await;
            let _ = io_tx.send(IoMsg::CtrlTimeout(request_id));
        });
    }

    fn send_ctrl_line(&mut self, line: String) {
        if let Some(tx) = &self.ctrl_tx {
            let _ = tx.send(line);
        }
    }

    fn send_data_line(&mut self, line: String) {
        if let Some(tx) = &self.data_tx {
            let _ = tx.send(line);
        }
    }

    fn stop(&mut self, graceful: bool) {
        if self.exited {
            return;
        }
        self.stop_requested = true;
        if graceful {
            // Closing both writer lanes drops stdin → the CLI exits on its own
            // and flushes its transcript (resume keeps working).
            self.ctrl_tx = None;
            self.data_tx = None;
            if !self.kill_scheduled {
                self.kill_scheduled = true;
                let io_tx = self.io_tx.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(GRACEFUL_KILL_DEADLINE).await;
                    let _ = io_tx.send(IoMsg::KillDeadline);
                });
            }
        } else {
            self.kill_now();
        }
    }

    fn kill_now(&mut self) {
        // Dropping the Job handle fires KILL_ON_JOB_CLOSE for the whole tree.
        if self.job.take().is_none() {
            if let Some(pid) = self.pid {
                let _ = std::process::Command::new("taskkill")
                    .args(["/F", "/T", "/PID", &pid.to_string()])
                    .output();
            }
        }
    }

    fn emit(&mut self, events: Vec<SessionEvent>) {
        if events.is_empty() {
            return;
        }
        let envelopes: Vec<Envelope> = events
            .into_iter()
            .map(|event| {
                self.seq += 1;
                Envelope {
                    seq: self.seq,
                    event,
                }
            })
            .collect();
        if let Some(sink) = &self.sink {
            let batch = Batch {
                session_id: self.session_id,
                events: envelopes,
            };
            if sink.send(batch).is_err() {
                // Webview is gone (reload); state keeps accumulating and the
                // next attach delivers a snapshot.
                self.sink = None;
            }
        }
    }
}

fn fail_pending(pending: PendingCtrl, reason: &str) {
    match pending {
        PendingCtrl::Initialize => {}
        PendingCtrl::SetMode { reply, .. }
        | PendingCtrl::SetModel { reply, .. }
        | PendingCtrl::Interrupt { reply } => {
            let _ = reply.send(Err(reason.to_string()));
        }
    }
}

/// Convenience: is the session in a state where stdin input makes sense?
#[allow(dead_code)]
pub fn accepts_input(status: SessionStatus) -> bool {
    !matches!(status, SessionStatus::Exited | SessionStatus::Failed)
}
