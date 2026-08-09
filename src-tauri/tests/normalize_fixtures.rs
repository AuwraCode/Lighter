//! Runs every fixture through the Rust normalizer and dumps (a) the emitted
//! event stream and (b) the final state snapshot into tests/normalized/.
//! The frontend reducer parity tests (vitest) replay (a) and must arrive at
//! exactly (b) — keeping Rust and TypeScript in lockstep on real CLI data.

use lighter_lib::protocol::inbound::parse_value;
use lighter_lib::session::state::SessionState;
use serde_json::{json, Value};
use std::path::PathBuf;
use uuid::Uuid;

#[test]
fn generate_normalized_dumps() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixtures_dir = manifest.join("tests").join("fixtures");
    let out_dir = manifest.join("tests").join("normalized");
    std::fs::create_dir_all(&out_dir).unwrap();

    let mut names: Vec<String> = std::fs::read_dir(&fixtures_dir)
        .expect("fixtures missing — run `cargo run --bin probe -- all`")
        .filter_map(|e| {
            let path = e.unwrap().path();
            (path.extension().and_then(|x| x.to_str()) == Some("ndjson"))
                .then(|| path.file_stem().unwrap().to_string_lossy().to_string())
        })
        .collect();
    names.sort();
    assert!(!names.is_empty());

    for name in names {
        let content =
            std::fs::read_to_string(fixtures_dir.join(format!("{name}.ndjson"))).unwrap();
        let mut state = SessionState::new(Uuid::nil(), "test".into(), "C:/test".into());
        let mut seq = 0u64;
        let mut envelopes: Vec<Value> = Vec::new();

        for line in content.lines().filter(|l| !l.trim().is_empty()) {
            let wrapper: Value = serde_json::from_str(line).unwrap();
            if wrapper["dir"] != "from_cli" {
                continue;
            }
            let frame = parse_value(wrapper["frame"].clone());
            for event in state.apply_frame(frame) {
                seq += 1;
                envelopes.push(json!({
                    "seq": seq,
                    "event": serde_json::to_value(&event).unwrap(),
                }));
            }
        }

        assert!(!envelopes.is_empty(), "{name}: normalizer emitted nothing");
        let snapshot = serde_json::to_value(state.snapshot(seq)).unwrap();
        std::fs::write(
            out_dir.join(format!("{name}.events.json")),
            serde_json::to_string_pretty(&envelopes).unwrap(),
        )
        .unwrap();
        std::fs::write(
            out_dir.join(format!("{name}.state.json")),
            serde_json::to_string_pretty(&snapshot).unwrap(),
        )
        .unwrap();
    }
}
