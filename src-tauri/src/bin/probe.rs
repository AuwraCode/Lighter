//! Protocol probe: drives a real `claude.exe` over the stream-json protocol and
//! records every frame (both directions) into NDJSON fixtures used by parser
//! tests. Run with `cargo run --bin probe -- <scenario>|all`.
//!
//! Fixture line format:
//!   {"dir":"from_cli"|"to_cli"|"stderr"|"meta","t":<ms since spawn>,"frame":...}

use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};
use tokio::sync::mpsc;

const DEFAULT_MODEL: &str = "haiku";
const SCENARIOS: &[&str] = &[
    "basic",
    "handshake",
    "controls",
    "tool_approve",
    "tool_deny",
    "always_allow",
    "interrupt",
    "midturn",
    "resume",
    "compact",
    "subagent",
    "ask_question",
];

#[derive(Debug)]
enum Msg {
    FromCli(Value),
    FromCliUnparsed(String),
    Stderr(String),
    Exit(Option<i32>),
}

struct Probe {
    pid: Option<u32>,
    exited: bool,
    stdin: Option<ChildStdin>,
    rx: mpsc::UnboundedReceiver<Msg>,
    records: Vec<String>,
    started: Instant,
    req_counter: u32,
    session_id: String,
}

impl Probe {
    async fn spawn(cwd: &PathBuf, extra_args: &[&str], resume: Option<&str>) -> Probe {
        let session_id = uuid::Uuid::new_v4().to_string();
        let mut args: Vec<String> = [
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
            "--replay-user-messages",
            "--permission-prompt-tool",
            "stdio",
            "--strict-mcp-config",
            "--max-budget-usd",
            "0.5",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        match resume {
            Some(id) => {
                args.push("--resume".into());
                args.push(id.to_string());
            }
            None => {
                args.push("--session-id".into());
                args.push(session_id.clone());
            }
        }
        if !extra_args.iter().any(|a| *a == "--model") {
            args.push("--model".into());
            args.push(DEFAULT_MODEL.into());
        }
        args.extend(extra_args.iter().map(|s| s.to_string()));

        let mut child = Command::new("claude")
            .args(&args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn claude");

        let pid = child.id();
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let (tx, rx) = mpsc::unbounded_channel();

        let tx_out = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                let msg = match serde_json::from_str::<Value>(&line) {
                    Ok(v) => Msg::FromCli(v),
                    Err(_) => Msg::FromCliUnparsed(line),
                };
                if tx_out.send(msg).is_err() {
                    break;
                }
            }
        });

