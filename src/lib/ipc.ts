import { Channel, invoke } from "@tauri-apps/api/core";
import type { Batch } from "./generated/Batch";
import type { PermissionDecisionDto } from "./generated/PermissionDecisionDto";
import type { RegistryBatch } from "./generated/RegistryBatch";
import type { SessionConfig } from "./generated/SessionConfig";
import type { SessionInfo } from "./generated/SessionInfo";
import type { SessionRecord } from "./generated/SessionRecord";
import type { SessionSnapshot } from "./generated/SessionSnapshot";
import type { SessionSummary } from "./generated/SessionSummary";
import type { TranscriptItem } from "./generated/TranscriptItem";

export type BatchHandler = (batch: Batch) => void;

function makeChannel(onBatch: BatchHandler): Channel<Batch> {
  const channel = new Channel<Batch>();
  channel.onmessage = onBatch;
  return channel;
}

export function createSession(
  config: SessionConfig,
  onBatch: BatchHandler,
): Promise<SessionInfo> {
  return invoke("create_session", { config, channel: makeChannel(onBatch) });
}

export function attachSession(
  sessionId: string,
  onBatch: BatchHandler,
): Promise<SessionSnapshot> {
  return invoke("attach_session", {
    sessionId,
    channel: makeChannel(onBatch),
  });
}

export function sendUserMessage(sessionId: string, text: string): Promise<void> {
  return invoke("send_user_message", { sessionId, text });
}

export function respondPermission(
  sessionId: string,
  requestId: string,
  decision: PermissionDecisionDto,
): Promise<void> {
  return invoke("respond_permission", { sessionId, requestId, decision });
}

export function setPermissionMode(sessionId: string, mode: string): Promise<void> {
  return invoke("set_permission_mode", { sessionId, mode });
}

export function setModel(sessionId: string, model: string): Promise<void> {
  return invoke("set_model", { sessionId, model });
}

export function interruptSession(sessionId: string): Promise<void> {
  return invoke("interrupt_session", { sessionId });
}

export function stopSession(sessionId: string, graceful = true): Promise<void> {
  return invoke("stop_session", { sessionId, graceful });
}

/** Resolves with a warning string when worktree cleanup was refused. */
export function removeSession(
  sessionId: string,
  cleanupWorktree = false,
): Promise<string | null> {
  return invoke("remove_session", { sessionId, cleanupWorktree });
}

export function listSessions(): Promise<SessionInfo[]> {
  return invoke("list_sessions");
}

/** Only visible sessions receive text deltas (single view: one id; split
 *  view: all pane ids; dashboard: none). */
export function setVisibleSessions(sessionIds: string[]): Promise<void> {
  return invoke("set_visible_sessions", { sessionIds });
}

/** Subscribe to dashboard summaries; resolves with the current full list. */
export function attachRegistry(
  onBatch: (batch: RegistryBatch) => void,
): Promise<SessionSummary[]> {
  const channel = new Channel<RegistryBatch>();
  channel.onmessage = onBatch;
  return invoke("attach_registry", { channel });
}

export function listSessionRecords(): Promise<SessionRecord[]> {
  return invoke("list_session_records");
}

export function deleteSessionRecord(recordId: string): Promise<void> {
  return invoke("delete_session_record", { recordId });
}

export function resumeSession(recordId: string): Promise<SessionInfo> {
  const channel = new Channel<Batch>();
  channel.onmessage = () => {};
  return invoke("resume_session", { recordId, channel });
}

export function loadHistory(
  sessionId: string,
  cwd: string,
  claudeConfigDir: string | null,
): Promise<TranscriptItem[]> {
  return invoke("load_history", { sessionId, cwd, claudeConfigDir });
}
