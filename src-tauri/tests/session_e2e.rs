//! Live end-to-end test of the session backend against the real claude CLI.
//! Costs a few cents (haiku) and requires a logged-in `claude` — therefore
//! ignored by default:
//!
//!   cargo test --test session_e2e -- --ignored --nocapture

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use tauri::ipc::{Channel, InvokeResponseBody};

use lighter_lib::session::events::{PermissionDecisionDto, SessionConfig, SessionStatus};
use lighter_lib::session::manager::SessionManager;
use lighter_lib::session::router::SessionCommand;

type EventLog = Arc<Mutex<Vec<Value>>>;

fn capture_channel(log: EventLog) -> Channel<lighter_lib::session::events::Batch> {
    Channel::new(move |body| {
        if let InvokeResponseBody::Json(json) = body {
            if let Ok(batch) = serde_json::from_str::<Value>(&json) {
                if let Some(events) = batch["events"].as_array() {
                    let mut log = log.lock().unwrap();
                    for env in events {
                        log.push(env["event"].clone());
                    }
                }
            }
        }
        Ok(())
    })
}

fn wait_for<F: Fn(&[Value]) -> bool>(log: &EventLog, pred: F, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        {
            let events = log.lock().unwrap();
            if pred(&events) {
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn has_event<'a>(events: &'a [Value], ty: &'a str) -> impl Iterator<Item = &'a Value> + 'a {
    events.iter().filter(move |e| e["type"] == ty)
}

fn config(cwd: &str) -> SessionConfig {
    SessionConfig {
        cwd: cwd.to_string(),
        model: Some("haiku".into()),
        ..Default::default()
    }
}

#[test]
#[ignore = "requires logged-in claude CLI and spends API credits"]
fn resume_restores_context_history_and_cost() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();

    let tmp = std::env::temp_dir().join("lighter-e2e-resume");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let cwd = tmp.to_str().unwrap();

    let manager = SessionManager::default();

    // Session 1: teach it a codeword, stop gracefully.
    let log1: EventLog = Arc::new(Mutex::new(Vec::new()));
    let s1 = manager
        .create(config(cwd), capture_channel(log1.clone()))
        .unwrap();
    manager
        .command(
            s1.id,
            SessionCommand::SendUser {
                text: "Remember the codeword: MANGO. Just confirm you memorized it.".into(),
            },
        )
        .unwrap();
    assert!(wait_for(&log1, |e| has_event(e, "TurnCompleted").next().is_some(), Duration::from_secs(90)));
    manager
        .command(s1.id, SessionCommand::Stop { graceful: true })
        .unwrap();
    assert!(wait_for(&log1, |e| has_event(e, "Exited").next().is_some(), Duration::from_secs(25)));
    manager.remove(s1.id, false).unwrap();

    // Transcript backfill from the CLI's own JSONL.
    let history = lighter_lib::history::load_history(cwd, s1.id).expect("history backfill");
    let history_text = serde_json::to_string(&history).unwrap();
    assert!(
        history_text.contains("MANGO"),
        "backfilled history should contain the codeword"
    );
    println!("history backfill ok ({} items)", history.len());

    // Resume with a base cost: context + cost both carry over.
    let log2: EventLog = Arc::new(Mutex::new(Vec::new()));
    let mut cfg = config(cwd);
    cfg.resume_session_id = Some(s1.id.to_string());
    cfg.worktree_policy = Some("never".into());
    let s2 = manager
        .create_with_base_cost(cfg, capture_channel(log2.clone()), 1.25)
        .unwrap();
    assert_eq!(s2.id, s1.id, "resume keeps the session id");
    manager
        .command(
            s2.id,
            SessionCommand::SendUser {
                text: "What is the codeword? Answer with just the codeword.".into(),
            },
        )
        .unwrap();
    assert!(wait_for(&log2, |e| has_event(e, "TurnCompleted").next().is_some(), Duration::from_secs(90)));
    {
        let events = log2.lock().unwrap();
        let turn = has_event(&events, "TurnCompleted").next().unwrap();
        let result_text = turn["stats"]["result_text"].as_str().unwrap_or_default();
        assert!(
            result_text.contains("MANGO"),
            "resumed session forgot the codeword: {result_text}"
        );
        let total = turn["stats"]["total_cost_usd"].as_f64().unwrap();
        assert!(
            total >= 1.25,
            "resumed cost must include the base cost, got {total}"
        );
    }
    manager
        .command(s2.id, SessionCommand::Stop { graceful: true })
        .unwrap();
    assert!(wait_for(&log2, |e| has_event(e, "Exited").next().is_some(), Duration::from_secs(25)));
    println!("resume e2e ok");
}

#[test]
#[ignore = "requires logged-in claude CLI and spends API credits"]
fn same_repo_second_session_gets_worktree() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();

    // A real repo with one commit.
    let repo = std::env::temp_dir().join("lighter-e2e-repo");
    let _ = std::fs::remove_dir_all(&repo);
    std::fs::create_dir_all(&repo).unwrap();
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["config", "user.email", "t@t.local"],
        vec!["config", "user.name", "t"],
    ] {
        assert!(std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(&args)
            .status()
            .unwrap()
            .success());
    }
    std::fs::write(repo.join("readme.md"), "hi").unwrap();
    for args in [vec!["add", "."], vec!["commit", "-q", "-m", "init"]] {
        assert!(std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(&args)
            .status()
            .unwrap()
            .success());
    }

    let manager = SessionManager::default();
    let cwd = repo.to_str().unwrap();

    // First session: no isolation needed (repo is free).
    let log1: EventLog = Arc::new(Mutex::new(Vec::new()));
    let s1 = manager
        .create(config(cwd), capture_channel(log1.clone()))
        .unwrap();
    assert_eq!(
        dunce::canonicalize(&s1.cwd).unwrap(),
        dunce::canonicalize(&repo).unwrap(),
        "first session must use the repo directly"
    );

    // Second session on the same repo: lands in a lighter/* worktree.
    let log2: EventLog = Arc::new(Mutex::new(Vec::new()));
    let s2 = manager
        .create(config(cwd), capture_channel(log2.clone()))
        .unwrap();
    assert_ne!(s2.cwd, s1.cwd, "second session must be isolated");
    assert!(
        s2.cwd.contains("worktrees"),
        "unexpected worktree location: {}",
        s2.cwd
    );
    assert!(std::path::Path::new(&s2.cwd).join("readme.md").exists());

    // Both actually work, and the CLI runs inside the worktree.
    for (log, id) in [(&log1, s1.id), (&log2, s2.id)] {
        manager
            .command(id, SessionCommand::SendUser { text: "Say exactly: OK".into() })
            .unwrap();
        assert!(
            wait_for(log, |e| has_event(e, "TurnCompleted").next().is_some(), Duration::from_secs(120)),
            "session did not complete"
        );
    }
    {
        let events = log2.lock().unwrap();
        let ready = has_event(&events, "Ready").last().unwrap();
        assert_eq!(
            dunce::canonicalize(ready["meta"]["cwd"].as_str().unwrap()).unwrap(),
            dunce::canonicalize(&s2.cwd).unwrap(),
            "CLI init cwd must be the worktree"
        );
        assert!(ready["meta"]["worktree"]["branch"]
            .as_str()
            .unwrap()
            .starts_with("lighter/"));
    }

    // Cleanup: stop both, remove second with worktree cleanup.
    for (log, id) in [(&log1, s1.id), (&log2, s2.id)] {
        manager
            .command(id, SessionCommand::Stop { graceful: true })
            .unwrap();
        assert!(wait_for(log, |e| has_event(e, "Exited").next().is_some(), Duration::from_secs(25)));
    }
    let warning = manager.remove(s2.id, true).unwrap();
    assert!(warning.is_none(), "clean worktree removal warned: {warning:?}");
    assert!(!std::path::Path::new(&s2.cwd).exists(), "worktree dir must be gone");
    println!("worktree isolation e2e ok");
}

