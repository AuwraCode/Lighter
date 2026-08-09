//! Parses every captured fixture and asserts the tolerant parser fully
//! understands claude 2.1.226's output: zero `Frame::Unknown`, and the
//! load-bearing frames carry the fields the app depends on.
//!
//! Fixtures are recorded from the real CLI via `cargo run --bin probe -- all`
//! and change on every re-capture, so tests assert structure, not content.

use lighter_lib::protocol::inbound::{parse_line, Frame, InboundControlRequest};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn fixture_lines() -> Vec<(String, String, Value)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("fixtures dir missing — run `cargo run --bin probe -- all`") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("ndjson") {
            continue;
        }
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let content = std::fs::read_to_string(&path).unwrap();
        for line in content.lines().filter(|l| !l.trim().is_empty()) {
            let wrapper: Value = serde_json::from_str(line).unwrap();
            out.push((
                name.clone(),
                wrapper["dir"].as_str().unwrap().to_string(),
                wrapper["frame"].clone(),
            ));
        }
    }
    assert!(!out.is_empty(), "no fixture lines found");
    out
}

#[test]
fn every_cli_frame_parses_to_a_known_variant() {
    let mut unknown: Vec<String> = Vec::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();

    for (fixture, dir, frame) in fixture_lines() {
        if dir != "from_cli" {
            continue;
        }
        let line = serde_json::to_string(&frame).unwrap();
        let parsed = parse_line(&line).expect("fixture line must be valid JSON");
        let descriptor = parsed.descriptor();
        *counts.entry(descriptor.clone()).or_default() += 1;
        if matches!(parsed, Frame::Unknown(_)) {
            unknown.push(format!("{fixture}: {}", &line[..line.len().min(300)]));
        }
    }

    println!("frame variant counts across all fixtures:");
    for (k, v) in &counts {
        println!("  {v:>4}  {k}");
    }
    assert!(
        unknown.is_empty(),
        "unparsed frames:\n{}",
        unknown.join("\n")
    );
}

#[test]
fn init_frames_carry_session_essentials() {
    let mut seen = 0;
    for (_, dir, frame) in fixture_lines() {
        if dir != "from_cli" {
            continue;
        }
        if let Frame::Init(init) = parse_line(&serde_json::to_string(&frame).unwrap()).unwrap() {
            seen += 1;
            assert!(!init.session_id.is_empty());
            assert!(!init.cwd.is_empty());
            assert!(!init.model.is_empty());
            assert!(!init.permission_mode.is_empty());
            assert!(!init.tools.is_empty());
            assert!(!init.slash_commands.is_empty());
        }
    }
    assert!(seen >= 5, "expected init frames across fixtures, saw {seen}");
}

#[test]
fn results_carry_cost_and_error_semantics() {
    let mut success = 0;
    let mut errors = 0;
    for (fixture, dir, frame) in fixture_lines() {
        if dir != "from_cli" {
            continue;
        }
        if let Frame::Result(r) = parse_line(&serde_json::to_string(&frame).unwrap()).unwrap() {
            assert!(
                r.total_cost_usd.is_some(),
                "{fixture}: result without total_cost_usd"
            );
            assert!(r.usage.is_some());
            if r.is_error {
                errors += 1;
                assert_ne!(r.subtype, "success");
            } else {
                success += 1;
            }
        }
    }
    assert!(success >= 8, "expected successful results, saw {success}");
    assert!(
        errors >= 1,
        "expected at least the interrupt error result, saw {errors}"
    );
}

#[test]
fn permission_prompts_round_trip() {
    let mut prompts = 0;
    for (_, dir, frame) in fixture_lines() {
        if dir != "from_cli" {
            continue;
        }
        if let Frame::ControlRequest(req) =
            parse_line(&serde_json::to_string(&frame).unwrap()).unwrap()
        {
            if let InboundControlRequest::CanUseTool {
                tool_name, input, ..
            } = &req.request
            {
                prompts += 1;
                assert!(!request_id_is_empty(&req.request_id));
                assert!(!tool_name.is_empty());
                assert!(input.is_object());
            }
        }
    }
    assert!(
        prompts >= 2,
        "expected can_use_tool prompts in tool fixtures, saw {prompts}"
    );
}

fn request_id_is_empty(id: &str) -> bool {
    id.trim().is_empty()
}

#[test]
fn interrupt_produces_error_result_and_injected_user_frame() {
    let mut saw_aborted = false;
    let mut saw_injected_user = false;
    for (fixture, dir, frame) in fixture_lines() {
        if fixture != "interrupt" || dir != "from_cli" {
            continue;
        }
        match parse_line(&serde_json::to_string(&frame).unwrap()).unwrap() {
            Frame::Result(r) => {
                if r.is_error && r.terminal_reason.as_deref() == Some("aborted_streaming") {
                    saw_aborted = true;
                }
            }
            Frame::User(u) if !u.is_replay => {
                saw_injected_user = true;
            }
            _ => {}
        }
    }
    assert!(saw_aborted, "interrupt fixture must contain aborted result");
    assert!(saw_injected_user, "interrupt injects a user frame");
}

#[test]
fn subagent_frames_carry_parent_tool_use_id() {
    let mut nested = 0;
    for (fixture, dir, frame) in fixture_lines() {
        if fixture != "subagent" || dir != "from_cli" {
            continue;
        }
        match parse_line(&serde_json::to_string(&frame).unwrap()).unwrap() {
            Frame::Assistant(a) if a.parent_tool_use_id.is_some() => nested += 1,
            Frame::User(u) if u.parent_tool_use_id.is_some() => nested += 1,
            _ => {}
        }
    }
    assert!(nested >= 2, "expected nested subagent frames, saw {nested}");
}
