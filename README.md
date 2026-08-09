# Lighter

A multi-session cockpit for [Claude Code](https://claude.com/claude-code):
run, watch and steer up to ~8 parallel `claude` CLI sessions from one
Linear-style desktop app. Windows · Tauri v2 (Rust) · React 19.

![icon](icon-source.png)

## What it does

- **Session manager** — each session is its own `claude` process speaking the
  stream-json protocol over stdin/stdout. Spawned inside a Windows Job Object
  (`KILL_ON_JOB_CLOSE`), so the whole child tree dies with the app — even on a
  crash.
- **Dashboard** — live tiles (status, last message, cost, tokens, pending
  approvals) fed by a 250 ms summary stream; only the focused session receives
  text deltas, so eight streaming sessions don't jank the UI.
- **Full interactivity** — streaming markdown transcript, tool-call cards,
  thinking blocks, permission prompts (allow / always-allow / deny with a
  reason), live permission-mode and model switching, `Esc` interrupt,
  `Ctrl+K` palette with the session's real slash commands.
- **Presets** — named launch configs (folder, model, effort, permission mode,
  tool allow/deny lists, system-prompt append, initial prompt, worktree
  policy). One click → running session.
- **Git worktree isolation** — when two sessions target the same repo, the
  newcomer automatically gets its own `lighter/<slug>` branch + worktree under
  `~/.lighter/worktrees`.
- **Resume** — sessions survive app restarts (`--resume`), cost carries over,
  and earlier turns can be backfilled from the CLI's own transcript files.

## Development

Prereqs: Rust (MSVC), Node 20+, pnpm, and a logged-in
[Claude Code CLI](https://claude.com/claude-code) on PATH.

```powershell
pnpm install
pnpm tauri dev
```

Useful scripts:

| command | what it does |
|---|---|
| `pnpm tauri dev` | run the app with hot reload |
| `pnpm tauri build` | build the NSIS installer |
| `pnpm test` | frontend reducer parity tests (vitest) |
| `pnpm typegen` | regenerate TypeScript bindings from Rust types (ts-rs) |
| `cargo test` (in `src-tauri`) | protocol parser + unit tests against recorded fixtures |
| `cargo run --bin probe -- all` (in `src-tauri`) | re-record protocol fixtures from the real CLI (spends a few cents, model: haiku) |
| `cargo test --test session_e2e -- --ignored --nocapture` | live backend e2e (roundtrip, interrupt, permissions, worktrees, resume) |

## Architecture (short version)

```
claude.exe ⇄ stdin/stdout NDJSON
   │  reader (16 MB line cap) / two-lane writer (control > data)
   ▼
router task (one per session) — owns SessionState, normalizes frames
   │  seq-stamped events, 33 ms delta coalescing, focus gating
   ▼
tauri ipc Channel per session + one global registry channel (250 ms summaries)
   │
React: vanilla zustand store per session + registry store
   └─ virtualized transcript, markdown on completed items only
```

- The wire protocol is documented from live captures in
  [`src-tauri/PROTOCOL.md`](src-tauri/PROTOCOL.md); the parser is tolerant by
  construction (unknown frames never crash) and tested against fixtures in
  `src-tauri/tests/fixtures/`.
- Webview reloads are lossless: state lives in Rust, attach returns an atomic
  snapshot, and the TS reducer is verified to replay the Rust normalizer's
  output to the byte (`src/stores/session.test.ts`).
- Verified against claude CLI **2.1.226**; the app shows a notice when it
  detects a different version.

## Status

Feature-complete v0.1: all planned phases (protocol probe → parser → session
core → batching → permissions → multi-session dashboard → presets → worktrees
→ resume → polish → packaging) are implemented with unit + live e2e coverage.