        let tx_err = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx_err.send(Msg::Stderr(line));
            }
        });

        // Dedicated waiter: owns the Child, reports exit through the channel.
        tokio::spawn(async move {
            let code = child.wait().await.ok().and_then(|s| s.code());
            let _ = tx.send(Msg::Exit(code));
        });

        let mut probe = Probe {
            pid,
            exited: false,
            stdin,
            rx,
            records: Vec::new(),
            started: Instant::now(),
            req_counter: 0,
            session_id,
        };
        probe.record(
            "meta",
            json!({ "event": "spawn", "args": args, "resume": resume }),
        );
        probe
    }

    fn elapsed_ms(&self) -> u128 {
        self.started.elapsed().as_millis()
    }

    fn record(&mut self, dir: &str, frame: Value) {
        let line = json!({ "dir": dir, "t": self.elapsed_ms(), "frame": frame });
        self.records.push(serde_json::to_string(&line).unwrap());
    }

    async fn send_raw(&mut self, frame: Value) {
        self.record("to_cli", frame.clone());
        if let Some(stdin) = self.stdin.as_mut() {
            let mut buf = serde_json::to_vec(&frame).unwrap();
            buf.push(b'\n');
            let _ = stdin.write_all(&buf).await;
            let _ = stdin.flush().await;
        }
    }

    async fn send_user(&mut self, text: &str) {
        let frame = json!({
            "type": "user",
            "message": { "role": "user", "content": [{ "type": "text", "text": text }] },
        });
        self.send_raw(frame).await;
    }

    async fn send_control(&mut self, request: Value) -> String {
        self.req_counter += 1;
        let request_id = format!("probe_req_{}", self.req_counter);
        let frame = json!({
            "type": "control_request",
            "request_id": request_id,
            "request": request,
        });
        self.send_raw(frame).await;
        request_id
    }

    async fn send_permission_response(&mut self, request_id: &str, response: Value) {
        let frame = json!({
            "type": "control_response",
            "response": { "subtype": "success", "request_id": request_id, "response": response },
        });
        self.send_raw(frame).await;
    }

    /// Pull one message within `dur`, recording it. None on timeout.
    async fn recv(&mut self, dur: Duration) -> Option<Msg> {
        let msg = tokio::time::timeout(dur, self.rx.recv()).await.ok()??;
        match &msg {
            Msg::FromCli(v) => self.record("from_cli", v.clone()),
            Msg::FromCliUnparsed(s) => {
                let s = s.clone();
                self.record("meta", json!({ "event": "unparseable_stdout", "line": s }));
            }
            Msg::Stderr(s) => {
                let s = s.clone();
                self.record("stderr", json!(s));
            }
            Msg::Exit(code) => {
                let code = *code;
                self.exited = true;
                self.record("meta", json!({ "event": "exit", "code": code }));
            }
        }
        Some(msg)
    }

    /// Drain frames until `pred` matches (returning that frame) or deadline.
    /// `auto_allow`: automatically approve any can_use_tool along the way.
    async fn wait_for<F: Fn(&Value) -> bool>(
        &mut self,
        pred: F,
        total: Duration,
        auto_allow: bool,
    ) -> Option<Value> {
        let deadline = Instant::now() + total;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                self.record("meta", json!({ "event": "wait_timeout" }));
                return None;
            }
            match self.recv(left).await {
                Some(Msg::FromCli(v)) => {
                    if auto_allow {
                        if let Some((req_id, input)) = as_can_use_tool(&v) {
                            self.send_permission_response(
                                &req_id,
                                json!({ "behavior": "allow", "updatedInput": input }),
                            )
                            .await;
                        }
                    }
                    if pred(&v) {
                        return Some(v);
                    }
                }
                Some(Msg::Exit(_)) => return None,
                Some(_) => {}
                None => {
                    self.record("meta", json!({ "event": "wait_timeout" }));
                    return None;
                }
            }
        }
    }

    async fn wait_for_control_response(&mut self, request_id: &str, total: Duration) -> Option<Value> {
        let id = request_id.to_string();
        self.wait_for(
            move |v| {
                v["type"] == "control_response"
                    && v["response"]["request_id"].as_str() == Some(id.as_str())
            },
            total,
            false,
        )
        .await
    }

    async fn shutdown(&mut self) {
        // Close stdin: the CLI should exit on its own and flush its transcript.
        self.record("meta", json!({ "event": "close_stdin" }));
        self.stdin.take();
        let deadline = Instant::now() + Duration::from_secs(20);
        while !self.exited {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                self.record("meta", json!({ "event": "kill_after_timeout" }));
                if let Some(pid) = self.pid {
                    let _ = std::process::Command::new("taskkill")
                        .args(["/F", "/T", "/PID", &pid.to_string()])
                        .output();
                }
                break;
            }
            if self.recv(left).await.is_none() {
                break;
            }
        }
        // Drain anything left in the channel.
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::FromCli(v) => self.record("from_cli", v),
                Msg::FromCliUnparsed(s) => {
                    self.record("meta", json!({ "event": "unparseable_stdout", "line": s }))
                }
                Msg::Stderr(s) => self.record("stderr", json!(s)),
                Msg::Exit(code) => {
                    self.exited = true;
                    self.record("meta", json!({ "event": "exit", "code": code }));
                }
            }
        }
    }

    fn write_fixture(&self, name: &str) {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures");
        std::fs::create_dir_all(&dir).unwrap();
        let mut content = self.records.join("\n");
        content.push('\n');
        content = sanitize(&content);
        let path = dir.join(format!("{name}.ndjson"));
        std::fs::write(&path, content).unwrap();
        println!("  wrote {} ({} frames)", path.display(), self.records.len());
    }
}

fn sanitize(s: &str) -> String {
    // Scrub the local username / email from captured frames (paths appear in
    // JSON-escaped form, so cover both separators and the escaped backslash).
    s.replace("C:\\\\Users\\\\grzeg", "C:\\\\Users\\\\USER")
        .replace("C:/Users/grzeg", "C:/Users/USER")
        .replace("C:\\Users\\grzeg", "C:\\Users\\USER")
        .replace("C--Users-grzeg", "C--Users-USER")
        .replace("grzegorzhandzel992@gmail.com", "user@example.com")
}