#[test]
#[ignore = "requires logged-in claude CLI and spends API credits"]
fn multi_session_parallel_with_registry() {
    use std::collections::HashMap;

    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();

    let manager = SessionManager::default();

    // Registry: merge batches into a live map, exactly like the frontend does.
    let registry: Arc<Mutex<HashMap<String, Value>>> = Arc::new(Mutex::new(HashMap::new()));
    let reg_map = registry.clone();
    let reg_channel = Channel::new(move |body: InvokeResponseBody| {
        if let InvokeResponseBody::Json(json) = body {
            if let Ok(batch) = serde_json::from_str::<Value>(&json) {
                let mut map = reg_map.lock().unwrap();
                if let Some(updates) = batch["updates"].as_array() {
                    for u in updates {
                        if let Some(id) = u["id"].as_str() {
                            map.insert(id.to_string(), u.clone());
                        }
                    }
                }
                if let Some(removed) = batch["removed"].as_array() {
                    for r in removed {
                        if let Some(id) = r.as_str() {
                            map.remove(id);
                        }
                    }
                }
            }
        }
        Ok(())
    });
    let initial = rt
        .block_on(manager.attach_registry(reg_channel))
        .expect("attach registry");
    assert!(initial.is_empty());

    // Three parallel sessions, each with its own event log.
    let mut ids = Vec::new();
    let mut logs = Vec::new();
    for i in 0..3 {
        let tmp = std::env::temp_dir().join(format!("lighter-e2e-multi-{i}"));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let log: EventLog = Arc::new(Mutex::new(Vec::new()));
        let info = manager
            .create(config(tmp.to_str().unwrap()), capture_channel(log.clone()))
            .expect("create");
        manager
            .command(
                info.id,
                SessionCommand::SendUser {
                    text: format!("Say exactly: S{i}"),
                },
            )
            .unwrap();
        ids.push(info.id);
        logs.push(log);
    }

    for (i, log) in logs.iter().enumerate() {
        assert!(
            wait_for(log, |e| has_event(e, "TurnCompleted").next().is_some(), Duration::from_secs(120)),
            "session {i} did not complete"
        );
    }

    // Registry converges: all three visible, Idle, with real cost.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        {
            let map = registry.lock().unwrap();
            let done = ids.iter().all(|id| {
                map.get(&id.to_string()).is_some_and(|s| {
                    s["status"] == "Idle" && s["total_cost_usd"].as_f64().unwrap_or(0.0) > 0.0
                })
            });
            if done {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "registry did not converge: {:?}",
                map.values()
                    .map(|s| (s["title"].clone(), s["status"].clone()))
                    .collect::<Vec<_>>()
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    println!("registry converged for 3 sessions");

    // Removing a session propagates as a registry removal.
    manager.remove(ids[0], false).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while registry.lock().unwrap().contains_key(&ids[0].to_string()) {
        assert!(Instant::now() < deadline, "removal did not reach registry");
        std::thread::sleep(Duration::from_millis(100));
    }
    println!("registry removal ok");

    for id in &ids[1..] {
        manager
            .command(*id, SessionCommand::Stop { graceful: true })
            .unwrap();
    }
    for (i, log) in logs.iter().enumerate().skip(1) {
        assert!(
            wait_for(log, |e| has_event(e, "Exited").next().is_some(), Duration::from_secs(25)),
            "session {i} did not exit"
        );
    }
}

#[test]
#[ignore = "requires logged-in claude CLI and spends API credits"]
fn permission_flow_allow_and_deny() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();

    let tmp = std::env::temp_dir().join("lighter-e2e-perm");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let manager = SessionManager::default();
    let log: EventLog = Arc::new(Mutex::new(Vec::new()));
    let channel = capture_channel(log.clone());
    let mut cfg = config(tmp.to_str().unwrap());
    cfg.permission_mode = Some("manual".into()); // everything prompts

    let info = manager.create(cfg, channel).expect("create session");

    let respond = |request_id: String, allow: bool| {
        let (reply, rx) = tokio::sync::oneshot::channel();
        manager
            .command(
                info.id,
                SessionCommand::RespondPermission {
                    request_id,
                    decision: PermissionDecisionDto {
                        allow,
                        use_suggestions: false,
                        message: (!allow).then(|| "Not allowed in this test".to_string()),
                        interrupt: false,
                    },
                    reply,
                },
            )
            .unwrap();
        rt.block_on(async {
            tokio::time::timeout(Duration::from_secs(10), rx)
                .await
                .expect("respond timeout")
                .expect("respond dropped")
                .expect("respond failed");
        });
    };

    let find_request = |from: usize| -> Option<String> {
        let events = log.lock().unwrap();
        events[from..]
            .iter()
            .find(|e| e["type"] == "PermissionRequested")
            .and_then(|e| e["request"]["request_id"].as_str().map(String::from))
    };

    // Turn 1: allow → file is written.
    manager
        .command(
            info.id,
            SessionCommand::SendUser {
                text: "Use the Write tool to create a file named allowed.txt containing exactly: ok-allow".into(),
            },
        )
        .unwrap();
    assert!(
        wait_for(&log, |e| has_event(e, "PermissionRequested").next().is_some(), Duration::from_secs(90)),
        "no permission prompt for Write"
    );
    respond(find_request(0).unwrap(), true);
    assert!(
        wait_for(&log, |e| has_event(e, "TurnCompleted").next().is_some(), Duration::from_secs(90)),
        "allow turn did not complete"
    );
    let written = std::fs::read_to_string(tmp.join("allowed.txt")).expect("allowed.txt missing");
    assert!(written.contains("ok-allow"));
    {
        let events = log.lock().unwrap();
        assert!(events
            .iter()
            .any(|e| e["type"] == "PermissionResolved" && e["outcome"] == "Allowed"));
    }
    println!("allow flow ok");

    // Turn 2: deny → file must not exist, tool result is an error.
    let mark = log.lock().unwrap().len();
    manager
        .command(
            info.id,
            SessionCommand::SendUser {
                text: "Use the Write tool to create a file named denied.txt containing exactly: nope".into(),
            },
        )
        .unwrap();
    assert!(
        wait_for(&log, |e| e[mark..].iter().any(|ev| ev["type"] == "PermissionRequested"), Duration::from_secs(90)),
        "no permission prompt for second Write"
    );
    respond(find_request(mark).unwrap(), false);
    assert!(
        wait_for(&log, |e| e[mark..].iter().any(|ev| ev["type"] == "TurnCompleted"), Duration::from_secs(90)),
        "deny turn did not complete"
    );
    assert!(!tmp.join("denied.txt").exists(), "denied.txt must not be written");
    {
        let events = log.lock().unwrap();
        assert!(events[mark..]
            .iter()
            .any(|e| e["type"] == "PermissionResolved" && e["outcome"] == "Denied"));
        let denied_tool_errored = events[mark..].iter().any(|e| {
            (e["type"] == "ItemUpdated" || e["type"] == "ItemCompleted")
                && e["item"]["kind"] == "ToolUse"
                && e["item"]["output"]["is_error"] == true
        });
        assert!(denied_tool_errored, "denied tool_use should carry an error result");
    }
    println!("deny flow ok");

    manager
        .command(info.id, SessionCommand::Stop { graceful: true })
        .unwrap();
    assert!(
        wait_for(&log, |e| has_event(e, "Exited").next().is_some(), Duration::from_secs(25)),
        "no Exited event"
    );
}

