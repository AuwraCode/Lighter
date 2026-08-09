# Claude Code stream-json protocol — observed reference (CLI 2.1.226)

Everything below was captured live by `cargo run --bin probe -- all` into
`tests/fixtures/*.ndjson` (direction-tagged NDJSON; sanitized). The parser in
`src/protocol/inbound.rs` is tested against those fixtures (`cargo test`).

## Spawn

```
claude -p --input-format stream-json --output-format stream-json --verbose
  --include-partial-messages --replay-user-messages --permission-prompt-tool stdio
  --session-id <uuid-we-generate> --model <m> --permission-mode <pm> [...]
```

- Process stays alive across turns; closing stdin ends it gracefully (exit 0).
- `--resume <id>` instead of `--session-id` resumes; **history is NOT re-emitted**
  (`resume_b` fixture) — transcript backfill must read the CLI's own JSONL under
  `~/.claude*/projects/<cwd-slug>/<session-id>.jsonl`.
- stderr stays silent in normal operation.

## Readiness & the init frame

- **`system/init` is NOT sent at spawn.** It is emitted after *every* user
  message (one per turn start, also after `/compact`). Do not block on it.
- The **`initialize` control request is optional** but answered immediately
  (<1s, before any turn): rich payload with `commands` (name, description,
  argumentHint, aliases), `models` (value, displayName, description,
  resolvedModel, supportedEffortLevels, supportsFastMode...),
  `available_output_styles`, `agents`, `account` (email, organization,
  subscriptionType), `current_permission_mode`, `pid`. Use it as the readiness
  signal and to build the command palette / model picker.

## Frames from the CLI (stdout)

| type/subtype | notes |
|---|---|
| `system/init` | session_id, cwd, model (resolved), permissionMode, tools[], slash_commands[], skills[], agents[], mcp_servers[], capabilities[] (`interrupt_receipt_v1`, `interrupt_cancel_queued_v1`, `msg_lifecycle_v1`), output_style, claude_code_version, apiKeySource |
| `system/status` | `status: "requesting" \| "compacting" \| null`; compaction end carries `compact_result: "success"\|"failed"` + `compact_error` |
| `system/thinking_tokens` | estimated_tokens (+delta) while thinking streams |
| `system/task_started/updated/notification` | subagent (Task) lifecycle |
| `rate_limit_event` | rate_limit_info {status, rateLimitType, resetsAt, ...} |
| `user` (isReplay:true) | echo of every accepted stdin user message (`--replay-user-messages`) |
| `user` | tool_result echoes; top-level `tool_use_result` carries rich structured output; `parent_tool_use_id` set inside subagents; interrupt injects a synthetic `[Request interrupted by user]` text frame |
| `assistant` | full API message **re-emitted after EACH completed content block**, carrying only that block; same message `id` across re-emits; tool_use blocks include extra `caller` field |
| `stream_event` | wraps raw API events: message_start (has `ttft_ms`), content_block_start (text/thinking/tool_use), content_block_delta (text_delta / thinking_delta / signature_delta / input_json_delta), content_block_stop, message_delta (usage incl. `output_tokens_details.thinking_tokens`), message_stop |
| `result` | per **turn**; `subtype: success \| error_during_execution \| ...`, `is_error`, `result` (final text), `total_cost_usd` (**cumulative per process** — per-turn = delta), `usage`, `modelUsage` (incl. `contextWindow`!), `num_turns`, `duration_ms`, `stop_reason`, `terminal_reason` (`completed` / `aborted_streaming`), `permission_denials[]` |
| `control_request` | see permission prompts below |
| `control_response` | success: `{request_id, subtype:"success", response:{...}}`; error: `{request_id, subtype:"error", error:"..."}` |

## Control protocol

App → CLI (`{"type":"control_request","request_id":"...","request":{...}}`):

- `initialize` (`hooks: null`) → rich success payload (above).
- `interrupt` → success `{still_queued:[]}` (queued-but-unstarted user messages
  that were cancelled). Mid-stream: partial assistant kept, then
  `result` `error_during_execution` / `terminal_reason:"aborted_streaming"`,
  and a synthetic user frame. **Session keeps working afterwards.**
- `set_permission_mode` → success `{mode}`. Valid: `acceptEdits, auto,
  bypassPermissions, default, dontAsk, plan`. ⚠️ asymmetry: the *CLI flag*
  accepts `manual` (≈ default) but not `default`; the *control request*
  accepts `default` but not `manual`.
- `set_model` → success (empty). Works live in 2.1.226.
- unknown subtype → error response `Unsupported control request subtype: ...`.

CLI → app permission prompt (only for calls the safe-command classifier does
not auto-approve — `echo`/read-only Bash and Task never prompt; Write/Edit do):

```json
{"type":"control_request","request_id":"<uuid>","request":{
  "subtype":"can_use_tool","tool_name":"Write","display_name":"Write",
  "description":"probe.txt","input":{...},
  "permission_suggestions":[{"type":"setMode","mode":"acceptEdits","destination":"session"}],
  "tool_use_id":"toolu_..."}}
```

Reply on stdin:

```json
{"type":"control_response","response":{"subtype":"success","request_id":"<same>",
  "response":{"behavior":"allow","updatedInput":{...},
              "updatedPermissions":[<suggestions echoed back>]}}}
```

- `updatedPermissions` = echoed `permission_suggestions` implements
  **always-allow** (verified: second identical Write did not prompt).
- Deny: `{"behavior":"deny","message":"...","interrupt":false}` → tool_result
  `is_error:true` with the message; turn continues and completes
  (`subtype:"success"`), denial listed in `result.permission_denials`.

## Input frames (stdin)

```json
{"type":"user","message":{"role":"user","content":[{"type":"text","text":"..."}]}}
```

- Slash commands are plain text (`/compact` executes; failure surfaces as
  `system/status` with `compact_result:"failed"` + assistant text, result with
  `num_turns:0`).
- **Messages sent mid-turn are queued natively** and start their own turn when
  the current one finishes (fixture `midturn`: two results). No app-side outbox
  needed; `interrupt` cancels the queue (`still_queued`).

## Open items / version notes

- `permission_suggestions` may contain other entry types (e.g. `addRules`) for
  other tools/rules — pass entries through opaquely, never model them.
- Compaction fixture only captured the *failed* path ("Not enough messages");
  success path shape assumed symmetric (`compact_result:"success"`).
- Auth-expiry signature not captured (account healthy); handle generically via
  stderr tail + `result.is_error` + process exit.
- Fixtures pinned to claude 2.1.226; re-run `cargo run --bin probe -- all`
  after CLI upgrades and re-run `cargo test`.