fn as_can_use_tool(v: &Value) -> Option<(String, Value)> {
    if v["type"] == "control_request" && v["request"]["subtype"] == "can_use_tool" {
        Some((
            v["request_id"].as_str()?.to_string(),
            v["request"]["input"].clone(),
        ))
    } else {
        None
    }
}

fn is_result(v: &Value) -> bool {
    v["type"] == "result"
}

fn is_init(v: &Value) -> bool {
    v["type"] == "system" && v["subtype"] == "init"
}

fn is_stream_delta(v: &Value) -> bool {
    v["type"] == "stream_event" && v["event"]["type"] == "content_block_delta"
}

fn workspace(name: &str, git: bool) -> PathBuf {
    let dir = std::env::temp_dir().join("lighter-probe").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    if git {
        let ok = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&dir)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok, "git init failed in {}", dir.display());
    }
    dir
}

async fn scenario_basic() {
    let cwd = workspace("basic", false);
    let mut p = Probe::spawn(&cwd, &[], None).await;
    p.send_user("Say exactly: HELLO").await;
    p.wait_for(is_result, Duration::from_secs(90), false).await;
    p.shutdown().await;
    p.write_fixture("basic");
}

async fn scenario_handshake() {
    let cwd = workspace("handshake", false);
    let mut p = Probe::spawn(&cwd, &[], None).await;
    // Mirror the Agent SDK: initialize before the first user message.
    let req_id = p
        .send_control(json!({ "subtype": "initialize", "hooks": null }))
        .await;
    p.wait_for_control_response(&req_id, Duration::from_secs(30))
        .await;
    p.send_user("Say exactly: HI").await;
    p.wait_for(is_result, Duration::from_secs(90), false).await;
    p.shutdown().await;
    p.write_fixture("handshake");
}

async fn scenario_controls() {
    let cwd = workspace("controls", false);
    let mut p = Probe::spawn(&cwd, &["--permission-mode", "manual"], None).await;
    p.wait_for(is_init, Duration::from_secs(60), false).await;

    for (label, req) in [
        (
            "set_mode_ok",
            json!({ "subtype": "set_permission_mode", "mode": "acceptEdits" }),
        ),
        (
            "set_mode_bogus",
            json!({ "subtype": "set_permission_mode", "mode": "definitely-not-a-mode" }),
        ),
        ("set_model", json!({ "subtype": "set_model", "model": "sonnet" })),
        ("interrupt_idle", json!({ "subtype": "interrupt" })),
        ("bogus_subtype", json!({ "subtype": "lighter_made_this_up" })),
    ] {
        p.record("meta", json!({ "event": "control_test", "label": label }));
        let req_id = p.send_control(req).await;
        p.wait_for_control_response(&req_id, Duration::from_secs(15))
            .await;
    }

    // Confirm the mode actually stuck / model actually switched: one tiny turn.
    p.send_user("Say exactly: OK").await;
    p.wait_for(is_result, Duration::from_secs(90), false).await;
    p.shutdown().await;
    p.write_fixture("controls");
}

async fn scenario_tool(name: &str, deny: bool) {
    let cwd = workspace(name, true);
    let mut p = Probe::spawn(&cwd, &["--permission-mode", "manual"], None).await;
    // Write mutates the filesystem, so unlike read-only Bash commands it is
    // never auto-approved by the safe-command classifier — it must round-trip
    // through our stdio permission prompt.
    p.send_user("Use the Write tool to create a file named probe.txt containing exactly: probe-123")
        .await;

    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        match p.recv(left).await {
            Some(Msg::FromCli(v)) => {
                if let Some((req_id, input)) = as_can_use_tool(&v) {
                    if deny {
                        p.send_permission_response(
                            &req_id,
                            json!({ "behavior": "deny", "message": "Denied by probe test" }),
                        )
                        .await;
                    } else {
                        p.send_permission_response(
                            &req_id,
                            json!({ "behavior": "allow", "updatedInput": input }),
                        )
                        .await;
                    }
                }
                if is_result(&v) {
                    break;
                }
            }
            Some(Msg::Exit(_)) | None => break,
            Some(_) => {}
        }
    }
    p.shutdown().await;
    p.write_fixture(name);
}

