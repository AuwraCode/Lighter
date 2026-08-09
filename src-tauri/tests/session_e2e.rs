//! Live end-to-end test of the session backend against the real claude CLI.
//! Costs a few cents (haiku) and requires a logged-in `claude` — therefore
//! ignored by default:
//!
//!   cargo test --test session_e2e -- --ignored --nocapture

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use tauri::ipc::{Channel, InvokeResponseBody};

use lighter_lib::session::events::{SessionConfig, SessionStatus};
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