#[test]
#[ignore = "requires logged-in claude CLI and spends API credits"]
fn single_session_roundtrip_interrupt_and_stop() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();

    let tmp = std::env::temp_dir().join("lighter-e2e");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let manager = SessionManager::default();
    let log: EventLog = Arc::new(Mutex::new(Vec::new()));
    let channel = capture_channel(log.clone());

    let info = manager
        .create(config(tmp.to_str().unwrap()), channel)
        .expect("create session");
    println!("session {} started", info.id);

    // Handshake response arrives without any user turn.
    assert!(
        wait_for(&log, |e| has_event(e, "Handshake").next().is_some(), Duration::from_secs(20)),
        "no Handshake event — initialize control request failed"
    );

    // Turn 1: round trip.
    manager
        .command(info.id, SessionCommand::SendUser { text: "Say exactly: ROUNDTRIP".into() })
        .unwrap();
    assert!(
        wait_for(&log, |e| has_event(e, "TurnCompleted").next().is_some(), Duration::from_secs(90)),
        "turn 1 did not complete"
    );
    {
        let events = log.lock().unwrap();
        let ready = has_event(&events, "Ready").last().expect("Ready event");
        assert_eq!(ready["meta"]["session_id"].as_str().unwrap(), info.id.to_string());
        assert!(!ready["meta"]["model"].as_str().unwrap().is_empty());
        let texts: Vec<&str> = events
            .iter()
            .filter(|e| e["type"] == "ItemCompleted" && e["item"]["kind"] == "AssistantText")
            .filter_map(|e| e["item"]["text"].as_str())
            .collect();
        assert!(
            texts.iter().any(|t| t.contains("ROUNDTRIP")),
            "assistant reply missing ROUNDTRIP: {texts:?}"
        );
        let deltas = events.iter().filter(|e| e["type"] == "ItemDelta").count();
        assert!(deltas > 0, "no streaming deltas observed");
        let turn = has_event(&events, "TurnCompleted").next().unwrap();
        assert!(turn["stats"]["total_cost_usd"].as_f64().unwrap() > 0.0);
    }
    println!("turn 1 ok");

    // Turn 2: interrupt mid-stream.
    let before = log.lock().unwrap().len();
    manager
        .command(
            info.id,
            SessionCommand::SendUser {
                text: "Count from 1 to 300, one number per line, no other text.".into(),
            },
        )
        .unwrap();
    assert!(
        wait_for(
            &log,
            |e| e[before..].iter().any(|ev| ev["type"] == "ItemDelta"),
            Duration::from_secs(60)
        ),
        "no deltas for counting turn"
    );
    let (reply, rx) = tokio::sync::oneshot::channel();
    manager
        .command(info.id, SessionCommand::Interrupt { reply })
        .unwrap();
    rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(15), rx)
            .await
            .expect("interrupt reply timeout")
            .expect("interrupt reply dropped")
            .expect("interrupt failed");
    });
    assert!(
        wait_for(
            &log,
            |e| {
                e[before..]
                    .iter()
                    .any(|ev| ev["type"] == "TurnCompleted" && ev["stats"]["is_error"] == true)
            },
            Duration::from_secs(30)
        ),
        "interrupted turn did not produce error result"
    );
    println!("interrupt ok");

    // Turn 3: focus gating. Unfocused sessions must receive no ItemDelta;
    // refocusing syncs in-flight text via ItemUpdated.
    let before = log.lock().unwrap().len();
    manager
        .command(
            info.id,
            SessionCommand::SendUser {
                text: "Count from 1 to 300, one number per line, no other text.".into(),
            },
        )
        .unwrap();
    assert!(
        wait_for(
            &log,
            |e| e[before..].iter().any(|ev| ev["type"] == "ItemDelta"),
            Duration::from_secs(60)
        ),
        "no deltas for focus-gating turn"
    );
    manager
        .command(info.id, SessionCommand::SetFocus(false))
        .unwrap();
    std::thread::sleep(Duration::from_millis(500)); // drain in-flight flushes
    let mark = log.lock().unwrap().len();
    std::thread::sleep(Duration::from_secs(3));
    {
        let events = log.lock().unwrap();
        let deltas_while_unfocused = events[mark..]
            .iter()
            .filter(|e| e["type"] == "ItemDelta")
            .count();
        assert_eq!(
            deltas_while_unfocused, 0,
            "unfocused session must not receive deltas"
        );
    }
    manager
        .command(info.id, SessionCommand::SetFocus(true))
        .unwrap();
    assert!(
        wait_for(
            &log,
            |e| e[mark..].iter().any(|ev| ev["type"] == "ItemUpdated"
                || ev["type"] == "TurnCompleted"),
            Duration::from_secs(10)
        ),
        "refocus did not sync in-flight items"
    );
    println!("focus gating ok");
    let (reply, rx) = tokio::sync::oneshot::channel();
    manager
        .command(info.id, SessionCommand::Interrupt { reply })
        .unwrap();
    let _ = rt.block_on(async { tokio::time::timeout(Duration::from_secs(15), rx).await });

    // Graceful stop: CLI exits on stdin close, Exited event arrives.
    manager
        .command(info.id, SessionCommand::Stop { graceful: true })
        .unwrap();
    assert!(
        wait_for(&log, |e| has_event(e, "Exited").next().is_some(), Duration::from_secs(25)),
        "no Exited event after graceful stop"
    );
    {
        let events = log.lock().unwrap();
        // The CLI's exit code mirrors the last turn's status (1 after an
        // interrupted turn) — a requested stop must still read as Exited.
        let status = has_event(&events, "Status").last().unwrap();
        assert_eq!(status["status"], serde_json::to_value(SessionStatus::Exited).unwrap());
    }
    println!("graceful stop ok");
}