async fn scenario_always_allow() {
    let cwd = workspace("always_allow", true);
    let mut p = Probe::spawn(&cwd, &["--permission-mode", "manual"], None).await;

    // Turn 1: approve and echo back whatever permission_suggestions the CLI offers.
    p.send_user("Use the Write tool to create a file named aa-one.txt containing exactly: one")
        .await;
    let deadline = Instant::now() + Duration::from_secs(150);
    let mut turn = 1;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        match p.recv(left).await {
            Some(Msg::FromCli(v)) => {
                if v["type"] == "control_request" && v["request"]["subtype"] == "can_use_tool" {
                    let req_id = v["request_id"].as_str().unwrap_or_default().to_string();
                    let input = v["request"]["input"].clone();
                    let suggestions = v["request"]["permission_suggestions"].clone();
                    let mut response = json!({ "behavior": "allow", "updatedInput": input });
                    if suggestions.is_array() {
                        response["updatedPermissions"] = suggestions;
                    }
                    p.record(
                        "meta",
                        json!({ "event": "always_allow_response", "turn": turn }),
                    );
                    p.send_permission_response(&req_id, response).await;
                }
                if is_result(&v) {
                    if turn == 1 {
                        turn = 2;
                        p.record("meta", json!({ "event": "second_turn_start" }));
                        p.send_user(
                            "Use the Write tool to create a file named aa-two.txt containing exactly: two",
                        )
                        .await;
                    } else {
                        break;
                    }
                }
            }
            Some(Msg::Exit(_)) | None => break,
            Some(_) => {}
        }
    }
    p.shutdown().await;
    p.write_fixture("always_allow");
}

async fn scenario_interrupt() {
    let cwd = workspace("interrupt", false);
    let mut p = Probe::spawn(&cwd, &[], None).await;
    p.send_user("Count from 1 to 300, one number per line, no other text.")
        .await;

    // Let it stream a bit, then interrupt.
    let mut deltas = 0;
    let deadline = Instant::now() + Duration::from_secs(60);
    while deltas < 12 && Instant::now() < deadline {
        match p.recv(Duration::from_secs(10)).await {
            Some(Msg::FromCli(v)) if is_stream_delta(&v) => deltas += 1,
            Some(Msg::Exit(_)) | None => break,
            Some(_) => {}
        }
    }
    p.record(
        "meta",
        json!({ "event": "sending_interrupt", "deltas_seen": deltas }),
    );
    let req_id = p.send_control(json!({ "subtype": "interrupt" })).await;
    p.wait_for_control_response(&req_id, Duration::from_secs(30))
        .await;
    p.wait_for(is_result, Duration::from_secs(30), false).await;
    p.shutdown().await;
    p.write_fixture("interrupt");
}

async fn scenario_midturn() {
    let cwd = workspace("midturn", false);
    let mut p = Probe::spawn(&cwd, &[], None).await;
    p.send_user("Count from 1 to 40, one number per line, no other text.")
        .await;
    // As soon as streaming starts, inject a second user message mid-turn.
    p.wait_for(is_stream_delta, Duration::from_secs(60), false)
        .await;
    p.record("meta", json!({ "event": "sending_midturn_message" }));
    p.send_user("Say exactly: DONE").await;
    // Observe: one result or two? Does the CLI queue the message?
    p.wait_for(is_result, Duration::from_secs(90), false).await;
    let second = p.wait_for(is_result, Duration::from_secs(60), false).await;
    p.record(
        "meta",
        json!({ "event": "second_result_observed", "observed": second.is_some() }),
    );
    p.shutdown().await;
    p.write_fixture("midturn");
}

async fn scenario_resume() {
    let cwd = workspace("resume", false);
    let mut p = Probe::spawn(&cwd, &[], None).await;
    let session_id = p.session_id.clone();
    p.send_user("Remember the codeword: PINEAPPLE. Just confirm you memorized it.")
        .await;
    p.wait_for(is_result, Duration::from_secs(90), false).await;
    p.shutdown().await;
    p.write_fixture("resume_a");

    let mut p2 = Probe::spawn(&cwd, &[], Some(&session_id)).await;
    p2.send_user("What is the codeword? Answer with just the codeword.")
        .await;
    p2.wait_for(is_result, Duration::from_secs(90), false).await;
    p2.shutdown().await;
    p2.write_fixture("resume_b");
}

async fn scenario_compact() {
    let cwd = workspace("compact", false);
    let mut p = Probe::spawn(&cwd, &[], None).await;
    p.send_user("Say exactly: OK").await;
    p.wait_for(is_result, Duration::from_secs(90), false).await;
    p.record("meta", json!({ "event": "sending_slash_compact" }));
    p.send_user("/compact").await;
    p.wait_for(is_result, Duration::from_secs(120), false).await;
    p.shutdown().await;
    p.write_fixture("compact");
}

async fn scenario_subagent() {
    let cwd = workspace("subagent", false);
    let mut p = Probe::spawn(
        &cwd,
        &["--forward-subagent-text", "--permission-mode", "manual"],
        None,
    )
    .await;
    p.send_user(
        "Use the Task tool to launch an agent that computes 2+2 and reports the result. Then tell me what it said.",
    )
    .await;
    p.wait_for(is_result, Duration::from_secs(180), true).await;
    p.shutdown().await;
    p.write_fixture("subagent");
}

/// AskUserQuestion: the client is expected to collect answers inside the
/// can_use_tool flow and return them via updatedInput.answers. Verify.
async fn scenario_ask_question() {
    let cwd = workspace("ask_question", false);
    let mut p = Probe::spawn(&cwd, &["--permission-mode", "default"], None).await;
    p.send_user(
        "Use the AskUserQuestion tool to ask me ONE question: which color do I prefer, with exactly two options: Red and Blue. Afterwards, tell me in plain text which color I chose.",
    )
    .await;

    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        match p.recv(left).await {
            Some(Msg::FromCli(v)) => {
                if v["type"] == "control_request"
                    && v["request"]["subtype"] == "can_use_tool"
                {
                    let req_id = v["request_id"].as_str().unwrap_or_default().to_string();
                    let tool = v["request"]["tool_name"].as_str().unwrap_or_default();
                    let mut input = v["request"]["input"].clone();
                    if tool == "AskUserQuestion" {
                        // Answer the first question with its first option label.
                        let question = input["questions"][0]["question"]
                            .as_str()
                            .unwrap_or("?")
                            .to_string();
                        let label = input["questions"][0]["options"][0]["label"]
                            .as_str()
                            .unwrap_or("Red")
                            .to_string();
                        input["answers"] = json!({ question.clone(): label.clone() });
                        p.record(
                            "meta",
                            json!({ "event": "answering_question", "question": question, "label": label }),
                        );
                    }
                    p.send_permission_response(
                        &req_id,
                        json!({ "behavior": "allow", "updatedInput": input }),
                    )
                    .await;
                }
                if is_result(&v) {
                    break;
                }
            }
            Some(Msg::Exit(_)) | None => break,
            Some(_) => {}
        }
    }
    p.shutdown().await;
    p.write_fixture("ask_question");
}

#[tokio::main]
async fn main() {
    let arg = std::env::args().nth(1).unwrap_or_else(|| "all".into());
    let selected: Vec<&str> = if arg == "all" {
        SCENARIOS.to_vec()
    } else {
        vec![Box::leak(arg.into_boxed_str())]
    };

    for name in selected {
        println!("== scenario: {name}");
        let run = async {
            match name {
                "basic" => scenario_basic().await,
                "handshake" => scenario_handshake().await,
                "controls" => scenario_controls().await,
                "tool_approve" => scenario_tool("tool_approve", false).await,
                "tool_deny" => scenario_tool("tool_deny", true).await,
                "always_allow" => scenario_always_allow().await,
                "interrupt" => scenario_interrupt().await,
                "midturn" => scenario_midturn().await,
                "resume" => scenario_resume().await,
                "compact" => scenario_compact().await,
                "subagent" => scenario_subagent().await,
                "ask_question" => scenario_ask_question().await,
                other => println!("  unknown scenario: {other}"),
            }
        };
        // Hard cap per scenario so a wedged CLI can't hang the whole probe run.
        if tokio::time::timeout(Duration::from_secs(420), run)
            .await
            .is_err()
        {
            println!("  scenario {name} hit the global timeout");
        }
    }
    println!("done.");
}
